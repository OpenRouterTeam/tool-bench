mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use catalog::Catalog;
use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use scenario::{
    SolutionCall,
    oracle_tools::{ArgumentValidation, OracleTools},
};
use serde_json::{Value, json};
use support::ContractTrial;

const SCALE: &str = "ops/multi/checkout-saturation-scale-and-resolve";
const MEMORY: &str = "ops/multi/oom-limit-and-sentry-resolve";
const ROLLBACK: &str = "ops/multi/datadog-latency-argocd-rollback";
const INCIDENT: &str = "Q7CHKAP15PCT1D";
const OTHER_INCIDENT: &str = "Q9CHKWEB5XX3VK";
const ISSUE: &str = "REPORT-BUILDER-4T";
const SCALE_PATH: &str = "/apis/apps/v1/namespaces/shop-prod/deployments/checkout";
const MEMORY_PATH: &str = "/apis/apps/v1/namespaces/analytics/deployments/report-builder";
const NOTE: &str =
    "Per RB-CHK-07: scaled the checkout Deployment in shop-prod from 3 to 9 replicas.";
static EXPORT_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

fn prepared(id: &str) -> ContractTrial {
    prepared_with(id, |_| {})
}

fn prepared_with(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    match id {
        SCALE => ContractTrial::load_many_with_dataset(
            id,
            &[
                (&mock_kubernetes::Kubernetes, &[]),
                (
                    &mock_pagerduty::Pagerduty,
                    &[
                        ("authorization", "Token token=local-fixture-credential"),
                        ("from", "lena.weber@tidewater.io"),
                    ],
                ),
            ],
            prepare,
        ),
        MEMORY => ContractTrial::load_many_with_dataset(
            id,
            &[
                (&mock_kubernetes::Kubernetes, &[]),
                (
                    &mock_sentry::Sentry,
                    &[("authorization", "Bearer sntrys_toolbench")],
                ),
            ],
            prepare,
        ),
        _ => ContractTrial::load_many_with_dataset(
            id,
            &[
                (
                    &mock_datadog::Datadog,
                    &[
                        ("dd-api-key", "local-api-key"),
                        ("dd-application-key", "local-application-key"),
                    ],
                ),
                (
                    &mock_argocd::Argocd,
                    &[("authorization", "Bearer argocd-toolbench")],
                ),
            ],
            prepare,
        ),
    }
}

fn native_arguments(endpoint: &str, tool: &str, arguments: Value) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(&root, &catalog).unwrap();
    let call: SolutionCall = serde_json::from_value(json!({
        "endpoint":endpoint,"tool":tool,"arguments":arguments
    }))
    .unwrap();
    assert_eq!(
        tools.validate(&call).unwrap().arguments,
        ArgumentValidation::StaticValidated
    );
}

#[track_caller]
fn observed(trial: &ContractTrial) -> Grade {
    let assessment = trial.assess();
    if let Some(root) = std::env::var_os("OPS_MULTI_PROBE_OUT") {
        let directory = std::path::PathBuf::from(root)
            .join(EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string());
        std::fs::create_dir(&directory).unwrap();
        let world = trial.world.lock().unwrap();
        let caller = std::panic::Location::caller();
        for (name, value) in [
            (
                "assessment.json",
                serde_json::to_value(&assessment).unwrap(),
            ),
            ("initial-state.json", world.initial_snapshot()),
            ("state.json", world.snapshot()),
            ("events.json", json!({"events":world.events().entries()})),
            ("journal.json", json!({"entries":world.journal.entries()})),
            (
                "case.json",
                json!({
                    "source_file":caller.file(),"source_line":caller.line(),
                    "scope":"in-process operational cross-system probes, not MCP execution, runtime admission or model performance"
                }),
            ),
        ] {
            std::fs::write(
                directory.join(name),
                serde_json::to_vec_pretty(&value).unwrap(),
            )
            .unwrap();
        }
    }
    assessment
}

#[track_caller]
fn assert_outcome(trial: &ContractTrial, expected: Outcome) {
    let assessment = observed(trial);
    assert_eq!(assessment.outcome, expected, "{assessment:?}");
}

async fn scale(trial: &ContractTrial) {
    scale_to(trial, 9).await;
}

async fn scale_to(trial: &ContractTrial, replicas: u32) {
    native_arguments(
        "kubernetes",
        "resources_scale",
        json!({"apiVersion":"apps/v1","kind":"Deployment","namespace":"shop-prod","name":"checkout","scale":replicas}),
    );
    trial
        .call_from(
            "kubernetes",
            "PATCH",
            &format!("{SCALE_PATH}/scale"),
            json!({"spec":{"replicas":replicas}}),
        )
        .await;
}

async fn note(trial: &ContractTrial, incident: &str, content: &str) {
    native_arguments(
        "pagerduty",
        "add_note_to_incident",
        json!({"incident_id":incident,"note":content}),
    );
    trial
        .call_from(
            "pagerduty",
            "POST",
            &format!("/incidents/{incident}/notes"),
            json!({"note":{"content":content}}),
        )
        .await;
}

async fn resolve_incident(trial: &ContractTrial, urgency: Option<&str>) {
    let mut attributes = json!({"id":INCIDENT,"type":"incident_reference","status":"resolved"});
    let mut request = json!({"incident_ids":[INCIDENT],"status":"resolved"});
    if let Some(urgency) = urgency {
        attributes["urgency"] = json!(urgency);
        request["urgency"] = json!(urgency);
    }
    native_arguments(
        "pagerduty",
        "manage_incidents",
        json!({"manage_request":request}),
    );
    trial
        .call_from(
            "pagerduty",
            "PUT",
            "/incidents",
            json!({"incidents":[attributes]}),
        )
        .await;
}

async fn memory(trial: &ContractTrial, quantity: &str, extra_annotation: bool) {
    let mut resource = trial.world.lock().unwrap().snapshot()["services"]["kubernetes"]
        ["deployments.apps"]["analytics/report-builder"]
        .clone();
    let container = resource["spec"]["template"]["spec"]["containers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|container| container["name"] == "report-builder")
        .unwrap();
    container["resources"]["limits"]["memory"] = json!(quantity);
    if extra_annotation {
        resource["metadata"]["annotations"]["unrequested"] = json!("changed");
    }
    native_arguments(
        "kubernetes",
        "resources_create_or_update",
        json!({"resource":resource.to_string()}),
    );
    trial
        .call_from("kubernetes", "PUT", MEMORY_PATH, resource)
        .await;
}

async fn resolve_issue(trial: &ContractTrial, status: &str) {
    native_arguments(
        "sentry",
        "update_issue",
        json!({"organizationSlug":"lumen-labs","issueId":ISSUE,"status":status}),
    );
    trial
        .call_from(
            "sentry",
            "PUT",
            &format!("/api/0/organizations/lumen-labs/issues/{ISSUE}/"),
            json!({"status":status}),
        )
        .await;
}

#[tokio::test]
async fn operational_reference_outcomes_are_reachable() {
    let trial = prepared(SCALE);
    scale(&trial).await;
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Pass);
    let trial = prepared(MEMORY);
    memory(&trial, "1Gi", false).await;
    resolve_issue(&trial, "resolved").await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn scale_rejects_incident_closure_before_remediation() {
    let trial = prepared(SCALE);
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, None).await;
    scale(&trial).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn scale_rejects_a_bare_digit_instead_of_truthful_communication() {
    let trial = prepared(SCALE);
    scale(&trial).await;
    note(&trial, INCIDENT, "9").await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn scale_rejects_a_contradictory_staging_note() {
    let trial = prepared(SCALE);
    scale(&trial).await;
    note(
        &trial,
        INCIDENT,
        "Staging was scaled to 9; production was left unchanged.",
    )
    .await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn scale_rejects_unrequested_target_urgency_changes() {
    let trial = prepared(SCALE);
    scale(&trial).await;
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, Some("low")).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn scale_rejects_an_extra_note_on_an_unrelated_incident() {
    let trial = prepared(SCALE);
    scale(&trial).await;
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, None).await;
    note(&trial, OTHER_INCIDENT, "Unrequested update.").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn memory_accepts_an_equivalent_quantity_spelling() {
    let trial = prepared(MEMORY);
    memory(&trial, "1024Mi", false).await;
    resolve_issue(&trial, "resolved").await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn memory_rejects_resolution_before_the_limit_is_raised() {
    let trial = prepared(MEMORY);
    resolve_issue(&trial, "resolved").await;
    memory(&trial, "1Gi", false).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn memory_rejects_unrequested_workload_annotations() {
    let trial = prepared(MEMORY);
    memory(&trial, "1Gi", true).await;
    resolve_issue(&trial, "resolved").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn memory_rejects_deferring_resolution_to_the_next_release() {
    let trial = prepared(MEMORY);
    memory(&trial, "1Gi", false).await;
    resolve_issue(&trial, "resolvedInNextRelease").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn unfinished_operational_work_is_not_a_success() {
    for id in [SCALE, MEMORY] {
        let trial = prepared(id);
        assert_outcome(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn scale_rejects_a_premature_note_even_if_resolution_waits_for_remediation() {
    let trial = prepared(SCALE);
    note(&trial, INCIDENT, NOTE).await;
    scale(&trial).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn scale_requires_a_note_not_only_a_resolved_incident() {
    let trial = prepared(SCALE);
    scale(&trial).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn scale_note_in_unlisted_wording_passes_with_the_new_replica_count_and_fails_without_it() {
    for (content, expected) in [
        (
            "Scaled the checkout deployment up to 9 replicas as the runbook asks; the new pods are Ready.",
            Outcome::Pass,
        ),
        (
            "The checkout remediation has been completed.",
            Outcome::Fail,
        ),
        (
            "Scaled the checkout deployment up to 6 replicas as the runbook asks.",
            Outcome::Fail,
        ),
    ] {
        let trial = prepared(SCALE);
        scale(&trial).await;
        note(&trial, INCIDENT, content).await;
        resolve_incident(&trial, None).await;
        assert_outcome(&trial, expected);
    }
}

#[tokio::test]
async fn scale_accepts_additional_observations_and_multiple_truthful_notes() {
    let trial = prepared(SCALE);
    scale(&trial).await;
    trial
        .call_from("kubernetes", "GET", SCALE_PATH, Value::Null)
        .await;
    trial
        .call_from(
            "kubernetes",
            "GET",
            "/api/v1/namespaces/shop-prod/pods",
            Value::Null,
        )
        .await;
    note(&trial, INCIDENT, NOTE).await;
    note(
        &trial,
        INCIDENT,
        "Scaled production shop-prod/checkout from 3 to 9 replicas per RB-CHK-07; all 9 pods are Ready.",
    )
    .await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn scale_accepts_full_deployment_updates_without_requiring_the_scale_endpoint() {
    let trial = prepared(SCALE);
    let mut resource = trial.world.lock().unwrap().snapshot()["services"]["kubernetes"]
        ["deployments.apps"]["shop-prod/checkout"]
        .clone();
    resource["spec"]["replicas"] = json!(9);
    native_arguments(
        "kubernetes",
        "resources_create_or_update",
        json!({"resource":resource.to_string()}),
    );
    trial
        .call_from("kubernetes", "PUT", SCALE_PATH, resource)
        .await;
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn scale_accepts_safe_staged_increases_to_the_requested_capacity() {
    let trial = prepared(SCALE);
    scale_to(&trial, 6).await;
    scale(&trial).await;
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn scale_rejects_claiming_completion_at_an_intermediate_capacity() {
    let trial = prepared(SCALE);
    scale_to(&trial, 6).await;
    note(&trial, INCIDENT, NOTE).await;
    scale(&trial).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn scale_rejects_overscaling_even_when_the_final_capacity_is_restored() {
    let trial = prepared(SCALE);
    scale_to(&trial, 10).await;
    scale(&trial).await;
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn scale_allows_acknowledging_the_target_before_remediation() {
    let trial = prepared(SCALE);
    native_arguments(
        "pagerduty",
        "manage_incidents",
        json!({"manage_request":{"incident_ids":[INCIDENT],"status":"acknowledged"}}),
    );
    trial
        .call_from(
            "pagerduty",
            "PUT",
            "/incidents",
            json!({"incidents":[{"id":INCIDENT,"type":"incident_reference","status":"acknowledged"}]}),
        )
        .await;
    scale(&trial).await;
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn scale_rejects_restored_target_image_changes() {
    let trial = prepared(SCALE);
    let mut original = trial.world.lock().unwrap().snapshot()["services"]["kubernetes"]
        ["deployments.apps"]["shop-prod/checkout"]
        .clone();
    original["metadata"]
        .as_object_mut()
        .unwrap()
        .remove("resourceVersion");
    let mut resource = original.clone();
    resource["spec"]["template"]["spec"]["containers"][0]["image"] = json!("unrequested:v1");
    trial
        .call_from("kubernetes", "PUT", SCALE_PATH, resource)
        .await;
    trial
        .call_from("kubernetes", "PUT", SCALE_PATH, original)
        .await;
    scale(&trial).await;
    note(&trial, INCIDENT, NOTE).await;
    resolve_incident(&trial, None).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn memory_accepts_exact_decimal_and_numeric_quantity_equivalents() {
    for quantity in ["1073741824", "1.073741824e9", "1073741824000m"] {
        let trial = prepared(MEMORY);
        memory(&trial, quantity, false).await;
        resolve_issue(&trial, "resolved").await;
        assert_outcome(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn memory_rejects_a_larger_unrequested_allocation() {
    let trial = prepared(MEMORY);
    memory(&trial, "2Gi", false).await;
    resolve_issue(&trial, "resolved").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn memory_remediation_alone_does_not_complete_the_sentry_workflow() {
    let trial = prepared(MEMORY);
    memory(&trial, "1Gi", false).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn memory_rejects_resolution_of_either_lookalike_issue() {
    for issue in ["BILLING-EXPORTER-2D", "REPORT-BUILDER-4Q"] {
        let trial = prepared(MEMORY);
        memory(&trial, "1Gi", false).await;
        native_arguments(
            "sentry",
            "update_issue",
            json!({"organizationSlug":"lumen-labs","issueId":issue,"status":"resolved"}),
        );
        trial
            .call_from(
                "sentry",
                "PUT",
                &format!("/api/0/organizations/lumen-labs/issues/{issue}/"),
                json!({"status":"resolved"}),
            )
            .await;
        assert_outcome(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn memory_accepts_plain_resolution_using_the_returned_numeric_issue_identity() {
    let trial = prepared(MEMORY);
    memory(&trial, "1024Mi", false).await;
    native_arguments(
        "sentry",
        "update_issue",
        json!({"organizationSlug":"lumen-labs","issueId":"6184422318","status":"resolved"}),
    );
    trial
        .call_from(
            "sentry",
            "PUT",
            "/api/0/organizations/lumen-labs/issues/6184422318/",
            json!({"status":"resolved"}),
        )
        .await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn memory_can_use_an_equivalent_representation_after_resolution() {
    let trial = prepared(MEMORY);
    memory(&trial, "1Gi", false).await;
    resolve_issue(&trial, "resolved").await;
    memory(&trial, "1024Mi", false).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn memory_rejects_a_restored_cpu_change() {
    let trial = prepared(MEMORY);
    let mut original = trial.world.lock().unwrap().snapshot()["services"]["kubernetes"]
        ["deployments.apps"]["analytics/report-builder"]
        .clone();
    original["metadata"]
        .as_object_mut()
        .unwrap()
        .remove("resourceVersion");
    let mut resource = original.clone();
    resource["spec"]["template"]["spec"]["containers"][0]["resources"]["requests"]["cpu"] =
        json!("900m");
    trial
        .call_from("kubernetes", "PUT", MEMORY_PATH, resource)
        .await;
    trial
        .call_from("kubernetes", "PUT", MEMORY_PATH, original)
        .await;
    memory(&trial, "1Gi", false).await;
    resolve_issue(&trial, "resolved").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_target_needs_real_authored_rendered_content_before_admission() {
    let trial = prepared_with(ROLLBACK, |dataset| {
        dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("rendered_revisions")
            .remove("billing-api-production-v2.8.0");
    });
    native_arguments(
        "datadog",
        "query_metrics",
        json!({"from":1793253600,"to":1793282400,"query":"avg:billing.api.latency.p95{env:prod,service:billing-api}"}),
    );
    trial.call_from("datadog","GET",
        "/api/v1/query?from=1793253600&to=1793282400&query=avg%3Abilling.api.latency.p95%7Benv%3Aprod%2Cservice%3Abilling-api%7D",
        Value::Null).await;
    let mut spec = trial.world.lock().unwrap().snapshot()["services"]["argocd"]["applications"]
        ["argocd/billing-api"]["spec"]
        .clone();
    spec["source"]["targetRevision"] = json!("v2.8.0");
    native_arguments(
        "argocd",
        "update_application_spec",
        json!({"applicationName":"billing-api","applicationNamespace":"argocd","spec":spec}),
    );
    let (status, response) = trial
        .request_from(
            "argocd",
            "PUT",
            "/api/v1/applications/billing-api/spec?appNamespace=argocd",
            spec,
        )
        .await;
    let assessment = observed(&trial);
    assert_eq!(
        assessment.outcome,
        Outcome::EnvironmentError,
        "{assessment:?}"
    );
    assert!(assessment.failures.iter().any(|failure| {
        failure.location == "environment/unsupported_argocd_operation"
            && failure.reason.contains("rendered manifest snapshot")
    }));
    assert_eq!(status.as_u16(), 501, "{status}: {response}");
}
