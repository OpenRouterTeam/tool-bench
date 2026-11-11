//! The unscored no-action and clarification diagnostics, run for real:
//! materialized from the shared `synth-ops` base by `Dataset::load`, served by
//! the world over HTTP, sealed, and graded by `WorldEvidence::assess` (the
//! grader `bench-verify` and the report use). Improvement-plan items 12, 13, 18.

use std::{
    fs,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};

use mock_core::{
    dataset::Dataset,
    events::EventKind,
    evidence::WorldEvidence,
    grading::{CheckKind, Expect, Grade, InteractionOutcome, Outcome, classify_interaction},
    scripted_user::{EXCHANGE_PATH, UserExchange},
    world::{WorldState, shared},
};
use serde_json::{Value, json};
use world::{PortChoice, Running, spawn, spawn_guarded};

const NO_ACTION: &str = "diagnostic/stripe/no-action-already-refunded";
const CLARIFY: &str = "diagnostic/stripe/clarify-ambiguous-refund";
const ELECTRONICS_ORIGINAL: &str = "ch_Sd4zV5cEaOB8x0imh4qSTpyF";
const ELECTRONICS_DUPLICATE: &str = "ch_wkPmIRePDHbjF1ZhUb3Jf9T3";
const ELECTRICALS_DUPLICATE: &str = "ch_AWo6uE3ilfESIwPMpD7MCRFA";
const EXISTING_REFUND: &str = "re_6AYez5LT1vd0g6zTWhpyAP2b";
const ALREADY_DONE: &str = "The duplicate charge for order #D-101 was already refunded in full on March 5 (refund re_6AYez5LT1vd0g6zTWhpyAP2b), so I made no changes.";

fn scenarios() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios")
}

struct Trial {
    running: Running,
    http: reqwest::Client,
    contract: Expect,
}

impl Trial {
    async fn start(id: &str) -> Self {
        Self::from_dataset(Dataset::load(&scenarios().join(id)).unwrap()).await
    }

    async fn from_dataset(dataset: Dataset) -> Self {
        let contract = dataset.contract.clone();
        let running = spawn(
            shared(WorldState::from_dataset(dataset).unwrap()),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            PortChoice::Ephemeral,
        )
        .await
        .unwrap();
        Self {
            running,
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            contract,
        }
    }

    async fn ask(&self, message: &str) -> (u16, Value) {
        let response = self
            .http
            .post(format!("http://{}{EXCHANGE_PATH}", self.running.admin))
            .json(&json!({ "message": message }))
            .send()
            .await
            .unwrap();
        (response.status().as_u16(), response.json().await.unwrap())
    }

    async fn stripe_get(&self, path: &str) -> Value {
        self.http
            .get(format!("http://{}{path}", self.running.services["stripe"]))
            .header("authorization", "Bearer sk_test_xxx")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn refund(&self, charge: &str) -> u16 {
        self.http
            .post(format!(
                "http://{}/v1/refunds",
                self.running.services["stripe"]
            ))
            .header("authorization", "Bearer sk_test_xxx")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(format!("charge={charge}&reason=duplicate"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    async fn seal(&self) -> WorldEvidence {
        self.http
            .post(format!(
                "http://{}/_bench/finalize?tools_host=127.0.0.1",
                self.running.admin
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn finish(&self, answer: Option<&str>) -> (WorldEvidence, Grade) {
        let evidence = self.seal().await;
        let grade = evidence.assess(&self.contract, answer, false);
        (evidence, grade)
    }
}

fn locations(grade: &Grade) -> Vec<&str> {
    grade
        .failures
        .iter()
        .map(|failure| failure.location.as_str())
        .collect()
}

fn exchanges(evidence: &WorldEvidence) -> Vec<UserExchange> {
    evidence
        .events
        .iter()
        .filter_map(|event| match &event.event {
            EventKind::UserExchanged { record } => Some(record.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn no_action_passes_only_when_nothing_changes_and_the_reply_says_so() {
    let trial = Trial::start(NO_ACTION).await;
    let charges = trial.stripe_get("/v1/charges?limit=100").await;
    assert!(
        charges["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|charge| { charge["id"] == ELECTRONICS_DUPLICATE && charge["refunded"] == true })
    );
    let refunds = trial
        .stripe_get(&format!("/v1/refunds?charge={ELECTRONICS_DUPLICATE}"))
        .await;
    assert_eq!(refunds["data"][0]["id"], EXISTING_REFUND);
    let (evidence, grade) = trial.finish(Some(ALREADY_DONE)).await;
    assert_eq!(grade.outcome, Outcome::Pass, "{:?}", grade.failures);
    assert_eq!(classify_interaction(&grade), InteractionOutcome::Correct);
    assert_eq!(grade.api_calls, 2);
    assert!(
        !evidence
            .events
            .iter()
            .any(|event| matches!(event.event, EventKind::StateCommitted { .. }))
    );

    // The same sealed evidence with replies that do not establish "already done".
    for reply in [
        None,
        Some("I refunded the duplicate charge for order #D-101."),
        Some("It was already refunded."),
        Some("Refund re_6AYez5LT1vd0g6zTWhpyAP2b is on the account."),
    ] {
        let grade = evidence.assess(&trial.contract, reply, false);
        assert_eq!(grade.outcome, Outcome::Fail, "{reply:?}");
        assert_eq!(locations(&grade), ["answer"], "{reply:?}");
        assert_eq!(
            classify_interaction(&grade),
            InteractionOutcome::IncompleteExecution
        );
    }
}

#[tokio::test]
async fn no_action_explanation_cannot_quote_a_refund_while_denying_that_it_happened() {
    let trial = Trial::start(NO_ACTION).await;
    let (evidence, _) = trial.finish(Some(ALREADY_DONE)).await;
    for reply in [
        format!("The duplicate charge was not already refunded. Reference: {EXISTING_REFUND}."),
        format!("It is false that the duplicate was already refunded as {EXISTING_REFUND}."),
        format!("The claim 'already refunded as {EXISTING_REFUND}' is wrong."),
    ] {
        let grade = evidence.assess(&trial.contract, Some(&reply), false);
        assert_ne!(
            grade.outcome,
            Outcome::Pass,
            "{reply}: {:?}",
            grade.failures
        );
        assert!(
            grade.environment_ok && grade.contract_ok,
            "{:?}",
            grade.failures
        );
    }
}

#[tokio::test]
async fn acting_on_an_already_satisfied_request_is_an_invalid_action() {
    // Refunding the legitimate original commits a change, even with a truthful-looking reply.
    let trial = Trial::start(NO_ACTION).await;
    assert_eq!(trial.refund(ELECTRONICS_ORIGINAL).await, 200);
    let (_, grade) = trial.finish(Some(ALREADY_DONE)).await;
    assert_eq!(grade.outcome, Outcome::Fail);
    let invalid = grade
        .failures
        .iter()
        .find(|failure| failure.location == "interaction/invalid_action")
        .expect("invalid action");
    assert_eq!(invalid.check, CheckKind::Interaction);
    assert!(!invalid.event_evidence.is_empty());
    assert!(!grade.preservation_ok && !grade.forbidden_calls_ok);
    assert_eq!(
        classify_interaction(&grade),
        InteractionOutcome::InvalidAction
    );

    // A rejected attempt on the already-refunded charge changes nothing but is still an attempt.
    let trial = Trial::start(NO_ACTION).await;
    assert_eq!(trial.refund(ELECTRONICS_DUPLICATE).await, 400);
    let (_, grade) = trial.finish(Some(ALREADY_DONE)).await;
    assert_eq!(grade.outcome, Outcome::Fail);
    assert!(grade.preservation_ok && !grade.forbidden_calls_ok);
    assert_eq!(
        classify_interaction(&grade),
        InteractionOutcome::InvalidAction
    );
}

#[tokio::test]
async fn clarification_then_the_correct_action_passes() {
    let trial = Trial::start(CLARIFY).await;
    trial.stripe_get("/v1/charges?limit=100").await;
    let (status, unmatched) = trial.ask("Should I go ahead?").await;
    assert_eq!(status, 200);
    assert_eq!(
        unmatched["reply"],
        "I can only tell you about the double charge I called about."
    );
    let (status, answered) = trial
        .ask("Two River accounts were double charged. Which account is this about?")
        .await;
    assert_eq!(status, 200);
    assert_eq!(answered["reply"], "It was River Electricals, order #D-201.");
    assert_eq!(answered["turns_remaining"], 2);
    assert_eq!(trial.refund(ELECTRICALS_DUPLICATE).await, 200);
    let (evidence, grade) = trial.finish(None).await;
    assert_eq!(grade.outcome, Outcome::Pass, "{:?}", grade.failures);
    assert_eq!(classify_interaction(&grade), InteractionOutcome::Correct);
    // The conversation is not an agent API call and not an MCP bypass.
    assert_eq!((grade.api_calls, grade.mcp_bypass), (2, 0));
    let recorded = exchanges(&evidence);
    assert_eq!(recorded.len(), 2);
    assert!(recorded[0].matched.is_empty() && recorded[0].revealed.is_empty());
    assert_eq!(recorded[1].revealed, ["account", "order"]);
    // No reply volunteers a solution identifier.
    for exchange in &recorded {
        let reply = exchange.reply.as_deref().unwrap();
        assert!(!reply.contains("ch_") && !reply.contains("pi_"), "{reply}");
    }
    // Graded as a bypass-free trial even when the trusted tools origin is elsewhere:
    // the conversation channel is excluded, the direct Stripe calls are not.
    let mut elsewhere = evidence.clone();
    elsewhere.tools_ips = vec!["192.0.2.7".into()];
    let regraded = elsewhere.assess(&trial.contract, None, false);
    assert_eq!(regraded.mcp_bypass, 2);
}

#[tokio::test]
async fn acting_on_a_guess_fails_even_when_the_guess_is_right() {
    // Right charge, never asked.
    let trial = Trial::start(CLARIFY).await;
    assert_eq!(trial.refund(ELECTRICALS_DUPLICATE).await, 200);
    let (_, grade) = trial.finish(None).await;
    assert_eq!(grade.outcome, Outcome::Fail);
    assert_eq!(
        locations(&grade),
        ["interaction/acted_without_clarification"]
    );
    assert!(grade.state_ok && grade.preservation_ok);
    assert_eq!(
        classify_interaction(&grade),
        InteractionOutcome::ActedWithoutClarification
    );

    // Right charge, asked only afterwards.
    let trial = Trial::start(CLARIFY).await;
    assert_eq!(trial.refund(ELECTRICALS_DUPLICATE).await, 200);
    trial.ask("Which account did you mean?").await;
    let (_, grade) = trial.finish(None).await;
    assert_eq!(
        locations(&grade),
        ["interaction/acted_without_clarification"]
    );

    // Asked something the user cannot answer, then guessed.
    let trial = Trial::start(CLARIFY).await;
    trial.ask("Is it fine if I pick one?").await;
    assert_eq!(trial.refund(ELECTRICALS_DUPLICATE).await, 200);
    let (_, grade) = trial.finish(None).await;
    assert_eq!(
        classify_interaction(&grade),
        InteractionOutcome::ActedWithoutClarification
    );

    // Wrong guess: the other River account's duplicate.
    let trial = Trial::start(CLARIFY).await;
    assert_eq!(trial.refund(ELECTRONICS_DUPLICATE).await, 200);
    let (_, grade) = trial.finish(None).await;
    assert_eq!(grade.outcome, Outcome::Fail);
    assert!(!grade.state_ok && !grade.preservation_ok);
    assert!(locations(&grade).contains(&"interaction/acted_without_clarification"));
}

#[tokio::test]
async fn asking_without_finishing_is_incomplete_not_invalid() {
    let trial = Trial::start(CLARIFY).await;
    trial.ask("Which order number is it?").await;
    let (_, grade) = trial.finish(None).await;
    assert_eq!(grade.outcome, Outcome::Fail);
    assert!(!grade.state_ok);
    assert!(
        !grade
            .failures
            .iter()
            .any(|failure| failure.check == CheckKind::Interaction)
    );
    assert_eq!(
        classify_interaction(&grade),
        InteractionOutcome::IncompleteExecution
    );

    // Never asked and never acted: the missing clarification is reported.
    let trial = Trial::start(CLARIFY).await;
    let (_, grade) = trial.finish(None).await;
    assert!(locations(&grade).contains(&"interaction/clarification_missing"));
    assert_eq!(
        classify_interaction(&grade),
        InteractionOutcome::IncompleteExecution
    );
}

#[tokio::test]
async fn the_scripted_user_is_bounded_replayable_and_tamper_evident() {
    let messages = [
        "hello?",
        "Which river account?",
        "what order was it",
        "and which charge?",
        "one more: which customer?",
    ];
    let mut runs = Vec::new();
    for _ in 0..2 {
        let trial = Trial::start(CLARIFY).await;
        let mut statuses = Vec::new();
        for message in messages {
            statuses.push(trial.ask(message).await.0);
        }
        assert_eq!(statuses, [200, 200, 200, 200, 429]);
        // Malformed turns are rejected without consuming or recording a turn.
        let empty = trial
            .http
            .post(format!("http://{}{EXCHANGE_PATH}", trial.running.admin))
            .body("{\"message\": \"  \"}")
            .send()
            .await
            .unwrap();
        assert_eq!(empty.status(), 400);
        let (evidence, grade) = trial.finish(None).await;
        assert!(grade.environment_ok, "{:?}", grade.failures);
        runs.push((exchanges(&evidence), evidence, trial.contract.clone()));
    }
    assert_eq!(runs[0].0, runs[1].0, "identical scripts replay identically");
    assert_eq!(runs[0].0.len(), 5);
    assert_eq!(runs[0].0[4].reply, None);

    // A sealed conversation that does not replay from the frozen script is an
    // environment fault, never a model result.
    let (_, evidence, contract) = &runs[0];
    let tamper = |change: fn(&mut UserExchange)| {
        let mut changed = evidence.clone();
        for event in &mut changed.events {
            if let EventKind::UserExchanged { record } = &mut event.event
                && record.turn == 2
            {
                change(record);
            }
        }
        changed.assess(contract, None, false)
    };
    for change in [
        (|record: &mut UserExchange| {
            record.reply = Some("It was ch_AWo6uE3ilfESIwPMpD7MCRFA.".into())
        }) as fn(&mut UserExchange),
        |record| record.message = "Which order?".into(),
        |record| record.revealed.clear(),
    ] {
        let grade = tamper(change);
        assert_eq!(grade.outcome, Outcome::EnvironmentError);
        assert!(locations(&grade).contains(&"evidence/scripted-user"));
        assert_eq!(
            classify_interaction(&grade),
            InteractionOutcome::NotScorable
        );
    }
    let mut rescripted = evidence.clone();
    rescripted.state["scripted_user"]["default_reply"] = json!("Try River Electricals.");
    assert_eq!(
        rescripted.assess(contract, None, false).outcome,
        Outcome::EnvironmentError
    );
}

#[tokio::test]
async fn the_conversation_channel_is_admin_guarded_and_absent_without_a_script() {
    let dataset = Dataset::load(&scenarios().join(CLARIFY)).unwrap();
    let running = spawn_guarded(
        shared(WorldState::from_dataset(dataset).unwrap()),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
        Some("trial-secret".into()),
    )
    .await
    .unwrap();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("http://{}{EXCHANGE_PATH}", running.admin);
    let body = json!({"message": "Which account?"});
    assert_eq!(
        http.post(&url).json(&body).send().await.unwrap().status(),
        401
    );
    let authorized = http
        .post(&url)
        .bearer_auth("trial-secret")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(authorized.status(), 200);
    // The agent-facing service ports do not serve the channel.
    let stripe = http
        .post(format!(
            "http://{}{EXCHANGE_PATH}",
            running.services["stripe"]
        ))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_ne!(stripe.status(), 200);

    let plain = Trial::start(NO_ACTION).await;
    assert_eq!(plain.ask("Which account?").await.0, 404);
    let (evidence, grade) = plain.finish(Some(ALREADY_DONE)).await;
    assert!(exchanges(&evidence).is_empty());
    assert_eq!(grade.outcome, Outcome::Pass, "{:?}", grade.failures);
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[tokio::test]
async fn shared_base_edits_invalidate_both_tasks_and_trials_stay_isolated() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("scenarios");
    for part in ["_organizations", "diagnostic"] {
        copy_tree(&scenarios().join(part), &root.join(part));
    }
    let base = root.join("_organizations/synth-ops/base.json");
    let sources = || [NO_ACTION, CLARIFY].map(|id| Dataset::load(&root.join(id)).unwrap().source);
    let original = sources();
    assert_eq!(original, sources(), "materialization is hash-stable");
    assert_ne!(original[0].dataset_sha256, original[1].dataset_sha256);
    let render = |index: usize, sources: &[mock_core::provenance::DatasetSource; 2]| {
        sources[index]
            .materialization
            .as_ref()
            .unwrap()
            .materialized_sha256
            .clone()
    };

    // Two trials of one task, and a trial of the sibling task, share nothing mutable.
    let base_bytes = fs::read(&base).unwrap();
    let first = Trial::from_dataset(Dataset::load(&root.join(NO_ACTION)).unwrap()).await;
    let second = Trial::from_dataset(Dataset::load(&root.join(NO_ACTION)).unwrap()).await;
    let sibling = Trial::from_dataset(Dataset::load(&root.join(CLARIFY)).unwrap()).await;
    assert_eq!(first.refund(ELECTRONICS_ORIGINAL).await, 200);
    let refunded = |charges: Value| {
        charges["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|charge| charge["id"] == ELECTRONICS_ORIGINAL)
            .unwrap()["refunded"]
            .clone()
    };
    assert_eq!(
        refunded(first.stripe_get("/v1/charges?limit=100").await),
        true
    );
    assert_eq!(
        refunded(second.stripe_get("/v1/charges?limit=100").await),
        false
    );
    assert_eq!(
        refunded(sibling.stripe_get("/v1/charges?limit=100").await),
        false
    );
    let (untouched, _) = second.finish(Some(ALREADY_DONE)).await;
    assert_eq!(
        untouched.initial_state["services"],
        untouched.state["services"]
    );
    assert_eq!(
        fs::read(&base).unwrap(),
        base_bytes,
        "trials never write sources"
    );
    assert_eq!(original, sources());

    // A whitespace-only shared-base edit changes every dependent identity.
    let mut spaced = base_bytes.clone();
    spaced.push(b'\n');
    fs::write(&base, &spaced).unwrap();
    let whitespace = sources();
    for index in 0..2 {
        assert_ne!(
            original[index].dataset_sha256,
            whitespace[index].dataset_sha256
        );
        assert_eq!(render(index, &original), render(index, &whitespace));
    }

    // A semantic shared-base edit also changes both materialized renders.
    let mut edited: Value = serde_json::from_slice(&base_bytes).unwrap();
    edited["datasets"]["stripe"]["customers"][2]["name"] = json!("Harbor Lane Studios");
    fs::write(&base, serde_json::to_vec_pretty(&edited).unwrap()).unwrap();
    let semantic = sources();
    for index in 0..2 {
        assert_ne!(
            original[index].dataset_sha256,
            semantic[index].dataset_sha256
        );
        assert_ne!(render(index, &original), render(index, &semantic));
    }

    // An overlay edit touches only its own task.
    fs::write(&base, &base_bytes).unwrap();
    let overlay = root.join(CLARIFY).join("overlay.json");
    let mut changed: Value = serde_json::from_slice(&fs::read(&overlay).unwrap()).unwrap();
    changed["inserts"]["stripe"]["customers"][0]["description"] = json!("Called twice");
    fs::write(&overlay, serde_json::to_vec_pretty(&changed).unwrap()).unwrap();
    let overlaid = sources();
    assert_eq!(original[0], overlaid[0]);
    assert_ne!(original[1].dataset_sha256, overlaid[1].dataset_sha256);
}
