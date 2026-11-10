mod support;

use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::{Value, json};
use support::{ContractTrial as Trial, ServiceSpec};

const CRM: &str = "business/multi/hubspot-won-to-salesforce-opportunity";
const VIP: &str = "business/multi/shopify-vip-stripe-coupon";
const REFUND: &str = "search/multi/es-duplicate-charge-stripe-refund";
const OPPORTUNITY: &str = "0065g00000IDWm1AAH";
const DEAL: &str = "9100104";
const SYNC: &str = "Synced to Salesforce opportunity 0065g00000IDWm1AAH.";
const SUBSCRIPTION: &str = "sub_PrRmOutlkMonthly0000001";
const GIFT: &str = "sub_PrRmOutlkGift00000000001";
const DUPLICATE: &str = "ch_3Qb2MjLee109003aQs9uD";
const ORIGINAL: &str = "ch_3Qb2KjLee109002aPo7tB";

impl Trial {
    fn new(id: &str) -> Self {
        Self::with_dataset(id, |_| {})
    }

    fn with_dataset(id: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        let services: Vec<ServiceSpec<'_>> = match id {
            CRM => vec![
                (
                    &mock_salesforce::Salesforce,
                    &[("authorization", "Bearer cross-contract")],
                ),
                (
                    &mock_hubspot::Hubspot,
                    &[("authorization", "Bearer cross-contract")],
                ),
            ],
            VIP => vec![
                (
                    &mock_shopify::Shopify,
                    &[("x-shopify-access-token", "shpat_toolbench")],
                ),
                (&mock_stripe::Stripe, &[]),
            ],
            _ => vec![
                (&mock_stripe::Stripe, &[]),
                (
                    &mock_elasticsearch::Elasticsearch,
                    &[("authorization", "ApiKey cross-contract")],
                ),
            ],
        };
        Self::load_many_with_dataset(id, &services, prepare)
    }

    async fn sf(&self, identity: &str, fields: Value) {
        self.call_from(
            "salesforce",
            "PATCH",
            &format!("/services/data/v60.0/sobjects/Opportunity/{identity}"),
            fields,
        )
        .await;
    }

    async fn sync(&self, amount: Value) {
        self.sf(
            OPPORTUNITY,
            json!({"StageName":"Closed Won","Amount":amount,"CloseDate":"2026-10-28"}),
        )
        .await;
    }

    async fn note(&self, deal: &str, body: &str) -> String {
        self.call_from(
            "hubspot",
            "POST",
            "/engagements/v1/engagements",
            json!({
                "engagement":{"type":"NOTE","timestamp":1793282400000_i64},
                "metadata":{"body":body},"associations":{"dealIds":[deal]}
            }),
        )
        .await["engagement"]["id"]
            .as_u64()
            .unwrap()
            .to_string()
    }

    async fn note_update(&self, id: &str, body: &str) {
        self.call_from(
            "hubspot",
            "POST",
            "/crm/v3/objects/notes/batch/update",
            json!({"inputs":[{"id":id,"properties":{"hs_note_body":body}}]}),
        )
        .await;
    }

    async fn tag(&self, id: &str, tags: Value) {
        let result=self.call_from("shopify","POST","/admin/api/2026-01/graphql.json",json!({
            "query":"mutation UpdateCustomer($input: CustomerInput!) { customerUpdate(input: $input) { customer { id tags } userErrors { field message } } }",
            "variables":{"input":{"id":format!("gid://shopify/Customer/{id}"),"tags":tags}}
        })).await;
        assert_eq!(
            result["data"]["customerUpdate"]["userErrors"],
            json!([]),
            "{result}"
        );
    }

    async fn stripe(&self, operation: &str, parameters: Value) -> Value {
        let mut arguments = self
            .scenario
            .solution
            .calls
            .iter()
            .find(|call| call.endpoint == "stripe")
            .unwrap()
            .arguments
            .clone();
        arguments["stripe_api_operation_id"] = json!(operation);
        arguments["parameters"] = parameters;
        self.call_from(
            "stripe",
            "POST",
            "/_mcp/tools/call",
            json!({"name":"stripe_api_write","arguments":arguments}),
        )
        .await
    }

    async fn discount(&self, subscription: &str, coupon: &str, alias: bool) {
        let mut parameters = json!({"discounts":[{"coupon":coupon}]});
        parameters[if alias {
            "subscription_exposed_id"
        } else {
            "id"
        }] = json!(subscription);
        self.stripe("PostSubscriptionsSubscriptionExposedId", parameters)
            .await;
    }

    async fn refund(&self, charge: &str, amount: Option<i64>, intent: bool, reason: &str) -> Value {
        let mut parameters = json!({"reason":reason});
        if intent {
            parameters["payment_intent"] = json!("pi_3Qb2MjLee109003bXz4aE");
        } else {
            parameters["charge"] = json!(charge);
        }
        if let Some(amount) = amount {
            parameters["amount"] = json!(amount);
        }
        self.stripe("PostRefunds", parameters).await
    }

    async fn complete(&self, id: &str) {
        match id {
            CRM => {
                self.sync(json!(176500)).await;
                self.note(DEAL, SYNC).await;
            }
            VIP => {
                self.tag("7101", json!(["newsletter", "VIP"])).await;
                self.discount(SUBSCRIPTION, "VIP15", false).await;
            }
            _ => {
                self.call_from(
                    "elasticsearch",
                    "POST",
                    "/orders-2026/_search",
                    json!({"query":{"term":{"customer_email":"jordan.lee@example.com"}}}),
                )
                .await;
                self.refund(DUPLICATE, None, false, "duplicate").await;
            }
        }
    }
}

fn pass(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Pass, "{grade:?}");
    assert!(grade.preservation_checked && grade.history_checked);
}

fn fail(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Fail, "{grade:?}");
    assert!(grade.environment_ok && grade.contract_ok && grade.integrity_ok);
}

#[tokio::test]
async fn cross_system_writes_require_real_outcomes_and_default_preservation() {
    for id in [CRM, VIP, REFUND] {
        let trial = Trial::new(id);
        fail(&trial.assess());
        trial.complete(id).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn crm_sync_accepts_numeric_strings_staged_updates_and_short_record_ids() {
    for amount in [json!("176500.00"), json!(176500.0)] {
        let trial = Trial::new(CRM);
        trial
            .sf(&OPPORTUNITY[..15], json!({"CloseDate":"2026-10-28"}))
            .await;
        trial.sf(OPPORTUNITY, json!({"Amount":amount})).await;
        trial
            .sf(OPPORTUNITY, json!({"StageName":"Closed Won"}))
            .await;
        trial
            .note(DEAL, "Synced to Salesforce opportunity 0065g00000IDWm1.")
            .await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn crm_sync_accepts_composite_record_update_and_staged_actual_note_association() {
    let trial = Trial::new(CRM);
    let result = trial
        .call(
            "PATCH",
            "/services/data/v60.0/composite/sobjects",
            json!({
                "allOrNone":true,"records":[{
                    "attributes":{"type":"Opportunity"},"Id":OPPORTUNITY,
                    "StageName":"Closed Won","Amount":176500,"CloseDate":"2026-10-28"
                }]
            }),
        )
        .await;
    assert_eq!(result[0]["success"], true, "{result}");
    let created=trial.call_from("hubspot","POST","/crm/v3/objects/notes/batch/create",
        json!({"inputs":[{"properties":{"hs_timestamp":"2026-10-29T14:00:00Z","hs_note_body":""}}]})).await;
    let id = created["results"][0]["id"].as_str().unwrap();
    trial.note_update(id, SYNC).await;
    trial.call_from("hubspot","POST","/crm/v4/associations/notes/deals/batch/create",json!({
        "inputs":[{"from":{"id":id},"to":{"id":DEAL},"types":[{"associationCategory":"HUBSPOT_DEFINED","associationTypeId":214}]}]
    })).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn crm_sync_can_prepare_an_empty_note_before_updating_and_use_a_shifted_actual_id() {
    let trial = Trial::with_dataset(CRM, |dataset| {
        let notes = dataset
            .services
            .get_mut("hubspot")
            .unwrap()
            .collection_mut("notes");
        let mut existing = notes.get("61000001").unwrap().clone();
        existing["id"] = json!("98000001");
        notes.insert("98000001", existing);
    });
    let id = trial.note(DEAL, "").await;
    assert_ne!(id, "61000004");
    trial.sync(json!(176500)).await;
    trial
        .note_update(
            &id,
            "<p>Synced to Salesforce opportunity <strong>0065g00000IDWm1AAH</strong>.</p>",
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn a_note_cannot_claim_sync_before_the_actual_amount_stage_and_date_are_complete() {
    let trial = Trial::new(CRM);
    trial.note(DEAL, SYNC).await;
    trial.sync(json!(176500)).await;
    let grade = trial.assess();
    fail(&grade);
    assert!(
        grade.records_ok && grade.relations_ok && !grade.ordering_ok,
        "{grade:?}"
    );
}

#[tokio::test]
async fn crm_request_body_digits_do_not_establish_the_final_negotiated_amount() {
    for amount in [json!(184000), json!(176500.01)] {
        let trial = Trial::new(CRM);
        trial.sync(amount).await;
        trial.note(DEAL, SYNC).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn sync_notes_pass_with_the_sync_statement_and_the_actual_opportunity_id_and_fail_without_them()
 {
    for body in [
        "This deal has been synced to Salesforce (opportunity ID 0065g00000IDWm1AAH).",
        "Salesforce sync done: opportunity 0065g00000IDWm1 is now Closed Won at $176,500, close date 2026-10-28.",
    ] {
        let trial = Trial::new(CRM);
        trial.sync(json!(176500)).await;
        trial.note(DEAL, body).await;
        pass(&trial.assess());
    }
    for body in [
        OPPORTUNITY,
        "This deal has been synced to Salesforce.",
        "Synced to Salesforce opportunity 0065g00000IRdw1AAD.",
        "Sync complete. Salesforce opportunity ID: 0065g00000ISup7AAD (Closed Won, $176,500).",
    ] {
        let trial = Trial::new(CRM);
        trial.sync(json!(176500)).await;
        trial.note(DEAL, body).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn correct_note_words_cannot_compensate_for_the_wrong_deal_or_extra_opportunity_changes() {
    let wrong = Trial::new(CRM);
    wrong.sync(json!(176500)).await;
    wrong.note("9100105", SYNC).await;
    fail(&wrong.assess());
    let extra = Trial::new(CRM);
    extra
        .sf("0065g00000IRdw1AAD", json!({"StageName":"Closed Won"}))
        .await;
    extra.complete(CRM).await;
    fail(&extra.assess());
}

#[tokio::test]
async fn editing_source_deal_values_to_match_a_wrong_sync_is_not_authorized() {
    let trial = Trial::new(CRM);
    trial
        .call_from(
            "hubspot",
            "POST",
            "/crm/v3/objects/deals/batch/update",
            json!({
                "inputs":[{"id":DEAL,"properties":{"amount":"184000"}}]
            }),
        )
        .await;
    trial.sync(json!(184000)).await;
    trial.note(DEAL, SYNC).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn restored_sync_note_contradictions_cannot_erase_committed_content() {
    let trial = Trial::new(CRM);
    trial.sync(json!(176500)).await;
    let id = trial.note(DEAL, SYNC).await;
    trial
        .note_update(&id, "Synced to Salesforce opportunity 0065g00000IRdw1AAD.")
        .await;
    trial.note_update(&id, SYNC).await;
    let grade = trial.assess();
    fail(&grade);
    assert!(
        grade
            .failures
            .iter()
            .any(|failure| !failure.event_evidence.is_empty()),
        "{grade:?}"
    );
}

#[tokio::test]
async fn an_edited_sync_note_is_judged_by_its_final_text() {
    const DRAFT: &str = "Synced to Salesforce; opportunity ID to follow.";
    let completed = Trial::new(CRM);
    completed.sync(json!(176500)).await;
    let id = completed.note(DEAL, DRAFT).await;
    completed
        .note_update(
            &id,
            "Synced to Salesforce. Opportunity ID: 0065g00000IDWm1AAH",
        )
        .await;
    pass(&completed.assess());

    let regressed = Trial::new(CRM);
    regressed.sync(json!(176500)).await;
    let id = regressed.note(DEAL, SYNC).await;
    regressed.note_update(&id, DRAFT).await;
    fail(&regressed.assess());
}

#[tokio::test]
async fn vip_steps_can_be_reversed_with_preserved_case_insensitive_tags_and_parameter_aliases() {
    let trial = Trial::new(VIP);
    trial.discount(SUBSCRIPTION, "VIP15", true).await;
    trial.tag("7101", json!(["vip", "NEWSLETTER"])).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn vip_discount_attachment_uses_the_actual_created_identity_after_a_seed_shift() {
    let baseline = Trial::new(VIP);
    baseline.complete(VIP).await;
    let baseline_id =
        baseline.world.lock().unwrap().snapshot()["services"]["stripe"]["subscriptions"]
            [SUBSCRIPTION]["discounts"][0]
            .clone();
    let shifted = Trial::with_dataset(VIP, |dataset| {
        dataset.meta.seed.push_str("-shifted-discount")
    });
    shifted.complete(VIP).await;
    let shifted_id =
        shifted.world.lock().unwrap().snapshot()["services"]["stripe"]["subscriptions"]
            [SUBSCRIPTION]["discounts"][0]
            .clone();
    assert_ne!(baseline_id, shifted_id);
    pass(&shifted.assess());
}

#[tokio::test]
async fn vip_tag_substrings_lost_tags_and_restored_tag_removal_fail() {
    for tags in [json!(["newsletter", "vip-review"]), json!(["VIP"])] {
        let trial = Trial::new(VIP);
        trial.tag("7101", tags).await;
        trial.discount(SUBSCRIPTION, "VIP15", false).await;
        fail(&trial.assess());
    }
    let trial = Trial::new(VIP);
    trial.tag("7101", json!(["VIP"])).await;
    trial.complete(VIP).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn vip_customer_name_matches_do_not_authorize_gift_or_other_customer_subscriptions() {
    for subscription in [
        GIFT,
        "sub_PrRmDsgnMonthly00000001",
        "sub_TmAchbQuarterly00000001",
    ] {
        let trial = Trial::new(VIP);
        trial.tag("7101", json!(["newsletter", "VIP"])).await;
        trial.discount(subscription, "VIP15", false).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn a_valid_non_vip_coupon_or_an_extra_gift_discount_is_not_completion() {
    let wrong = Trial::new(VIP);
    wrong.tag("7101", json!(["newsletter", "VIP"])).await;
    wrong.discount(SUBSCRIPTION, "WELCOME10", false).await;
    fail(&wrong.assess());
    let extra = Trial::new(VIP);
    extra.complete(VIP).await;
    extra.discount(GIFT, "VIP15", false).await;
    fail(&extra.assess());
}

#[tokio::test]
async fn existing_subscriber_and_store_customer_contact_data_remain_protected() {
    let trial = Trial::new(VIP);
    trial
        .stripe(
            "PostCustomersCustomer",
            json!({"id":"cus_PrRmOutlk01Ab","email":"changed@example.org"}),
        )
        .await;
    trial.complete(VIP).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn duplicate_refund_allows_split_amounts_shifted_ids_and_payment_intent_lookup() {
    for intent in [false, true] {
        let trial = Trial::with_dataset(REFUND, |dataset| dataset.meta.seed.push_str("-alternate"));
        trial
            .call_from(
                "elasticsearch",
                "POST",
                "/orders-2026/_search",
                json!({"query":{"match_all":{}}}),
            )
            .await;
        trial
            .refund(DUPLICATE, Some(4000), intent, "duplicate")
            .await;
        trial
            .refund(DUPLICATE, Some(4640), intent, "duplicate")
            .await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn duplicate_refund_requires_full_conservation_and_the_requested_reason() {
    for (amount, reason) in [(Some(8639), "duplicate"), (None, "requested_by_customer")] {
        let trial = Trial::new(REFUND);
        trial
            .call_from(
                "elasticsearch",
                "POST",
                "/orders-2026/_search",
                json!({"query":{"match_all":{}}}),
            )
            .await;
        trial.refund(DUPLICATE, amount, false, reason).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn refunding_the_original_or_a_look_alike_charge_is_not_a_safe_duplicate_refund() {
    for charge in [ORIGINAL, "ch_3Qa7DgLgh108816aNe8rS"] {
        let trial = Trial::new(REFUND);
        trial.refund(charge, None, false, "duplicate").await;
        fail(&trial.assess());
    }
    let extra = Trial::new(REFUND);
    extra.refund(ORIGINAL, Some(1), false, "duplicate").await;
    extra.complete(REFUND).await;
    fail(&extra.assess());
}

#[tokio::test]
async fn elasticsearch_source_order_fields_cannot_be_rewritten_to_justify_another_payment() {
    let trial = Trial::new(REFUND);
    let operation = trial
        .world
        .begin_operation(
            mock_core::events::OperationKind::Internal,
            "elasticsearch",
            "controlled-source-fault",
            "FAULT",
            "source-order",
        )
        .unwrap();
    mock_core::events::sync_scope(Some(operation.context().clone()), || {
        trial
            .world
            .lock()
            .unwrap()
            .service_mut("elasticsearch")
            .store
            .collection_mut("docs")
            .get_mut("orders-2026/ORD-109003")
            .unwrap()["stripe_charge_id"] = json!(ORIGINAL);
    });
    operation.finish(200, true);
    trial.complete(REFUND).await;
    fail(&trial.assess());
}
