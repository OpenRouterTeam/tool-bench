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
use serde_json::{Value, json};
use tower::ServiceExt;

const ZONE: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

struct Client {
    world: SharedWorld,
    app: Router,
}

impl Client {
    fn new(enterprise: bool) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture");
        let mut dataset = Dataset::load(&root).unwrap();
        if enterprise {
            dataset
                .services
                .get_mut("cloudflare")
                .unwrap()
                .collection_mut("zones")
                .get_mut(ZONE)
                .unwrap()["plan"] = json!({"legacy_id":"enterprise","name":"Enterprise Website"});
        }
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = service_app(world.clone(), &mock_cloudflare::Cloudflare);
        Self { world, app }
    }

    async fn request(&self, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("host", "world:8117")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn state(&self) -> Value {
        self.world.lock().unwrap().snapshot()["services"]["cloudflare"].clone()
    }
}

#[tokio::test]
async fn cname_self_targets_and_url_targets_are_rejected_without_committing_records() {
    for content in [
        "status.octo-corp.com",
        "STATUS.OCTO-CORP.COM.",
        "https://octocorp.statuspage.io/path",
    ] {
        let client = Client::new(false);
        let initial = client.state();
        let (status, result) = client
            .request(
                "POST",
                &format!("/client/v4/zones/{ZONE}/dns_records"),
                json!({"type":"CNAME","name":"status","content":content,"proxied":false,"ttl":1}),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{content}: {result}");
        assert_eq!(initial, client.state());
    }
}

#[tokio::test]
async fn dns_custom_ttl_respects_the_declared_plan_while_auto_is_not_one_second() {
    for (enterprise, ttl, expected) in [
        (false, 30, StatusCode::BAD_REQUEST),
        (false, 60, StatusCode::OK),
        (true, 30, StatusCode::OK),
        (false, 1, StatusCode::OK),
    ] {
        let client = Client::new(enterprise);
        let (status,result)=client.request("POST",&format!("/client/v4/zones/{ZONE}/dns_records"),
            json!({"type":"CNAME","name":"status","content":"octocorp.statuspage.io","proxied":false,"ttl":ttl})).await;
        assert_eq!(status, expected, "{enterprise} {ttl}: {result}");
        if expected == StatusCode::OK {
            assert_eq!(result["result"]["ttl"], ttl);
        }
    }
}

#[tokio::test]
async fn invalid_dns_batches_roll_back_earlier_posts_instead_of_leaving_partial_success() {
    let client = Client::new(false);
    let initial = client.state();
    let (status,result)=client.request("POST",&format!("/client/v4/zones/{ZONE}/dns_records/batch"),json!({
        "posts":[
            {"type":"CNAME","name":"status","content":"octocorp.statuspage.io","ttl":1,"proxied":false},
            {"type":"A","name":"invalid","content":"not-an-address","ttl":1}
        ]
    })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{result}");
    assert_eq!(client.state(), initial);
}

#[tokio::test]
async fn documented_invalid_route_wildcards_and_query_selectors_do_not_report_success() {
    for pattern in [
        "api.octo-corp.com/v2/*.jpg",
        "api.octo-corp.com/v*2/*",
        "api.*.octo-corp.com/v2/*",
        "api.octo-corp.com/v2/?foo=*",
    ] {
        let client = Client::new(false);
        let initial = client.state();
        let (status, result) = client
            .request(
                "POST",
                &format!("/client/v4/zones/{ZONE}/workers/routes"),
                json!({"pattern":pattern,"script":"api-gateway"}),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{pattern}: {result}");
        assert_eq!(initial, client.state());
    }
}

#[tokio::test]
async fn valid_route_patterns_keep_their_supplied_host_spelling_and_path_case() {
    for pattern in [
        "API.OCTO-CORP.COM/v2/*",
        "api.octo-corp.com/V2/*",
        "https://api.octo-corp.com/v2/*",
        "*.octo-corp.com/v2/*",
    ] {
        let client = Client::new(false);
        let (status, result) = client
            .request(
                "POST",
                &format!("/client/v4/zones/{ZONE}/workers/routes"),
                json!({"pattern":pattern,"script":"api-gateway"}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{pattern}: {result}");
        assert_eq!(result["result"]["pattern"], pattern);
    }
}
