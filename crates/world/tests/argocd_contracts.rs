mod support;

use mock_core::{dataset::Dataset, grading::Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const RESTART: &str = "code/argocd/restart-indexer-worker";
const ROLLBACK: &str = "code/argocd/roll-back-billing-worker";
const PAYMENTS: &str = "code/argocd/sync-prod-payments-api";
const WORKER: &str = "d39d8921-d825-549b-a290-0dcda1cafa6c";
const BILLING_DEPLOYMENT: &str = "4277b63d-291e-513f-9f6c-5d185a2f7bb5";

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_argocd::Argocd,
        &[("authorization", "Bearer argocd-toolbench")],
        prepare,
    )
}

async fn action(
    trial: &ContractTrial,
    application: &str,
    app_namespace: &str,
    namespace: &str,
    deployment: &str,
    action: &str,
) {
    trial.call("POST", &format!(
        "/api/v1/applications/{application}/resource/actions?appNamespace={app_namespace}&namespace={namespace}&resourceName={deployment}&group=apps&kind=Deployment&version=v1"
    ), json!(action)).await;
}

async fn restart(trial: &ContractTrial) {
    action(
        trial,
        "search-indexer",
        "argocd",
        "search",
        "search-indexer-worker",
        "restart",
    )
    .await;
    trial
        .call("GET", "/api/v1/applications/search-indexer", Value::Null)
        .await;
}

async fn observe_completed(trial: &ContractTrial, name: &str, namespace: &str) {
    for _attempt in 0..6 {
        let application = trial
            .call(
                "GET",
                &format!("/api/v1/applications/{name}?appNamespace={namespace}"),
                Value::Null,
            )
            .await;
        if !application["operation"].is_object() {
            return;
        }
    }
    panic!("controller did not complete the bounded operation");
}

async fn pin(trial: &ContractTrial, mutate: impl FnOnce(&mut Value)) {
    let mut application = trial.state()["applications"]["argocd/billing-worker"].clone();
    application["spec"]["source"]["targetRevision"] = json!("v1.42.0");
    mutate(&mut application);
    trial
        .call("PUT", "/api/v1/applications/billing-worker", application)
        .await;
    observe_completed(trial, "billing-worker", "argocd").await;
}

async fn sync_payments(trial: &ContractTrial) {
    trial
        .call(
            "POST",
            "/api/v1/applications/payments-api/sync",
            json!({"appNamespace":"payments","prune":true}),
        )
        .await;
    observe_completed(trial, "payments-api", "payments").await;
}

fn assert_outcome(trial: &ContractTrial, expected: Outcome) {
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, expected, "{assessment:?}");
}

#[tokio::test]
async fn the_three_http_workflows_reach_the_reviewed_positive_outcomes() {
    let trial = prepared(RESTART, |_| {});
    restart(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
    let trial = prepared(ROLLBACK, |_| {});
    pin(&trial, |_| {}).await;
    assert_outcome(&trial, Outcome::Pass);
    let trial = prepared(PAYMENTS, |_| {});
    sync_payments(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn spec_only_reference_preserves_opaque_configuration_and_zero_history_retention() {
    let trial = prepared(ROLLBACK, |dataset| {
        let application = dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("applications")
            .get_mut("argocd/billing-worker")
            .unwrap();
        application["spec"]["info"] = json!([{"name":"operator","value":"preserve ${literal}"}]);
        application["spec"]["revisionHistoryLimit"] = json!(0);
    });
    let initial = trial.state()["applications"]["argocd/billing-worker"].clone();
    let response = trial
        .call("GET", "/api/v1/applications/billing-worker", Value::Null)
        .await;
    let mut bindings = scenario::solution::Bindings::new();
    trial
        .scenario
        .solution
        .calls
        .iter()
        .find(|call| !call.capture.is_empty())
        .unwrap()
        .capture_result(
            &json!({"content":[{"type":"text","text":response.to_string()}]}),
            &mut bindings,
        )
        .unwrap();
    let arguments = trial
        .scenario
        .solution
        .calls
        .iter()
        .find(|call| call.tool == "update_application_spec")
        .unwrap()
        .resolve_arguments(&bindings)
        .unwrap();
    trial
        .call(
            "PUT",
            "/api/v1/applications/billing-worker/spec?appNamespace=argocd",
            arguments["spec"].clone(),
        )
        .await;
    observe_completed(&trial, "billing-worker", "argocd").await;
    let after = trial.state()["applications"]["argocd/billing-worker"].clone();
    assert_eq!(after["spec"]["info"], initial["spec"]["info"]);
    assert_eq!(after["spec"]["revisionHistoryLimit"], 0);
    assert_eq!(after["status"]["history"], json!([]));
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn destination_aliases_and_explicit_null_enabled_remain_equivalent_configuration() {
    let trial = prepared(ROLLBACK, |_| {});
    pin(&trial, |application| {
        application["spec"]["destination"] = json!({"name":"prod-us-east-1","namespace":"billing"});
        application["spec"]["syncPolicy"]["automated"]["enabled"] = Value::Null;
    })
    .await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_persistent_commit_sha_is_not_the_requested_git_tag_even_when_the_build_matches() {
    let trial = prepared(ROLLBACK, |_| {});
    let mut application = trial.state()["applications"]["argocd/billing-worker"].clone();
    application["spec"]["source"]["targetRevision"] =
        json!("b0826e986c9c73e0088c6ecb2a14458d247d2205");
    trial
        .call("PUT", "/api/v1/applications/billing-worker", application)
        .await;
    observe_completed(&trial, "billing-worker", "argocd").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn same_target_manual_sync_after_the_pin_is_a_valid_alternative() {
    let trial = prepared(ROLLBACK, |_| {});
    pin(&trial, |_| {}).await;
    trial
        .call(
            "POST",
            "/api/v1/applications/billing-worker/sync",
            json!({"appNamespace":"argocd","revision":"v1.42.0"}),
        )
        .await;
    observe_completed(&trial, "billing-worker", "argocd").await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn billing_dry_run_before_pinning_is_not_itself_a_deployment_but_can_precede_one() {
    let trial = prepared(ROLLBACK, |_| {});
    trial
        .call(
            "POST",
            "/api/v1/applications/billing-worker/sync",
            json!({"revision":"v1.42.0","dryRun":true}),
        )
        .await;
    observe_completed(&trial, "billing-worker", "argocd").await;
    assert_outcome(&trial, Outcome::Fail);
    pin(&trial, |_| {}).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn payments_accepts_current_main_sha_and_recoverable_preliminary_nonprune_sync() {
    let trial = prepared(PAYMENTS, |_| {});
    let arguments =
        json!({"appNamespace":"payments","revision":"fe01f33d2a5392fa17c1c35f25d294d24075c28b"});
    trial
        .call(
            "POST",
            "/api/v1/applications/payments-api/sync",
            arguments.clone(),
        )
        .await;
    observe_completed(&trial, "payments-api", "payments").await;
    assert_outcome(&trial, Outcome::Fail);
    let mut with_pruning = arguments;
    with_pruning["prune"] = json!(true);
    trial
        .call(
            "POST",
            "/api/v1/applications/payments-api/sync",
            with_pruning,
        )
        .await;
    observe_completed(&trial, "payments-api", "payments").await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_payments_dry_run_with_pruned_result_text_does_not_establish_actual_removal() {
    let trial = prepared(PAYMENTS, |_| {});
    trial
        .call(
            "POST",
            "/api/v1/applications/payments-api/sync",
            json!({"appNamespace":"payments","prune":true,"dryRun":true}),
        )
        .await;
    observe_completed(&trial, "payments-api", "payments").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn duplicate_target_restart_in_the_same_business_second_does_not_force_extra_rollouts() {
    let trial = prepared(RESTART, |_| {});
    restart(&trial).await;
    let first = trial.state()["resources"].clone();
    restart(&trial).await;
    assert_eq!(first, trial.state()["resources"]);
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn unsupported_source_fields_are_authenticated_environment_faults_without_public_simulator_markers()
 {
    let trial = prepared(ROLLBACK, |_| {});
    let initial = trial.state();
    let mut spec = initial["applications"]["argocd/billing-worker"]["spec"].clone();
    spec["source"]["helm"] = json!({"parameters":[{"name":"image.tag","value":"different"}]});
    let (status, response) = trial
        .request("PUT", "/api/v1/applications/billing-worker/spec", spec)
        .await;
    assert_eq!(status.as_u16(), 501);
    assert_eq!(initial, trial.state());
    let public = response.to_string().to_lowercase();
    for marker in ["benchmark", "mock", "modeled", "simulat"] {
        assert!(!public.contains(marker), "{public}");
    }
    let assessment = trial.assess();
    assert_eq!(
        assessment.outcome,
        Outcome::EnvironmentError,
        "{assessment:?}"
    );
    assert!(!assessment.environment_ok);
}

#[tokio::test]
async fn a_successful_rollback_must_not_leave_the_crashing_release_in_the_live_template() {
    let trial = prepared(ROLLBACK, |_| {});
    let before =
        trial.state()["resources"][BILLING_DEPLOYMENT]["manifest"]["spec"]["template"].clone();
    pin(&trial, |_| {}).await;
    assert_outcome(&trial, Outcome::Pass);
    let after =
        trial.state()["resources"][BILLING_DEPLOYMENT]["manifest"]["spec"]["template"].clone();
    assert_ne!(
        before, after,
        "application success metadata did not replace the live crashing workload"
    );
}

#[tokio::test]
async fn an_automated_policy_object_with_enabled_false_is_not_enabled_automation() {
    let trial = prepared(ROLLBACK, |_| {});
    pin(&trial, |application| {
        application["spec"]["syncPolicy"]["automated"]["enabled"] = json!(false);
    })
    .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn pinning_the_release_does_not_authorize_unrequested_application_metadata() {
    let trial = prepared(ROLLBACK, |_| {});
    pin(&trial, |application| {
        application["metadata"]["annotations"] = json!({"unrequested-setting":"changed"});
    })
    .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn the_authored_rollback_update_must_preserve_existing_labels_and_finalizers() {
    let trial = prepared(ROLLBACK, |_| {});
    let metadata = trial.state()["applications"]["argocd/billing-worker"]["metadata"].clone();
    let read = trial
        .call("GET", "/api/v1/applications/billing-worker", Value::Null)
        .await;
    let mut bindings = scenario::solution::Bindings::new();
    trial
        .scenario
        .solution
        .calls
        .iter()
        .find(|call| !call.capture.is_empty())
        .unwrap()
        .capture_result(
            &json!({"content":[{"type":"text","text":read.to_string()}]}),
            &mut bindings,
        )
        .unwrap();
    let update = trial
        .scenario
        .solution
        .calls
        .iter()
        .find(|call| call.tool == "update_application_spec")
        .unwrap();
    let arguments = update.resolve_arguments(&bindings).unwrap();
    trial
        .call(
            "PUT",
            "/api/v1/applications/billing-worker/spec?appNamespace=argocd",
            arguments["spec"].clone(),
        )
        .await;
    let after = trial.state()["applications"]["argocd/billing-worker"]["metadata"].clone();
    assert_eq!(after["labels"], metadata["labels"]);
    assert_eq!(after["finalizers"], metadata["finalizers"]);
}

#[tokio::test]
async fn spec_only_updates_preserve_application_metadata_and_return_the_spec_not_an_application() {
    let trial = prepared(ROLLBACK, |_| {});
    let initial = trial.state()["applications"]["argocd/billing-worker"].clone();
    let mut spec = initial["spec"].clone();
    spec["source"]["targetRevision"] = json!("v1.42.0");
    let response = trial
        .call(
            "PUT",
            "/api/v1/applications/billing-worker/spec?appNamespace=argocd",
            spec.clone(),
        )
        .await;
    assert_eq!(response, spec);
    let after = trial.state()["applications"]["argocd/billing-worker"].clone();
    for field in [
        "name",
        "namespace",
        "uid",
        "creationTimestamp",
        "labels",
        "annotations",
        "finalizers",
    ] {
        assert_eq!(
            initial["metadata"].get(field),
            after["metadata"].get(field),
            "{field}"
        );
    }
}

#[tokio::test]
async fn invalid_spec_only_updates_have_no_partial_application_or_resource_effects() {
    let trial = prepared(ROLLBACK, |_| {});
    let before = trial.state();
    let (status, _) = trial
        .request(
            "PUT",
            "/api/v1/applications/billing-worker/spec?appNamespace=argocd",
            json!({"metadata":{"name":"billing-worker"},"spec":before["applications"]["argocd/billing-worker"]["spec"]}),
        )
        .await;
    assert_eq!(status.as_u16(), 400);
    assert_eq!(before, trial.state());
}

#[tokio::test]
async fn restarting_the_worker_does_not_authorize_an_unlisted_sibling_workload() {
    let trial = prepared(RESTART, |dataset| {
        let resources = dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("resources");
        let mut extra = resources.get(WORKER).unwrap().clone();
        extra["uid"] = json!("unlisted-sibling-deployment");
        extra["name"] = json!("search-indexer-extra-worker");
        extra["manifest"]["metadata"]["name"] = json!("search-indexer-extra-worker");
        extra["manifest"]["metadata"]["uid"] = json!("unlisted-sibling-deployment");
        resources.insert("unlisted-sibling-deployment", extra);
    });
    restart(&trial).await;
    action(
        &trial,
        "search-indexer",
        "argocd",
        "search",
        "search-indexer-extra-worker",
        "restart",
    )
    .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_stale_restart_stamp_and_a_prefix_matching_sibling_are_not_a_worker_restart() {
    let trial = prepared(RESTART, |dataset| {
        let resources = dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("resources");
        let worker = resources.get_mut(WORKER).unwrap();
        worker["manifest"]["spec"]["template"]["metadata"]["annotations"] =
            json!({"kubectl.kubernetes.io/restartedAt":"2026-10-28T09:00:00Z"});
        let mut extra = worker.clone();
        extra["uid"] = json!("prefix-matching-sibling");
        extra["name"] = json!("search-indexer-worker-shadow");
        extra["manifest"]["metadata"]["name"] = json!("search-indexer-worker-shadow");
        extra["manifest"]["metadata"]["uid"] = json!("prefix-matching-sibling");
        resources.insert("prefix-matching-sibling", extra);
    });
    action(
        &trial,
        "search-indexer",
        "argocd",
        "search",
        "search-indexer-worker-shadow",
        "restart",
    )
    .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn production_sync_does_not_authorize_restarting_the_frozen_staging_workload() {
    let trial = prepared(PAYMENTS, |_| {});
    action(
        &trial,
        "payments-api",
        "payments-staging",
        "payments",
        "payments-api",
        "restart",
    )
    .await;
    sync_payments(&trial).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn production_pruning_is_scoped_to_actual_app_ownership_not_one_sentinel_uid() {
    let trial = prepared(PAYMENTS, |dataset| {
        let resources = dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("resources");
        let mut extra = resources
            .get("41db962d-0f81-5de8-8dc3-ced47c1d11fc")
            .unwrap()
            .clone();
        extra["uid"] = json!("another-obsolete-production-configmap");
        extra["name"] = json!("payments-api-another-obsolete-config");
        extra["manifest"]["metadata"]["uid"] = json!("another-obsolete-production-configmap");
        extra["manifest"]["metadata"]["name"] = json!("payments-api-another-obsolete-config");
        resources.insert("another-obsolete-production-configmap", extra);
    });
    sync_payments(&trial).await;
    assert!(
        trial.state()["resources"]
            .get("another-obsolete-production-configmap")
            .is_none()
    );
    assert_outcome(&trial, Outcome::Pass);
}
