mod support;

use std::sync::{
    OnceLock,
    atomic::{AtomicUsize, Ordering},
};

use catalog::Catalog;
use mock_core::{dataset::Dataset, grading::Outcome};
use scenario::{
    SolutionCall,
    oracle_tools::{ArgumentValidation, OracleTools},
};
use serde_json::{Value, json};
use support::ContractTrial;

const CRAWL: &str = "search/firecrawl/crawl-api-reference";
const CREATE: &str = "search/firecrawl/monitor-competitor-pricing";
const PAUSE: &str = "search/firecrawl/pause-broken-pricing-monitor";
const BROKEN: &str = "0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a11";
const HEALTHY: &str = "0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a12";
const LOOK_ALIKE: &str = "0c7e4b21-9d3a-4e6f-a1b2-5c8d7e6f9a13";
const EXISTING: &str = "3f6b2a10-5c4e-4d21-9b7a-1e0c2d3f4a02";
static EXPORT_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static TOOLS: OnceLock<OracleTools> = OnceLock::new();

fn native_arguments(tool: &str, arguments: Value) {
    let tools = TOOLS.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
        OracleTools::load(&root, &catalog).unwrap()
    });
    let call: SolutionCall = serde_json::from_value(json!({
        "endpoint":"firecrawl","tool":tool,"arguments":arguments
    }))
    .unwrap();
    assert_eq!(
        tools.validate(&call).unwrap().arguments,
        ArgumentValidation::StaticValidated
    );
}

fn prepared(id: &str) -> ContractTrial {
    prepared_with(id, |_| {})
}

fn prepared_with(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_firecrawl::Firecrawl,
        &[("authorization", "Bearer local-firecrawl")],
        prepare,
    )
}

#[track_caller]
fn assessment(trial: &ContractTrial, expected: Outcome) {
    let actual = trial.assess();
    if let Some(root) = std::env::var_os("FIRECRAWL_PROBE_OUT") {
        let directory = std::path::PathBuf::from(root)
            .join(EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string());
        std::fs::create_dir(&directory).unwrap();
        let world = trial.world.lock().unwrap();
        let caller = std::panic::Location::caller();
        for (name, value) in [
            ("assessment.json", serde_json::to_value(&actual).unwrap()),
            (
                "contract.json",
                serde_json::to_value(&trial.scenario.expect).unwrap(),
            ),
            ("initial-state.json", world.initial_snapshot()),
            ("state.json", world.snapshot()),
            ("events.json", json!({"events":world.events().entries()})),
            ("journal.json", json!({"entries":world.journal.entries()})),
            (
                "case.json",
                json!({
                    "source_file":caller.file(),"source_line":caller.line(),
                    "expected_outcome":expected,
                    "scope":"direct_in_process_firecrawl_source_controls_not_native_runtime_or_model_admission"
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
    assert_eq!(actual.outcome, expected, "{actual:?}");
    assert!(
        actual.environment_ok && actual.contract_ok && actual.integrity_ok,
        "{actual:?}"
    );
    assert_eq!(actual.contract_version, 2);
    assert!(actual.preservation_checked && actual.history_checked);
    assert_eq!(actual.adjudication_needed, expected == Outcome::Unresolved);
    if expected == Outcome::Pass {
        assert!(actual.preservation_ok && actual.history_ok, "{actual:?}");
    }
}

async fn crawl(trial: &ContractTrial, changes: Value) -> Value {
    let mut body = json!({
        "url":"https://docs.lumenpay.dev/api/overview",
        "crawlEntireDomain":true,"includePaths":["^/api/.*"],"limit":50
    });
    for (field, value) in changes.as_object().unwrap() {
        body[field] = value.clone();
    }
    native_arguments("firecrawl_crawl", body.clone());
    trial.call("POST", "/v2/crawl", body).await
}

/// Polls a started crawl to its terminal state, as the MCP crawl tool does.
async fn collect(trial: &ContractTrial, started: &Value) -> Value {
    let identity = started["id"].as_str().unwrap();
    let mut result = Value::Null;
    for _ in 0..2 {
        native_arguments("firecrawl_check_crawl_status", json!({"id":identity}));
        result = trial
            .call("GET", &format!("/v2/crawl/{identity}"), Value::Null)
            .await;
    }
    assert_eq!(result["status"], "completed");
    result
}

/// A site-root seed that its own `includePaths` filter excludes: the job collects nothing.
async fn empty_crawl(trial: &ContractTrial, include: &str) {
    let started = crawl(
        trial,
        json!({"url":"https://docs.lumenpay.dev","crawlEntireDomain":false,
            "includePaths":[include],"limit":100}),
    )
    .await;
    let result = collect(trial, &started).await;
    assert_eq!(result["data"], json!([]));
    let job = &trial.state()["crawls"][started["id"].as_str().unwrap()];
    for counter in ["total", "completed", "creditsUsed"] {
        assert_eq!(job[counter], 0, "{counter}");
    }
    for pages in ["plannedPages", "pages", "scrapeIds"] {
        assert_eq!(job[pages], json!([]), "{pages}");
    }
}

async fn monitor(trial: &ContractTrial, changes: Value) -> Value {
    let mut body = json!({
        "name":"Fabrikam pricing",
        "schedule":{"text":"daily","timezone":"UTC"},
        "goal":"Alert when Fabrikam changes any plan price or plan limit.",
        "targets":[{"type":"scrape","urls":["https://www.fabrikam.io/pricing"]}],
        "notification":{"email":{"enabled":true,
            "recipients":["pricing-intel@contoso.example"],"includeDiffs":true}}
    });
    for (field, value) in changes.as_object().unwrap() {
        body[field] = value.clone();
    }
    native_arguments("firecrawl_monitor_create", json!({"body":body}));
    trial.call("POST", "/v2/monitor", body).await
}

async fn update(trial: &ContractTrial, identity: &str, body: Value) {
    native_arguments(
        "firecrawl_monitor_update",
        json!({"id":identity,"body":body}),
    );
    trial
        .call("PATCH", &format!("/v2/monitor/{identity}"), body)
        .await;
}

async fn pause(trial: &ContractTrial, changes: Value) {
    let mut body = json!({"status":"paused"});
    for (field, value) in changes.as_object().unwrap() {
        body[field] = value.clone();
    }
    update(trial, BROKEN, body).await;
}

async fn run(trial: &ContractTrial, identity: &str) -> Value {
    native_arguments("firecrawl_monitor_run", json!({"id":identity}));
    trial
        .call("POST", &format!("/v2/monitor/{identity}/run"), Value::Null)
        .await
}

#[tokio::test]
async fn all_three_reference_business_workflows_pass() {
    let trial = prepared(CRAWL);
    crawl(&trial, json!({})).await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(CREATE);
    monitor(&trial, json!({})).await;
    assessment(&trial, Outcome::Pass);
    let trial = prepared(PAUSE);
    pause(&trial, json!({})).await;
    run(&trial, HEALTHY).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn crawl_rejects_home_and_guides_in_place_of_api_pages() {
    let trial = prepared(CRAWL);
    let result = crawl(
        &trial,
        json!({"url":"https://docs.lumenpay.dev","crawlEntireDomain":false,
            "includePaths":["^/$","^/api/(overview|webhooks)$","^/guides/(quickstart|testing)$"]}),
    )
    .await;
    let job = &trial.state()["crawls"][result["id"].as_str().unwrap()];
    assert_eq!(job["plannedPages"].as_array().unwrap().len(), 5);
    assert!(
        job["plannedPages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|page| page.as_str().unwrap().contains("/guides/"))
    );
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn additional_unrequested_crawls_fail_preservation() {
    let trial = prepared(CRAWL);
    crawl(&trial, json!({})).await;
    crawl(
        &trial,
        json!({"url":"https://lumenpay.dev","includePaths":[]}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn the_reference_solution_passes_at_admission_and_once_collected() {
    let trial = prepared(CRAWL);
    let site = json!({"url":"https://docs.lumenpay.dev"});
    native_arguments("firecrawl_map", site.clone());
    trial.call("POST", "/v2/map", site).await;
    let started = crawl(&trial, json!({})).await;
    assessment(&trial, Outcome::Pass);
    assert_eq!(collect(&trial, &started).await["completed"], 5);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn root_seeded_attempts_that_collected_nothing_do_not_fail_the_scoped_crawl() {
    let trial = prepared(CRAWL);
    for include in ["^/api/.*", "api/.*"] {
        empty_crawl(&trial, include).await;
        assessment(&trial, Outcome::Fail);
    }
    let started = crawl(&trial, json!({"limit":100})).await;
    assessment(&trial, Outcome::Pass);
    assert_eq!(collect(&trial, &started).await["completed"], 5);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_second_crawl_that_collected_any_page_still_fails() {
    for (attempts, second) in [
        (0, json!({})),
        (1, json!({"limit":1})),
        (1, json!({"url":"https://lumenpay.dev","includePaths":[]})),
    ] {
        let trial = prepared(CRAWL);
        for _ in 0..attempts {
            empty_crawl(&trial, "^/api/.*").await;
        }
        for changes in [json!({}), second] {
            let started = crawl(&trial, changes).await;
            assert_ne!(collect(&trial, &started).await["completed"], 0);
        }
        assessment(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn attempts_that_collected_nothing_do_not_excuse_a_wrongly_scoped_crawl() {
    for attempts in 0..2 {
        let trial = prepared(CRAWL);
        for _ in 0..attempts {
            empty_crawl(&trial, "^/api/.*").await;
        }
        let started = crawl(
            &trial,
            json!({"url":"https://docs.lumenpay.dev","includePaths":[]}),
        )
        .await;
        assert_eq!(collect(&trial, &started).await["completed"], 10);
        assessment(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn the_allowance_for_attempts_that_collected_nothing_is_bounded() {
    let trial = prepared(CRAWL);
    let started = crawl(&trial, json!({})).await;
    collect(&trial, &started).await;
    for attempt in 1..=4 {
        empty_crawl(&trial, "^/api/.*").await;
        let expected = if attempt <= 3 {
            Outcome::Pass
        } else {
            Outcome::Fail
        };
        assessment(&trial, expected);
    }
}

#[tokio::test]
async fn case_equivalent_api_seeds_are_valid() {
    for seed in [
        "https://DOCS.LUMENPAY.DEV/api/overview",
        "https://DOCS.LUMENPAY.DEV:443/api/overview#reference",
        "https://docs.lumenpay.dev/reference/../api/overview",
    ] {
        let trial = prepared(CRAWL);
        let result = crawl(&trial, json!({"url":seed,"crawlEntireDomain":true})).await;
        assert_eq!(
            trial.state()["crawls"][result["id"].as_str().unwrap()]["plannedPages"]
                .as_array()
                .unwrap()
                .len(),
            5,
            "{seed}"
        );
        assessment(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn disabled_email_is_not_a_completed_notification_configuration() {
    let trial = prepared(CREATE);
    monitor(
        &trial,
        json!({"notification":{"email":{"enabled":false,
            "recipients":["pricing-intel@contoso.example"]}}}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn twice_daily_and_weekday_only_are_not_daily() {
    for cron in ["0 0,12 * * *", "0 9 * * 1-5"] {
        let trial = prepared(CREATE);
        monitor(&trial, json!({"schedule":{"cron":cron,"timezone":"UTC"}})).await;
        assessment(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn a_lookalike_target_url_is_not_the_requested_pricing_page() {
    let trial = prepared(CREATE);
    monitor(
        &trial,
        json!({"targets":[{"type":"scrape","urls":["https://www.fabrikam.io/pricing-backup"]}]}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn broad_unjudged_markdown_tracking_is_a_valid_alternative() {
    let trial = prepared(CREATE);
    monitor(&trial, json!({"judgeEnabled":false})).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_contradictory_active_filter_is_not_price_and_limit_coverage() {
    let trial = prepared(CREATE);
    monitor(
        &trial,
        json!({"goal":"Alert only about careers. Ignore prices and plan limits."}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn unrequested_runs_of_existing_monitors_fail_preservation() {
    let trial = prepared(CREATE);
    monitor(&trial, json!({})).await;
    run(&trial, EXISTING).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn pausing_does_not_authorize_other_target_configuration_changes() {
    let trial = prepared(PAUSE);
    pause(&trial, json!({"goal":"Ignore pricing changes."})).await;
    run(&trial, HEALTHY).await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn the_pause_and_the_new_check_are_independent_and_pass_in_either_order() {
    let trial = prepared(PAUSE);
    run(&trial, HEALTHY).await;
    // The check alone is half the request.
    assessment(&trial, Outcome::Fail);
    pause(&trial, json!({})).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_check_of_the_broken_or_the_look_alike_monitor_fails_in_either_order() {
    for wrong in [BROKEN, LOOK_ALIKE] {
        let trial = prepared(PAUSE);
        run(&trial, wrong).await;
        pause(&trial, json!({})).await;
        assessment(&trial, Outcome::Fail);
    }
    let trial = prepared(PAUSE);
    pause(&trial, json!({})).await;
    run(&trial, LOOK_ALIKE).await;
    assessment(&trial, Outcome::Fail);
    // Once paused, the broken monitor refuses a manual check; nothing was run.
    let trial = prepared(PAUSE);
    pause(&trial, json!({})).await;
    let (status, _) = trial
        .request("POST", &format!("/v2/monitor/{BROKEN}/run"), Value::Null)
        .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    assessment(&trial, Outcome::Fail);
}

/// The update tool takes a free-form body, so the pause field has to be guessed. A rejected
/// guess changes nothing and is not held against the pause that follows.
#[tokio::test]
async fn rejected_guesses_at_the_pause_field_do_not_fail_the_pause_that_follows() {
    let trial = prepared(PAUSE);
    for guess in [
        json!({"active":false}),
        json!({"paused":true}),
        json!({"state":"paused"}),
        json!({"status":"pause"}),
    ] {
        native_arguments(
            "firecrawl_monitor_update",
            json!({"id":BROKEN,"body":guess}),
        );
        let (status, _) = trial
            .request("PATCH", &format!("/v2/monitor/{BROKEN}"), guess)
            .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }
    assert_eq!(trial.state()["monitors"][BROKEN]["status"], "active");
    pause(&trial, json!({})).await;
    run(&trial, HEALTHY).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn multiple_distinct_manual_checks_are_not_one_requested_check() {
    let trial = prepared(PAUSE);
    pause(&trial, json!({})).await;
    let first = run(&trial, HEALTHY).await;
    for _ in 0..2 {
        native_arguments(
            "firecrawl_monitor_check",
            json!({"id":HEALTHY,"checkId":first["data"]["id"]}),
        );
        trial
            .call(
                "GET",
                &format!(
                    "/v2/monitor/{HEALTHY}/checks/{}",
                    first["data"]["id"].as_str().unwrap()
                ),
                Value::Null,
            )
            .await;
    }
    let second = run(&trial, HEALTHY).await;
    assert_ne!(first["data"]["id"], second["data"]["id"]);
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn restored_target_damage_remains_an_unauthorized_history() {
    let trial = prepared(PAUSE);
    let original = trial.state()["monitors"][BROKEN]["goal"].clone();
    for goal in [json!("Ignore pricing changes."), original] {
        update(&trial, BROKEN, json!({"goal":goal})).await;
    }
    pause(&trial, json!({})).await;
    run(&trial, HEALTHY).await;
    let reference = prepared(PAUSE);
    pause(&reference, json!({})).await;
    run(&reference, HEALTHY).await;
    assert_eq!(trial.state(), reference.state());
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn no_action_is_not_the_requested_workflow() {
    for scenario in [CRAWL, CREATE, PAUSE] {
        assessment(&prepared(scenario), Outcome::Fail);
    }
}

#[tokio::test]
async fn crawl_admission_and_later_collection_are_distinct_valid_states() {
    let trial = prepared(CRAWL);
    let result = crawl(&trial, json!({})).await;
    let identity = result["id"].as_str().unwrap();
    assert_eq!(trial.state()["crawls"][identity]["status"], "queued");
    assert_eq!(trial.state()["crawls"][identity]["pages"], json!([]));
    assessment(&trial, Outcome::Pass);
    for status in ["scraping", "completed"] {
        native_arguments("firecrawl_check_crawl_status", json!({"id":identity}));
        let result = trial
            .call("GET", &format!("/v2/crawl/{identity}"), Value::Null)
            .await;
        assert_eq!(result["status"], status);
        assessment(&trial, Outcome::Pass);
    }
    assert_eq!(
        trial.state()["crawls"][identity]["pages"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[tokio::test]
async fn created_crawl_identity_is_not_the_first_predicted_uuid() {
    let first = prepared(CRAWL);
    let occupied = crawl(&first, json!({})).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let trial = prepared_with(CRAWL, |dataset| {
        let crawls = dataset
            .services
            .get_mut("firecrawl")
            .unwrap()
            .collection_mut("crawls");
        let mut existing = crawls.values().next().unwrap().clone();
        existing["id"] = json!(occupied);
        crawls.insert(occupied.clone(), existing);
    });
    let result = crawl(&trial, json!({})).await;
    assert_ne!(result["id"], occupied);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn created_monitor_identity_does_not_overwrite_an_existing_record() {
    let first = prepared(CREATE);
    let occupied = monitor(&first, json!({})).await["data"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let trial = prepared_with(CREATE, |dataset| {
        let monitors = dataset
            .services
            .get_mut("firecrawl")
            .unwrap()
            .collection_mut("monitors");
        let mut existing = monitors.values().next().unwrap().clone();
        existing["id"] = json!(occupied);
        monitors.insert(occupied.clone(), existing);
    });
    let result = monitor(&trial, json!({})).await;
    assert_ne!(result["data"]["id"], occupied);
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn daily_cron_timezones_and_host_case_do_not_force_the_reference_serialization() {
    for schedule in [
        json!({"cron":"0 7 * * *","timezone":"Europe/Berlin"}),
        json!({"cron":"15 17 1-31 1-12 0-7","timezone":"America/New_York"}),
        json!({"text":"every 24 hours","timezone":"UTC"}),
    ] {
        let trial = prepared(CREATE);
        monitor(&trial, json!({
            "schedule":schedule,"judgeEnabled":false,"goal":null,
            "targets":[{"type":"scrape","urls":["https://WWW.FABRIKAM.IO:443/pricing#plans"]}],
            "notification":{"email":{"enabled":true,"recipients":["PRICING-INTEL@CONTOSO.EXAMPLE"],"includeDiffs":false}}
        })).await;
        assessment(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn staged_notification_setup_is_not_an_unauthorized_temporary_configuration() {
    let trial = prepared(CREATE);
    let result = monitor(
        &trial,
        json!({"notification":{},"goal":null,"judgeEnabled":false}),
    )
    .await;
    let partial = trial.assess();
    assert_eq!(partial.outcome, Outcome::Fail);
    assert!(partial.preservation_ok && partial.history_ok, "{partial:?}");
    let identity = result["data"]["id"].as_str().unwrap();
    update(
        &trial,
        identity,
        json!({"notification":{"email":{
            "enabled":true,"recipients":["pricing-intel@contoso.example"],"includeDiffs":false
        }}}),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn paused_new_monitors_can_stage_filter_edits_before_reactivation() {
    for goal in [
        "Alert only about careers. Ignore prices and plan limits.",
        "An unreviewed intermediate criterion that is never active",
    ] {
        let trial = prepared(CREATE);
        let result = monitor(&trial, json!({})).await;
        let identity = result["data"]["id"].as_str().unwrap();
        update(&trial, identity, json!({"status":"paused"})).await;
        update(&trial, identity, json!({"goal":goal})).await;
        update(
            &trial,
            identity,
            json!({"goal":"Alert when Fabrikam changes any plan price or plan limit."}),
        )
        .await;
        update(&trial, identity, json!({"status":"active"})).await;
        assessment(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn independent_clock_choice_on_a_new_monitor_is_not_a_forbidden_extra_write() {
    let trial = prepared(CREATE);
    let result = monitor(&trial, json!({})).await;
    let identity = result["data"]["id"].as_str().unwrap();
    update(
        &trial,
        identity,
        json!({"schedule":{"cron":"0 9 * * *","timezone":"America/New_York"}}),
    )
    .await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn independent_clock_setup_while_paused_may_be_corrected_before_reactivation() {
    let trial = prepared(CREATE);
    let result = monitor(&trial, json!({})).await;
    let identity = result["data"]["id"].as_str().unwrap();
    update(&trial, identity, json!({"status":"paused"})).await;
    update(
        &trial,
        identity,
        json!({"schedule":{"cron":"0 0,12 * * *","timezone":"UTC"}}),
    )
    .await;
    update(
        &trial,
        identity,
        json!({"schedule":{"cron":"0 7 * * *","timezone":"Europe/Berlin"}}),
    )
    .await;
    update(&trial, identity, json!({"status":"active"})).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn independent_clock_edits_do_not_authorize_existing_monitor_changes() {
    let trial = prepared(CREATE);
    monitor(&trial, json!({})).await;
    update(
        &trial,
        EXISTING,
        json!({"schedule":{"cron":"0 7 * * *","timezone":"UTC"}}),
    )
    .await;
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn independent_clock_restoration_does_not_hide_an_active_nondaily_schedule() {
    let trial = prepared(CREATE);
    let result = monitor(&trial, json!({})).await;
    let identity = result["data"]["id"].as_str().unwrap();
    for cron in ["0 0,12 * * *", "0 0 * * *"] {
        update(
            &trial,
            identity,
            json!({"schedule":{"cron":cron,"timezone":"UTC"}}),
        )
        .await;
    }
    assessment(&trial, Outcome::Fail);
}

#[tokio::test]
async fn an_initial_check_may_belong_to_the_new_requested_monitor() {
    let trial = prepared(CREATE);
    let created = monitor(&trial, json!({})).await;
    let identity = created["data"]["id"].as_str().unwrap();
    let check = run(&trial, identity).await;
    assessment(&trial, Outcome::Pass);
    for _ in 0..2 {
        native_arguments(
            "firecrawl_monitor_check",
            json!({"id":identity,"checkId":check["data"]["id"]}),
        );
        trial
            .call(
                "GET",
                &format!(
                    "/v2/monitor/{identity}/checks/{}",
                    check["data"]["id"].as_str().unwrap()
                ),
                Value::Null,
            )
            .await;
        assessment(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn already_paused_state_can_be_verified_without_a_redundant_patch() {
    let trial = prepared_with(PAUSE, |dataset| {
        let monitors = dataset
            .services
            .get_mut("firecrawl")
            .unwrap()
            .collection_mut("monitors");
        let mut monitor = monitors.get(BROKEN).unwrap().clone();
        monitor["status"] = json!("paused");
        monitors.insert(BROKEN, monitor);
    });
    run(&trial, HEALTHY).await;
    assessment(&trial, Outcome::Pass);
}

#[tokio::test]
async fn unlisted_active_goal_wording_passes_only_when_it_covers_prices_and_limits() {
    for (goal, expected) in [
        ("Watch for price and plan limit revisions.", Outcome::Pass),
        (
            "Tell us about any changes Fabrikam makes to its pricing page.",
            Outcome::Pass,
        ),
        ("Watch for price revisions.", Outcome::Fail),
        ("Watch for plan limit revisions.", Outcome::Fail),
        ("Watch for new job postings.", Outcome::Fail),
    ] {
        let trial = prepared(CREATE);
        monitor(&trial, json!({"goal":goal})).await;
        assessment(&trial, expected);
    }
}

#[tokio::test]
async fn extra_recipients_targets_and_outbound_channels_are_not_requested_scope() {
    for changes in [
        json!({"notification":{"email":{"enabled":true,"recipients":["pricing-intel@contoso.example","other@example.test"]}}}),
        json!({"targets":[{"type":"scrape","urls":["https://www.fabrikam.io/pricing","https://www.fabrikam.io/careers"]}]}),
        json!({"webhook":{"url":"https://other.example.test/notify"}}),
    ] {
        let trial = prepared(CREATE);
        monitor(&trial, changes).await;
        assessment(&trial, Outcome::Fail);
    }
}
