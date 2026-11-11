//! Archived repositories are read-only for issue creation, over REST and GraphQL,
//! while reads and creation in active repositories are unaffected.

use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_core::{
    dataset::Dataset,
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use mock_github::GitHub;
use serde_json::{Value, json};
use tower::ServiceExt;

const HOST: &str = "github.octo-corp.com";
const ARCHIVED: &str = "/api/v3/repos/octo-org/empty";
const ACTIVE: &str = "/api/v3/repos/octo-org/payments-api";
const NEW_LABEL: &str = "label-that-must-not-exist";

fn fixture() -> (SharedWorld, Router) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/full");
    let world = shared(WorldState::from_dataset(Dataset::load(&dir).unwrap()).unwrap());
    let app = service_app(world.clone(), &GitHub);
    (world, app)
}

async fn send(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", HOST)
        .header("x-forwarded-proto", "https")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.unwrap();
    (
        parts.status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn graphql(app: &Router, query: &str, variables: Value) -> Value {
    let (status, body) = send(
        app,
        "POST",
        "/api/graphql",
        json!({"query": query, "variables": variables}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body
}

fn state(world: &SharedWorld) -> Value {
    world.lock().unwrap().snapshot()["services"]["github"].clone()
}

async fn archive(app: &Router, repo_path: &str) {
    let (status, body) = send(app, "PATCH", repo_path, json!({"archived": true})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["archived"], true);
}

async fn repository_node_id(app: &Router, name: &str) -> Value {
    let body = graphql(
        app,
        "query($name:String!){repository(owner:\"octo-org\",name:$name){id isArchived}}",
        json!({"name": name}),
    )
    .await;
    body["data"]["repository"]["id"].clone()
}

const CREATE_ISSUE: &str =
    "mutation($input:CreateIssueInput!){createIssue(input:$input){issue{id number title}}}";

#[tokio::test]
async fn rest_rejects_new_issue_in_archived_repository_without_side_effects() {
    let (world, app) = fixture();
    archive(&app, ARCHIVED).await;
    let before = state(&world);

    let (status, body) = send(
        &app,
        "POST",
        &format!("{ARCHIVED}/issues"),
        json!({"title": "Rejected", "body": "text", "labels": [NEW_LABEL]}),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["status"], "403");
    assert!(body["message"].as_str().unwrap().contains("archived"));
    assert_eq!(state(&world), before);
    let (_, labels) = send(&app, "GET", &format!("{ARCHIVED}/labels"), json!({})).await;
    assert!(
        labels
            .as_array()
            .unwrap()
            .iter()
            .all(|label| label["name"] != NEW_LABEL)
    );
}

#[tokio::test]
async fn graphql_rejects_new_issue_in_archived_repository_without_side_effects() {
    let (world, app) = fixture();
    archive(&app, ARCHIVED).await;
    let repository_id = repository_node_id(&app, "empty").await;
    let before = state(&world);

    let body = graphql(
        &app,
        CREATE_ISSUE,
        json!({"input": {"repositoryId": repository_id, "title": "Rejected", "body": "text"}}),
    )
    .await;

    assert!(body["data"]["createIssue"].is_null(), "{body}");
    let message = body["errors"][0]["message"].as_str().unwrap();
    assert!(message.contains("archived"), "{body}");
    assert_eq!(state(&world), before);
}

#[tokio::test]
async fn rejected_creation_does_not_consume_numbers_or_ids() {
    let (_, control_app) = fixture();
    let (_, control) = send(
        &control_app,
        "POST",
        &format!("{ACTIVE}/issues"),
        json!({"title": "Next"}),
    )
    .await;

    let (world, app) = fixture();
    archive(&app, ARCHIVED).await;
    let repository_id = repository_node_id(&app, "empty").await;
    for _ in 0..2 {
        let (status, _) = send(
            &app,
            "POST",
            &format!("{ARCHIVED}/issues"),
            json!({"title": "Rejected", "labels": [NEW_LABEL]}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        graphql(
            &app,
            CREATE_ISSUE,
            json!({"input": {"repositoryId": repository_id, "title": "Rejected"}}),
        )
        .await;
    }
    let (status, next) = send(
        &app,
        "POST",
        &format!("{ACTIVE}/issues"),
        json!({"title": "Next"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(next["id"], control["id"]);
    assert_eq!(next["node_id"], control["node_id"]);
    assert_eq!(next["number"], control["number"]);
    assert!(
        state(&world)["labels"]
            .as_object()
            .unwrap()
            .keys()
            .all(|key| !key.ends_with(NEW_LABEL))
    );
}

#[tokio::test]
async fn archived_repository_reads_and_active_repositories_stay_available() {
    let (_, app) = fixture();
    let (_, listed_before) = send(&app, "GET", &format!("{ARCHIVED}/issues"), json!({})).await;
    archive(&app, ARCHIVED).await;

    let (status, listed) = send(&app, "GET", &format!("{ARCHIVED}/issues"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed, listed_before);
    let (status, repo) = send(&app, "GET", ARCHIVED, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repo["archived"], true);
    let (status, labels) = send(&app, "GET", &format!("{ARCHIVED}/labels"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(labels.is_array());

    let (status, created) = send(
        &app,
        "POST",
        &format!("{ACTIVE}/issues"),
        json!({"title": "Active repository issue", "labels": ["fresh-label"]}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"], "Active repository issue");
    assert_eq!(created["labels"][0]["name"], "fresh-label");

    let active_id = repository_node_id(&app, "payments-api").await;
    let body = graphql(
        &app,
        CREATE_ISSUE,
        json!({"input": {"repositoryId": active_id, "title": "Active GraphQL issue"}}),
    )
    .await;
    assert_eq!(
        body["data"]["createIssue"]["issue"]["title"], "Active GraphQL issue",
        "{body}"
    );
}

#[tokio::test]
async fn unarchiving_restores_issue_creation() {
    let (_, app) = fixture();
    archive(&app, ARCHIVED).await;
    let (status, _) = send(
        &app,
        "POST",
        &format!("{ARCHIVED}/issues"),
        json!({"title": "Rejected"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(&app, "PATCH", ARCHIVED, json!({"archived": false})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, created) = send(
        &app,
        "POST",
        &format!("{ARCHIVED}/issues"),
        json!({"title": "Accepted"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"], "Accepted");
}
