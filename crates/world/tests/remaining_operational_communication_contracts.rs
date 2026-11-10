mod support;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use catalog::Catalog;
use mock_core::grading::{CheckDisposition, CheckKind, Grade, Outcome};
use scenario::{
    SolutionCall,
    oracle_tools::{ArgumentValidation, OracleTools},
};
use serde_json::{Value, json};
use support::ContractTrial;
use tower::ServiceExt;

const ANNOTATE: &str = "code/multi/terraform-failure-grafana-annotate";
const ESCALATE: &str = "ops/multi/pagerduty-sentry-github-issue";
const FAILED_RUN: &str = "run-Gy4tBHGggKx49nbH";
const PLAN_ERROR_RUN: &str = "run-nAAGvVVuIe2rfmNM";
const STAGING_RUN: &str = "run-YF4vEQXaSoBg49H3";
const ERROR_CODE: &str = "InvalidParameterCombination";
const FAILURE_TIME: i64 = 1_793_193_129_000;
const ISSUE_TITLE: &str =
    "CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s";
const ISSUE_BODY: &str = "PagerDuty incident #4810 (Checkout error rate above 5%, production).\n\nSentry: CHECKOUT-API-3F\nCulprit: checkout.views.orders in create_order\nError: OperationalError: could not obtain connection from pool within 5.0s\nEndpoint: POST /api/orders/ (1,842 events, 611 users)";
const ISSUES: &str = "/api/v3/repos/lumen-labs/checkout/issues";
const PRODUCTION_INCIDENT: &str = "Q7CHKAP15PCT1D";
const STAGING_INCIDENT: &str = "Q8CHKSTG5PCT2F";

fn annotation_trial(prepare: impl FnOnce(&mut mock_core::dataset::Dataset)) -> ContractTrial {
    let grafana = format!("Bearer {}", mock_grafana::TOKEN);
    ContractTrial::load_many_with_dataset(
        ANNOTATE,
        &[
            (
                &mock_terraform::Terraform,
                &[("authorization", "Bearer local-fixture-credential")],
            ),
            (&mock_grafana::Grafana, &[("authorization", &grafana)]),
        ],
        prepare,
    )
}

fn escalation_trial(prepare: impl FnOnce(&mut mock_core::dataset::Dataset)) -> ContractTrial {
    ContractTrial::load_many_with_dataset(
        ESCALATE,
        &[
            (
                &mock_pagerduty::Pagerduty,
                &[
                    ("authorization", "Token token=local-fixture-credential"),
                    ("from", "lena.weber@lumenlabs.dev"),
                ],
            ),
            (
                &mock_sentry::Sentry,
                &[("authorization", "Bearer sntrys_toolbench")],
            ),
            (&mock_github::GitHub, &[]),
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

#[track_caller]
fn assert_outcome(trial: &ContractTrial, expected: Outcome) -> Grade {
    let grade = trial.assess();
    assert_eq!(grade.outcome, expected, "{grade:#?}");
    grade
}

#[track_caller]
fn assert_failure_at(grade: &Grade, kind: CheckKind, disposition: CheckDisposition) {
    assert!(
        grade
            .failures
            .iter()
            .any(|failure| failure.check == kind && failure.disposition == disposition),
        "no {kind:?}/{disposition:?} failure in {grade:#?}"
    );
}

async fn annotate(trial: &ContractTrial, overrides: Value) -> i64 {
    let mut body = json!({
        "dashboardUID":"payments-db-prod","time":FAILURE_TIME,
        "text":format!("Terraform {FAILED_RUN} (Upgrade payments DB to Postgres 16) failed on payments-api-prod: InvalidParameterCombination on aws_db_instance.payments"),
        "tags":["terraform","payments"]
    });
    for (field, value) in overrides.as_object().unwrap() {
        body[field] = value.clone();
    }
    trial
        .call_from("grafana", "POST", "/api/annotations", body)
        .await["id"]
        .as_i64()
        .unwrap()
}

async fn open_issue(trial: &ContractTrial, title: &str, body: &str) -> u64 {
    trial
        .call_from("github", "POST", ISSUES, json!({"title":title,"body":body}))
        .await["number"]
        .as_u64()
        .unwrap()
}

async fn note(trial: &ContractTrial, incident: &str, content: &str) {
    trial
        .call_from(
            "pagerduty",
            "POST",
            &format!("/incidents/{incident}/notes"),
            json!({"note":{"content":content}}),
        )
        .await;
}

fn tracking(number: u64) -> String {
    format!(
        "Tracking the fix in GitHub issue lumen-labs/checkout#{number} (Sentry CHECKOUT-API-3F)."
    )
}

fn pass(trial: &ContractTrial) {
    assert_outcome(trial, Outcome::Pass);
}

fn fail(trial: &ContractTrial) -> Grade {
    assert_outcome(trial, Outcome::Fail)
}

fn instruction(id: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios")
            .join(id)
            .join("instruction.md"),
    )
    .unwrap()
}

#[test]
fn authored_instructions_dates_and_source_facts_are_unchanged_and_ground_the_contracts() {
    assert_eq!(
        instruction(ANNOTATE),
        "Our Postgres 16 upgrade of the production payments database failed in HCP Terraform this afternoon (org `acme-platform`). I want the DB graphs in Grafana to show exactly when it happened so we can line it up with the connection errors.\n\nFind the failed run on the production payments workspace and add one annotation to the production \"Payments DB\" dashboard in Grafana, at the exact time (to the second) that run errored. The annotation text must include the run ID and the AWS error code from the failure, and tag it `terraform` and `payments`.\n\nDon't change anything in Terraform, don't edit any dashboards, and leave the staging and legacy dashboards alone.\n"
    );
    assert_eq!(
        instruction(ESCALATE),
        "PagerDuty paged us about the checkout error rate in production. Please find the error behind it in Sentry (org `lumen-labs`) and open a GitHub issue in `lumen-labs/checkout` so the payments team can fix it:\n\n- the title should start with the Sentry short issue ID (e.g. `PROJECT-123: ...`)\n- the description must include the PagerDuty incident number, the Sentry culprit and the error message\n\nThen add a note to the PagerDuty incident with the number of the GitHub issue you opened.\n\nStaging has a similar alert from a load test; ignore it. Don't change the status of any incident or Sentry issue.\n"
    );
    let annotate = annotation_trial(|_| {});
    assert_eq!(annotate.scenario.meta.now, "2026-10-28T15:30:00Z");
    let initial = annotate.world.lock().unwrap().initial_snapshot();
    let run = &initial["services"]["terraform"]["runs"][FAILED_RUN];
    assert_eq!(run["status"], "errored");
    assert_eq!(run["workspace"], "ws-vJd7W10B1cG75Uxp");
    assert_eq!(run["message"], "Upgrade payments DB to Postgres 16");
    let errored_at = run["status-timestamps"]["errored-at"].as_str().unwrap();
    let nanos = mock_core::clock::parse_rfc3339_nanos(errored_at).unwrap();
    assert_eq!(nanos / 1_000_000, i128::from(FAILURE_TIME));
    let apply = run["apply"].as_str().unwrap();
    let log = initial["services"]["terraform"]["applies"][apply]["log"]
        .as_str()
        .unwrap();
    assert!(log.contains(&format!("api error {ERROR_CODE}:")));
    for other in [PLAN_ERROR_RUN, STAGING_RUN] {
        assert_ne!(
            initial["services"]["terraform"]["runs"][other]["id"],
            FAILED_RUN
        );
    }
    let dashboards = &initial["services"]["grafana"]["dashboards"];
    assert_eq!(dashboards["payments-db-prod"]["dashboard"]["id"], 120);
    assert_eq!(dashboards["payments-db-prod"]["folderUid"], "f-prod");
    assert_eq!(
        dashboards["payments-db-prod"]["dashboard"]["title"],
        "Payments DB"
    );

    let escalate = escalation_trial(|_| {});
    assert_eq!(escalate.scenario.meta.now, "2026-10-20T16:00:00Z");
    let initial = escalate.world.lock().unwrap().initial_snapshot();
    let incident = &initial["services"]["pagerduty"]["incidents"][PRODUCTION_INCIDENT];
    assert_eq!(incident["incident_number"], 4810);
    assert_eq!(incident["service"]["id"], "PSVCHKA");
    assert_eq!(
        initial["services"]["pagerduty"]["incidents"][STAGING_INCIDENT]["incident_number"],
        4811
    );
    let sentry = &initial["services"]["sentry"]["issues"]["CHECKOUT-API-3F"];
    assert_eq!(sentry["culprit"], "checkout.views.orders in create_order");
    assert_eq!(sentry["project"], "checkout-api");
    assert_eq!(
        sentry["title"],
        ISSUE_TITLE.strip_prefix("CHECKOUT-API-3F: ").unwrap()
    );
    assert!(
        ISSUE_BODY.contains(sentry["metadata"]["value"].as_str().unwrap())
            && ISSUE_BODY.contains(sentry["culprit"].as_str().unwrap())
    );
    assert_eq!(
        sentry["tags"]["transaction"][0]["value"],
        "POST /api/orders/"
    );
    assert_eq!(sentry["count"], "1842");
    assert_eq!(sentry["userCount"], 611);
    let highest = initial["services"]["github"]["issues"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| key.starts_with("lumen-labs/checkout#"))
        .count();
    assert_eq!(highest, 3);
}

#[test]
fn solution_calls_are_natively_argument_valid_and_the_note_binds_the_emitted_issue_number() {
    let annotate = annotation_trial(|_| {});
    for call in &annotate.scenario.solution.calls {
        native_arguments(&call.endpoint, &call.tool, call.arguments.clone());
    }
    let escalate = escalation_trial(|_| {});
    escalate.scenario.solution.validate_bindings().unwrap();
    let mut bindings = scenario::solution::Bindings::new();
    bindings.insert("issue_number".into(), json!(214));
    for call in &escalate.scenario.solution.calls {
        let arguments = call.resolve_arguments(&bindings).unwrap();
        native_arguments(&call.endpoint, &call.tool, arguments);
    }
    let note = escalate
        .scenario
        .solution
        .calls
        .iter()
        .find(|call| call.tool == "add_note_to_incident")
        .unwrap();
    assert!(note.template);
    assert!(
        note.arguments["note"]
            .as_str()
            .unwrap()
            .contains("${issue_number}")
    );
    assert!(!note.arguments["note"].as_str().unwrap().contains("214"));
    assert!(
        escalate
            .scenario
            .solution
            .calls
            .iter()
            .any(|call| !call.capture.is_empty() && call.tool == "list_issues")
    );
}

#[tokio::test]
async fn annotation_reference_and_alternative_plans_pass() {
    let reference = annotation_trial(|_| {});
    for path in [
        "/api/v2/organizations/acme-platform/workspaces/payments-api-prod",
        &format!("/api/v2/runs/{FAILED_RUN}"),
        "/api/v2/applies/apply-OttY2qKt1DNXCd1P",
    ] {
        reference
            .call_from("terraform", "GET", path, Value::Null)
            .await;
    }
    annotate(&reference, json!({})).await;
    pass(&reference);

    let wordings = [
        format!("Terraform {FAILED_RUN} failed: {ERROR_CODE}"),
        format!("Terraform run {FAILED_RUN} errored on payments-api-prod with {ERROR_CODE}"),
        format!("Postgres 16 upgrade run {FAILED_RUN} failed on payments-api-prod: {ERROR_CODE}"),
        format!(
            "Payments DB Postgres 16 upgrade errored in Terraform run {FAILED_RUN}: {ERROR_CODE}"
        ),
    ];
    for text in wordings {
        let trial = annotation_trial(|_| {});
        annotate(&trial, json!({"text":text})).await;
        pass(&trial);
    }
    let reversed = annotation_trial(|_| {});
    annotate(&reversed, json!({"tags":["payments","terraform"]})).await;
    pass(&reversed);
    let point = annotation_trial(|_| {});
    annotate(&point, json!({"timeEnd":FAILURE_TIME})).await;
    pass(&point);
    let by_identifier = annotation_trial(|_| {});
    let mut body = json!({
        "dashboardId":120,"time":FAILURE_TIME,
        "text":format!("Terraform {FAILED_RUN} failed: {ERROR_CODE}"),
        "tags":["terraform","payments"]
    });
    body["dashboardUID"] = Value::Null;
    by_identifier
        .call_from("grafana", "POST", "/api/annotations", body)
        .await;
    pass(&by_identifier);
}

#[tokio::test]
async fn terraform_log_delivery_preserves_chunk_offsets_and_tls_origin_rendering() {
    let trial = annotation_trial(|_| {});
    let app = mock_core::serve::service_app(trial.world.clone(), &mock_terraform::Terraform);
    let apply = "apply-OttY2qKt1DNXCd1P";
    let metadata = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v2/applies/{apply}"))
                .header("host", "app.terraform.io")
                .header("x-forwarded-proto", "https")
                .header("authorization", "Bearer local-fixture-credential")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(metadata.status(), StatusCode::OK);
    let bytes = to_bytes(metadata.into_body(), usize::MAX).await.unwrap();
    let metadata: Value = serde_json::from_slice(&bytes).unwrap();
    let log_url = metadata["data"]["attributes"]["log-read-url"]
        .as_str()
        .unwrap();
    assert_eq!(
        log_url,
        format!("https://app.terraform.io/archivist/v1/object/apply-log-{apply}")
    );
    let text =
        trial.world.lock().unwrap().snapshot()["services"]["terraform"]["applies"][apply]["log"]
            .as_str()
            .unwrap()
            .to_owned();
    let mut framed = vec![2];
    framed.extend_from_slice(text.as_bytes());
    framed.push(3);
    let mut reconstructed = Vec::new();
    for (offset, limit) in [(0usize, 512usize), (512, 256), (768, 384), (817, 336)] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("{log_url}?limit={limit}&offset={offset}"))
                    .header("host", "app.terraform.io")
                    .header("x-forwarded-proto", "https")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let chunk = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            chunk.as_ref(),
            &framed[offset.min(framed.len())..offset.saturating_add(limit).min(framed.len())]
        );
        reconstructed.extend_from_slice(&chunk);
    }
    assert_eq!(reconstructed, framed);
}

#[tokio::test]
async fn annotation_uses_the_emitted_identity_not_a_predicted_key() {
    let trial = annotation_trial(|dataset| {
        let annotations = dataset
            .services
            .get_mut("grafana")
            .unwrap()
            .collection_mut("annotations");
        let mut extra = annotations.get("52").unwrap().clone();
        extra["id"] = json!(90);
        extra["_key"] = json!("90");
        annotations.insert("90", extra);
    });
    let identity = annotate(&trial, json!({})).await;
    assert_eq!(identity, 91);
    assert!(
        trial.world.lock().unwrap().snapshot()["services"]["grafana"]["annotations"]["53"]
            .is_null()
    );
    pass(&trial);
}

#[tokio::test]
async fn annotation_rejects_wrong_dashboards_and_scopes() {
    for dashboard in [
        "payments-db-staging",
        "payments-db-legacy",
        "orders-db",
        "checkout-prod",
    ] {
        let trial = annotation_trial(|_| {});
        annotate(&trial, json!({"dashboardUID":dashboard})).await;
        fail(&trial);
    }
    let panel = annotation_trial(|_| {});
    annotate(&panel, json!({"panelId":1})).await;
    fail(&panel);
    let organization = annotation_trial(|_| {});
    organization
        .call_from(
            "grafana",
            "POST",
            "/api/annotations",
            json!({
                "time":FAILURE_TIME,
                "text":format!("Terraform {FAILED_RUN} failed: {ERROR_CODE}"),
                "tags":["terraform","payments"]
            }),
        )
        .await;
    fail(&organization);
}

#[tokio::test]
async fn annotation_rejects_wrong_instants_and_regions() {
    for time in [
        FAILURE_TIME - 1000,
        FAILURE_TIME + 1000,
        FAILURE_TIME + 1,
        FAILURE_TIME / 1000,
        1_793_190_000_000,
    ] {
        let trial = annotation_trial(|_| {});
        annotate(&trial, json!({"time":time})).await;
        fail(&trial);
    }
    let region = annotation_trial(|_| {});
    annotate(&region, json!({"timeEnd":FAILURE_TIME + 60_000})).await;
    fail(&region);
    let omitted = annotation_trial(|_| {});
    omitted
        .call_from(
            "grafana",
            "POST",
            "/api/annotations",
            json!({
                "dashboardUID":"payments-db-prod",
                "text":format!("Terraform {FAILED_RUN} failed: {ERROR_CODE}"),
                "tags":["terraform","payments"]
            }),
        )
        .await;
    fail(&omitted);
}

#[tokio::test]
async fn annotation_rejects_wrong_run_error_code_and_missing_required_facts() {
    for text in [
        format!("Terraform {PLAN_ERROR_RUN} failed: {ERROR_CODE}"),
        format!("Terraform {STAGING_RUN} failed: {ERROR_CODE}"),
        format!("Terraform {STAGING_RUN} failed: InvalidDBParameterGroupState"),
        format!("Terraform {FAILED_RUN} failed: InvalidDBParameterGroupState"),
        format!("Terraform {FAILED_RUN} failed during apply"),
        format!("Terraform failed with {ERROR_CODE}"),
        "Postgres 16 upgrade failed".to_string(),
        String::new(),
    ] {
        let trial = annotation_trial(|_| {});
        let (status, _) = trial
            .request_from(
                "grafana",
                "POST",
                "/api/annotations",
                json!({
                    "dashboardUID":"payments-db-prod","time":FAILURE_TIME,
                    "text":text,"tags":["terraform","payments"]
                }),
            )
            .await;
        if text.is_empty() {
            assert!(status.is_client_error());
        } else {
            assert!(status.is_success());
        }
        fail(&trial);
    }
    for identity in [
        format!("{FAILED_RUN}Z"),
        FAILED_RUN.replace("Gy4", "Gy5"),
        FAILED_RUN.to_lowercase(),
    ] {
        let lookalike = annotation_trial(|_| {});
        annotate(
            &lookalike,
            json!({"text":format!("Terraform {identity} failed: {ERROR_CODE}")}),
        )
        .await;
        fail(&lookalike);
    }
}

#[tokio::test]
async fn annotation_rejects_unrequested_or_missing_tags() {
    for tags in [
        json!(["terraform"]),
        json!(["payments"]),
        json!([]),
        json!(["terraform", "payments", "prod"]),
        json!(["terraform", "terraform"]),
        json!(["terraform", "payment"]),
        json!(["Terraform", "Payments"]),
    ] {
        let trial = annotation_trial(|_| {});
        annotate(&trial, json!({"tags":tags})).await;
        fail(&trial);
    }
}

#[tokio::test]
async fn annotation_counts_one_created_record_and_rejects_duplicates_or_decoys() {
    let duplicate = annotation_trial(|_| {});
    annotate(&duplicate, json!({})).await;
    annotate(&duplicate, json!({})).await;
    fail(&duplicate);
    let decoy = annotation_trial(|_| {});
    annotate(&decoy, json!({})).await;
    annotate(
        &decoy,
        json!({"dashboardUID":"payments-db-staging","text":format!("Terraform {STAGING_RUN} failed: InvalidDBParameterGroupState"),"tags":["terraform","payments","staging"]}),
    )
    .await;
    fail(&decoy);
    let none = annotation_trial(|_| {});
    let grade = fail(&none);
    assert_failure_at(&grade, CheckKind::Records, CheckDisposition::Failed);
}

#[tokio::test]
async fn annotation_unlisted_wording_passes_with_the_run_and_error_code_and_fails_without() {
    for text in [
        format!("{FAILED_RUN} {ERROR_CODE}"),
        format!("Terraform {FAILED_RUN} failed with {ERROR_CODE} at 13:12:09Z"),
        format!(
            "Payments DB upgrade to Postgres 16 errored during apply ({ERROR_CODE}), run {FAILED_RUN}"
        ),
    ] {
        let trial = annotation_trial(|_| {});
        annotate(&trial, json!({"text":text})).await;
        pass(&trial);
    }
    for text in [
        format!("Payments DB upgrade to Postgres 16 errored during apply, run {FAILED_RUN}"),
        format!("Payments DB upgrade to Postgres 16 errored during apply ({ERROR_CODE})"),
        format!(
            "Payments DB upgrade to Postgres 16 errored during apply ({ERROR_CODE}), run {PLAN_ERROR_RUN}"
        ),
        format!(
            "Payments DB upgrade to Postgres 16 errored during apply (InvalidParameterValue), run {FAILED_RUN}"
        ),
        format!(
            "Payments DB upgrade to Postgres 16 errored during apply ({ERROR_CODE}), run {FAILED_RUN}b"
        ),
    ] {
        let trial = annotation_trial(|_| {});
        annotate(&trial, json!({"text":text})).await;
        let grade = fail(&trial);
        assert!(!grade.adjudication_needed, "{grade:#?}");
    }
}

#[tokio::test]
async fn annotation_history_rejects_harmful_edits_even_when_restored() {
    for patch in [
        json!({"text":format!("Terraform {FAILED_RUN} succeeded: {ERROR_CODE}")}),
        json!({"time":FAILURE_TIME + 1000}),
        json!({"tags":["terraform"]}),
        json!({"text":format!("Terraform {STAGING_RUN} failed: {ERROR_CODE}")}),
    ] {
        let trial = annotation_trial(|_| {});
        let identity = annotate(&trial, json!({})).await;
        let restore = json!({
            "time":FAILURE_TIME,
            "text":format!("Terraform {FAILED_RUN} failed: {ERROR_CODE}"),
            "tags":["terraform","payments"]
        });
        trial
            .call_from(
                "grafana",
                "PATCH",
                &format!("/api/annotations/{identity}"),
                patch,
            )
            .await;
        trial
            .call_from(
                "grafana",
                "PATCH",
                &format!("/api/annotations/{identity}"),
                restore,
            )
            .await;
        assert!(!trial.assess().history_ok);
        assert_ne!(trial.assess().outcome, Outcome::Pass);
    }
    for existing in ["50", "51", "52"] {
        let trial = annotation_trial(|_| {});
        annotate(&trial, json!({})).await;
        let original = trial.world.lock().unwrap().snapshot()["services"]["grafana"]["annotations"]
            [existing]["text"]
            .clone();
        trial
            .call_from(
                "grafana",
                "PATCH",
                &format!("/api/annotations/{existing}"),
                json!({"text":"Edited unrelated annotation"}),
            )
            .await;
        trial
            .call_from(
                "grafana",
                "PATCH",
                &format!("/api/annotations/{existing}"),
                json!({"text":original}),
            )
            .await;
        fail(&trial);
        assert!(!trial.assess().history_ok);
    }
}

#[tokio::test]
async fn annotation_rejects_terraform_dashboard_and_deletion_side_effects() {
    let comment = annotation_trial(|_| {});
    annotate(&comment, json!({})).await;
    comment
        .call_from(
            "terraform",
            "POST",
            &format!("/api/v2/runs/{FAILED_RUN}/comments"),
            json!({"data":{"type":"comments","attributes":{"body":"annotated"}}}),
        )
        .await;
    fail(&comment);
    let dashboard = annotation_trial(|_| {});
    annotate(&dashboard, json!({})).await;
    let existing = dashboard.world.lock().unwrap().snapshot()["services"]["grafana"]["dashboards"]
        ["payments-db-prod"]
        .clone();
    dashboard
        .request_from(
            "grafana",
            "POST",
            "/api/dashboards/db",
            json!({"dashboard":existing["dashboard"],"overwrite":true}),
        )
        .await;
    fail(&dashboard);
    let deletion = annotation_trial(|_| {});
    annotate(&deletion, json!({})).await;
    deletion
        .call_from("grafana", "DELETE", "/api/annotations/52", Value::Null)
        .await;
    fail(&deletion);
    let recreated = annotation_trial(|_| {});
    annotate(&recreated, json!({})).await;
    recreated
        .call_from("grafana", "DELETE", "/api/annotations/51", Value::Null)
        .await;
    fail(&recreated);
}

async fn issue_flow(trial: &ContractTrial, title: &str, body: &str) -> u64 {
    let number = open_issue(trial, title, body).await;
    note(trial, PRODUCTION_INCIDENT, &tracking(number)).await;
    number
}

#[tokio::test]
async fn escalation_reference_and_alternative_plans_pass() {
    let reference = escalation_trial(|_| {});
    reference
        .call_from("pagerduty", "GET", "/incidents", Value::Null)
        .await;
    reference
        .call_from(
            "sentry",
            "GET",
            "/api/0/organizations/lumen-labs/issues/CHECKOUT-API-3F/",
            Value::Null,
        )
        .await;
    let number = issue_flow(&reference, ISSUE_TITLE, ISSUE_BODY).await;
    assert_eq!(number, 214);
    pass(&reference);

    let issues = [
        (
            "CHECKOUT-API-3F: could not obtain connection from pool".to_owned(),
            "PagerDuty incident 4810\nSentry issue: CHECKOUT-API-3F\nCulprit: checkout.views.orders in create_order\nError message: OperationalError: could not obtain connection from pool within 5.0s".to_string(),
        ),
        (
            "CHECKOUT-API-3F: OperationalError in create_order: could not obtain connection from pool within 5.0s".into(),
            "PagerDuty incident #4810\nSentry: CHECKOUT-API-3F\nCulprit: checkout.views.orders in create_order\nError: OperationalError: could not obtain connection from pool within 5.0s".into(),
        ),
    ];
    for (title, body) in issues {
        let trial = escalation_trial(|_| {});
        issue_flow(&trial, &title, &body).await;
        pass(&trial);
    }
    let notes = [
        "GitHub issue lumen-labs/checkout#NUMBER tracks the fix.",
        "Opened GitHub issue #NUMBER in lumen-labs/checkout for this incident.",
        "GitHub issue #NUMBER in lumen-labs/checkout",
        // Wording outside the reviewed examples that still gives the emitted issue number.
        "Opened GitHub issue NUMBER for the payments team.",
        "GitHub issue number: NUMBER",
        "https://github.octo-corp.com/lumen-labs/checkout/issues/NUMBER",
    ];
    for template in notes {
        let trial = escalation_trial(|_| {});
        let number = open_issue(&trial, ISSUE_TITLE, ISSUE_BODY).await;
        note(
            &trial,
            PRODUCTION_INCIDENT,
            &template.replace("NUMBER", &number.to_string()),
        )
        .await;
        pass(&trial);
    }
    let labelled = escalation_trial(|_| {});
    let number = labelled
        .call_from(
            "github",
            "POST",
            ISSUES,
            json!({"title":ISSUE_TITLE,"body":ISSUE_BODY,"labels":["bug"]}),
        )
        .await["number"]
        .as_u64()
        .unwrap();
    note(&labelled, PRODUCTION_INCIDENT, &tracking(number)).await;
    pass(&labelled);
}

#[tokio::test]
async fn escalation_binds_the_note_to_the_actual_number_when_the_dataset_changes() {
    let shifted = escalation_trial(|dataset| {
        let issues = dataset
            .services
            .get_mut("github")
            .unwrap()
            .collection_mut("issues");
        let mut extra = issues.get("lumen-labs/checkout#213").unwrap().clone();
        extra["number"] = json!(240);
        extra["id"] = json!(40240);
        extra["_key"] = json!("lumen-labs/checkout#240");
        issues.insert("lumen-labs/checkout#240", extra);
    });
    let number = open_issue(&shifted, ISSUE_TITLE, ISSUE_BODY).await;
    assert_eq!(number, 241);
    note(&shifted, PRODUCTION_INCIDENT, &tracking(number)).await;
    pass(&shifted);
    let predicted = escalation_trial(|dataset| {
        let issues = dataset
            .services
            .get_mut("github")
            .unwrap()
            .collection_mut("issues");
        let mut extra = issues.get("lumen-labs/checkout#213").unwrap().clone();
        extra["number"] = json!(240);
        extra["id"] = json!(40240);
        extra["_key"] = json!("lumen-labs/checkout#240");
        issues.insert("lumen-labs/checkout#240", extra);
    });
    open_issue(&predicted, ISSUE_TITLE, ISSUE_BODY).await;
    note(&predicted, PRODUCTION_INCIDENT, &tracking(214)).await;
    fail(&predicted);
}

#[tokio::test]
async fn escalation_rejects_wrong_repositories_duplicates_and_unrequested_issue_state() {
    for repository in ["payments-worker", "storefront"] {
        let trial = escalation_trial(|_| {});
        trial
            .call_from(
                "github",
                "POST",
                &format!("/api/v3/repos/lumen-labs/{repository}/issues"),
                json!({"title":ISSUE_TITLE,"body":ISSUE_BODY}),
            )
            .await;
        fail(&trial);
    }
    let duplicate = escalation_trial(|_| {});
    open_issue(&duplicate, ISSUE_TITLE, ISSUE_BODY).await;
    let number = open_issue(&duplicate, ISSUE_TITLE, ISSUE_BODY).await;
    note(&duplicate, PRODUCTION_INCIDENT, &tracking(number)).await;
    fail(&duplicate);
    let assigned = escalation_trial(|_| {});
    let number = assigned
        .call_from(
            "github",
            "POST",
            ISSUES,
            json!({"title":ISSUE_TITLE,"body":ISSUE_BODY,"assignees":["dmitri"]}),
        )
        .await["number"]
        .as_u64()
        .unwrap();
    note(&assigned, PRODUCTION_INCIDENT, &tracking(number)).await;
    fail(&assigned);
    let none = escalation_trial(|_| {});
    let grade = fail(&none);
    assert_failure_at(&grade, CheckKind::Records, CheckDisposition::Failed);
}

#[tokio::test]
async fn escalation_rejects_wrong_sentry_issue_incident_or_missing_source_facts() {
    let cases = [
        (
            "PAYMENTS-WORKER-1K: OperationalError: could not obtain connection from pool within 5.0s",
            "PagerDuty incident #4810\nSentry: PAYMENTS-WORKER-1K\nCulprit: payments_worker.tasks.capture in capture_payment\nError: OperationalError: could not obtain connection from pool within 5.0s",
        ),
        (
            "CHECKOUT-API-3C: KeyError: 'shipping_address'",
            "PagerDuty incident #4810\nSentry: CHECKOUT-API-3C\nCulprit: checkout.serializers.address in validate\nError: KeyError: 'shipping_address'",
        ),
        (
            "Fix CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s",
            ISSUE_BODY,
        ),
        (
            "checkout-api-3f: OperationalError: could not obtain connection from pool within 5.0s",
            ISSUE_BODY,
        ),
        (
            ISSUE_TITLE,
            "PagerDuty incident #4811\nSentry: CHECKOUT-API-3F\nCulprit: checkout.views.orders in create_order\nError: OperationalError: could not obtain connection from pool within 5.0s",
        ),
        (
            ISSUE_TITLE,
            "PagerDuty incident #48100\nSentry: CHECKOUT-API-3F\nCulprit: checkout.views.orders in create_order\nError: OperationalError: could not obtain connection from pool within 5.0s",
        ),
        (
            ISSUE_TITLE,
            "Sentry: CHECKOUT-API-3F\nCulprit: checkout.views.orders in create_order\nError: OperationalError: could not obtain connection from pool within 5.0s",
        ),
        (
            ISSUE_TITLE,
            "PagerDuty incident #4810\nSentry: CHECKOUT-API-3F\nError: OperationalError: could not obtain connection from pool within 5.0s",
        ),
        (
            ISSUE_TITLE,
            "PagerDuty incident #4810\nSentry: CHECKOUT-API-3F\nCulprit: checkout.views.orders in create_order",
        ),
        (
            ISSUE_TITLE,
            "PagerDuty incident #4810\nSentry: CHECKOUT-API-3F\nCulprit: payments_worker.tasks.capture in capture_payment\nError: OperationalError: could not obtain connection from pool within 5.0s",
        ),
        (ISSUE_TITLE, ""),
    ];
    for (title, body) in cases {
        let trial = escalation_trial(|_| {});
        issue_flow(&trial, title, body).await;
        fail(&trial);
    }
}

#[tokio::test]
async fn escalation_unlisted_issue_wording_passes_with_the_required_facts_and_fails_without() {
    for (title, body) in [
        (
            ISSUE_TITLE,
            "Incident 4810 happened in checkout.views.orders in create_order. Error could not obtain connection from pool within 5.0s. Sentry CHECKOUT-API-3F",
        ),
        (
            ISSUE_TITLE,
            "Paged by PagerDuty #4810. The culprit in Sentry is checkout.views.orders in create_order and the message is: could not obtain connection from pool within 5.0s",
        ),
        (
            "CHECKOUT-API-3F: connection pool exhausted in create_order",
            ISSUE_BODY,
        ),
        // Markdown code spans around the culprit do not change what the description says.
        (
            ISSUE_TITLE,
            "PagerDuty **#4810**\n\n- Culprit: `checkout.views.orders` in `create_order`\n- Message: `could not obtain connection from pool within 5.0s`",
        ),
    ] {
        let trial = escalation_trial(|_| {});
        issue_flow(&trial, title, body).await;
        pass(&trial);
    }
    for (title, body) in [
        (
            ISSUE_TITLE,
            "Incident 4811 happened in checkout.views.orders in create_order. Error could not obtain connection from pool within 5.0s. Sentry CHECKOUT-API-3F",
        ),
        (
            ISSUE_TITLE,
            "Incident 4810 happened in checkout.views.orders in create_order. Sentry CHECKOUT-API-3F",
        ),
        (
            ISSUE_TITLE,
            "Incident 4810 happened in checkout.serializers.address in validate. Error could not obtain connection from pool within 5.0s. Sentry CHECKOUT-API-3F",
        ),
        // The request asks only that the title start with the Sentry short ID.
        (
            "OperationalError: could not obtain connection from pool within 5.0s",
            ISSUE_BODY,
        ),
        (
            "CHECKOUT-API-3C: could not obtain connection from pool",
            ISSUE_BODY,
        ),
    ] {
        let trial = escalation_trial(|_| {});
        issue_flow(&trial, title, body).await;
        let grade = fail(&trial);
        assert!(!grade.adjudication_needed, "{grade:#?}");
    }
}

#[tokio::test]
async fn escalation_note_rejects_wrong_targets_numbers_and_unsupported_claims() {
    let staging = escalation_trial(|_| {});
    let number = open_issue(&staging, ISSUE_TITLE, ISSUE_BODY).await;
    note(&staging, STAGING_INCIDENT, &tracking(number)).await;
    fail(&staging);
    let both = escalation_trial(|_| {});
    let number = open_issue(&both, ISSUE_TITLE, ISSUE_BODY).await;
    note(&both, PRODUCTION_INCIDENT, &tracking(number)).await;
    note(&both, STAGING_INCIDENT, &tracking(number)).await;
    fail(&both);
    let twice = escalation_trial(|_| {});
    let number = open_issue(&twice, ISSUE_TITLE, ISSUE_BODY).await;
    note(&twice, PRODUCTION_INCIDENT, &tracking(number)).await;
    note(&twice, PRODUCTION_INCIDENT, &tracking(number)).await;
    fail(&twice);
    for wrong in [198_u64, 207, 213, 88, 64] {
        let trial = escalation_trial(|_| {});
        open_issue(&trial, ISSUE_TITLE, ISSUE_BODY).await;
        note(&trial, PRODUCTION_INCIDENT, &tracking(wrong)).await;
        fail(&trial);
    }
    let digitless = escalation_trial(|_| {});
    open_issue(&digitless, ISSUE_TITLE, ISSUE_BODY).await;
    note(
        &digitless,
        PRODUCTION_INCIDENT,
        "Opened the GitHub issue for this incident.",
    )
    .await;
    fail(&digitless);
    let missing = escalation_trial(|_| {});
    open_issue(&missing, ISSUE_TITLE, ISSUE_BODY).await;
    fail(&missing);
    let nonexistent = escalation_trial(|_| {});
    open_issue(&nonexistent, ISSUE_TITLE, ISSUE_BODY).await;
    note(&nonexistent, PRODUCTION_INCIDENT, &tracking(215)).await;
    fail(&nonexistent);
    let wrong_repository = escalation_trial(|_| {});
    open_issue(&wrong_repository, ISSUE_TITLE, ISSUE_BODY).await;
    note(
        &wrong_repository,
        PRODUCTION_INCIDENT,
        "Tracking the fix in GitHub issue lumen-labs/payments-worker#88.",
    )
    .await;
    fail(&wrong_repository);
    let claims = escalation_trial(|_| {});
    open_issue(&claims, ISSUE_TITLE, ISSUE_BODY).await;
    note(
        &claims,
        PRODUCTION_INCIDENT,
        "Fixed and deployed for incident 4810; the payments team was paged.",
    )
    .await;
    fail(&claims);
}

#[tokio::test]
async fn escalation_note_before_the_issue_exists_fails_even_if_the_predicted_number_matches() {
    let trial = escalation_trial(|_| {});
    note(&trial, PRODUCTION_INCIDENT, &tracking(214)).await;
    let number = open_issue(&trial, ISSUE_TITLE, ISSUE_BODY).await;
    assert_eq!(number, 214);
    let grade = trial.assess();
    assert_ne!(grade.outcome, Outcome::Pass, "{grade:#?}");
    assert_failure_at(&grade, CheckKind::Ordering, CheckDisposition::Failed);
}

#[tokio::test]
async fn escalation_rejects_status_changes_edits_and_unrelated_comments_even_when_undone() {
    let acknowledged = escalation_trial(|_| {});
    issue_flow(&acknowledged, ISSUE_TITLE, ISSUE_BODY).await;
    let incident =
        acknowledged.world.lock().unwrap().snapshot()["services"]["pagerduty"]["incidents"]
            [PRODUCTION_INCIDENT]
            .clone();
    acknowledged
        .call_from(
            "pagerduty",
            "PUT",
            "/incidents",
            json!({"incidents":[{"id":PRODUCTION_INCIDENT,"type":"incident_reference","status":"acknowledged"}]}),
        )
        .await;
    fail(&acknowledged);
    let urgency = escalation_trial(|_| {});
    issue_flow(&urgency, ISSUE_TITLE, ISSUE_BODY).await;
    for level in ["low", "high"] {
        urgency
            .call_from(
                "pagerduty",
                "PUT",
                "/incidents",
                json!({"incidents":[{"id":PRODUCTION_INCIDENT,"type":"incident_reference","urgency":level}]}),
            )
            .await;
    }
    assert_eq!(
        urgency.world.lock().unwrap().snapshot()["services"]["pagerduty"]["incidents"]
            [PRODUCTION_INCIDENT]["urgency"],
        incident["urgency"]
    );
    fail(&urgency);
    let resolved = escalation_trial(|_| {});
    issue_flow(&resolved, ISSUE_TITLE, ISSUE_BODY).await;
    resolved
        .call_from(
            "sentry",
            "PUT",
            "/api/0/organizations/lumen-labs/issues/CHECKOUT-API-3F/",
            json!({"status":"resolved"}),
        )
        .await;
    resolved
        .call_from(
            "sentry",
            "PUT",
            "/api/0/organizations/lumen-labs/issues/CHECKOUT-API-3F/",
            json!({"status":"unresolved"}),
        )
        .await;
    fail(&resolved);
    let existing = escalation_trial(|_| {});
    issue_flow(&existing, ISSUE_TITLE, ISSUE_BODY).await;
    existing
        .call_from(
            "github",
            "PATCH",
            &format!("{ISSUES}/207"),
            json!({"body":"Linked to the pool timeouts."}),
        )
        .await;
    fail(&existing);
    let comment = escalation_trial(|_| {});
    issue_flow(&comment, ISSUE_TITLE, ISSUE_BODY).await;
    comment
        .call_from(
            "github",
            "POST",
            &format!("{ISSUES}/207/comments"),
            json!({"body":"Likely the same pool problem."}),
        )
        .await;
    fail(&comment);
}

#[tokio::test]
async fn escalation_accepts_staged_completion_and_graphql_issue_creation() {
    let staged = escalation_trial(|_| {});
    let number = open_issue(&staged, ISSUE_TITLE, "").await;
    staged
        .call_from(
            "github",
            "PATCH",
            &format!("{ISSUES}/{number}"),
            json!({"body":ISSUE_BODY}),
        )
        .await;
    note(&staged, PRODUCTION_INCIDENT, &tracking(number)).await;
    pass(&staged);

    // An interim description of the new issue that states nothing known to be wrong is a draft,
    // not a claim: only the final description must carry the required facts.
    let revised = escalation_trial(|_| {});
    let number = issue_flow(&revised, ISSUE_TITLE, ISSUE_BODY).await;
    for (body, complete) in [("Edited body", false), (ISSUE_BODY, true)] {
        revised
            .call_from(
                "github",
                "PATCH",
                &format!("{ISSUES}/{number}"),
                json!({"body":body}),
            )
            .await;
        if complete {
            pass(&revised);
        } else {
            fail(&revised);
        }
    }

    let graphql = escalation_trial(|_| {});
    let repository = graphql
        .call_from(
            "github",
            "POST",
            "/api/graphql",
            json!({"query":"query{repository(owner:\"lumen-labs\",name:\"checkout\"){id}}"}),
        )
        .await;
    let created = graphql
        .call_from(
            "github",
            "POST",
            "/api/graphql",
            json!({
                "query":"mutation($input:CreateIssueInput!){createIssue(input:$input){issue{number}}}",
                "variables":{"input":{
                    "repositoryId":repository["data"]["repository"]["id"],
                    "title":ISSUE_TITLE,"body":ISSUE_BODY
                }}
            }),
        )
        .await;
    let number = created["data"]["createIssue"]["issue"]["number"]
        .as_u64()
        .unwrap();
    note(&graphql, PRODUCTION_INCIDENT, &tracking(number)).await;
    pass(&graphql);
}

#[tokio::test]
async fn escalation_creation_effects_stay_narrow() {
    let trial = escalation_trial(|_| {});
    issue_flow(&trial, ISSUE_TITLE, ISSUE_BODY).await;
    pass(&trial);
    let world = trial.world.lock().unwrap();
    let initial = world.initial_snapshot();
    let state = world.snapshot();
    for (service, collection) in [
        ("sentry", "issues"),
        ("pagerduty", "alerts"),
        ("pagerduty", "services"),
        ("github", "repos"),
        ("github", "issue_comments"),
    ] {
        assert_eq!(
            initial["services"][service][collection], state["services"][service][collection],
            "{service}/{collection}"
        );
    }
    let created: Vec<_> = state["services"]["github"]["issues"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| initial["services"]["github"]["issues"].get(*key).is_none())
        .collect();
    assert_eq!(created, ["lumen-labs/checkout#214"]);
    let notes: Vec<_> = state["services"]["pagerduty"]["notes"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| {
            initial["services"]["pagerduty"]["notes"]
                .get(*key)
                .is_none()
        })
        .collect();
    assert_eq!(notes, ["Q7CHKAP15PCT1D/1"]);
}

#[test]
fn both_scenarios_pass_the_repository_scenario_validation_including_oracle_arguments() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(&root, &catalog).unwrap();
    for id in [ANNOTATE, ESCALATE] {
        let scenario = scenario::Scenario::load(&root.join("scenarios").join(id)).unwrap();
        scenario
            .validate(&root.join("scenarios"), &catalog, &tools)
            .unwrap();
        assert_eq!(scenario.expect.version, 2);
        assert!(scenario.expect.audit_history);
    }
}
