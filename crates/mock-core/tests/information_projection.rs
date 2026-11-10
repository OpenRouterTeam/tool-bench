use std::path::Path;

use axum::{
    Router,
    body::Body,
    extract::{Path as RoutePath, Query, State},
    http::{HeaderMap, Request, StatusCode},
    routing::{get, post},
};
use mock_core::{
    dataset::Dataset,
    grading::{CheckDisposition, CheckKind, Expect, Grade, GradeInput, Outcome, grade},
    http::json as response,
    serve::service_app,
    service::MockService,
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tower::ServiceExt;

const TEXT: &str =
    "Scale plan is generally available.  It includes   SSO and audit logs. Pricing starts at $49.";

struct Probe;

impl MockService for Probe {
    fn name(&self) -> &'static str {
        "projection"
    }

    fn port(&self) -> u16 {
        8998
    }

    fn hosts(&self) -> &'static [&'static str] {
        &["projection.example"]
    }

    fn router(&self) -> Router<SharedWorld> {
        Router::new()
            .route("/read/{shape}", get(read))
            .route("/series/{shape}", get(series))
            .route("/action", post(action))
            .route("/graphql", post(graphql))
            .route("/aws", post(aws))
    }
}

async fn read(RoutePath(shape): RoutePath<String>) -> axum::response::Response {
    let url = "https://example.test/scale";
    let record = match shape.as_str() {
        "contents" => {
            json!({"url":url,"title":"Scale","publishedDate":"2026-10-01T00:00:00.000Z","text":TEXT})
        }
        "truncated" => json!({"url":url,"text":"Scale plan is generally available. It"}),
        "highlights" => {
            json!({"url":url,"highlights":["It includes SSO and audit logs.", "Pricing starts at $49."]})
        }
        "url-only" => json!({"url":url,"title":"Scale"}),
        "other-url" => json!({"url":"https://example.test/other","text":TEXT}),
        _ => json!({}),
    };
    response(StatusCode::OK, json!({"results":[record]}))
}

async fn series(RoutePath(shape): RoutePath<String>) -> axum::response::Response {
    let (scope, interval, points) = match shape.as_str() {
        "fine" => ("env:prod", 60, json!([[0.0, 10.0], [60_000.0, 90.0]])),
        "coarse" => ("env:prod", 120, json!([[0.0, 50.0]])),
        "mixed" => ("*", 60, json!([[0.0, 10.0], [60_000.0, 90.0]])),
        "glob" => ("env:pro*", 60, json!([[0.0, 10.0], [60_000.0, 90.0]])),
        "count" => ("env:prod", 60, json!([[0.0, 1.0], [60_000.0, 1.0]])),
        _ => ("env:prod", 60, json!([[60_000.0, 90.0]])),
    };
    response(
        StatusCode::OK,
        json!({"series":[{"metric":"latency","scope":scope,"interval":interval,"pointlist":points}]}),
    )
}

async fn action(State(world): State<SharedWorld>) -> axum::response::Response {
    world
        .lock()
        .unwrap()
        .service_mut("projection")
        .store
        .collection_mut("targets")
        .get_mut("target")
        .unwrap()["done"] = json!(true);
    response(StatusCode::OK, json!({"done":true}))
}

async fn graphql(
    State(world): State<SharedWorld>,
    axum::Json(body): axum::Json<Value>,
) -> axum::response::Response {
    let title = body["title"].as_str().unwrap_or_default();
    match body["operation"].as_str() {
        Some("createIssue") => {
            let mut world = world.lock().unwrap();
            let target = world
                .service_mut("projection")
                .store
                .collection_mut("targets")
                .get_mut("target")
                .unwrap();
            if target["title"] != title {
                target["title"] = json!(title);
            }
            response(
                StatusCode::OK,
                json!({"data":{"createIssue":{"title":title}}}),
            )
        }
        Some("createIssueStatus") => response(StatusCode::OK, json!({"data":{"status":"open"}})),
        _ => response(
            StatusCode::BAD_REQUEST,
            json!({"errors":[{"message":"rejected"}]}),
        ),
    }
}

async fn aws(
    headers: HeaderMap,
    Query(query): Query<BTreeMap<String, String>>,
) -> axum::response::Response {
    let shift: f64 = query
        .get("shift")
        .map_or(0.0, |shift| shift.parse().unwrap());
    let instant = json!(1_793_365_507.25 + shift);
    let target = headers
        .get("x-amz-target")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let body = match target {
        "Monitoring.DescribeAlarms" => json!({"MetricAlarms":[{
            "AlarmName":"db-lag","StateValue":"ALARM","StateTransitionedTimestamp":instant
        }]}),
        "Monitoring.DescribeAlarmHistory" => json!({"AlarmHistoryItems":[{
            "AlarmName":"db-lag","HistorySummary":"Alarm updated from OK to ALARM","Timestamp":instant
        }]}),
        _ => json!({"Metrics":[]}),
    };
    response(StatusCode::OK, body)
}

struct Trial {
    world: SharedWorld,
    app: Router,
}

fn trial() -> Trial {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut store = Store::default();
    store.collection_mut("documents").insert(
        "scale",
        json!({
            "url":"https://example.test/scale","title":"Scale","publishedDate":"2026-09-30",
            "keywords":["pricing","sso"],"text":TEXT
        }),
    );
    for (identity, environment, values) in [
        ("prod", "env:prod", [10, 90]),
        ("staging", "env:staging", [20, 20]),
    ] {
        store.collection_mut("metrics").insert(
            identity,
            json!({
                "metric":"latency","tags":[environment],
                "points":[[0, values[0]], [60, values[1]]]
            }),
        );
    }
    for (identity, state, instant) in [
        ("db-lag", "ALARM", "2026-10-30T13:05:07.25Z"),
        ("cache-hits", "OK", "2026-10-30T12:00:00Z"),
    ] {
        store.collection_mut("alarms").insert(
            identity,
            json!({"AlarmName":identity,"StateValue":state,"StateUpdatedTimestamp":instant}),
        );
    }
    store
        .collection_mut("targets")
        .insert("target", json!({"id":"target","done":false,"title":""}));
    dataset.services.insert("projection".into(), store);
    let world = shared(WorldState::from_dataset(dataset).unwrap());
    let app = service_app(world.clone(), &Probe);
    Trial { world, app }
}

fn expect(information: Value) -> Expect {
    serde_json::from_value(json!({
        "version":2,"audit_history":true,
        "state":[{"pointer":"/services/projection/targets/target/done","equals":true}],
        "preservation":{"allow":[{
            "pointer":"/services/projection/targets/target/done",
            "cause":{"service":"projection","methods":["POST"],"path_regex":"^/action$"}
        }]},
        "information":[information]
    }))
    .unwrap()
}

fn document_check(excerpts: Value) -> Value {
    json!({
        "prerequisite":{"service":"projection","method":"GET","path_regex":"^/read/[^/]+$"},
        "response_field":"/results",
        "source":{"collection":"/services/projection/documents","matching":{"url":"https://example.test/scale"}},
        "projection":[
            {"response":["/url"],"source":"/url"},
            {"response":["/text","/highlights"],"source":"/text","excerpts":excerpts}
        ],
        "action":{"service":"projection","method":"POST","path":"/action"}
    })
}

fn series_check() -> Value {
    json!({
        "prerequisite":{"service":"projection","method":"GET","path_regex":"^/series/[^/]+$"},
        "response_field":"/series",
        "source":{"collection":"/services/projection/metrics","matching":{"tags":["env:prod"]}},
        "series":{
            "fields":{"/metric":"/metric"},"scope":"/scope","source_tags":"/tags",
            "points":"/pointlist","interval":"/interval","source_points":"/points",
            "timestamp_scale":1000,"start":"1970-01-01T00:00:00Z","end":"1970-01-01T00:02:00Z"
        },
        "action":{"service":"projection","method":"POST","path":"/action"}
    })
}

async fn call(app: &Router, method: &str, path: &str) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert!(response.status().is_success());
}

async fn attempt(expect: &Expect, reads: &[&str]) -> Grade {
    let trial = trial();
    for path in reads {
        call(&trial.app, "GET", path).await;
    }
    call(&trial.app, "POST", "/action").await;
    let world = trial.world.lock().unwrap();
    grade(&GradeInput {
        expect,
        state: world.snapshot(),
        journal: world.journal.entries(),
        answer: None,
        tools_ips: vec!["unknown".into()],
        context_overflow: false,
        startup: &[],
        initial_state: Some(&world.initial_snapshot()),
        events: Some(world.events().entries()),
    })
}

fn facts() -> Value {
    json!(["SSO and audit logs", "Pricing starts at $49."])
}

#[tokio::test]
async fn projected_source_fields_accept_returned_content_without_authored_metadata() {
    let projected = expect(document_check(facts()));
    for read in ["/read/contents", "/read/highlights"] {
        let assessment = attempt(&projected, &[read]).await;
        assert_eq!(assessment.outcome, Outcome::Pass, "{read}: {assessment:?}");
    }
    let whole = expect(json!({
        "prerequisite":{"service":"projection","method":"GET","path_regex":"^/read/[^/]+$"},
        "response_field":"/results",
        "source":{"collection":"/services/projection/documents","matching":{"url":"https://example.test/scale"}},
        "action":{"service":"projection","method":"POST","path":"/action"}
    }));
    let assessment = attempt(&whole, &["/read/contents"]).await;
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[tokio::test]
async fn an_identity_without_the_relevant_returned_facts_is_not_information() {
    let expect = expect(document_check(facts()));
    for read in [
        "/read/url-only",
        "/read/truncated",
        "/read/other-url",
        "/read/empty",
    ] {
        let assessment = attempt(&expect, &[read]).await;
        assert_eq!(assessment.outcome, Outcome::Fail, "{read}: {assessment:?}");
        assert!(assessment.failures.iter().any(|failure| {
            failure.check == CheckKind::Ordering && failure.disposition == CheckDisposition::Failed
        }));
    }
    let assessment = attempt(&expect, &[]).await;
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[tokio::test]
async fn exact_projection_without_excerpts_requires_equal_source_values() {
    let expect = expect(document_check(json!([])));
    assert_eq!(
        attempt(&expect, &["/read/contents"]).await.outcome,
        Outcome::Pass
    );
    assert_eq!(
        attempt(&expect, &["/read/truncated"]).await.outcome,
        Outcome::Fail
    );
}

#[tokio::test]
async fn excerpts_absent_from_the_authored_source_are_an_environment_fault() {
    let expect = expect(document_check(json!(["Enterprise plan is free"])));
    let assessment = attempt(&expect, &["/read/contents"]).await;
    assert_eq!(
        assessment.outcome,
        Outcome::EnvironmentError,
        "{assessment:?}"
    );
}

#[tokio::test]
async fn series_need_isolated_source_scope_and_resolving_buckets() {
    let expect = expect(series_check());
    let assessment = attempt(&expect, &["/series/fine"]).await;
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    for read in ["/series/mixed", "/series/count", "/series/partial"] {
        let assessment = attempt(&expect, &[read]).await;
        assert_eq!(assessment.outcome, Outcome::Fail, "{read}: {assessment:?}");
    }
    for read in ["/series/coarse", "/series/glob"] {
        let assessment = attempt(&expect, &[read]).await;
        assert_eq!(
            assessment.outcome,
            Outcome::Unresolved,
            "{read}: {assessment:?}"
        );
    }
    let assessment = attempt(&expect, &["/series/mixed", "/series/fine"]).await;
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
}

#[test]
fn incoherent_projection_or_series_contracts_are_rejected_before_grading() {
    let mut invalid = Vec::new();
    for (field, value) in [
        ("projection", json!([{"response":[],"source":"/text"}])),
        ("projection", json!([{"response":["/text"],"source":""}])),
        (
            "projection",
            json!([{"response":["/text"],"source":"/text","excerpts":["  "]}]),
        ),
    ] {
        let mut check = document_check(facts());
        check[field] = value;
        invalid.push(check);
    }
    for (field, value) in [
        ("timestamp_scale", json!(0)),
        ("end", json!("1970-01-01T00:00:00Z")),
        ("fields", json!({})),
        ("points", json!("")),
    ] {
        let mut check = series_check();
        check["series"][field] = value;
        invalid.push(check);
    }
    let mut combined = series_check();
    combined["projection"] = document_check(facts())["projection"].clone();
    invalid.push(combined);
    let mut windowed = series_check();
    windowed["window"] =
        json!({"field":"/time","start":"1970-01-01T00:00:00Z","end":"1970-01-01T00:02:00Z"});
    invalid.push(windowed);
    for check in invalid {
        assert!(expect(check.clone()).validate().is_err(), "{check}");
    }
    assert!(expect(document_check(facts())).validate().is_ok());
    assert!(expect(series_check()).validate().is_ok());
}

async fn send(app: &Router, path: &str, target: Option<&str>, body: Value) -> StatusCode {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(target) = target {
        request = request.header("x-amz-target", target);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    response.status()
}

fn grade_trial(trial: &Trial, expect: &Expect) -> Grade {
    let world = trial.world.lock().unwrap();
    grade(&GradeInput {
        expect,
        state: world.snapshot(),
        journal: world.journal.entries(),
        answer: None,
        tools_ips: vec!["unknown".into()],
        context_overflow: false,
        startup: &[],
        initial_state: Some(&world.initial_snapshot()),
        events: Some(world.events().entries()),
    })
}

fn effect_expect(effect: bool) -> Expect {
    let mut check = document_check(facts());
    check["action"] = json!({"service":"projection","method":"POST","path":"/graphql"});
    if effect {
        check["action_effect"] = json!({
            "selector":{"collection":"/services/projection/targets","matching":{"id":"target"}},
            "field":"/title","nonempty_text":true
        });
    }
    serde_json::from_value(json!({
        "version":2,"audit_history":true,
        "state":[{"pointer":"/services/projection/targets/target/title","equals":"Scale GA"}],
        "preservation":{"allow":[{
            "pointer":"/services/projection/targets/target/title",
            "cause":{"service":"projection","methods":["POST"],"path_regex":"^/graphql$"}
        }]},
        "information":[check]
    }))
    .unwrap()
}

async fn create(app: &Router) {
    assert_eq!(
        send(
            app,
            "/graphql",
            None,
            json!({"operation":"createIssue","title":"Scale GA"})
        )
        .await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn only_requests_that_commit_the_declared_effect_need_prior_information() {
    let expect = effect_expect(true);
    let trial = trial();
    for body in [
        json!({"operation":"createIssueStatus"}),
        json!({"operation":"createIssue","title":""}),
        json!({"operation":"unsupported"}),
    ] {
        send(&trial.app, "/graphql", None, body).await;
    }
    call(&trial.app, "GET", "/read/contents").await;
    create(&trial.app).await;
    let assessment = grade_trial(&trial, &expect);
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    let heuristic = grade_trial(&trial, &effect_expect(false));
    assert_eq!(heuristic.outcome, Outcome::Fail, "{heuristic:?}");
}

#[tokio::test]
async fn an_effective_write_before_or_without_source_evidence_is_rejected() {
    let expect = effect_expect(true);
    for reads in [vec![], vec!["/read/url-only", "/read/other-url"]] {
        let trial = trial();
        for read in &reads {
            call(&trial.app, "GET", read).await;
        }
        create(&trial.app).await;
        call(&trial.app, "GET", "/read/contents").await;
        let assessment = grade_trial(&trial, &expect);
        assert_eq!(
            assessment.outcome,
            Outcome::Fail,
            "{reads:?}: {assessment:?}"
        );
        assert!(assessment.failures.iter().any(|failure| {
            failure.check == CheckKind::Ordering && failure.location == "information/0"
        }));
    }
}

fn alarm_check() -> Value {
    let read = |target: &str, field: &str, projection: Value| {
        json!({
            "prerequisite":{"service":"projection","method":"POST","path":"/aws","headers":{"x-amz-target":target}},
            "response_field":field,
            "source":{"collection":"/services/projection/alarms","matching":{"AlarmName":"db-lag","StateValue":"ALARM"}},
            "projection":projection
        })
    };
    let mut check = read(
        "Monitoring.DescribeAlarms",
        "/MetricAlarms",
        json!([
            {"response":["/AlarmName"],"source":"/AlarmName"},
            {"response":["/StateValue"],"source":"/StateValue"},
            {"response":["/StateTransitionedTimestamp"],"source":"/StateUpdatedTimestamp","format":"unix_seconds"}
        ]),
    );
    check["alternatives"] = json!([read(
        "Monitoring.DescribeAlarmHistory",
        "/AlarmHistoryItems",
        json!([
            {"response":["/AlarmName"],"source":"/AlarmName"},
            {"response":["/HistorySummary","/HistoryData"],"source":"/StateValue","excerpts":["ALARM"]},
            {"response":["/Timestamp"],"source":"/StateUpdatedTimestamp","format":"unix_seconds"}
        ]),
    )]);
    check["action"] = json!({"service":"projection","method":"POST","path":"/action"});
    check
}

#[tokio::test]
async fn either_reviewed_read_shape_supplies_the_source_with_exact_epoch_timestamps() {
    let expect = expect(alarm_check());
    expect.validate().unwrap();
    for target in [
        "Monitoring.DescribeAlarms",
        "Monitoring.DescribeAlarmHistory",
    ] {
        let trial = trial();
        send(
            &trial.app,
            "/aws",
            Some("Monitoring.ListMetrics"),
            json!({}),
        )
        .await;
        send(&trial.app, "/aws", Some(target), json!({})).await;
        call(&trial.app, "POST", "/action").await;
        let assessment = grade_trial(&trial, &expect);
        assert_eq!(
            assessment.outcome,
            Outcome::Pass,
            "{target}: {assessment:?}"
        );
    }
}

#[tokio::test]
async fn unrelated_reads_or_a_shifted_timestamp_do_not_supply_the_alarm() {
    let expect = expect(alarm_check());
    for (target, path) in [
        ("Monitoring.ListMetrics", "/aws"),
        ("Monitoring.DescribeAlarms", "/aws?shift=1"),
        ("Monitoring.DescribeAlarmHistory", "/aws?shift=0.001"),
    ] {
        let trial = trial();
        send(&trial.app, path, Some(target), json!({})).await;
        call(&trial.app, "POST", "/action").await;
        let assessment = grade_trial(&trial, &expect);
        assert_eq!(
            assessment.outcome,
            Outcome::Fail,
            "{target}{path}: {assessment:?}"
        );
        assert!(assessment.environment_ok, "{assessment:?}");
    }
}

#[test]
fn alternatives_and_formats_must_be_bounded_and_distinct() {
    let mut duplicate = alarm_check();
    let mut base = duplicate.clone();
    for field in ["alternatives", "action"] {
        base.as_object_mut().unwrap().remove(field);
    }
    duplicate["alternatives"] = json!([base]);
    let mut formatted_excerpt = alarm_check();
    formatted_excerpt["projection"][2]["excerpts"] = json!(["ALARM"]);
    let mut unbounded = alarm_check();
    unbounded["alternatives"][0]["prerequisite"] = json!({"service":"projection","method":"POST"});
    let mut bad_effect = effect_expect(true);
    bad_effect.information[0]
        .action_effect
        .as_mut()
        .unwrap()
        .nonempty_text = None;
    for check in [duplicate, formatted_excerpt, unbounded] {
        assert!(expect(check.clone()).validate().is_err(), "{check}");
    }
    assert!(bad_effect.validate().is_err());
}
