use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

fn initial() -> Value {
    json!({"services":{"firecrawl":{
        "pages":{
            "https://docs.example/api/one":{"markdown":"one"},
            "https://docs.example/api/two":{"markdown":"two"},
            "https://docs.example/guides/one":{"markdown":"guide"},
            "https://marketing.example/api/one":{"markdown":"marketing"}
        },"crawls":{},"monitors":{},"monitor_checks":{}
    }}})
}

fn crawl_goal(extra: Value) -> RecordCheck {
    let mut value = json!({
        "selector":{"collection":"/services/firecrawl/crawls","created":true},"equals":1,
        "firecrawl_crawl":{"origin":"https://docs.example","path_prefix":"/api/"}
    });
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    serde_json::from_value(value).unwrap()
}

fn job(changes: Value) -> Value {
    let mut value = json!({
        "id":"actual-opaque-crawl","url":"https://DOCS.EXAMPLE/api/one#section",
        "status":"queued","total":2,"completed":0,"pages":[],
        "plannedPages":["https://docs.example/api/one","https://docs.example/api/two"]
    });
    for (key, field) in changes.as_object().unwrap() {
        value[key] = field.clone();
    }
    value
}

fn with_job(job: Value) -> Value {
    let mut state = initial();
    state["services"]["firecrawl"]["crawls"]["actual-opaque-crawl"] = job;
    state
}

fn monitor_goal(extra: Value) -> RecordCheck {
    let mut value = json!({
        "selector":{"collection":"/services/firecrawl/monitors","created":true},"equals":1,
        "firecrawl_monitor":{
            "name":"Pricing","url":"https://www.vendor.example/pricing",
            "recipient":"pricing@example.test",
            "judged_goal":{"requirement":"Retain price and plan-limit changes",
                "accepted":["Track prices and plan limits."],"rejected":["Ignore prices and plan limits."]}
        }
    });
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    serde_json::from_value(value).unwrap()
}

fn monitor(changes: Value) -> Value {
    let mut value = json!({
        "id":"actual-opaque-monitor","name":"Pricing","status":"active",
        "schedule":{"cron":"0 9 * * *","timezone":"America/New_York"},
        "targets":[{"type":"scrape","urls":["https://WWW.VENDOR.EXAMPLE:443/pricing#plans"]}],
        "notification":{"email":{"enabled":true,"recipients":["PRICING@EXAMPLE.TEST"],"includeDiffs":false}},
        "goal":"Track prices and plan limits.","judgeEnabled":true,
        "lastCheckId":null,"lastRunAt":null
    });
    for (key, field) in changes.as_object().unwrap() {
        value[key] = field.clone();
    }
    value
}

fn with_monitor(record: Value) -> Value {
    let mut state = initial();
    state["services"]["firecrawl"]["monitors"]["actual-opaque-monitor"] = record;
    state
}

#[test]
fn admitted_crawl_scope_does_not_invent_completed_collection() {
    let goal = crawl_goal(json!({}));
    goal.check(Some(&initial()), &with_job(job(json!({}))))
        .unwrap();
    for changes in [
        json!({"status":"scraping"}),
        json!({"status":"completed","completed":2,
            "pages":["https://docs.example/api/two","https://docs.example/api/one"]}),
    ] {
        goal.check(Some(&initial()), &with_job(job(changes)))
            .unwrap();
    }
}

#[test]
fn exact_scope_rejects_mixed_duplicate_and_wrong_site_resources() {
    let goal = crawl_goal(json!({}));
    for changes in [
        json!({"plannedPages":["https://docs.example/api/one","https://docs.example/guides/one"]}),
        json!({"plannedPages":["https://docs.example/api/one","https://docs.example/api/one"]}),
        json!({"url":"https://docs.example.attacker.test/api/one"}),
        json!({"total":3}),
        json!({"total":"2"}),
        json!({"status":"completed","pages":[]}),
        json!({"pages":["https://marketing.example/api/one"],"completed":1}),
    ] {
        assert!(matches!(
            goal.check(Some(&initial()), &with_job(job(changes))),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn expected_api_scope_comes_from_authoritative_initial_pages() {
    let goal = crawl_goal(json!({}));
    let mut baseline = initial();
    baseline["services"]["firecrawl"]["pages"]["https://unrelated.example/api/unrelated"] =
        json!({});
    goal.check(Some(&baseline), &with_job(job(json!({}))))
        .unwrap();
    baseline["services"]["firecrawl"]["pages"]["https://docs.example/api/three"] = json!({});
    assert!(
        goal.check(Some(&baseline), &with_job(job(json!({}))))
            .is_err()
    );
}

#[test]
fn missing_admission_scope_evidence_is_not_an_ordinary_business_failure() {
    let goal = crawl_goal(json!({}));
    let mut record = job(json!({}));
    record.as_object_mut().unwrap().remove("plannedPages");
    assert!(matches!(
        goal.check(Some(&initial()), &with_job(record)),
        Err(RecordCheckError::MissingEvidence(_))
    ));
}

#[test]
fn broad_unjudged_tracking_does_not_require_filter_prose_or_diffs() {
    let goal = monitor_goal(json!({}));
    for changes in [
        json!({}),
        json!({"judgeEnabled":false,"goal":null}),
        json!({"judgeEnabled":false,"goal":"A broader whole-page monitor"}),
        json!({"schedule":{"text":"daily at 7am","timezone":"Europe/Berlin"}}),
    ] {
        goal.check(Some(&initial()), &with_monitor(monitor(changes)))
            .unwrap();
    }
}

#[test]
fn wrong_configuration_fails_but_unreviewed_filtering_remains_unresolved() {
    let goal = monitor_goal(json!({}));
    for changes in [
        json!({"notification":{"email":{"enabled":false,"recipients":["pricing@example.test"]}}}),
        json!({"notification":{"email":{"enabled":true,"recipients":["other@example.test"]}}}),
        json!({"goal":"Ignore prices and plan limits."}),
        json!({"webhook":{"url":"https://other.example/notifications"}}),
        json!({"schedule":{"cron":"0 0,12 * * *","timezone":"UTC"}}),
        json!({"status":"paused"}),
    ] {
        assert!(matches!(
            goal.check(Some(&initial()), &with_monitor(monitor(changes))),
            Err(RecordCheckError::Failed(_))
        ));
    }
    assert!(matches!(
        goal.check(
            Some(&initial()),
            &with_monitor(monitor(json!({"goal":"Unreviewed plausible filtering"})))
        ),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn initial_manual_checks_must_be_real_new_children() {
    let goal = monitor_goal(json!({}));
    let mut state = with_monitor(monitor(
        json!({"lastCheckId":"actual-check","lastRunAt":"2026-10-28T16:30:00Z"}),
    ));
    assert!(goal.check(Some(&initial()), &state).is_err());
    state["services"]["firecrawl"]["monitor_checks"]["actual-check"] = json!({
        "id":"actual-check","monitorId":"actual-opaque-monitor","trigger":"manual"
    });
    goal.check(Some(&initial()), &state).unwrap();
    state["services"]["firecrawl"]["monitor_checks"]["actual-check"]["monitorId"] =
        json!("another-monitor");
    assert!(goal.check(Some(&initial()), &state).is_err());
}

#[test]
fn semantic_goals_require_nonvacuous_unfiltered_created_resource_selection() {
    for extra in [
        json!({"equals":0}),
        json!({"selector":{"collection":"/services/firecrawl/crawls","created":false}}),
        json!({"selector":{"collection":"/services/firecrawl/crawls","created":true,"matching":{"status":"queued"}}}),
        json!({"fields":{"/total":2}}),
    ] {
        assert!(crawl_goal(extra).validate().is_err());
    }
    assert!(
        monitor_goal(json!({"fields":{"/schedule":{"cron":"0 9 * * *"}}}))
            .validate()
            .is_err()
    );
}
