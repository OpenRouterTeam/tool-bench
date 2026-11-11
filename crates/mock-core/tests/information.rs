use std::{path::Path, sync::Arc};

use axum::{
    Extension, Router,
    body::Body,
    extract::{Path as RoutePath, State},
    http::{Request, StatusCode},
    routing::{get, post},
};
use mock_core::{
    dataset::Dataset,
    events::{EventKind, OperationEvent},
    grading::{CheckKind, Expect, Grade, GradeInput, Outcome, grade},
    http::json as response,
    journal::JournalEntry,
    serve::service_app,
    service::MockService,
    startup::StartupCall,
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::json;
use tokio::sync::Notify;
use tower::ServiceExt;

#[derive(Default)]
struct Gates {
    started: Notify,
    release: Notify,
}

struct Probe(Arc<Gates>);

impl MockService for Probe {
    fn name(&self) -> &'static str {
        "information"
    }

    fn port(&self) -> u16 {
        8999
    }

    fn hosts(&self) -> &'static [&'static str] {
        &["information.example"]
    }

    fn router(&self) -> Router<SharedWorld> {
        Router::new()
            .route("/records/{identity}", get(read_records))
            .route("/action", post(action))
            .layer(Extension(self.0.clone()))
    }
}

async fn read_records(
    State(world): State<SharedWorld>,
    Extension(gates): Extension<Arc<Gates>>,
    RoutePath(identity): RoutePath<String>,
) -> axum::response::Response {
    let selected = if matches!(identity.as_str(), "blocked" | "forged") {
        "valid"
    } else {
        identity.as_str()
    };
    let mut record = world
        .lock()
        .unwrap()
        .service("information")
        .unwrap()
        .store
        .collection("records")
        .unwrap()
        .get(selected)
        .cloned();
    if identity == "forged" {
        record.as_mut().unwrap()["message"] = json!("fabricated content with the same identity");
    }
    if identity == "blocked" {
        gates.started.notify_one();
        gates.release.notified().await;
    }
    response(
        StatusCode::OK,
        json!({"data":record.into_iter().collect::<Vec<_>>()}),
    )
}

async fn action(State(world): State<SharedWorld>) -> axum::response::Response {
    world
        .lock()
        .unwrap()
        .service_mut("information")
        .store
        .collection_mut("targets")
        .get_mut("target")
        .unwrap()["done"] = json!(true);
    response(StatusCode::OK, json!({"done":true}))
}

struct Trial {
    world: SharedWorld,
    app: Router,
    gates: Arc<Gates>,
}

fn trial() -> Trial {
    let directory =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
    let mut dataset = Dataset::load(&directory).unwrap();
    let mut store = Store::default();
    for (identity, resource, time) in [
        ("valid", "checkout", "2026-10-30T14:29:00Z"),
        ("old", "checkout", "2026-10-30T12:59:59.999999999Z"),
        ("future", "checkout", "2026-10-30T14:30:00Z"),
        ("other", "payments", "2026-10-30T14:29:00Z"),
    ] {
        store.collection_mut("records").insert(
            identity,
            json!({
                "id":identity,"resource":resource,"time":time,"message":"authored evidence"
            }),
        );
    }
    store
        .collection_mut("targets")
        .insert("target", json!({"id":"target","done":false}));
    dataset.services.insert("information".into(), store);
    let world = shared(WorldState::from_dataset(dataset).unwrap());
    let gates = Arc::new(Gates::default());
    let app = service_app(world.clone(), &Probe(gates.clone()));
    Trial { world, app, gates }
}

fn expect() -> Expect {
    serde_json::from_value(json!({
        "version":2,"audit_history":true,
        "state":[{"pointer":"/services/information/targets/target/done","equals":true}],
        "required_calls":[{"service":"information","method":"POST","path":"/action"}],
        "preservation":{"allow":[{
            "pointer":"/services/information/targets/target/done",
            "cause":{"service":"information","methods":["POST"],"path_regex":"^/action$"}
        }]},
        "information":[{
            "prerequisite":{"service":"information","method":"GET","path_regex":"^/records/[^/]+$"},
            "response_field":"/data",
            "source":{"collection":"/services/information/records","matching":{"resource":"checkout"}},
            "window":{"field":"/time","start":"2026-10-30T13:00:00Z","end":"2026-10-30T14:30:00Z"},
            "action":{"service":"information","method":"POST","path":"/action"}
        }]
    })).unwrap()
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

fn assess(
    trial: &Trial,
    expect: &Expect,
    startup: &[StartupCall],
    alter: impl FnOnce(&mut Vec<OperationEvent>, &mut Vec<JournalEntry>),
) -> Grade {
    let world = trial.world.lock().unwrap();
    let mut events = world.events().entries().to_vec();
    let mut journal = world.journal.entries().to_vec();
    alter(&mut events, &mut journal);
    grade(&GradeInput {
        expect,
        state: world.snapshot(),
        journal: &journal,
        answer: None,
        tools_ips: vec!["unknown".into()],
        context_overflow: false,
        startup,
        initial_state: Some(&world.initial_snapshot()),
        events: Some(&events),
    })
}

#[tokio::test]
async fn a_source_bound_record_received_before_the_action_establishes_the_prerequisite() {
    let trial = trial();
    call(&trial.app, "GET", "/records/valid").await;
    call(&trial.app, "POST", "/action").await;
    let assessment = assess(&trial, &expect(), &[], |_, _| {});
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    assert!(assessment.ordering_ok && assessment.history_ok && assessment.environment_ok);
}

#[tokio::test]
async fn empty_unrelated_or_fabricated_records_cannot_substitute_for_information() {
    for identity in [None, Some("empty"), Some("other"), Some("forged")] {
        let trial = trial();
        if let Some(identity) = identity {
            call(&trial.app, "GET", &format!("/records/{identity}")).await;
        }
        call(&trial.app, "POST", "/action").await;
        let assessment = assess(&trial, &expect(), &[], |_, _| {});
        assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
        assert!(!assessment.ordering_ok && assessment.environment_ok);
        assert!(assessment.failures.iter().any(|failure| {
            failure.check == CheckKind::Ordering && !failure.event_evidence.is_empty()
        }));
    }
}

#[tokio::test]
async fn exact_half_open_source_windows_exclude_old_and_future_records() {
    for identity in ["old", "future"] {
        let trial = trial();
        call(&trial.app, "GET", &format!("/records/{identity}")).await;
        call(&trial.app, "POST", "/action").await;
        let assessment = assess(&trial, &expect(), &[], |_, _| {});
        assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
        assert!(assessment.environment_ok);
    }
}

#[tokio::test]
async fn a_read_started_first_but_completed_after_action_admission_is_insufficient() {
    let trial = trial();
    let app = trial.app.clone();
    let reading = tokio::spawn(async move { call(&app, "GET", "/records/blocked").await });
    trial.gates.started.notified().await;
    call(&trial.app, "POST", "/action").await;
    trial.gates.release.notify_one();
    reading.await.unwrap();
    let assessment = assess(&trial, &expect(), &[], |_, _| {});
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(!assessment.ordering_ok && assessment.environment_ok);
}

#[tokio::test]
async fn a_later_valid_read_does_not_retroactively_authorize_an_earlier_action() {
    let trial = trial();
    call(&trial.app, "POST", "/action").await;
    call(&trial.app, "GET", "/records/valid").await;
    let assessment = assess(&trial, &expect(), &[], |_, _| {});
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[tokio::test]
async fn unrelated_reads_do_not_prescribe_a_reference_query_or_read_count() {
    let trial = trial();
    for identity in ["other", "empty", "old", "valid", "future"] {
        call(&trial.app, "GET", &format!("/records/{identity}")).await;
    }
    call(&trial.app, "POST", "/action").await;
    let assessment = assess(&trial, &expect(), &[], |_, _| {});
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
}

#[tokio::test]
async fn startup_reads_are_excluded_from_request_prerequisites_but_repeated_reads_can_qualify() {
    for repeats in [1, 2] {
        let trial = trial();
        for _ in 0..repeats {
            call(&trial.app, "GET", "/records/valid").await;
        }
        call(&trial.app, "POST", "/action").await;
        let startup = [StartupCall {
            service: "information".into(),
            method: "GET".into(),
            path: "/records/valid".into(),
            query: None,
        }];
        let assessment = assess(&trial, &expect(), &startup, |_, _| {});
        assert_eq!(
            assessment.outcome,
            if repeats == 1 {
                Outcome::Fail
            } else {
                Outcome::Pass
            },
            "{assessment:?}",
        );
    }
}

#[tokio::test]
async fn missing_or_incomplete_operation_evidence_is_an_environment_fault() {
    let trial = trial();
    call(&trial.app, "GET", "/records/valid").await;
    call(&trial.app, "POST", "/action").await;
    for remove in [false, true] {
        let assessment = assess(&trial, &expect(), &[], |events, _| {
            if remove {
                events.clear();
            } else {
                let event = events
                    .iter_mut()
                    .find(|event| {
                        event
                            .operation
                            .as_ref()
                            .is_some_and(|operation| operation.method == "GET")
                            && matches!(event.event, EventKind::ResponseCompleted { .. })
                    })
                    .unwrap();
                event.event = EventKind::ResponseCompleted {
                    status: 200,
                    body_complete: false,
                };
            }
        });
        assert_eq!(
            assessment.outcome,
            Outcome::EnvironmentError,
            "{assessment:?}"
        );
        assert!(assessment.failures.iter().any(|failure| {
            failure.check == CheckKind::Environment && failure.location.starts_with("information")
        }));
    }
}

#[test]
fn unbounded_or_incoherent_information_contracts_are_rejected_before_grading() {
    for (field, value) in [
        ("response_field", json!("")),
        ("action", json!({"service":"information","method":"POST"})),
        (
            "action",
            json!({"service":"information","method":"POST","path":"/action","response_status":[404]}),
        ),
        (
            "source",
            json!({"collection":"/services/information/records","created":true}),
        ),
        (
            "window",
            json!({"field":"/time","start":"2026-10-30T14:30:00Z","end":"2026-10-30T13:00:00Z"}),
        ),
    ] {
        let mut source = serde_json::to_value(expect()).unwrap();
        source["information"][0][field] = value;
        let invalid: Expect = serde_json::from_value(source).unwrap();
        assert!(invalid.validate().is_err());
    }
    let mut invalid = expect();
    invalid.audit_history = false;
    assert!(invalid.validate().is_err());
    invalid.audit_history = true;
    invalid.version = 1;
    assert!(invalid.validate().is_err());
}

#[test]
fn empty_information_metadata_does_not_change_legacy_contract_bytes() {
    assert!(
        serde_json::to_value(Expect::default())
            .unwrap()
            .get("information")
            .is_none()
    );
}
