mod support;

use catalog::Catalog;
use mock_core::{
    dataset::Dataset,
    grading::{CheckDisposition, CheckKind, Grade, Outcome},
};
use scenario::{
    SolutionCall,
    oracle_tools::{ArgumentValidation, OracleTools},
};
use serde_json::{Value, json};
use support::ContractTrial;

const ROLLBACK: &str = "ops/multi/datadog-latency-argocd-rollback";
const SPIKE: &str = "code/multi/sentry-spike-vercel-rollback";
const PRODUCTION: &str = "argocd/billing-api";
const STAGING: &str = "argocd/billing-api-staging";
const WORKER: &str = "argocd/billing-worker";
const TARGET_COMMIT: &str = "f4befdc6efa7504dc43d29f4de52c0f1529f52d2";
const REGRESSED_COMMIT: &str = "9c2e41d7b85a03f6e1d4c8b7a925e06f3b17d4ac";
const OLDER_COMMIT: &str = "2d61be0a94c3f7e85b1d06a2c4f9e83b7a51d6e0";
const API_DEPLOYMENT: &str = "36d012a8-2d0c-5489-91a6-ab358783e78b";
const FULL_DAY: (i64, i64) = (1_793_253_600, 1_793_282_400);
const LAST_TWO_DAYS: (i64, i64) = (1_793_109_900, 1_793_282_700);
const LAST_WEEK: (i64, i64) = (1_792_677_900, 1_793_282_700);
const PROD_LATENCY: &str = "avg:billing.api.latency.p95{env:prod,service:billing-api}";

fn rollback_trial(prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_many_with_dataset(
        ROLLBACK,
        &[
            (
                &mock_argocd::Argocd,
                &[("authorization", "Bearer argocd-toolbench")],
            ),
            (
                &mock_datadog::Datadog,
                &[
                    ("dd-api-key", "local-api-key"),
                    ("dd-application-key", "local-application-key"),
                ],
            ),
        ],
        prepare,
    )
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

fn encoded(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

async fn latency(trial: &ContractTrial, window: (i64, i64), query: &str) -> Value {
    native_arguments(
        "datadog",
        "query_metrics",
        json!({"from":window.0,"to":window.1,"query":query}),
    );
    trial
        .call_from(
            "datadog",
            "GET",
            &format!(
                "/api/v1/query?from={}&to={}&query={}",
                window.0,
                window.1,
                encoded(query)
            ),
            Value::Null,
        )
        .await
}

async fn investigate(trial: &ContractTrial) {
    latency(trial, FULL_DAY, PROD_LATENCY).await;
}

async fn observe(trial: &ContractTrial, name: &str) {
    for _attempt in 0..6 {
        native_arguments(
            "argocd",
            "get_application",
            json!({"applicationName":name,"applicationNamespace":"argocd"}),
        );
        let application = trial
            .call_from(
                "argocd",
                "GET",
                &format!("/api/v1/applications/{name}?appNamespace=argocd"),
                Value::Null,
            )
            .await;
        if !application["operation"].is_object() {
            return;
        }
    }
    panic!("controller did not complete the bounded operation");
}

fn application(trial: &ContractTrial, key: &str) -> Value {
    trial.world.lock().unwrap().snapshot()["services"]["argocd"]["applications"][key].clone()
}

async fn pin_spec(
    trial: &ContractTrial,
    name: &str,
    revision: &str,
    mutate: impl FnOnce(&mut Value),
) {
    let mut spec = application(trial, &format!("argocd/{name}"))["spec"].clone();
    spec["source"]["targetRevision"] = json!(revision);
    mutate(&mut spec);
    native_arguments(
        "argocd",
        "update_application_spec",
        json!({"applicationName":name,"applicationNamespace":"argocd","spec":spec}),
    );
    trial
        .call_from(
            "argocd",
            "PUT",
            &format!("/api/v1/applications/{name}/spec?appNamespace=argocd"),
            spec,
        )
        .await;
}

async fn pin_application(trial: &ContractTrial, revision: &str, mutate: impl FnOnce(&mut Value)) {
    let current = application(trial, PRODUCTION);
    let mut whole = json!({
        "metadata":{"name":current["metadata"]["name"],"namespace":current["metadata"]["namespace"]},
        "spec":current["spec"]
    });
    whole["spec"]["source"]["targetRevision"] = json!(revision);
    mutate(&mut whole);
    native_arguments(
        "argocd",
        "update_application",
        json!({"applicationName":"billing-api","application":whole}),
    );
    trial
        .call_from("argocd", "PUT", "/api/v1/applications/billing-api", whole)
        .await;
}

async fn roll_back(trial: &ContractTrial) {
    pin_spec(trial, "billing-api", TARGET_COMMIT, |_| {}).await;
    observe(trial, "billing-api").await;
}

#[track_caller]
fn assert_outcome(trial: &ContractTrial, expected: Outcome) -> Grade {
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, expected, "{assessment:?}");
    assert!(assessment.contract_ok, "{assessment:?}");
    if expected != Outcome::EnvironmentError {
        assert!(assessment.environment_ok, "{assessment:?}");
    }
    assessment
}

fn deployed_image(trial: &ContractTrial) -> Value {
    trial.world.lock().unwrap().snapshot()["services"]["argocd"]["resources"][API_DEPLOYMENT]
        ["manifest"]["spec"]["template"]["spec"]["containers"][0]["image"]
        .clone()
}

#[tokio::test]
async fn rollback_reference_pins_the_commit_read_from_history_and_observes_the_rollout() {
    let trial = rollback_trial(|_| {});
    investigate(&trial).await;
    let read = trial
        .call_from(
            "argocd",
            "GET",
            "/api/v1/applications/billing-api",
            Value::Null,
        )
        .await;
    let calls = &trial.scenario.solution.calls;
    let mut bindings = scenario::solution::Bindings::new();
    calls
        .iter()
        .find(|call| !call.capture.is_empty())
        .unwrap()
        .capture_result(
            &json!({"content":[{"type":"text","text":read.to_string()}]}),
            &mut bindings,
        )
        .unwrap();
    assert_eq!(bindings["previous_release"], TARGET_COMMIT);
    let arguments = calls
        .iter()
        .find(|call| call.tool == "update_application_spec")
        .unwrap()
        .resolve_arguments(&bindings)
        .unwrap();
    native_arguments("argocd", "update_application_spec", arguments.clone());
    trial
        .call_from(
            "argocd",
            "PUT",
            "/api/v1/applications/billing-api/spec?appNamespace=argocd",
            arguments["spec"].clone(),
        )
        .await;
    // The reference observes the rollout with exactly the reads that follow its update.
    for _read in calls
        .iter()
        .skip_while(|call| call.tool != "update_application_spec")
        .skip(1)
    {
        trial
            .call_from(
                "argocd",
                "GET",
                "/api/v1/applications/billing-api?appNamespace=argocd",
                Value::Null,
            )
            .await;
    }
    assert_outcome(&trial, Outcome::Pass);
    let deployed = application(&trial, PRODUCTION);
    assert_eq!(deployed["status"]["sync"]["revision"], TARGET_COMMIT);
    assert_eq!(
        deployed_image(&trial),
        "ghcr.io/lumen-retail/billing-api:2.8.0"
    );
    assert_eq!(
        application(&trial, STAGING)["spec"]["source"]["targetRevision"],
        "main"
    );
}

#[tokio::test]
async fn rollback_accepts_a_grouped_last_day_query_as_the_regression_evidence() {
    let trial = rollback_trial(|_| {});
    let grouped = latency(
        &trial,
        (1_793_196_300, 1_793_282_700),
        "max:billing.api.latency.p95{service:billing-api} by {env}",
    )
    .await;
    assert_eq!(grouped["series"].as_array().unwrap().len(), 2, "{grouped}");
    roll_back(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn rollback_accepts_rolled_up_latency_series_as_the_regression_evidence() {
    for (window, query, interval) in [
        (LAST_TWO_DAYS, PROD_LATENCY, 600),
        (LAST_WEEK, PROD_LATENCY, 3600),
        (
            FULL_DAY,
            "max:billing.api.latency.p95{env:prod,service:billing-api}.rollup(max, 3600)",
            3600,
        ),
    ] {
        let trial = rollback_trial(|_| {});
        let response = latency(&trial, window, query).await;
        assert_eq!(response["series"][0]["interval"], interval, "{query}");
        roll_back(&trial).await;
        assert_outcome(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn rollback_rejects_a_native_full_replacement_that_drops_application_metadata() {
    let trial = rollback_trial(|_| {});
    investigate(&trial).await;
    let original = application(&trial, PRODUCTION);
    pin_application(&trial, TARGET_COMMIT, |_| {}).await;
    observe(&trial, "billing-api").await;
    let replaced = application(&trial, PRODUCTION);
    assert_eq!(replaced["status"]["sync"]["revision"], TARGET_COMMIT);
    assert_ne!(
        replaced["metadata"]["labels"],
        original["metadata"]["labels"]
    );
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_accepts_a_manual_sync_of_the_pinned_commit_and_extra_observation() {
    let trial = rollback_trial(|_| {});
    latency(
        &trial,
        (1_793_271_600, 1_793_282_400),
        "avg:billing.api.latency.p95{env:staging,service:billing-api}",
    )
    .await;
    investigate(&trial).await;
    roll_back(&trial).await;
    native_arguments(
        "argocd",
        "sync_application",
        json!({"applicationName":"billing-api","applicationNamespace":"argocd","revision":TARGET_COMMIT}),
    );
    trial
        .call_from(
            "argocd",
            "POST",
            "/api/v1/applications/billing-api/sync",
            json!({"appNamespace":"argocd","revision":TARGET_COMMIT}),
        )
        .await;
    observe(&trial, "billing-api").await;
    observe(&trial, "billing-api-staging").await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn rollback_rejects_the_release_immediately_before_the_latest() {
    let trial = rollback_trial(|_| {});
    investigate(&trial).await;
    pin_spec(&trial, "billing-api", REGRESSED_COMMIT, |_| {}).await;
    observe(&trial, "billing-api").await;
    assert_eq!(
        deployed_image(&trial),
        "ghcr.io/lumen-retail/billing-api:2.9.0"
    );
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_rejects_overshooting_to_an_older_release() {
    let trial = rollback_trial(|_| {});
    investigate(&trial).await;
    pin_spec(&trial, "billing-api", OLDER_COMMIT, |_| {}).await;
    observe(&trial, "billing-api").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_rejects_a_tag_or_abbreviated_sha_when_the_full_commit_was_requested() {
    for revision in ["v2.8.0", "f4befdc"] {
        let trial = rollback_trial(|_| {});
        investigate(&trial).await;
        pin_spec(&trial, "billing-api", revision, |_| {}).await;
        observe(&trial, "billing-api").await;
        assert_eq!(
            application(&trial, PRODUCTION)["status"]["sync"]["revision"],
            TARGET_COMMIT,
            "{revision}"
        );
        assert_outcome(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn rollback_rejects_a_pin_whose_deployment_was_never_observed_to_complete() {
    let trial = rollback_trial(|_| {});
    investigate(&trial).await;
    pin_spec(&trial, "billing-api", TARGET_COMMIT, |_| {}).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_rejects_disabling_automated_sync() {
    let trial = rollback_trial(|_| {});
    investigate(&trial).await;
    pin_spec(&trial, "billing-api", TARGET_COMMIT, |spec| {
        spec["syncPolicy"]
            .as_object_mut()
            .unwrap()
            .shift_remove("automated");
    })
    .await;
    native_arguments(
        "argocd",
        "sync_application",
        json!({"applicationName":"billing-api","applicationNamespace":"argocd"}),
    );
    trial
        .call_from(
            "argocd",
            "POST",
            "/api/v1/applications/billing-api/sync",
            json!({"appNamespace":"argocd"}),
        )
        .await;
    observe(&trial, "billing-api").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_rejects_a_restored_unrequested_production_retry_policy() {
    let trial = rollback_trial(|_| {});
    investigate(&trial).await;
    let original = application(&trial, PRODUCTION);
    pin_spec(&trial, "billing-api", TARGET_COMMIT, |spec| {
        spec["syncPolicy"]["retry"]["limit"] = json!(1);
    })
    .await;
    observe(&trial, "billing-api").await;
    pin_spec(&trial, "billing-api", TARGET_COMMIT, |spec| {
        spec["syncPolicy"]["retry"] = original["spec"]["syncPolicy"]["retry"].clone();
    })
    .await;
    observe(&trial, "billing-api").await;
    let restored = application(&trial, PRODUCTION);
    assert_eq!(
        restored["spec"]["syncPolicy"],
        original["spec"]["syncPolicy"]
    );
    assert_eq!(
        restored["metadata"]["labels"],
        original["metadata"]["labels"]
    );
    assert_eq!(
        restored["metadata"]["finalizers"],
        original["metadata"]["finalizers"]
    );
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_rejects_a_restored_staging_pin() {
    let trial = rollback_trial(|_| {});
    investigate(&trial).await;
    roll_back(&trial).await;
    pin_spec(&trial, "billing-api-staging", "HEAD", |_| {}).await;
    observe(&trial, "billing-api-staging").await;
    pin_spec(&trial, "billing-api-staging", "main", |_| {}).await;
    observe(&trial, "billing-api-staging").await;
    assert_eq!(
        application(&trial, STAGING)["spec"]["source"]["targetRevision"],
        "main"
    );
    assert_eq!(
        application(&trial, WORKER)["spec"]["source"]["targetRevision"],
        "main"
    );
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_without_reading_the_latency_series_has_no_causal_evidence() {
    let trial = rollback_trial(|_| {});
    native_arguments(
        "datadog",
        "get_monitors",
        json!({"tags":["service:billing-api"]}),
    );
    trial
        .call_from(
            "datadog",
            "GET",
            "/api/v1/monitor?monitor_tags=service%3Abilling-api",
            Value::Null,
        )
        .await;
    roll_back(&trial).await;
    let assessment = assert_outcome(&trial, Outcome::Fail);
    assert!(assessment.failures.iter().any(|failure| {
        failure.check == CheckKind::Ordering && failure.location == "information/0"
    }));
}

#[tokio::test]
async fn rollback_evidence_read_after_the_pin_does_not_justify_it() {
    let trial = rollback_trial(|_| {});
    pin_spec(&trial, "billing-api", TARGET_COMMIT, |_| {}).await;
    investigate(&trial).await;
    observe(&trial, "billing-api").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn rollback_wrong_source_series_are_not_regression_evidence() {
    for (window, query) in [
        (
            FULL_DAY,
            "avg:billing.api.latency.p95{env:staging,service:billing-api}",
        ),
        (FULL_DAY, "avg:billing.api.latency.p95{service:billing-api}"),
        (
            FULL_DAY,
            "avg:billing.worker.job.duration.p95{env:prod,service:billing-worker}",
        ),
        ((1_793_272_000, 1_793_282_400), PROD_LATENCY),
        (
            FULL_DAY,
            "count:billing.api.latency.p95{env:prod,service:billing-api}.rollup(count, 600)",
        ),
        (
            FULL_DAY,
            "avg:billing.api.latency.p95{env:prod,service:billing-api}.rollup(sum, 600)",
        ),
        (
            LAST_TWO_DAYS,
            "avg:billing.api.latency.p95{service:billing-api}",
        ),
    ] {
        let trial = rollback_trial(|_| {});
        let response = latency(&trial, window, query).await;
        assert!(
            !response["series"].as_array().unwrap().is_empty(),
            "{query}: {response}"
        );
        roll_back(&trial).await;
        let assessment = assert_outcome(&trial, Outcome::Fail);
        assert!(
            assessment.failures.iter().any(|failure| {
                failure.location == "information/0"
                    && failure.disposition == CheckDisposition::Failed
            }),
            "{query}: {assessment:?}"
        );
    }
}

#[tokio::test]
async fn rollback_daily_or_unparsed_series_need_adjudication_not_a_guessed_failure() {
    for (window, query) in [
        (
            FULL_DAY,
            "avg:billing.api.latency.p95{env:prod,service:billing-api}.rollup(avg, 86400)",
        ),
        (
            FULL_DAY,
            "avg:billing.api.latency.p95{env:prod,service:billing-*}",
        ),
    ] {
        let trial = rollback_trial(|_| {});
        latency(&trial, window, query).await;
        roll_back(&trial).await;
        let assessment = assert_outcome(&trial, Outcome::Unresolved);
        assert!(
            assessment.failures.iter().all(|failure| {
                failure.location == "information/0"
                    && failure.disposition == CheckDisposition::Unresolved
            }),
            "{query}: {assessment:?}"
        );
    }
}

#[tokio::test]
async fn rollback_missing_authored_release_content_remains_an_environment_fault() {
    let trial = rollback_trial(|dataset| {
        dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("rendered_revisions")
            .remove("billing-api-production-v2.8.0");
    });
    investigate(&trial).await;
    let mut spec = application(&trial, PRODUCTION)["spec"].clone();
    spec["source"]["targetRevision"] = json!(TARGET_COMMIT);
    let (status, response) = trial
        .request_from(
            "argocd",
            "PUT",
            "/api/v1/applications/billing-api/spec?appNamespace=argocd",
            spec,
        )
        .await;
    assert_eq!(status.as_u16(), 501, "{response}");
    let assessment = assert_outcome(&trial, Outcome::EnvironmentError);
    assert!(
        assessment
            .failures
            .iter()
            .any(|failure| { failure.location == "environment/unsupported_argocd_operation" })
    );
}

const STOREFRONT: &str = "prj_3dw2VoUa166qmaLV3P6dm828isAX";
const PREVIOUS_BUILD: &str = "dpl_8o704GjcwumkLAaxPwjoxb4V6QDF";
const REGRESSED_BUILD: &str = "dpl_7wC1vKiBnuwDgHLXzdLRklqAYci5";
const FAILED_BUILD: &str = "dpl_PHkhNPm61EU1aF79kakZb9UhQNNQ";
const PREVIEW_BUILD: &str = "dpl_1KT70nyZgoWTlCSRmxLe0svxe4Xx";

fn spike_trial(prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_many_with_dataset(
        SPIKE,
        &[
            (&mock_vercel::Vercel, &[("authorization", "Bearer tb")]),
            (
                &mock_sentry::Sentry,
                &[("authorization", "Bearer sntrys_toolbench")],
            ),
        ],
        prepare,
    )
}

async fn vercel_tool(trial: &ContractTrial, name: &str, arguments: Value) -> Value {
    native_arguments("vercel", name, arguments.clone());
    trial
        .call_from(
            "vercel",
            "POST",
            "/_mcp/tools/call",
            json!({"name":name,"arguments":arguments}),
        )
        .await
}

async fn redeploy(trial: &ContractTrial, project: &str, deployment: &str) {
    vercel_tool(
        trial,
        "vercel-create-deployment",
        json!({"name":project,"deploymentId":deployment,"target":"production"}),
    )
    .await;
}

async fn update_issue(trial: &ContractTrial, issue: &str, change: Value) {
    let mut arguments = json!({"organizationSlug":"acme-shop","issueId":issue});
    for (field, value) in change.as_object().unwrap() {
        arguments[field] = value.clone();
    }
    native_arguments("sentry", "update_issue", arguments);
    trial
        .call_from(
            "sentry",
            "PUT",
            &format!("/api/0/organizations/acme-shop/issues/{issue}/"),
            change,
        )
        .await;
}

async fn hand_over(trial: &ContractTrial) {
    update_issue(
        trial,
        "STOREFRONT-5R",
        json!({"assignedTo":"team:payments-web"}),
    )
    .await;
}

async fn spike_reference(trial: &ContractTrial) {
    native_arguments(
        "sentry",
        "search_issues",
        json!({"organizationSlug":"acme-shop","projectSlugOrId":"storefront","query":"is:unresolved environment:production"}),
    );
    trial
        .call_from(
            "sentry",
            "GET",
            "/api/0/organizations/acme-shop/issues/?query=project%3Astorefront%20is%3Aunresolved%20environment%3Aproduction",
            Value::Null,
        )
        .await;
    vercel_tool(
        trial,
        "vercel-list-all-deployments",
        json!({"app":"storefront"}),
    )
    .await;
    redeploy(trial, "storefront", PREVIOUS_BUILD).await;
    hand_over(trial).await;
}

fn vercel_production(trial: &ContractTrial) -> Value {
    trial.world.lock().unwrap().snapshot()["services"]["vercel"]["projects"][STOREFRONT]["targets"]
        ["production"]
        .clone()
}

#[tokio::test]
async fn spike_reference_restores_the_exact_previously_serving_build_and_hands_over() {
    let trial = spike_trial(|_| {});
    spike_reference(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
    let production = vercel_production(&trial);
    assert_eq!(production["meta"]["originalDeploymentId"], PREVIOUS_BUILD);
    assert_eq!(production["readyState"], "READY");
}

#[tokio::test]
async fn spike_accepts_numeric_identities_and_handing_over_before_the_rollback() {
    let trial = spike_trial(|_| {});
    update_issue(&trial, "7201330418", json!({"assignedTo":"team:4508004"})).await;
    redeploy(&trial, "storefront", PREVIOUS_BUILD).await;
    vercel_tool(
        &trial,
        "vercel-get-deployment",
        json!({"idOrUrl":vercel_production(&trial)["id"]}),
    )
    .await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn spike_accepts_restating_the_unresolved_status_with_the_handover() {
    let trial = spike_trial(|_| {});
    redeploy(&trial, "storefront", PREVIOUS_BUILD).await;
    let sentry = |trial: &ContractTrial| {
        trial.world.lock().unwrap().snapshot()["services"]["sentry"].clone()
    };
    let before = sentry(&trial);
    // "Leave it unresolved" said in the same call: the issue already is, so Sentry
    // records the assignment and nothing about its status.
    update_issue(
        &trial,
        "STOREFRONT-5R",
        json!({"assignedTo":"team:payments-web","status":"unresolved"}),
    )
    .await;
    let after = sentry(&trial);
    assert_eq!(
        after["issues"]["STOREFRONT-5R"]["substatus"],
        before["issues"]["STOREFRONT-5R"]["substatus"]
    );
    let recorded: Vec<&str> = after["activities"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| before["activities"].get(key.as_str()).is_none())
        .map(|(_, activity)| activity["type"].as_str().unwrap())
        .collect();
    assert_eq!(recorded, ["assigned"]);
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn spike_rejects_redeploying_any_build_other_than_the_previous_production_server() {
    for build in [REGRESSED_BUILD, PREVIEW_BUILD] {
        let trial = spike_trial(|_| {});
        redeploy(&trial, "storefront", build).await;
        hand_over(&trial).await;
        assert_outcome(&trial, Outcome::Fail);
    }
    let trial = spike_trial(|_| {});
    let (status, response) = trial
        .request_from(
            "vercel",
            "POST",
            "/_mcp/tools/call",
            json!({"name":"vercel-create-deployment","arguments":{
                "name":"storefront","deploymentId":FAILED_BUILD,"target":"production"
            }}),
        )
        .await;
    hand_over(&trial).await;
    let assessment = trial.assess();
    assert_eq!(
        assessment.outcome,
        Outcome::Fail,
        "{status}: {response} {assessment:?}"
    );
}

#[tokio::test]
async fn spike_display_metadata_cannot_launder_the_regressed_build() {
    let trial = spike_trial(|_| {});
    vercel_tool(
        &trial,
        "vercel-create-deployment",
        json!({
            "name":"storefront","deploymentId":REGRESSED_BUILD,"target":"production",
            "meta":{"githubCommitSha":"c3a9f12e8b47d05a6e21f9b3c7d84e50a1b6f2d9"}
        }),
    )
    .await;
    hand_over(&trial).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn spike_rejects_redeploying_another_project() {
    let trial = spike_trial(|_| {});
    redeploy(&trial, "storefront", PREVIOUS_BUILD).await;
    redeploy(
        &trial,
        "storefront-admin",
        "dpl_FtAVsX1HHV2AIfRXwGMbHX0OrVir",
    )
    .await;
    hand_over(&trial).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn spike_partial_completion_is_not_success() {
    let trial = spike_trial(|_| {});
    redeploy(&trial, "storefront", PREVIOUS_BUILD).await;
    assert_outcome(&trial, Outcome::Fail);
    let trial = spike_trial(|_| {});
    hand_over(&trial).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn spike_rejects_handing_over_a_lookalike_or_to_the_wrong_team() {
    for (issue, team) in [
        ("STOREFRONT-5T", "team:payments-web"),
        ("STOREFRONT-4W", "team:payments-web"),
        ("STOREFRONT-5R", "team:storefront"),
    ] {
        let trial = spike_trial(|_| {});
        redeploy(&trial, "storefront", PREVIOUS_BUILD).await;
        update_issue(&trial, issue, json!({"assignedTo":team})).await;
        if issue != "STOREFRONT-5R" {
            hand_over(&trial).await;
        }
        assert_outcome(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn spike_rejects_restored_issue_status_and_restored_environment_variables() {
    let trial = spike_trial(|_| {});
    spike_reference(&trial).await;
    update_issue(&trial, "STOREFRONT-5R", json!({"status":"resolved"})).await;
    update_issue(&trial, "STOREFRONT-5R", json!({"status":"unresolved"})).await;
    assert_eq!(
        trial.world.lock().unwrap().snapshot()["services"]["sentry"]["issues"]["STOREFRONT-5R"]["status"],
        "unresolved"
    );
    assert_outcome(&trial, Outcome::Fail);
    let trial = spike_trial(|_| {});
    spike_reference(&trial).await;
    vercel_tool(
        &trial,
        "vercel-create-environment-variables",
        json!({"projectId":"storefront","environmentVariables":[
            {"key":"PAYMENTS_CLIENT","value":"v2","target":["production"],"type":"plain"}
        ]}),
    )
    .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn spike_rejects_a_transient_assignment_to_a_person() {
    let trial = spike_trial(|_| {});
    redeploy(&trial, "storefront", PREVIOUS_BUILD).await;
    update_issue(
        &trial,
        "STOREFRONT-5R",
        json!({"assignedTo":"user:3100207"}),
    )
    .await;
    hand_over(&trial).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[test]
fn both_reference_programs_satisfy_the_locked_tool_schemas() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(&root, &catalog).unwrap();
    for id in [ROLLBACK, SPIKE] {
        let scenario = scenario::Scenario::load(&root.join("scenarios").join(id)).unwrap();
        scenario.expect.validate().unwrap();
        assert!(!scenario.solution.calls.is_empty(), "{id}");
        for call in &scenario.solution.calls {
            tools
                .validate(call)
                .unwrap_or_else(|error| panic!("{id} {}: {error}", call.tool));
        }
    }
}
