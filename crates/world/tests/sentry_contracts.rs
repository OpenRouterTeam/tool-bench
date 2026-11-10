mod support;

use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::{Value, json};
use support::ContractTrial;

const CREATE: &str = "ops/sentry/create-notification-project";
const WORKER: &str = "ops/sentry/resolve-worker-pool-timeouts";
const STOREFRONT: &str = "ops/sentry/triage-storefront-map-error";
const TARGET: &str = "PAYMENTS-WORKER-1K";
const EXTRA: &str = "PAYMENTS-WORKER-ZZ";

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_sentry::Sentry,
        &[("authorization", "Bearer sntrys_toolbench")],
        prepare,
    )
}

async fn update(trial: &ContractTrial, issue: &str, body: Value) {
    trial
        .call(
            "PUT",
            &format!("/api/0/organizations/lumen-labs/issues/{issue}/"),
            body,
        )
        .await;
}

async fn resolve_worker(trial: &ContractTrial) {
    update(trial, TARGET, json!({"status":"resolved"})).await;
}

async fn create(trial: &ContractTrial) -> String {
    let project = trial
        .call(
            "POST",
            "/api/0/teams/lumen-labs/payments/projects/",
            json!({"name":"notification-service","platform":"node"}),
        )
        .await;
    dsn_for(trial, project["slug"].as_str().unwrap()).await
}

async fn dsn_for(trial: &ContractTrial, slug: &str) -> String {
    let keys = trial
        .call(
            "GET",
            &format!("/api/0/projects/lumen-labs/{slug}/keys/"),
            Value::Null,
        )
        .await;
    keys.as_array()
        .unwrap()
        .iter()
        .find(|key| key["isActive"] == true)
        .unwrap()["dsn"]["public"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn pass(assessment: &Grade) {
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    assert!(
        assessment.preservation_checked && assessment.history_checked,
        "{assessment:?}"
    );
}

fn fail(assessment: &Grade) {
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(
        assessment.contract_ok && assessment.environment_ok && assessment.integrity_ok,
        "{assessment:?}"
    );
    assert!(
        assessment.preservation_checked && assessment.history_checked,
        "{assessment:?}"
    );
}

#[tokio::test]
async fn worker_plain_resolution_preserves_the_existing_assignee() {
    let trial = prepared(WORKER, |_| {});
    let before = trial.state()["issues"][TARGET]["assignedTo"].clone();
    resolve_worker(&trial).await;
    assert_eq!(trial.state()["issues"][TARGET]["assignedTo"], before);
    assert_eq!(trial.state()["issues"][TARGET]["status"], "resolved");
    pass(&trial.assess());
}

#[tokio::test]
async fn worker_resolution_does_not_authorize_a_next_release_deferral() {
    let trial = prepared(WORKER, |_| {});
    update(&trial, TARGET, json!({"status":"resolvedInNextRelease"})).await;
    assert_eq!(
        trial.state()["issues"][TARGET]["statusDetails"]["inNextRelease"],
        true
    );
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[tokio::test]
async fn worker_resolution_does_not_authorize_target_priority() {
    let trial = prepared(WORKER, |_| {});
    update(&trial, TARGET, json!({"priority":"low"})).await;
    resolve_worker(&trial).await;
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[tokio::test]
async fn worker_resolution_does_not_authorize_an_unlisted_issue() {
    let trial = prepared(WORKER, |dataset| {
        let issues = dataset
            .services
            .get_mut("sentry")
            .unwrap()
            .collection_mut("issues");
        let mut extra = issues.get(TARGET).unwrap().clone();
        extra["id"] = json!("6184499999");
        extra["shortId"] = json!(EXTRA);
        extra["title"] = json!("Unrelated queue metrics alert");
        issues.insert(EXTRA, extra);
    });
    update(&trial, EXTRA, json!({"priority":"low"})).await;
    resolve_worker(&trial).await;
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[tokio::test]
async fn created_project_requires_the_actual_active_dsn_not_an_id_shaped_string() {
    let trial = prepared(CREATE, |_| {});
    let actual = create(&trial).await;
    let scheme = actual.split_once("://").unwrap().0;
    let endpoint = actual.split_once('@').unwrap().1;
    let fabricated = format!("{scheme}://00000000000000000000000000000000@{endpoint}");
    assert_ne!(actual, fabricated);
    let assessment = trial.assess_with_answer(Some(&fabricated));
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[tokio::test]
async fn actual_created_project_and_key_identities_survive_allocator_history() {
    let trial = prepared(CREATE, |dataset| {
        dataset
            .services
            .get_mut("sentry")
            .unwrap()
            .collection_mut("projects")
            .get_mut("checkout-api")
            .unwrap()["id"] = json!("9508812003");
    });
    let actual = create(&trial).await;
    let state = trial.state();
    let project = state["projects"]
        .as_object()
        .unwrap()
        .values()
        .find(|project| project["name"] == "notification-service")
        .unwrap();
    assert_ne!(project["id"], "4508812006");
    assert!(actual.ends_with(project["id"].as_str().unwrap()));
    let assessment = trial.assess_with_answer(Some(&actual));
    pass(&assessment);
}

#[tokio::test]
async fn split_numeric_team_assignment_and_next_release_resolution_are_valid() {
    let trial = prepared(STOREFRONT, |_| {});
    update(
        &trial,
        "STOREFRONT-WEB-8D",
        json!({"assignedTo":"team:4507214"}),
    )
    .await;
    update(
        &trial,
        "STOREFRONT-WEB-8D",
        json!({"status":"resolvedInNextRelease"}),
    )
    .await;
    let state = trial.state();
    assert_eq!(
        state["issues"]["STOREFRONT-WEB-8D"]["assignedTo"]["id"],
        "4507214"
    );
    assert_eq!(
        state["issues"]["STOREFRONT-WEB-8D"]["statusDetails"]["inNextRelease"],
        true
    );
    pass(&trial.assess());
}

#[tokio::test]
async fn repeated_and_numeric_worker_updates_have_no_trial_wide_call_cap() {
    let trial = prepared(WORKER, |_| {});
    for _attempt in 0..3 {
        update(&trial, "6184417730", json!({"status":"resolved"})).await;
    }
    pass(&trial.assess());
}

#[tokio::test]
async fn documented_organization_and_team_id_aliases_reach_the_same_business_targets() {
    let worker = prepared(WORKER, |_| {});
    worker
        .call(
            "PUT",
            "/api/0/organizations/1180274/issues/6184417730/",
            json!({"status":"resolved"}),
        )
        .await;
    pass(&worker.assess());
    let project = prepared(CREATE, |_| {});
    let created = project
        .call(
            "POST",
            "/api/0/teams/1180274/4507219/projects/",
            json!({"name":"notification-service","platform":"node"}),
        )
        .await;
    let dsn = dsn_for(&project, created["slug"].as_str().unwrap()).await;
    pass(&project.assess_with_answer(Some(&dsn)));
}

#[tokio::test]
async fn restored_target_priority_cannot_erase_a_committed_violation() {
    let trial = prepared(WORKER, |_| {});
    for priority in ["low", "high"] {
        update(&trial, TARGET, json!({"priority":priority})).await;
    }
    resolve_worker(&trial).await;
    let reference = prepared(WORKER, |_| {});
    resolve_worker(&reference).await;
    assert_eq!(trial.state(), reference.state());
    let assessment = trial.assess();
    fail(&assessment);
    assert!(!assessment.history_ok);
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| !failure.event_evidence.is_empty())
    );
}

#[tokio::test]
async fn storefront_accepts_combined_and_reverse_ordered_authorized_updates() {
    for variant in ["combined", "status_first", "nested_details"] {
        let trial = prepared(STOREFRONT, |_| {});
        match variant {
            "combined" => {
                update(
                    &trial,
                    "6184421045",
                    json!({
                        "status":"resolvedInNextRelease","assignedTo":"team:4507214"
                    }),
                )
                .await
            }
            "status_first" => {
                update(
                    &trial,
                    "6184421045",
                    json!({"status":"resolvedInNextRelease"}),
                )
                .await;
                update(&trial, "6184421045", json!({"assignedTo":"team:4507214"})).await;
            }
            _ => {
                update(
                    &trial,
                    "6184421045",
                    json!({
                        "status":"resolved","statusDetails":{"inNextRelease":true},
                        "assignedTo":"team:4507214"
                    }),
                )
                .await
            }
        }
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn storefront_rejects_wrong_team_and_immediate_resolution() {
    for body in [
        json!({"status":"resolvedInNextRelease","assignedTo":"team:4507223"}),
        json!({"status":"resolved","assignedTo":"team:4507214"}),
    ] {
        let trial = prepared(STOREFRONT, |_| {});
        update(&trial, "6184421045", body).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn new_project_accepts_an_explicit_alternate_slug_and_unspecified_platform() {
    let trial = prepared(CREATE, |_| {});
    let project = trial
        .call(
            "POST",
            "/api/0/teams/lumen-labs/payments/projects/",
            json!({"name":"notification-service","slug":"notifications-node"}),
        )
        .await;
    assert_eq!(project["slug"], "notifications-node");
    let dsn = dsn_for(&trial, "notifications-node").await;
    pass(&trial.assess_with_answer(Some(&dsn)));
}

#[tokio::test]
async fn new_project_slug_changes_preserve_key_and_project_identity() {
    let trial = prepared(CREATE, |_| {});
    let original_dsn = create(&trial).await;
    trial
        .call(
            "PUT",
            "/api/0/projects/lumen-labs/notification-service/",
            json!({"slug":"notifications-node"}),
        )
        .await;
    let current_dsn = dsn_for(&trial, "notifications-node").await;
    assert_eq!(original_dsn, current_dsn);
    let (status, _) = trial
        .request(
            "GET",
            "/api/0/projects/lumen-labs/notification-service/",
            Value::Null,
        )
        .await;
    assert_eq!(status.as_u16(), 404);
    pass(&trial.assess_with_answer(Some(&current_dsn)));
}

#[tokio::test]
async fn invalid_project_slugs_reject_without_mutation_and_do_not_block_recovery() {
    let trial = prepared(CREATE, |_| {});
    let before = trial.state();
    for slug in ["12345", "path/escape", "invalid@host", "BadCase"] {
        let (status, _) = trial
            .request(
                "POST",
                "/api/0/teams/lumen-labs/payments/projects/",
                json!({"name":"notification-service","slug":slug}),
            )
            .await;
        assert_eq!(status.as_u16(), 400);
        assert_eq!(trial.state(), before);
    }
    let dsn = create(&trial).await;
    let created = trial.state();
    for slug in ["path/escape", "", "12345"] {
        let (status, _) = trial
            .request(
                "PUT",
                "/api/0/projects/lumen-labs/notification-service/",
                json!({"slug":slug}),
            )
            .await;
        assert_eq!(status.as_u16(), 400);
        assert_eq!(trial.state(), created);
    }
    pass(&trial.assess_with_answer(Some(&dsn)));
}

#[tokio::test]
async fn source_ingestion_can_differ_from_the_api_origin_and_have_a_prefix() {
    let trial = prepared(CREATE, |dataset| {
        dataset
            .services
            .get_mut("sentry")
            .unwrap()
            .collection_mut("instance")
            .insert(
                "instance",
                json!({
                    "public_origin":"https://sentry.eu.lumenlabs.dev",
                    "ingestion_origin":"https://ingest.example.net/sentry-prefix"
                }),
            );
    });
    let dsn = create(&trial).await;
    assert!(dsn.starts_with("https://"));
    assert!(dsn.contains("@ingest.example.net/sentry-prefix/"));
    pass(&trial.assess_with_answer(Some(&dsn)));
}

#[test]
#[should_panic(expected = "unsupported Sentry instance fixture shape")]
fn unknown_instance_fields_cannot_silently_disappear() {
    prepared(CREATE, |dataset| {
        dataset
            .services
            .get_mut("sentry")
            .unwrap()
            .collection_mut("instance")
            .insert(
                "instance",
                json!({
                    "public_origin":"https://sentry.lumenlabs.dev",
                    "ingestion_origin":"https://sentry.lumenlabs.dev",
                    "ignored_setting":true
                }),
            );
    });
}

#[tokio::test]
async fn additional_key_for_the_new_project_is_not_tied_to_the_default_label() {
    let trial = prepared(CREATE, |_| {});
    create(&trial).await;
    let key = trial
        .call(
            "POST",
            "/api/0/projects/lumen-labs/notification-service/keys/",
            json!({"name":"Production SDK"}),
        )
        .await;
    let dsn = key["dsn"]["public"].as_str().unwrap();
    pass(&trial.assess_with_answer(Some(dsn)));
}

#[tokio::test]
async fn creating_a_key_for_an_existing_unrelated_project_is_collateral() {
    let trial = prepared(CREATE, |_| {});
    let dsn = create(&trial).await;
    trial
        .call(
            "POST",
            "/api/0/projects/lumen-labs/checkout-api/keys/",
            json!({"name":"Unrequested"}),
        )
        .await;
    let assessment = trial.assess_with_answer(Some(&dsn));
    fail(&assessment);
    assert!(!assessment.preservation_ok && !assessment.history_ok);
}

#[tokio::test]
async fn temporary_extra_project_team_access_is_not_erased_by_removal() {
    let trial = prepared(CREATE, |_| {});
    let dsn = create(&trial).await;
    for method in ["POST", "DELETE"] {
        trial
            .call(
                method,
                "/api/0/projects/lumen-labs/notification-service/teams/payments-infra/",
                json!({}),
            )
            .await;
    }
    let reference = prepared(CREATE, |_| {});
    create(&reference).await;
    assert_eq!(trial.state(), reference.state());
    let assessment = trial.assess_with_answer(Some(&dsn));
    fail(&assessment);
    assert!(!assessment.history_ok);
}

#[tokio::test]
async fn deactivating_the_only_delivered_key_cannot_produce_a_usable_dsn() {
    let trial = prepared(CREATE, |_| {});
    let dsn = create(&trial).await;
    let public = dsn.split_once("://").unwrap().1.split_once('@').unwrap().0;
    trial
        .call(
            "PUT",
            &format!("/api/0/projects/lumen-labs/notification-service/keys/{public}/"),
            json!({"isActive":false}),
        )
        .await;
    let assessment = trial.assess_with_answer(Some(&dsn));
    fail(&assessment);
    assert!(!assessment.answer_ok);
}

#[tokio::test]
async fn a_platform_sentry_does_not_know_is_refused_and_the_retry_still_passes() {
    let trial = prepared(CREATE, |_| {});
    let before = trial.state();
    for platform in ["nodejs", "javascript-node", "node.js", "react"] {
        let (status, body) = trial
            .request(
                "POST",
                "/api/0/teams/lumen-labs/payments/projects/",
                json!({"name":"notification-service","platform":platform}),
            )
            .await;
        assert_eq!(status.as_u16(), 400, "{platform}: {body}");
        assert_eq!(body, json!({"platform":["Invalid platform"]}));
        assert_eq!(trial.state(), before);
    }
    let dsn = create(&trial).await;
    let (status, body) = trial
        .request(
            "PUT",
            "/api/0/projects/lumen-labs/notification-service/",
            json!({"platform":"nodejs"}),
        )
        .await;
    assert_eq!(status.as_u16(), 400, "{body}");
    pass(&trial.assess_with_answer(Some(&dsn)));
}

#[tokio::test]
async fn a_known_platform_other_than_node_is_stored_and_is_not_the_requested_service() {
    for platform in ["javascript", "node-express", "python"] {
        let trial = prepared(CREATE, |_| {});
        let project = trial
            .call(
                "POST",
                "/api/0/teams/lumen-labs/payments/projects/",
                json!({"name":"notification-service","platform":platform}),
            )
            .await;
        assert_eq!(project["platform"], platform);
        let dsn = dsn_for(&trial, project["slug"].as_str().unwrap()).await;
        fail(&trial.assess_with_answer(Some(&dsn)));
    }
}
