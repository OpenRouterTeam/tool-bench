use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_argocd::Argocd;
use mock_core::{
    dataset::Dataset,
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};
use tower::ServiceExt;

const DEGRADED: &str = "find-degraded-app";
const ROLLBACK: &str = "roll-back-billing-worker";
const RESTART: &str = "restart-indexer-worker";
const PAYMENTS: &str = "sync-prod-payments-api";

const WORKER_UID: &str = "d39d8921-d825-549b-a290-0dcda1cafa6c";
const LEGACY_PROD_UID: &str = "41db962d-0f81-5de8-8dc3-ced47c1d11fc";
const LEGACY_STAGING_UID: &str = "675645aa-e28d-5292-a17c-94c47c5d9f17";

fn world(slug: &str) -> SharedWorld {
    world_with(slug, |_| {})
}

fn world_with(slug: &str, prepare: impl FnOnce(&mut Dataset)) -> SharedWorld {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../scenarios/code/argocd")
        .join(slug);
    let mut dataset = Dataset::load(&dir).unwrap();
    prepare(&mut dataset);
    shared(WorldState::from_dataset(dataset).unwrap())
}

fn app(w: &SharedWorld) -> Router {
    service_app(w.clone(), &Argocd)
}

async fn raw(
    app: &Router,
    method: &str,
    uri: &str,
    body: &str,
    token: Option<&str>,
) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "world:8108")
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn call(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, Value) {
    let (status, text) = raw(app, method, uri, body, Some("argocd-toolbench")).await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn state(w: &SharedWorld) -> Value {
    w.lock().unwrap().snapshot()["services"]["argocd"].clone()
}

async fn observe_completed(app: &Router, name: &str, namespace: &str) -> Value {
    for _attempt in 0..6 {
        let (status, application) = call(
            app,
            "GET",
            &format!("/api/v1/applications/{name}?appNamespace={namespace}"),
            "",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{application}");
        if !application["operation"].is_object() {
            return application;
        }
    }
    panic!("controller did not finish the bounded fixture operation");
}

#[tokio::test]
async fn accepted_sync_is_not_completion_and_other_application_reads_cannot_advance_it() {
    let world = world(PAYMENTS);
    let router = app(&world);
    let (_, accepted) = call(
        &router,
        "POST",
        "/api/v1/applications/payments-api/sync",
        r#"{"appNamespace":"payments","prune":true}"#,
    )
    .await;
    assert!(accepted["operation"].is_object());
    assert!(accepted["status"].get("operationState").is_none());
    assert!(state(&world)["resources"].get(LEGACY_PROD_UID).is_some());
    call(
        &router,
        "GET",
        "/api/v1/applications/payments-api?appNamespace=payments-staging",
        "",
    )
    .await;
    assert!(
        state(&world)["applications"]["payments/payments-api"]["status"]
            .get("operationState")
            .is_none()
    );
    let (_, running) = call(
        &router,
        "GET",
        "/api/v1/applications/payments-api?appNamespace=payments",
        "",
    )
    .await;
    assert_eq!(running["status"]["operationState"]["phase"], "Running");
    assert!(
        running["status"]["operationState"]
            .get("finishedAt")
            .is_none()
    );
    assert!(state(&world)["resources"].get(LEGACY_PROD_UID).is_some());
    let completed = observe_completed(&router, "payments-api", "payments").await;
    assert_eq!(completed["status"]["operationState"]["phase"], "Succeeded");
    assert!(state(&world)["resources"].get(LEGACY_PROD_UID).is_none());
}

#[tokio::test]
async fn rollback_updates_live_workloads_and_real_child_ownership_not_commit_health_labels() {
    let world = world(ROLLBACK);
    let router = app(&world);
    let initial = state(&world);
    call(
        &router,
        "PUT",
        "/api/v1/applications/billing-worker",
        &billing_worker_body("v1.42.0", true),
    )
    .await;
    assert_eq!(
        state(&world)["resources"]["4277b63d-291e-513f-9f6c-5d185a2f7bb5"],
        initial["resources"]["4277b63d-291e-513f-9f6c-5d185a2f7bb5"]
    );
    observe_completed(&router, "billing-worker", "argocd").await;
    let final_state = state(&world);
    let deployment = &final_state["resources"]["4277b63d-291e-513f-9f6c-5d185a2f7bb5"];
    assert_eq!(
        deployment["manifest"]["metadata"]["uid"],
        "4277b63d-291e-513f-9f6c-5d185a2f7bb5"
    );
    assert_eq!(
        deployment["manifest"]["spec"]["template"]["spec"]["containers"][0]["image"],
        "ghcr.io/lumen-retail/billing-worker:1.42.0"
    );
    assert_eq!(
        deployment["manifest"]["metadata"]["generation"],
        deployment["manifest"]["status"]["observedGeneration"]
    );
    let pods: Vec<_> = final_state["resources"]
        .as_object()
        .unwrap()
        .values()
        .filter(|resource| resource["app"] == "argocd/billing-worker" && resource["kind"] == "Pod")
        .collect();
    assert_eq!(pods.len(), 2);
    for pod in pods {
        assert!(
            initial["resources"]
                .get(pod["uid"].as_str().unwrap())
                .is_none()
        );
        assert_eq!(
            pod["manifest"]["status"]["containerStatuses"][0]["ready"],
            true
        );
        assert_eq!(
            pod["manifest"]["spec"]["containers"][0]["image"],
            "ghcr.io/lumen-retail/billing-worker:1.42.0"
        );
        let parent = pod["parentRefs"][0]["uid"].as_str().unwrap();
        let replica_set = &final_state["resources"][parent];
        assert_eq!(replica_set["kind"], "ReplicaSet");
        assert_eq!(replica_set["parentRefs"][0]["uid"], deployment["uid"]);
        assert_eq!(
            pod["manifest"]["metadata"]["ownerReferences"][0]["uid"],
            replica_set["uid"]
        );
        assert_eq!(pod["cluster"], deployment["cluster"]);
    }
    for (uid, resource) in initial["resources"].as_object().unwrap() {
        if resource["app"] != "argocd/billing-worker" {
            assert_eq!(resource, &final_state["resources"][uid], "{uid}");
        }
    }
}

#[tokio::test]
async fn changing_git_health_metadata_cannot_make_an_existing_crashing_workload_healthy() {
    let world = world_with(ROLLBACK, |dataset| {
        dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("revisions")
            .get_mut("cf30c03f33e63617efbbbb2da615dff59a7cb010")
            .unwrap()["health"] = json!({"status":"Healthy"});
    });
    let router = app(&world);
    let before = state(&world);
    call(
        &router,
        "POST",
        "/api/v1/applications/billing-worker/sync",
        "",
    )
    .await;
    let completed = observe_completed(&router, "billing-worker", "argocd").await;
    assert_eq!(completed["status"]["health"]["status"], "Degraded");
    assert_eq!(completed["status"]["operationState"]["phase"], "Failed");
    assert_eq!(before["sync_history"], state(&world)["sync_history"]);
}

#[tokio::test]
async fn disabled_automation_persists_the_pin_without_deploying_and_duplicate_operations_reject() {
    let world = world(ROLLBACK);
    let router = app(&world);
    let mut request: Value = serde_json::from_str(&billing_worker_body("v1.42.0", true)).unwrap();
    request["spec"]["syncPolicy"]["automated"]["enabled"] = json!(false);
    call(
        &router,
        "PUT",
        "/api/v1/applications/billing-worker",
        &request.to_string(),
    )
    .await;
    let (_, observed) = call(&router, "GET", "/api/v1/applications/billing-worker", "").await;
    assert!(!observed["operation"].is_object());
    assert_eq!(observed["status"]["health"]["status"], "Degraded");
    call(
        &router,
        "POST",
        "/api/v1/applications/billing-worker/sync",
        "",
    )
    .await;
    let (status, error) = call(
        &router,
        "POST",
        "/api/v1/applications/billing-worker/sync",
        "",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], 9);
    let completed = observe_completed(&router, "billing-worker", "argocd").await;
    assert_eq!(completed["status"]["health"]["status"], "Healthy");
    assert_eq!(
        completed["spec"]["syncPolicy"]["automated"]["enabled"],
        false
    );
}

#[tokio::test]
async fn pending_prune_finalizers_are_not_deleted_or_recorded_as_successful_deployment() {
    let world = world_with(PAYMENTS, |dataset| {
        dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("resources")
            .get_mut(LEGACY_PROD_UID)
            .unwrap()["manifest"]["metadata"]["finalizers"] = json!(["ops.example.test/retain"]);
    });
    let router = app(&world);
    let initial_history = state(&world)["sync_history"].clone();
    call(
        &router,
        "POST",
        "/api/v1/applications/payments-api/sync",
        r#"{"appNamespace":"payments","prune":true}"#,
    )
    .await;
    call(
        &router,
        "GET",
        "/api/v1/applications/payments-api?appNamespace=payments",
        "",
    )
    .await;
    let (_, application) = call(
        &router,
        "GET",
        "/api/v1/applications/payments-api?appNamespace=payments",
        "",
    )
    .await;
    assert_eq!(application["status"]["operationState"]["phase"], "Running");
    assert!(
        application["status"]["operationState"]
            .get("finishedAt")
            .is_none()
    );
    assert!(
        state(&world)["resources"][LEGACY_PROD_UID]["manifest"]["metadata"]["deletionTimestamp"]
            .is_string()
    );
    assert_eq!(initial_history, state(&world)["sync_history"]);
}

#[tokio::test]
async fn history_retention_zero_does_not_erase_authoritative_completion_or_dry_run_distinction() {
    let world = world_with(PAYMENTS, |dataset| {
        dataset
            .services
            .get_mut("argocd")
            .unwrap()
            .collection_mut("applications")
            .get_mut("payments/payments-api")
            .unwrap()["spec"]["revisionHistoryLimit"] = json!(0);
    });
    let router = app(&world);
    let initial = state(&world);
    call(
        &router,
        "POST",
        "/api/v1/applications/payments-api/sync",
        r#"{"appNamespace":"payments","prune":true,"dryRun":true}"#,
    )
    .await;
    observe_completed(&router, "payments-api", "payments").await;
    assert_eq!(initial["resources"], state(&world)["resources"]);
    assert_eq!(initial["sync_history"], state(&world)["sync_history"]);
    call(
        &router,
        "POST",
        "/api/v1/applications/payments-api/sync",
        r#"{"appNamespace":"payments","prune":true}"#,
    )
    .await;
    let completed = observe_completed(&router, "payments-api", "payments").await;
    assert_eq!(completed["status"]["history"], json!([]));
    assert_eq!(
        state(&world)["sync_history"].as_object().unwrap().len(),
        initial["sync_history"].as_object().unwrap().len() + 1
    );
}

fn names(list: &Value) -> Vec<String> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            format!(
                "{}/{}",
                a["metadata"]["namespace"].as_str().unwrap(),
                a["metadata"]["name"].as_str().unwrap()
            )
        })
        .collect()
}

#[tokio::test]
async fn requires_a_bearer_token() {
    let a = app(&world(DEGRADED));
    let (status, body) = raw(&a, "GET", "/api/v1/applications", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"error": "no session information", "code": 16, "message": "no session information"})
    );
    let (status, _) = raw(&a, "GET", "/api/v1/applications", "", Some("nope")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn lists_applications_sorted_with_filters() {
    let a = app(&world(DEGRADED));
    let (status, all) = call(&a, "GET", "/api/v1/applications", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        names(&all),
        [
            "argocd/catalog-api",
            "argocd/checkout-web",
            "argocd/checkout-web-staging",
            "argocd/ingress-nginx",
            "argocd/order-events-consumer"
        ]
    );
    let (_, found) = call(&a, "GET", "/api/v1/applications?search=checkout", "").await;
    assert_eq!(
        names(&found),
        ["argocd/checkout-web", "argocd/checkout-web-staging"]
    );
    let (_, by_project) = call(&a, "GET", "/api/v1/applications?projects=platform", "").await;
    assert_eq!(names(&by_project), ["argocd/ingress-nginx"]);
    let (_, by_label) = call(
        &a,
        "GET",
        "/api/v1/applications?selector=env%3Dproduction,tier%3Dfrontend",
        "",
    )
    .await;
    assert_eq!(names(&by_label), ["argocd/checkout-web"]);
    let (_, none) = call(
        &a,
        "GET",
        "/api/v1/applications?search=nothing-like-this",
        "",
    )
    .await;
    assert_eq!(none, json!({"metadata": {}, "items": []}));
}

#[tokio::test]
async fn gets_application_with_derived_status() {
    let a = app(&world(DEGRADED));
    let (status, app_doc) = call(&a, "GET", "/api/v1/applications/checkout-web", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app_doc["status"]["health"]["status"], "Degraded");
    assert_eq!(app_doc["status"]["sync"]["status"], "Synced");
    assert_eq!(
        app_doc["status"]["sync"]["revision"],
        "a23f2a7b21f788c07d95ceaeb529e54b7ae3dbcd"
    );
    let resources = app_doc["status"]["resources"].as_array().unwrap();
    let kinds: Vec<&str> = resources
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["Deployment", "Service", "ConfigMap", "Ingress"]);
    assert_eq!(resources[0]["health"]["status"], "Degraded");
    assert!(resources[1].get("group").is_none());
    let images = app_doc["status"]["summary"]["images"].as_array().unwrap();
    assert!(images.contains(&json!("ghcr.io/lumen-retail/checkout-web:2.14.0-rc3")));
    assert_eq!(
        app_doc["status"]["summary"]["externalURLs"],
        json!(["https://shop.lumen-retail.com/"])
    );
}

#[tokio::test]
async fn missing_applications_and_projects_are_grpc_not_found() {
    let a = app(&world(PAYMENTS));
    // Both payments-api apps live outside the default namespace.
    let (status, body) = call(&a, "GET", "/api/v1/applications/payments-api", "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        json!({"error": "applications.argoproj.io \"payments-api\" not found", "code": 5, "message": "applications.argoproj.io \"payments-api\" not found"})
    );
    let (status, body) = call(
        &a,
        "GET",
        "/api/v1/applications/payments-api?appNamespace=payments-staging",
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["spec"]["project"], "payments-staging");

    let (status, body) = call(&a, "GET", "/api/v1/projects/payments", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["spec"]["sourceNamespaces"],
        json!(["payments", "argocd"])
    );
    let (status, body) = call(&a, "GET", "/api/v1/projects/nope", "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body["message"],
        "appprojects.argoproj.io \"nope\" not found"
    );

    let (status, body) = call(&a, "GET", "/api/v1/nothing", "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body,
        json!({"error": "Not Found", "code": 5, "message": "Not Found"})
    );
    let (status, _) = call(&a, "PATCH", "/api/v1/clusters", "").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn lists_clusters_with_filters_and_app_counts() {
    let a = app(&world(DEGRADED));
    let (_, all) = call(&a, "GET", "/api/v1/clusters", "").await;
    let items = all["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[1]["name"], "prod-us-east-1");
    assert_eq!(items[1]["info"]["applicationsCount"], 3);
    assert_eq!(items[1]["connectionState"]["status"], "Successful");
    let (_, one) = call(
        &a,
        "GET",
        "/api/v1/clusters?server=https%3A%2F%2Fstaging-use1.k8s.lumen.internal",
        "",
    )
    .await;
    assert_eq!(one["items"].as_array().unwrap().len(), 1);
    assert_eq!(one["items"][0]["info"]["applicationsCount"], 1);
    let (_, none) = call(&a, "GET", "/api/v1/clusters?name=eu-west", "").await;
    assert_eq!(none["items"], json!([]));
}

#[tokio::test]
async fn resource_tree_managed_resources_and_manifests() {
    let a = app(&world(DEGRADED));
    let (_, tree) = call(
        &a,
        "GET",
        "/api/v1/applications/checkout-web/resource-tree",
        "",
    )
    .await;
    let nodes = tree["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 10);
    let bad = nodes
        .iter()
        .find(|n| n["name"] == "checkout-web-7f8b9c6d5-q2x7m")
        .unwrap();
    assert_eq!(bad["kind"], "Pod");
    assert!(bad.get("group").is_none());
    assert_eq!(bad["parentRefs"][0]["kind"], "ReplicaSet");
    assert_eq!(
        bad["info"][0],
        json!({"name": "Status Reason", "value": "ImagePullBackOff"})
    );
    assert!(bad.get("manifest").is_none() && bad.get("app").is_none());

    let (_, managed) = call(
        &a,
        "GET",
        "/api/v1/applications/checkout-web/managed-resources?kind=ConfigMap",
        "",
    )
    .await;
    let items = managed["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let live: Value = serde_json::from_str(items[0]["liveState"].as_str().unwrap()).unwrap();
    assert_eq!(live["data"]["LOG_LEVEL"], "info");
    assert_eq!(items[0]["modified"], false);

    let (status, res) = call(
        &a,
        "GET",
        "/api/v1/applications/checkout-web/resource?appNamespace=argocd&namespace=storefront&resourceName=checkout-web&group=apps&kind=Deployment&version=v1",
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let manifest: Value = serde_json::from_str(res["manifest"].as_str().unwrap()).unwrap();
    assert_eq!(manifest["kind"], "Deployment");
    assert_eq!(manifest["spec"]["replicas"], 3);

    let (status, err) = call(
        &a,
        "GET",
        "/api/v1/applications/checkout-web/resource?namespace=storefront&resourceName=nope&group=apps&kind=Deployment&version=v1",
        "",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], 3);
    assert_eq!(
        err["message"],
        "Deployment apps nope not found as part of application checkout-web"
    );
}

#[tokio::test]
async fn application_and_resource_events() {
    let a = app(&world(DEGRADED));
    let (_, app_events) = call(&a, "GET", "/api/v1/applications/checkout-web/events", "").await;
    let reasons: Vec<&str> = app_events["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        reasons,
        [
            "OperationStarted",
            "OperationCompleted",
            "ResourceUpdated",
            "ResourceUpdated"
        ]
    );
    let (_, pod_events) = call(
        &a,
        "GET",
        "/api/v1/applications/checkout-web/events?appNamespace=argocd&resourceNamespace=storefront&resourceUID=0d3df349-e40e-5789-bc1f-b9740a79c8a0&resourceName=checkout-web-7f8b9c6d5-q2x7m",
        "",
    )
    .await;
    let items = pod_events["items"].as_array().unwrap();
    assert_eq!(items.len(), 5);
    assert!(
        items[2]["message"]
            .as_str()
            .unwrap()
            .contains("checkout-web:2.14.0-rc3: not found")
    );
    assert_eq!(items[2]["type"], "Warning");
}

#[tokio::test]
async fn streams_workload_logs_as_ndjson() {
    let a = app(&world(RESTART));
    let (status, text) = raw(
        &a,
        "GET",
        "/api/v1/applications/search-indexer/logs?appNamespace=argocd&namespace=search&resourceName=search-indexer-worker&group=apps&kind=Deployment&version=v1&follow=false&tailLines=2&container=search-indexer-worker",
        "",
        Some("argocd-toolbench"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // 3 worker pods x 2 tail lines, then the `last` marker.
    assert_eq!(lines.len(), 7);
    assert!(text.ends_with('\n'));
    assert_eq!(
        lines[0]["result"]["podName"],
        "search-indexer-worker-85c7b6d9f-m7n8p"
    );
    assert!(
        lines[0]["result"]["content"]
            .as_str()
            .unwrap()
            .contains("unable to authenticate")
    );
    assert_eq!(lines[6]["result"]["last"], true);

    let (_, text) = raw(
        &a,
        "GET",
        "/api/v1/applications/search-indexer/pods/search-indexer-api-6d8f9b7c4-f3g4h/logs?follow=false&tailLines=100",
        "",
        Some("argocd-toolbench"),
    )
    .await;
    assert_eq!(text.lines().count(), 2);

    let (status, text) = raw(
        &a,
        "GET",
        "/api/v1/applications/nope/logs?follow=false",
        "",
        Some("argocd-toolbench"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let chunk: Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(chunk["error"]["grpc_code"], 5);
    assert_eq!(chunk["error"]["http_code"], 404);
}

#[tokio::test]
async fn lists_and_runs_resource_actions() {
    let w = world(RESTART);
    let a = app(&w);
    let q = "appNamespace=argocd&namespace=search&resourceName=search-indexer-worker&group=apps&kind=Deployment&version=v1";
    let (status, actions) = call(
        &a,
        "GET",
        &format!("/api/v1/applications/search-indexer/resource/actions?{q}"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        actions["actions"][0],
        json!({"name": "restart", "disabled": false, "iconClass": "fa fa-fw fa-redo", "displayName": "Restart"})
    );
    assert_eq!(actions["actions"][2]["name"], "resume");
    assert_eq!(actions["actions"][2]["disabled"], true);

    let path = format!("/api/v1/applications/search-indexer/resource/actions?{q}");
    let (status, err) = call(&a, "POST", &path, r#""resume""#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["message"], "action resume is disabled");
    let (status, err) = call(&a, "POST", &path, r#""explode""#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], 3);

    let (status, body) = call(&a, "POST", &path, r#""restart""#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({}));
    let s = state(&w);
    let annotations =
        &s["resources"][WORKER_UID]["manifest"]["spec"]["template"]["metadata"]["annotations"];
    assert_eq!(
        annotations["kubectl.kubernetes.io/restartedAt"],
        "2026-10-29T14:05:00Z"
    );
    let events: Vec<&Value> = s["events"].as_object().unwrap().values().collect();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]["message"],
        "admin ran action restart on resource apps/Deployment/search-indexer-worker"
    );
    // A ConfigMap has no actions.
    let (_, none) = call(
        &a,
        "GET",
        "/api/v1/applications/search-indexer/resource/actions?namespace=search&resourceName=search-indexer-config&group=&kind=ConfigMap&version=v1",
        "",
    )
    .await;
    assert_eq!(none, json!({"actions": []}));
}

#[tokio::test]
async fn sync_with_prune_deletes_orphans_and_records_history() {
    let w = world(PAYMENTS);
    let a = app(&w);
    let (status, synced) = call(
        &a,
        "POST",
        "/api/v1/applications/payments-api/sync",
        r#"{"appNamespace":"payments","prune":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(synced["operation"].is_object());
    assert_eq!(synced["status"]["sync"]["status"], "OutOfSync");
    let synced = observe_completed(&a, "payments-api", "payments").await;
    let sha = "fe01f33d2a5392fa17c1c35f25d294d24075c28b";
    assert_eq!(synced["status"]["sync"]["status"], "Synced");
    assert_eq!(synced["status"]["sync"]["revision"], sha);
    let op = &synced["status"]["operationState"];
    assert_eq!(op["phase"], "Succeeded");
    assert_eq!(
        op["operation"]["sync"],
        json!({"revision": sha, "prune": true})
    );
    assert_eq!(op["operation"]["initiatedBy"], json!({"username": "admin"}));
    let results = op["syncResult"]["resources"].as_array().unwrap();
    let pruned = results
        .iter()
        .find(|r| r["name"] == "payments-api-legacy-webhooks")
        .unwrap();
    assert_eq!(pruned["status"], "Pruned");
    assert_eq!(
        results[0]["message"],
        "deployment.apps/payments-api configured"
    );
    let history = synced["status"]["history"].as_array().unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[2]["id"], 2);
    assert_eq!(history[2]["revision"], sha);
    assert_eq!(history[2]["deployedAt"], "2026-10-29T14:15:00Z");

    let s = state(&w);
    assert!(s["resources"].get(LEGACY_PROD_UID).is_none());
    assert!(s["resources"].get(LEGACY_STAGING_UID).is_some());
    let deployment = s["resources"]
        .as_object()
        .unwrap()
        .values()
        .find(|r| r["app"] == "payments/payments-api" && r["kind"] == "Deployment")
        .unwrap();
    assert_eq!(deployment["status"], "Synced");
    assert_eq!(
        deployment["manifest"]["spec"]["template"]["spec"]["containers"][0]["image"],
        "ghcr.io/lumen-retail/payments-api:3.18.0"
    );
    assert_eq!(
        s["applications"]["payments-staging/payments-api"]["status"]["sync"]["status"],
        "OutOfSync"
    );
}

#[tokio::test]
async fn sync_without_prune_stays_out_of_sync() {
    let w = world(PAYMENTS);
    let a = app(&w);
    let (_, _) = call(
        &a,
        "POST",
        "/api/v1/applications/payments-api/sync",
        r#"{"appNamespace":"payments"}"#,
    )
    .await;
    let synced = observe_completed(&a, "payments-api", "payments").await;
    assert_eq!(synced["status"]["sync"]["status"], "OutOfSync");
    let skipped = synced["status"]["operationState"]["syncResult"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "payments-api-legacy-webhooks")
        .unwrap()
        .clone();
    assert_eq!(skipped["status"], "PruneSkipped");
    assert_eq!(skipped["message"], "ignored (requires pruning)");
    assert!(state(&w)["resources"].get(LEGACY_PROD_UID).is_some());

    // Dry run changes nothing but the operation state.
    let w = world(PAYMENTS);
    let a = app(&w);
    let (_, _) = call(
        &a,
        "POST",
        "/api/v1/applications/payments-api/sync",
        r#"{"appNamespace":"payments","prune":true,"dryRun":true}"#,
    )
    .await;
    let dry = observe_completed(&a, "payments-api", "payments").await;
    assert_eq!(dry["status"]["history"].as_array().unwrap().len(), 2);
    assert_eq!(dry["status"]["sync"]["status"], "OutOfSync");
    assert!(state(&w)["resources"].get(LEGACY_PROD_UID).is_some());
}

#[tokio::test]
async fn sync_errors_match_argocd() {
    let a = app(&world(ROLLBACK));
    let (status, err) = call(
        &a,
        "POST",
        "/api/v1/applications/billing-worker/sync",
        r#"{"revision":"v1.42.0"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        err,
        json!({"error": "Cannot sync to v1.42.0: auto-sync currently set to main", "code": 9, "message": "Cannot sync to v1.42.0: auto-sync currently set to main"})
    );
    let (status, err) = call(&a, "POST", "/api/v1/applications/ghost/sync", "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err["code"], 5);

    let a = app(&world(PAYMENTS));
    let (status, err) = call(
        &a,
        "POST",
        "/api/v1/applications/payments-api/sync",
        r#"{"appNamespace":"payments","revision":"v9.9.9"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        err["message"],
        "rpc error: code = Unknown desc = unable to resolve 'v9.9.9' to a commit SHA"
    );
}

fn billing_worker_body(target: &str, automated: bool) -> String {
    let mut sync_policy = json!({
        "syncOptions": ["CreateNamespace=true", "ApplyOutOfSyncOnly=true"],
        "retry": {"limit": 5, "backoff": {"duration": "5s", "maxDuration": "3m0s", "factor": 2}}
    });
    if automated {
        sync_policy["automated"] = json!({"prune": true, "selfHeal": true});
    }
    json!({
        "metadata": {"name": "billing-worker", "namespace": "argocd"},
        "spec": {
            "project": "billing",
            "source": {"repoURL": "https://github.com/lumen-retail/billing-worker-deploy.git", "path": "overlays/production", "targetRevision": target},
            "syncPolicy": sync_policy,
            "destination": {"server": "https://prod-use1.k8s.lumen.internal", "namespace": "billing"}
        }
    })
    .to_string()
}

#[tokio::test]
async fn update_with_auto_sync_rolls_back() {
    let w = world(ROLLBACK);
    let a = app(&w);
    let (status, updated) = call(
        &a,
        "PUT",
        "/api/v1/applications/billing-worker",
        &billing_worker_body("v1.42.0", true),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["status"]["sync"]["status"], "OutOfSync");
    assert!(updated["operation"].is_object());
    let updated = observe_completed(&a, "billing-worker", "argocd").await;
    let b = "b0826e986c9c73e0088c6ecb2a14458d247d2205";
    assert_eq!(updated["spec"]["source"]["targetRevision"], "v1.42.0");
    assert_eq!(updated["status"]["sync"]["status"], "Synced");
    assert_eq!(updated["status"]["sync"]["revision"], b);
    assert_eq!(updated["status"]["health"]["status"], "Healthy");
    let op = &updated["status"]["operationState"];
    assert_eq!(op["operation"]["initiatedBy"], json!({"automated": true}));
    assert_eq!(op["syncResult"]["revision"], b);
    assert_eq!(updated["metadata"]["generation"], 7);
    // Update replaces labels with the request's (none here).
    assert!(updated["metadata"].get("labels").is_none());
    let reasons: Vec<String> = state(&w)["events"]
        .as_object()
        .unwrap()
        .values()
        .filter(|e| e["involvedObject"]["name"] == "billing-worker")
        .map(|e| e["message"].as_str().unwrap().to_owned())
        .collect();
    assert!(reasons.contains(&"admin updated application spec".to_owned()));
    assert!(reasons.contains(&"Updated health status: Degraded -> Healthy".to_owned()));
}

#[tokio::test]
async fn update_without_auto_sync_goes_out_of_sync_then_syncs() {
    let w = world(ROLLBACK);
    let a = app(&w);
    let (_, updated) = call(
        &a,
        "PUT",
        "/api/v1/applications/billing-worker",
        &billing_worker_body("v1.42.0", false),
    )
    .await;
    assert_eq!(updated["status"]["sync"]["status"], "OutOfSync");
    assert_eq!(updated["status"]["history"].as_array().unwrap().len(), 3);
    let (status, _) = call(&a, "POST", "/api/v1/applications/billing-worker/sync", "").await;
    assert_eq!(status, StatusCode::OK);
    let synced = observe_completed(&a, "billing-worker", "argocd").await;
    assert_eq!(synced["status"]["sync"]["status"], "Synced");
    assert_eq!(synced["status"]["history"][3]["id"], 3);

    let (status, err) = call(
        &a,
        "PUT",
        "/api/v1/applications/billing-worker",
        &billing_worker_body("v0.0.1", false),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .ends_with("unable to resolve 'v0.0.1' to a commit SHA")
    );
    let (status, _) = call(
        &a,
        "PUT",
        "/api/v1/applications/ghost",
        &billing_worker_body("main", false),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn creates_validates_and_deletes_applications() {
    let w = world(ROLLBACK);
    let a = app(&w);
    let body = json!({
        "metadata": {"name": "billing-worker-canary", "namespace": "argocd"},
        "spec": {
            "project": "billing",
            "source": {"repoURL": "https://github.com/lumen-retail/billing-worker-deploy.git", "path": "overlays/canary", "targetRevision": "v1.42.0"},
            "syncPolicy": {"syncOptions": [], "retry": {"limit": 1, "backoff": {"duration": "5s", "maxDuration": "1m", "factor": 2}}},
            "destination": {"name": "prod-us-east-1", "namespace": "billing"}
        }
    });
    let (status, created) = call(&a, "POST", "/api/v1/applications", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["metadata"]["generation"], 1);
    assert_eq!(
        created["metadata"]["creationTimestamp"],
        "2026-10-29T14:05:00Z"
    );
    assert_eq!(created["status"]["sync"]["status"], "OutOfSync");
    assert_eq!(created["status"]["health"]["status"], "Missing");
    assert!(
        state(&w)["applications"]
            .get("argocd/billing-worker-canary")
            .is_some()
    );
    // Same spec again: idempotent. Different spec: rejected without upsert.
    let (status, _) = call(&a, "POST", "/api/v1/applications", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    let mut changed = body.clone();
    changed["spec"]["source"]["targetRevision"] = json!("main");
    let (status, err) = call(&a, "POST", "/api/v1/applications", &changed.to_string()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        err["message"],
        "existing application spec is different, use upsert flag to force update"
    );

    let mut bad = body.clone();
    bad["spec"]["project"] = json!("nope");
    let (_, err) = call(&a, "POST", "/api/v1/applications", &bad.to_string()).await;
    assert_eq!(
        err["message"],
        "application spec for billing-worker-canary is invalid: InvalidSpecError: application references project nope which does not exist"
    );
    let mut bad = body.clone();
    bad["spec"]["destination"]["namespace"] = json!("kube-system");
    let (status, err) = call(&a, "POST", "/api/v1/applications", &bad.to_string()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("do not match any of the allowed destinations in project 'billing'")
    );
    let mut bad = body.clone();
    bad["spec"]["source"]["repoURL"] = json!("https://gitlab.com/someone/else.git");
    let (_, err) = call(&a, "POST", "/api/v1/applications", &bad.to_string()).await;
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("is not permitted in project 'billing'")
    );
    let (status, err) = call(&a, "POST", "/api/v1/applications", "{nope").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], 3);

    // Cascade delete removes the app and its resources; orphan delete keeps them.
    let (status, body) = call(
        &a,
        "DELETE",
        "/api/v1/applications/billing-api?cascade=true",
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({}));
    let (status, body) = call(
        &a,
        "DELETE",
        "/api/v1/applications/billing-worker-staging?cascade=false",
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let s = state(&w);
    assert!(s["applications"].get("argocd/billing-api").is_none());
    let resources = s["resources"].as_object().unwrap();
    assert!(!resources.values().any(|r| r["app"] == "argocd/billing-api"));
    assert!(
        resources
            .values()
            .any(|r| r["app"].is_null() && r["name"] == "billing-worker")
    );
    let (status, _) = call(&a, "DELETE", "/api/v1/applications/billing-api", "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
