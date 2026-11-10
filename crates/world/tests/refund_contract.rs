use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use mock_core::{
    dataset::Dataset,
    grading::{Grade, GradeInput, Outcome, grade},
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use scenario::{Scenario, solution::Bindings};
use serde_json::{Value, json};
use tower::ServiceExt;

#[derive(Clone, Copy)]
enum Variant {
    Standard,
    ShiftedId,
    PaymentIntent,
    HelpfulMetadata,
    WrongReference,
    CollateralMutation,
    PrematureReply,
    BareReference,
    DeniedRefund,
    UnreviewedParaphrase,
    GroundedAlternative,
    SplitRefund,
    SplitReordered,
    AtomicReply,
    LegacyForecast,
    MinimalReply,
}

struct Run {
    assessment: Grade,
    bindings: Bindings,
    world: SharedWorld,
}

fn scenario() -> Scenario {
    Scenario::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/business/multi/zendesk-refund-via-stripe"),
    )
    .unwrap()
}

#[test]
fn shared_refund_accounting_does_not_drift_between_single_and_cross_service_contracts() {
    let cross = scenario();
    let single = Scenario::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/business/stripe/refund-duplicate-charge"),
    )
    .unwrap();
    let stripe_records = |scenario: &Scenario| {
        scenario
            .expect
            .records
            .iter()
            .filter(|check| check.selector.collection.starts_with("/services/stripe/"))
            .cloned()
            .collect::<Vec<_>>()
    };
    let stripe_relations = |scenario: &Scenario| {
        scenario
            .expect
            .relations
            .iter()
            .filter(|check| check.source.collection.starts_with("/services/stripe/"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(stripe_records(&cross), stripe_records(&single));
    assert_eq!(stripe_relations(&cross), stripe_relations(&single));
}

#[test]
fn elasticsearch_refund_preserves_the_shared_accounting_contract_with_its_own_source_relationships()
{
    fn normalize(value: &mut Value) {
        match value {
            Value::String(text) if text == "ch_r1lxhPeTsZKQxIfuhPANytDa" => {
                *text = "ch_3Qb2MjLee109003aQs9uD".into()
            }
            Value::String(text) if text == "pi_qZ8HsvmHI2loKwRfwA0eFYWI" => {
                *text = "pi_3Qb2MjLee109003bXz4aE".into()
            }
            Value::Number(number) if number.as_i64() == Some(48000) => *value = json!(8640),
            Value::Number(number) if number.as_i64() == Some(-48000) => *value = json!(-8640),
            Value::Array(values) => values.iter_mut().for_each(normalize),
            Value::Object(fields) => fields.values_mut().for_each(normalize),
            _ => {}
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    let single = Scenario::load(&root.join("business/stripe/refund-duplicate-charge")).unwrap();
    let cross =
        Scenario::load(&root.join("search/multi/es-duplicate-charge-stripe-refund")).unwrap();
    let accounting = |scenario: &Scenario| {
        json!({
            "records":scenario.expect.records,
            "relations":scenario.expect.relations.iter()
                .filter(|check|check.target.collection.starts_with("/services/stripe/"))
                .collect::<Vec<_>>()
        })
    };
    let mut expected = accounting(&single);
    normalize(&mut expected);
    assert_eq!(accounting(&cross), expected);
    assert_eq!(
        cross.expect.relations.len(),
        single.expect.relations.len() + 2
    );
}

async fn call(app: &Router, name: &str, arguments: Value) -> Value {
    let request = Request::post("/_mcp/tools/call")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"name": name, "arguments": arguments}).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert!(response.status().is_success());
    let result: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_ne!(result["isError"], true, "{result}");
    result
}

async fn run(variant: Variant, prediction: Option<Value>) -> Run {
    let scenario = scenario();
    let world = shared(WorldState::from_dataset(Dataset::load(&scenario.dir).unwrap()).unwrap());
    if matches!(variant, Variant::ShiftedId) {
        world
            .lock()
            .unwrap()
            .service_mut("stripe")
            .ids
            .next_prefixed("refund", "re_", 24);
    }
    let stripe = service_app(world.clone(), &mock_stripe::Stripe);
    let zendesk = service_app(world.clone(), &mock_zendesk::Zendesk);
    let refund_index = scenario
        .solution
        .calls
        .iter()
        .position(|call| call.arguments["stripe_api_operation_id"] == "PostRefunds")
        .unwrap();
    let reply_index = scenario
        .solution
        .calls
        .iter()
        .position(|call| call.tool == "create_ticket_comment")
        .unwrap();
    let mut order: Vec<_> = (0..scenario.solution.calls.len()).collect();
    let mut bindings = Bindings::new();
    if matches!(variant, Variant::PrematureReply) {
        order.swap(refund_index, reply_index);
        bindings.insert(
            "refund_id".into(),
            prediction.expect("a predicted ID is required"),
        );
    }
    for index in order {
        let oracle_call = &scenario.solution.calls[index];
        if matches!(variant, Variant::PrematureReply) && index == refund_index {
            bindings.remove("refund_id");
        }
        let mut arguments = oracle_call.resolve_arguments(&bindings).unwrap();
        if index == refund_index
            && matches!(variant, Variant::SplitRefund | Variant::SplitReordered)
        {
            let mut references = Vec::new();
            for amount in [18000, 30000] {
                arguments["parameters"]["amount"] = json!(amount);
                let result = call(&stripe, &oracle_call.tool, arguments.clone()).await;
                let mut captured = Bindings::new();
                oracle_call.capture_result(&result, &mut captured).unwrap();
                references.push(captured["refund_id"].as_str().unwrap().to_owned());
            }
            references.sort();
            if matches!(variant, Variant::SplitReordered) {
                references.reverse();
            }
            bindings.insert("refund_id".into(), json!(references.join(", ")));
            continue;
        }
        if index == refund_index {
            match variant {
                Variant::PaymentIntent => {
                    let intent = world
                        .lock()
                        .unwrap()
                        .service("stripe")
                        .unwrap()
                        .store
                        .collection("charges")
                        .unwrap()
                        .get("ch_r1lxhPeTsZKQxIfuhPANytDa")
                        .unwrap()["payment_intent"]
                        .clone();
                    arguments["parameters"]
                        .as_object_mut()
                        .unwrap()
                        .remove("charge");
                    arguments["parameters"]["payment_intent"] = intent;
                }
                Variant::HelpfulMetadata => {
                    arguments["parameters"]["metadata"] = json!({"order": "1077"});
                }
                _ => {}
            }
        }
        if index == reply_index && matches!(variant, Variant::WrongReference) {
            arguments["comment"] = json!("Your refund re_unrelated was issued.");
        }
        if index == reply_index {
            let reference = bindings["refund_id"].as_str().unwrap();
            let body = match variant {
                Variant::BareReference => Some(reference.to_owned()),
                Variant::MinimalReply => Some(format!(
                    "Your duplicate payment was refunded. Stripe refund references: {reference}."
                )),
                Variant::DeniedRefund => Some(format!(
                    "The duplicate payment has not been refunded. Stripe refund reference: {reference}."
                )),
                Variant::UnreviewedParaphrase => Some(format!(
                    "Hannah, the entire extra $480 payment for #1077 was returned. You can reference {reference}; the original is untouched."
                )),
                Variant::GroundedAlternative
                | Variant::SplitRefund
                | Variant::SplitReordered
                | Variant::AtomicReply => Some(format!(
                    "Hannah, the full duplicate payment of $480.00 for order #1077 has been refunded. Stripe refund references: {reference}. The original payment is unchanged."
                )),
                Variant::LegacyForecast => Some(format!(
                    "Hi Hannah, the duplicate $480.00 charge for order #1077 was refunded. The refund ID is {reference}; it should appear on your Visa ending 1881 within 5-10 business days. The original charge stays in place."
                )),
                _ => None,
            };
            if let Some(body) = body {
                arguments["comment"] = json!(body);
            }
            if matches!(variant, Variant::AtomicReply) {
                call(
                    &zendesk,
                    "update_ticket",
                    json!({
                        "ticket_id": 5102, "status": "solved",
                        "comment": {"body": arguments["comment"], "public": true}
                    }),
                )
                .await;
                continue;
            }
        }
        let app = match oracle_call.endpoint.as_str() {
            "stripe" => &stripe,
            "zendesk" => &zendesk,
            endpoint => panic!("unexpected endpoint {endpoint}"),
        };
        let result = call(app, &oracle_call.tool, arguments).await;
        oracle_call.capture_result(&result, &mut bindings).unwrap();
    }
    if matches!(variant, Variant::CollateralMutation) {
        call(
            &zendesk,
            "update_ticket",
            json!({"ticket_id": 5088, "priority": "urgent"}),
        )
        .await;
    }
    let assessment = {
        let state = world.lock().unwrap();
        let initial = state.initial_snapshot();
        grade(&GradeInput {
            expect: &scenario.expect,
            state: state.snapshot(),
            journal: state.journal.entries(),
            answer: None,
            tools_ips: vec!["unknown".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&initial),
            events: Some(state.events().entries()),
        })
    };
    Run {
        assessment,
        bindings,
        world,
    }
}

#[tokio::test]
async fn the_real_multi_service_oracle_passes_with_dynamic_refund_references() {
    let result = run(Variant::Standard, None).await;
    assert_eq!(result.assessment.reward, 1.0, "{:?}", result.assessment);
    assert!(result.assessment.history_checked);
    assert!(result.assessment.relations_ok);
    assert!(result.assessment.ordering_ok);
}

#[test]
fn the_strict_contract_rejects_no_op_without_an_environment_fault() {
    let scenario = scenario();
    let world = shared(WorldState::from_dataset(Dataset::load(&scenario.dir).unwrap()).unwrap());
    let state = world.lock().unwrap();
    let initial = state.initial_snapshot();
    let assessment = grade(&GradeInput {
        expect: &scenario.expect,
        state: state.snapshot(),
        journal: state.journal.entries(),
        answer: None,
        tools_ips: vec!["unknown".into()],
        context_overflow: false,
        startup: &[],
        initial_state: Some(&initial),
        events: Some(state.events().entries()),
    });
    assert_eq!(assessment.reward, 0.0);
    assert!(assessment.environment_ok);
    assert!(assessment.contract_ok);
    assert!(!assessment.records_ok);
}

#[tokio::test]
async fn a_shifted_id_stream_and_a_payment_intent_plan_are_valid_alternatives() {
    let standard = run(Variant::Standard, None).await;
    let shifted = run(Variant::ShiftedId, None).await;
    assert_ne!(
        standard.bindings["refund_id"],
        shifted.bindings["refund_id"]
    );
    assert_eq!(shifted.assessment.reward, 1.0, "{:?}", shifted.assessment);
    let intent = run(Variant::PaymentIntent, None).await;
    assert_eq!(intent.assessment.reward, 1.0, "{:?}", intent.assessment);
    let metadata = run(Variant::HelpfulMetadata, None).await;
    assert_eq!(metadata.assessment.reward, 1.0, "{:?}", metadata.assessment);
}

#[tokio::test]
async fn wrong_references_and_unrelated_ticket_changes_fail_independently() {
    let wrong = run(Variant::WrongReference, None).await;
    assert!(wrong.assessment.state_ok);
    assert!(!wrong.assessment.records_ok);
    assert!(wrong.assessment.relations_ok);
    assert_eq!(wrong.assessment.reward, 0.0);
    let extra = run(Variant::CollateralMutation, None).await;
    assert!(extra.assessment.state_ok);
    assert!(extra.assessment.records_ok);
    assert!(extra.assessment.forbidden_calls_ok);
    assert!(!extra.assessment.preservation_ok);
    assert_eq!(extra.assessment.reward, 0.0);
}

#[tokio::test]
async fn predicting_the_future_id_cannot_make_a_premature_public_reply_pass() {
    let standard = run(Variant::Standard, None).await;
    let premature = run(
        Variant::PrematureReply,
        Some(standard.bindings["refund_id"].clone()),
    )
    .await;
    assert_eq!(
        premature.world.lock().unwrap().snapshot()["services"]["stripe"]["charges"]["ch_r1lxhPeTsZKQxIfuhPANytDa"]
            ["amount_refunded"],
        json!(48000)
    );
    assert!(premature.assessment.state_ok);
    assert!(!premature.assessment.records_ok);
    assert!(premature.assessment.relations_ok);
    assert!(!premature.assessment.ordering_ok);
    assert_eq!(premature.assessment.reward, 0.0);
}

#[tokio::test]
async fn id_mentions_do_not_prove_refund_communication_and_unknown_paraphrases_are_not_model_failures()
 {
    for variant in [
        Variant::BareReference,
        Variant::DeniedRefund,
        Variant::UnreviewedParaphrase,
        Variant::LegacyForecast,
    ] {
        let result = run(variant, None).await;
        assert!(result.assessment.state_ok && result.assessment.relations_ok);
        assert_eq!(
            result.assessment.outcome,
            Outcome::Unresolved,
            "{:?}",
            result.assessment
        );
        assert!(result.assessment.adjudication_needed && result.assessment.environment_ok);
    }
}

#[tokio::test]
async fn grounded_alternatives_split_refunds_and_atomic_closure_are_not_forced_into_one_oracle_plan()
 {
    for variant in [
        Variant::GroundedAlternative,
        Variant::SplitRefund,
        Variant::SplitReordered,
        Variant::AtomicReply,
        Variant::MinimalReply,
    ] {
        let result = run(variant, None).await;
        assert_eq!(
            result.assessment.outcome,
            Outcome::Pass,
            "{:?}",
            result.assessment
        );
        assert!(result.assessment.history_checked && result.assessment.relations_ok);
    }
}
