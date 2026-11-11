use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use mock_core::{
    dataset::Dataset,
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use mock_shopify::Shopify;
use serde_json::{Value, json};
use tower::ServiceExt;

const PRODUCT: &str = "gid://shopify/Product/8102";
const ONLINE: &str = "gid://shopify/Publication/3001";
const POS: &str = "gid://shopify/Publication/3002";

fn setup() -> (SharedWorld, Router) {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
    let world = shared(WorldState::from_dataset(Dataset::load(&directory).unwrap()).unwrap());
    let app = service_app(world.clone(), &Shopify);
    (world, app)
}

async fn request(app: &Router, query: &str, variables: Value) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::post("/admin/api/2026-01/graphql.json")
                .header("host", "octo-corp.myshopify.com")
                .header("x-shopify-access-token", "shpat_toolbench")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"query": query, "variables": variables}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_success());
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert!(value.get("errors").is_none(), "{value}");
    value
}

async fn publish(app: &Router, publications: Vec<&str>) -> Value {
    request(app, r#"mutation Publish($id: ID!, $input: [PublicationInput!]!) {
        publishablePublish(id: $id, input: $input) {
            publishable { __typename ... on Product {
                id status onlineStoreUrl publishedAt
                resourcePublications(first: 100) { nodes { isPublished publishDate publication { id name } } }
            }}
            userErrors { field message }
        }
    }"#, json!({"id": PRODUCT, "input": publications.into_iter().map(|identity| json!({"publicationId": identity})).collect::<Vec<_>>()})).await
}

async fn activate(app: &Router) {
    let result = request(app, r#"mutation Activate($input: ProductInput!) {
        productUpdate(input: $input) { product { id status onlineStoreUrl } userErrors { field message } }
    }"#, json!({"input": {"id": PRODUCT, "status": "ACTIVE"}})).await;
    assert_eq!(result["data"]["productUpdate"]["userErrors"], json!([]));
}

#[tokio::test]
async fn publication_discovery_paginates_real_channel_identities() {
    let (_world, app) = setup();
    let query = r#"query Publications($first: Int!, $after: String) {
        publications(first: $first, after: $after) { nodes { id name app { title } } pageInfo { endCursor hasNextPage } }
    }"#;
    let first = request(&app, query, json!({"first": 1})).await;
    assert_eq!(first["data"]["publications"]["nodes"][0]["id"], ONLINE);
    assert_eq!(
        first["data"]["publications"]["pageInfo"]["hasNextPage"],
        true
    );
    let second = request(
        &app,
        query,
        json!({"first": 1, "after": first["data"]["publications"]["pageInfo"]["endCursor"]}),
    )
    .await;
    assert_eq!(second["data"]["publications"]["nodes"][0]["id"], POS);
    assert_eq!(
        second["data"]["publications"]["pageInfo"]["hasNextPage"],
        false
    );
}

#[tokio::test]
async fn visibility_requires_both_activation_and_the_online_store_membership() {
    let (world, app) = setup();
    activate(&app).await;
    assert_eq!(
        world.lock().unwrap().snapshot()["services"]["shopify"]["products"]["8102"]["onlineStoreVisible"],
        false
    );
    let result = publish(&app, vec![POS]).await;
    assert_eq!(
        result["data"]["publishablePublish"]["userErrors"],
        json!([])
    );
    assert!(result["data"]["publishablePublish"]["publishable"]["onlineStoreUrl"].is_null());
    let result = publish(&app, vec![ONLINE]).await;
    assert_eq!(
        result["data"]["publishablePublish"]["userErrors"],
        json!([])
    );
    assert!(
        result["data"]["publishablePublish"]["publishable"]["onlineStoreUrl"]
            .as_str()
            .unwrap()
            .contains("/products/")
    );
    assert_eq!(
        world.lock().unwrap().snapshot()["services"]["shopify"]["products"]["8102"]["onlineStoreVisible"],
        true
    );
}

#[tokio::test]
async fn a_draft_membership_does_not_claim_live_visibility_before_activation() {
    let (world, app) = setup();
    let result = publish(&app, vec![ONLINE]).await;
    assert!(result["data"]["publishablePublish"]["publishable"]["onlineStoreUrl"].is_null());
    assert_eq!(
        world.lock().unwrap().snapshot()["services"]["shopify"]["products"]["8102"]["onlineStoreVisible"],
        false
    );
    activate(&app).await;
    assert_eq!(
        world.lock().unwrap().snapshot()["services"]["shopify"]["products"]["8102"]["onlineStoreVisible"],
        true
    );
}

#[tokio::test]
async fn invalid_batch_publications_are_atomic_and_retries_are_idempotent() {
    let (world, app) = setup();
    activate(&app).await;
    let before = world.lock().unwrap().snapshot();
    let invalid = publish(&app, vec![ONLINE, "gid://shopify/Publication/999999"]).await;
    assert!(
        !invalid["data"]["publishablePublish"]["userErrors"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(before, world.lock().unwrap().snapshot());
    publish(&app, vec![ONLINE]).await;
    let after = world.lock().unwrap().snapshot();
    publish(&app, vec![ONLINE, ONLINE]).await;
    assert_eq!(after, world.lock().unwrap().snapshot());
}

#[test]
fn initialization_is_part_of_the_baseline_and_does_not_invent_a_draft_assignment() {
    let (world, _app) = setup();
    let state = world.lock().unwrap();
    assert_eq!(
        state.initial_snapshot()["services"],
        state.snapshot()["services"]
    );
    assert!(
        !state.snapshot()["services"]["shopify"]["resource_publications"]
            .as_object()
            .unwrap()
            .values()
            .any(|row| row["productId"] == 8102)
    );
    assert!(state.events().entries().is_empty());
}
