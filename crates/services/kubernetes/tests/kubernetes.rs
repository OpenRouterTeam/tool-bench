use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use mock_core::{
    dataset::Dataset,
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use mock_kubernetes::{Kubernetes, util::b64_decode};
use serde_json::{Value, json};
use tower::ServiceExt;

const CRASHLOOP: &str = "crashloop-root-cause";
const HELM: &str = "uninstall-finished-loadtest";
const BAD_POD: &str = "ledger-worker-zx4mrndj8b-gdg8j";

fn world(slug: &str) -> SharedWorld {
    world_with(slug, |_| {})
}

fn world_with(slug: &str, prepare: impl FnOnce(&mut Dataset)) -> SharedWorld {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../scenarios/ops/kubernetes")
        .join(slug);
    let mut dataset = Dataset::load(&dir).unwrap();
    prepare(&mut dataset);
    shared(WorldState::from_dataset(dataset).unwrap())
}

struct Resp {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
    text: String,
}

async fn send(app: &Router, method: &str, uri: &str, headers: &[(&str, &str)], body: &str) -> Resp {
    send_bytes(app, method, uri, headers, body.as_bytes()).await
}

async fn send_bytes(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Resp {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "world:8115");
    let mut has_ct = false;
    for (k, v) in headers {
        has_ct |= k.eq_ignore_ascii_case("content-type");
        req = req.header(*k, *v);
    }
    if !has_ct {
        req = req.header("content-type", "application/json");
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::from(body.to_vec())).unwrap())
        .await
        .unwrap();
    let (parts, body) = resp.into_parts();
    let bytes = to_bytes(body, usize::MAX).await.unwrap();
    Resp {
        status: parts.status,
        headers: parts.headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        text: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn get(app: &Router, uri: &str) -> Resp {
    send(app, "GET", uri, &[], "").await
}

fn names(list: &Value) -> Vec<String> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["metadata"]["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn version_and_legacy_discovery() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let v = get(&app, "/version").await;
    assert_eq!(v.body["gitVersion"], "v1.33.4");

    let api = get(&app, "/api").await;
    assert_eq!(api.body["kind"], "APIVersions");
    assert_eq!(api.body["versions"], json!(["v1"]));

    let apis = get(&app, "/apis").await;
    let groups: Vec<&str> = apis.body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["name"].as_str().unwrap())
        .collect();
    assert!(groups.contains(&"apps") && groups.contains(&"metrics.k8s.io"));
    let autoscaling = apis.body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["name"] == "autoscaling")
        .unwrap();
    assert_eq!(autoscaling["preferredVersion"]["version"], "v2");

    let core = get(&app, "/api/v1").await;
    let resources: Vec<&str> = core.body["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(resources.contains(&"pods") && resources.contains(&"pods/log"));

    let apps = get(&app, "/apis/apps/v1").await;
    let scale = apps.body["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "deployments/scale")
        .unwrap();
    assert_eq!(scale["kind"], "Scale");
    assert_eq!(scale["group"], "autoscaling");

    let missing = get(&app, "/apis/tekton.dev/v1/namespaces/x/pipelineruns").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(missing.body["kind"], "Status");
    assert_eq!(missing.body["reason"], "NotFound");
    assert_eq!(
        missing.body["message"],
        "the server could not find the requested resource"
    );
}

#[tokio::test]
async fn wrong_token_is_unauthorized() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let r = send(
        &app,
        "GET",
        "/api/v1/namespaces",
        &[("authorization", "Bearer nope")],
        "",
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body["reason"], "Unauthorized");
    let ok = send(
        &app,
        "GET",
        "/api/v1/namespaces",
        &[("authorization", "Bearer k8s-toolbench")],
        "",
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);
}

#[tokio::test]
async fn lists_with_selectors_and_paging() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let pods = get(&app, "/api/v1/namespaces/payments/pods").await;
    assert_eq!(pods.body["kind"], "PodList");
    assert_eq!(pods.body["apiVersion"], "v1");
    let all = names(&pods.body);
    assert_eq!(all.len(), 6);
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted, "lists are ordered by key like etcd");
    assert!(
        pods.body["items"][0].get("kind").is_none(),
        "built-in list items omit type meta"
    );

    let ledger = get(
        &app,
        "/api/v1/pods?labelSelector=app.kubernetes.io%2Fname%3Dledger-worker",
    )
    .await;
    assert_eq!(names(&ledger.body).len(), 3);
    let in_set = get(
        &app,
        "/api/v1/namespaces/payments/pods?labelSelector=app.kubernetes.io%2Fname+in+%28ledger-worker%2Crefund-scheduler%29",
    )
    .await;
    assert_eq!(names(&in_set.body).len(), 3);
    let by_field = get(
        &app,
        "/api/v1/namespaces/payments/events?fieldSelector=involvedObject.name%3Dledger-worker-zx4mrndj8b-gdg8j%2Ctype%3DWarning",
    )
    .await;
    assert_eq!(names(&by_field.body).len(), 1);
    let bad = get(&app, "/api/v1/pods?fieldSelector=nonsense").await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    let page1 = get(&app, "/api/v1/namespaces/payments/pods?limit=4").await;
    assert_eq!(names(&page1.body), all[..4]);
    assert_eq!(page1.body["metadata"]["remainingItemCount"], 2);
    let token = page1.body["metadata"]["continue"].as_str().unwrap();
    let page2 = get(
        &app,
        &format!("/api/v1/namespaces/payments/pods?limit=4&continue={token}"),
    )
    .await;
    assert_eq!(names(&page2.body), all[4..]);
    assert!(page2.body["metadata"].get("continue").is_none());
}

#[tokio::test]
async fn table_output_matches_kubectl_columns() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let t = send(
        &app,
        "GET",
        "/api/v1/namespaces/payments/pods",
        &[(
            "accept",
            "application/json;as=Table;v=v1;g=meta.k8s.io,application/json",
        )],
        "",
    )
    .await;
    assert_eq!(t.body["kind"], "Table");
    let cols: Vec<&str> = t.body["columnDefinitions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(&cols[..5], ["Name", "Ready", "Status", "Restarts", "Age"]);
    let row = t.body["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["cells"][0] == BAD_POD)
        .unwrap();
    assert_eq!(row["cells"][1], "0/1");
    assert_eq!(row["cells"][2], "CrashLoopBackOff");
    assert_eq!(row["cells"][3], "14 (6m16s ago)");
    assert_eq!(row["object"]["kind"], "PartialObjectMetadata");
    assert_eq!(row["object"]["metadata"]["namespace"], "payments");
}

#[tokio::test]
async fn not_found_status_shape() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let r = get(&app, "/apis/apps/v1/namespaces/payments/deployments/nope").await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(
        r.body,
        json!({
            "kind": "Status", "apiVersion": "v1", "metadata": {}, "status": "Failure",
            "message": "deployments.apps \"nope\" not found", "reason": "NotFound",
            "details": {"name": "nope", "group": "apps", "kind": "deployments"}, "code": 404
        })
    );
}

#[tokio::test]
async fn create_conflict_and_validation() {
    let w = world(CRASHLOOP);
    let app = service_app(w.clone(), &Kubernetes);
    let cm = r#"{"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"ledger-flags"},"data":{"RETRY":"3"}}"#;
    let created = send(
        &app,
        "POST",
        "/api/v1/namespaces/payments/configmaps",
        &[],
        cm,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["metadata"]["namespace"], "payments");
    assert_eq!(
        created.body["metadata"]["creationTimestamp"],
        "2026-11-03T08:40:00Z"
    );
    assert!(created.body["metadata"]["uid"].as_str().unwrap().len() == 36);

    let again = send(
        &app,
        "POST",
        "/api/v1/namespaces/payments/configmaps",
        &[],
        cm,
    )
    .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.body["reason"], "AlreadyExists");
    assert_eq!(
        again.body["message"],
        "configmaps \"ledger-flags\" already exists"
    );

    let no_ns = send(&app, "POST", "/api/v1/namespaces/ghost/configmaps", &[], cm).await;
    assert_eq!(no_ns.status, StatusCode::NOT_FOUND);
    assert_eq!(no_ns.body["message"], "namespaces \"ghost\" not found");

    let bad_name = send(
        &app,
        "POST",
        "/api/v1/namespaces/payments/configmaps",
        &[],
        r#"{"metadata":{"name":"Bad_Name"}}"#,
    )
    .await;
    assert_eq!(bad_name.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad_name.body["reason"], "Invalid");

    let no_containers = send(
        &app,
        "POST",
        "/api/v1/namespaces/payments/pods",
        &[],
        r#"{"metadata":{"name":"p"},"spec":{}}"#,
    )
    .await;
    assert_eq!(no_containers.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        no_containers.body["message"],
        "Pod \"p\" is invalid: spec.containers: Required value"
    );

    let state = w.lock().unwrap().snapshot();
    assert_eq!(
        state["services"]["kubernetes"]["configmaps"]["payments/ledger-flags"]["data"]["RETRY"],
        "3"
    );
}

#[tokio::test]
async fn patch_flavours_and_server_side_apply() {
    let w = world(CRASHLOOP);
    let app = service_app(w.clone(), &Kubernetes);
    let path = "/apis/apps/v1/namespaces/payments/deployments/ledger-worker";
    let merge = send(
        &app,
        "PATCH",
        path,
        &[("content-type", "application/merge-patch+json")],
        r#"{"metadata":{"labels":{"tier":"backend"}}}"#,
    )
    .await;
    assert_eq!(merge.status, StatusCode::OK);
    assert_eq!(merge.body["metadata"]["labels"]["tier"], "backend");
    assert_eq!(
        merge.body["metadata"]["generation"], 7,
        "metadata-only change keeps generation"
    );

    let apply = send(
        &app,
        "PATCH",
        &format!("{path}?fieldManager=kubernetes-mcp-server&force=true"),
        &[("content-type", "application/apply-patch+yaml")],
        r#"{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"ledger-worker","namespace":"payments"},"spec":{"template":{"spec":{"containers":[{"name":"ledger-worker","resources":{"limits":{"memory":"512Mi"}}}]}}}}"#,
    )
    .await;
    assert_eq!(apply.status, StatusCode::OK);
    let c = &apply.body["spec"]["template"]["spec"]["containers"][0];
    assert_eq!(c["resources"]["limits"]["memory"], "512Mi");
    assert_eq!(
        c["image"], "ghcr.io/octo-corp/ledger-worker:1.9.0",
        "apply merges containers by name"
    );
    assert_eq!(apply.body["metadata"]["generation"], 8);

    let jp = send(
        &app,
        "PATCH",
        path,
        &[("content-type", "application/json-patch+json")],
        r#"[{"op":"replace","path":"/spec/replicas","value":3}]"#,
    )
    .await;
    assert_eq!(jp.body["spec"]["replicas"], 3);
    let bad_jp = send(
        &app,
        "PATCH",
        path,
        &[("content-type", "application/json-patch+json")],
        r#"[{"op":"replace","path":"/spec/nothing","value":3}]"#,
    )
    .await;
    assert_eq!(bad_jp.status, StatusCode::UNPROCESSABLE_ENTITY);

    let created = send(
        &app,
        "PATCH",
        "/api/v1/namespaces/payments/configmaps/new-cm?fieldManager=x",
        &[("content-type", "application/apply-patch+yaml")],
        r#"{"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"new-cm"},"data":{"a":"1"}}"#,
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);

    let missing = send(
        &app,
        "PATCH",
        "/apis/apps/v1/namespaces/payments/deployments/nope",
        &[("content-type", "application/merge-patch+json")],
        "{}",
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let snap = w.lock().unwrap().snapshot();
    let dep = &snap["services"]["kubernetes"]["deployments.apps"]["payments/ledger-worker"];
    assert_eq!(dep["spec"]["replicas"], 3);
    assert_eq!(dep["metadata"]["labels"]["tier"], "backend");
}

#[tokio::test]
async fn update_checks_resource_version() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let path = "/api/v1/namespaces/payments/services/ledger-worker";
    let mut svc = get(&app, path).await.body;
    svc["metadata"]["labels"]["x"] = json!("y");
    let ok = send(&app, "PUT", path, &[], &svc.to_string()).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_ne!(
        ok.body["metadata"]["resourceVersion"],
        svc["metadata"]["resourceVersion"]
    );
    let stale = send(&app, "PUT", path, &[], &svc.to_string()).await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(stale.body["reason"], "Conflict");
}

#[tokio::test]
async fn scale_subresource_updates_deployment() {
    let w = world(CRASHLOOP);
    let app = service_app(w.clone(), &Kubernetes);
    let path = "/apis/apps/v1/namespaces/payments/deployments/payments-api/scale";
    let scale = get(&app, path).await;
    assert_eq!(scale.body["kind"], "Scale");
    assert_eq!(scale.body["apiVersion"], "autoscaling/v1");
    assert_eq!(scale.body["spec"]["replicas"], 3);
    assert_eq!(
        scale.body["status"]["selector"],
        "app.kubernetes.io/name=payments-api"
    );

    let mut next = scale.body.clone();
    next["spec"]["replicas"] = json!(6);
    let put = send(&app, "PUT", path, &[], &next.to_string()).await;
    assert_eq!(put.status, StatusCode::OK);
    assert_eq!(put.body["spec"]["replicas"], 6);
    let negative = send(
        &app,
        "PATCH",
        path,
        &[("content-type", "application/merge-patch+json")],
        r#"{"spec":{"replicas":-1}}"#,
    )
    .await;
    assert_eq!(negative.status, StatusCode::UNPROCESSABLE_ENTITY);
    let snap = w.lock().unwrap().snapshot();
    let dep = &snap["services"]["kubernetes"]["deployments.apps"]["payments/payments-api"];
    assert_eq!(dep["spec"]["replicas"], 6);
    assert_eq!(dep["metadata"]["generation"], 2);
    assert_eq!(dep["status"]["observedGeneration"], 2);
    assert_eq!(dep["status"]["readyReplicas"], 6);
    let k8s = &snap["services"]["kubernetes"];
    let owned = |plural: &str, kind: &str, owner: &str| -> Vec<Value> {
        k8s[plural]
            .as_object()
            .map(|m| {
                m.values()
                    .filter(|o| {
                        o["metadata"]["ownerReferences"]
                            .as_array()
                            .is_some_and(|r| {
                                r.iter().any(|r| r["kind"] == kind && r["name"] == owner)
                            })
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    let rs: Vec<Value> = owned("replicasets.apps", "Deployment", "payments-api")
        .into_iter()
        .filter(|r| r["metadata"]["namespace"] == "payments")
        .collect();
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0]["spec"]["replicas"], 6);
    assert_eq!(rs[0]["status"]["readyReplicas"], 6);
    let rs_name = rs[0]["metadata"]["name"].as_str().unwrap();
    assert_eq!(owned("pods", "ReplicaSet", rs_name).len(), 6);

    let pods_of = |snap: &Value, rs: &str| -> Vec<Value> {
        snap["services"]["kubernetes"]["pods"]
            .as_object()
            .unwrap()
            .values()
            .filter(|p| {
                p["metadata"]["ownerReferences"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|r| r["name"] == rs))
            })
            .cloned()
            .collect()
    };
    let scale_to = |n: i64| {
        let app = app.clone();
        async move {
            let body = format!(r#"{{"spec":{{"replicas":{n}}}}}"#);
            let r = send(
                &app,
                "PATCH",
                path,
                &[("content-type", "application/merge-patch+json")],
                &body,
            )
            .await;
            assert_eq!(r.status, StatusCode::OK);
        }
    };
    scale_to(0).await;
    assert!(pods_of(&w.lock().unwrap().snapshot(), rs_name).is_empty());
    scale_to(4).await;
    let snap = w.lock().unwrap().snapshot();
    let pods = pods_of(&snap, rs_name);
    assert_eq!(pods.len(), 4);
    let mut ids: Vec<&str> = pods
        .iter()
        .flat_map(|p| p["status"]["containerStatuses"].as_array().unwrap())
        .map(|c| c["containerID"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 4);
    assert!(pods.iter().all(|p| p["status"]["phase"] == "Running"));
    let dep = &snap["services"]["kubernetes"]["deployments.apps"]["payments/payments-api"];
    assert_eq!(dep["status"]["readyReplicas"], 4);
}

#[tokio::test]
async fn scale_counts_unready_pods_as_unavailable() {
    let w = world(CRASHLOOP);
    let app = service_app(w.clone(), &Kubernetes);
    let path = "/apis/apps/v1/namespaces/payments/deployments/ledger-worker/scale";
    let before = w.lock().unwrap().snapshot();
    let dep = &before["services"]["kubernetes"]["deployments.apps"]["payments/ledger-worker"];
    let replicas = dep["spec"]["replicas"].as_i64().unwrap();
    let ready = dep["status"]["readyReplicas"].as_i64().unwrap_or(0);
    assert!(ready < replicas);
    let body = format!(r#"{{"spec":{{"replicas":{}}}}}"#, replicas + 1);
    let r = send(
        &app,
        "PATCH",
        path,
        &[("content-type", "application/merge-patch+json")],
        &body,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let snap = w.lock().unwrap().snapshot();
    let dep = &snap["services"]["kubernetes"]["deployments.apps"]["payments/ledger-worker"];
    assert_eq!(dep["status"]["replicas"], replicas + 1);
    assert_eq!(dep["status"]["readyReplicas"], ready + 1);
    assert_eq!(dep["status"]["unavailableReplicas"], replicas - ready);
}

/// The ReplicaSets of production `payments-api` as `(name, revision, replicas, pods)`, oldest
/// revision first.
fn payments_api_sets(snap: &Value) -> Vec<(String, String, i64, usize)> {
    let state = &snap["services"]["kubernetes"];
    let mut sets: Vec<_> = state["replicasets.apps"]
        .as_object()
        .unwrap()
        .values()
        .filter(|rs| {
            rs["metadata"]["namespace"] == "payments"
                && rs["metadata"]["ownerReferences"][0]["name"] == "payments-api"
        })
        .map(|rs| {
            let name = rs["metadata"]["name"].as_str().unwrap().to_owned();
            let pods = state["pods"]
                .as_object()
                .unwrap()
                .values()
                .filter(|pod| pod["metadata"]["ownerReferences"][0]["name"] == name.as_str())
                .count();
            let revision = rs["metadata"]["annotations"]["deployment.kubernetes.io/revision"]
                .as_str()
                .unwrap()
                .to_owned();
            (
                name,
                revision,
                rs["spec"]["replicas"].as_i64().unwrap(),
                pods,
            )
        })
        .collect();
    sets.sort_by(|left, right| left.1.cmp(&right.1));
    sets
}

#[tokio::test]
async fn a_template_change_rolls_out_a_new_replica_set_and_a_respelled_quantity_does_not() {
    let w = world(CRASHLOOP);
    let app = service_app(w.clone(), &Kubernetes);
    let path = "/apis/apps/v1/namespaces/payments/deployments/payments-api";
    let strategic = [("content-type", "application/strategic-merge-patch+json")];
    let original = get(&app, path).await.body;
    let container = original["spec"]["template"]["spec"]["containers"][0].clone();
    let name = container["name"].as_str().unwrap();
    let limit = |memory: &str| {
        json!({"spec":{"template":{"spec":{"containers":[{
            "name":name,"resources":{"limits":{"memory":memory}}
        }]}}}})
        .to_string()
    };

    // A new limit and a new replica count in one write: one rollout, already at five.
    let mut patch: Value = serde_json::from_str(&limit("2Gi")).unwrap();
    patch["spec"]["replicas"] = json!(5);
    let r = send(&app, "PATCH", path, &strategic, &patch.to_string()).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let snap = w.lock().unwrap().snapshot();
    let sets = payments_api_sets(&snap);
    assert_eq!(sets.len(), 2, "{sets:?}");
    assert_eq!(sets[0], ("payments-api-86zf47v9z".into(), "1".into(), 0, 0));
    let (rolled, revision, replicas, pods) = sets[1].clone();
    assert_ne!(rolled, "payments-api-86zf47v9z");
    assert_eq!((revision.as_str(), replicas, pods), ("2", 5, 5));
    let state = &snap["services"]["kubernetes"];
    for pod in state["pods"].as_object().unwrap().values() {
        if pod["metadata"]["ownerReferences"][0]["name"] == rolled.as_str() {
            assert_eq!(pod["status"]["phase"], "Running");
            assert_eq!(
                pod["spec"]["containers"][0]["resources"]["limits"]["memory"],
                "2Gi"
            );
            assert_eq!(
                pod["metadata"]["labels"]["pod-template-hash"],
                state["replicasets.apps"][format!("payments/{rolled}")]["metadata"]["labels"]["pod-template-hash"]
            );
        }
    }
    let dep = &state["deployments.apps"]["payments/payments-api"];
    assert_eq!(
        dep["metadata"]["annotations"]["deployment.kubernetes.io/revision"],
        "2"
    );
    assert_eq!(
        dep["metadata"]["generation"],
        original["metadata"]["generation"].as_i64().unwrap() + 1
    );
    assert_eq!(
        dep["status"]["observedGeneration"],
        dep["metadata"]["generation"]
    );
    for field in [
        "replicas",
        "updatedReplicas",
        "readyReplicas",
        "availableReplicas",
    ] {
        assert_eq!(dep["status"][field], 5, "{field}");
    }

    // The same quantity spelled differently is the same template: nothing rolls.
    let r = send(&app, "PATCH", path, &strategic, &limit("2048Mi")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    assert_eq!(payments_api_sets(&w.lock().unwrap().snapshot()), sets);

    // Going back to the first template reuses its ReplicaSet as the newest revision.
    let memory = container["resources"]["limits"]["memory"].clone();
    let back = if memory.is_null() {
        json!([{"op":"remove","path":"/spec/template/spec/containers/0/resources/limits/memory"}])
    } else {
        json!([{"op":"replace","path":"/spec/template/spec/containers/0/resources/limits/memory","value":memory}])
    };
    let r = send(
        &app,
        "PATCH",
        path,
        &[("content-type", "application/json-patch+json")],
        &back.to_string(),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let sets = payments_api_sets(&w.lock().unwrap().snapshot());
    assert_eq!(
        sets,
        [
            (rolled, "2".to_owned(), 0, 0),
            ("payments-api-86zf47v9z".to_owned(), "3".to_owned(), 5, 5),
        ]
    );
}

#[tokio::test]
async fn pod_logs_previous_tail_and_errors() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let base = format!("/api/v1/namespaces/payments/pods/{BAD_POD}/log");
    let prev = get(&app, &format!("{base}?previous=true")).await;
    assert_eq!(prev.status, StatusCode::OK);
    assert!(
        prev.text
            .contains("password authentication failed for user \"ledger\"")
    );
    let tail = get(&app, &format!("{base}?tailLines=1")).await;
    assert_eq!(tail.text.lines().count(), 1);
    assert!(tail.text.contains("FATAL"));
    let bad_container = get(&app, &format!("{base}?container=sidecar")).await;
    assert_eq!(bad_container.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        bad_container.body["message"],
        format!("container sidecar is not valid for pod {BAD_POD}")
    );
    let healthy = get(
        &app,
        "/api/v1/namespaces/payments/pods/ledger-worker-zx4mrndj8b-dwvg9/log?previous=true",
    )
    .await;
    assert_eq!(healthy.status, StatusCode::BAD_REQUEST);
    let missing = get(&app, "/api/v1/namespaces/payments/pods/nope/log").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_replicaset_pod_gets_it_replaced() {
    let w = world(CRASHLOOP);
    let app = service_app(w.clone(), &Kubernetes);
    let del = send(
        &app,
        "DELETE",
        &format!("/api/v1/namespaces/payments/pods/{BAD_POD}"),
        &[],
        "",
    )
    .await;
    assert_eq!(del.status, StatusCode::OK);
    assert!(del.body["metadata"]["deletionTimestamp"].is_string());
    let pods = get(
        &app,
        "/api/v1/namespaces/payments/pods?labelSelector=app.kubernetes.io%2Fname%3Dledger-worker",
    )
    .await;
    let n = names(&pods.body);
    assert_eq!(n.len(), 2);
    assert!(!n.contains(&BAD_POD.to_owned()));
    let replacement = pods.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["metadata"]["name"] != "ledger-worker-zx4mrndj8b-dwvg9")
        .unwrap();
    assert_eq!(replacement["status"]["phase"], "Pending");
    assert!(
        replacement["metadata"]["name"]
            .as_str()
            .unwrap()
            .starts_with("ledger-worker-zx4mrndj8b-")
    );
    let again = send(
        &app,
        "DELETE",
        &format!("/api/v1/namespaces/payments/pods/{BAD_POD}"),
        &[],
        "",
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_deployment_cascades() {
    let w = world(CRASHLOOP);
    let app = service_app(w.clone(), &Kubernetes);
    let del = send(
        &app,
        "DELETE",
        "/apis/apps/v1/namespaces/payments/deployments/payments-api",
        &[],
        r#"{"propagationPolicy":"Background"}"#,
    )
    .await;
    assert_eq!(del.status, StatusCode::OK);
    let rs = get(&app, "/apis/apps/v1/namespaces/payments/replicasets").await;
    assert!(
        names(&rs.body)
            .iter()
            .all(|n| !n.starts_with("payments-api"))
    );
    let pods = get(&app, "/api/v1/namespaces/payments/pods").await;
    assert!(
        names(&pods.body)
            .iter()
            .all(|n| !n.starts_with("payments-api"))
    );
    assert_eq!(names(&pods.body).len(), 3);
}

#[tokio::test]
async fn access_reviews_allow_all_but_exec() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let path = "/apis/authorization.k8s.io/v1/selfsubjectaccessreviews";
    let list = send(
        &app,
        "POST",
        path,
        &[],
        r#"{"spec":{"resourceAttributes":{"verb":"list","resource":"pods"}}}"#,
    )
    .await;
    assert_eq!(list.status, StatusCode::CREATED);
    assert_eq!(list.body["status"]["allowed"], true);
    let exec = send(&app, "POST", path, &[], r#"{"spec":{"resourceAttributes":{"verb":"create","resource":"pods","subresource":"exec"}}}"#).await;
    assert_eq!(exec.body["status"]["allowed"], false);
    let forbidden = send(
        &app,
        "POST",
        &format!("/api/v1/namespaces/payments/pods/{BAD_POD}/exec?command=ls"),
        &[],
        "",
    )
    .await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    assert_eq!(forbidden.body["reason"], "Forbidden");
}

#[tokio::test]
async fn protobuf_access_review_from_typed_clients() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    // client-go typed SelfSubjectAccessReview{namespace: payments, verb: list, version: v1, resource: pods}
    let mut body = b"k8s\0".to_vec();
    let field = |n: u8, b: &[u8]| {
        let mut v = vec![(n << 3) | 2, b.len() as u8];
        v.extend_from_slice(b);
        v
    };
    let ra = [
        field(1, b"payments"),
        field(2, b"list"),
        field(4, b"v1"),
        field(5, b"pods"),
    ]
    .concat();
    let raw = [field(1, b""), field(2, &field(1, &ra))].concat();
    let tm = [
        field(1, b"authorization.k8s.io/v1"),
        field(2, b"SelfSubjectAccessReview"),
    ]
    .concat();
    body.extend([field(1, &tm), field(2, &raw)].concat());
    let req = Request::builder()
        .method("POST")
        .uri("/apis/authorization.k8s.io/v1/selfsubjectaccessreviews")
        .header("content-type", "application/vnd.kubernetes.protobuf")
        .body(Body::from(body))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let v: Value =
        serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(v["spec"]["resourceAttributes"]["namespace"], "payments");
    assert_eq!(v["status"]["allowed"], true);
}

#[tokio::test]
async fn metrics_and_node_proxy() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let pm = get(
        &app,
        "/apis/metrics.k8s.io/v1beta1/namespaces/payments/pods",
    )
    .await;
    assert_eq!(pm.body["kind"], "PodMetricsList");
    assert_eq!(names(&pm.body).len(), 4);
    assert!(pm.body["items"][0]["timestamp"].is_string());
    let nm = get(
        &app,
        "/apis/metrics.k8s.io/v1beta1/nodes/ip-10-0-1-21.ec2.internal",
    )
    .await;
    assert_eq!(nm.body["usage"]["cpu"], "1873m");
    let ro = send(
        &app,
        "DELETE",
        "/apis/metrics.k8s.io/v1beta1/nodes/ip-10-0-1-21.ec2.internal",
        &[],
        "",
    )
    .await;
    assert_eq!(ro.status, StatusCode::METHOD_NOT_ALLOWED);

    let stats = get(
        &app,
        "/api/v1/nodes/ip-10-0-1-21.ec2.internal/proxy/stats/summary",
    )
    .await;
    assert_eq!(stats.body["node"]["nodeName"], "ip-10-0-1-21.ec2.internal");
    let logs = get(
        &app,
        "/api/v1/nodes/ip-10-0-1-21.ec2.internal/proxy/logs?query=kubelet&tailLines=2",
    )
    .await;
    assert_eq!(logs.status, StatusCode::OK);
    assert_eq!(logs.text.lines().count(), 2);
    let unknown = get(
        &app,
        "/api/v1/nodes/ip-10-0-1-21.ec2.internal/proxy/logs?query=nope",
    )
    .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    let no_node = get(&app, "/api/v1/nodes/ghost/proxy/stats/summary").await;
    assert_eq!(no_node.body["message"], "nodes \"ghost\" not found");
}

#[tokio::test]
async fn watch_streams_current_objects() {
    let app = service_app(world(CRASHLOOP), &Kubernetes);
    let r = get(&app, "/api/v1/namespaces/payments/services?watch=true").await;
    let events: Vec<Value> = r
        .text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["type"], "ADDED");
    assert_eq!(r.headers["content-type"], "application/json");
}

#[tokio::test]
async fn helm_releases_are_served_as_secrets() {
    let w = world(HELM);
    let app = service_app(w.clone(), &Kubernetes);
    let list = get(
        &app,
        "/api/v1/namespaces/perf/secrets?labelSelector=owner%3Dhelm",
    )
    .await;
    let n = names(&list.body);
    assert_eq!(
        n,
        [
            "sh.helm.release.v1.checkout-loadtest.v1",
            "sh.helm.release.v1.checkout-loadtest.v2",
            "sh.helm.release.v1.k6-operator.v3",
            "sh.helm.release.v1.search-loadtest.v1"
        ]
    );
    let secret = &list.body["items"][1];
    assert_eq!(secret["type"], "helm.sh/release.v1");
    assert_eq!(secret["metadata"]["labels"]["status"], "deployed");
    let outer = b64_decode(secret["data"]["release"].as_str().unwrap()).unwrap();
    let inner = b64_decode(std::str::from_utf8(&outer).unwrap()).unwrap();
    let release: Value = serde_json::from_slice(&inner).unwrap();
    assert_eq!(release["name"], "checkout-loadtest");
    assert_eq!(release["version"], 2);

    let del = send(
        &app,
        "DELETE",
        "/api/v1/namespaces/perf/secrets/sh.helm.release.v1.checkout-loadtest.v1",
        &[],
        "",
    )
    .await;
    assert_eq!(del.status, StatusCode::OK);
    let snap = w.lock().unwrap().snapshot();
    let rels = &snap["services"]["kubernetes"]["helm_releases"];
    assert!(rels.get("perf/checkout-loadtest.v1").is_none());
    assert!(rels.get("perf/checkout-loadtest.v2").is_some());
}

#[tokio::test]
async fn deleting_a_job_removes_its_pods() {
    let w = world(HELM);
    let app = service_app(w.clone(), &Kubernetes);
    let del = send(
        &app,
        "DELETE",
        "/apis/batch/v1/namespaces/perf/jobs/checkout-loadtest",
        &[],
        "",
    )
    .await;
    assert_eq!(del.status, StatusCode::OK);
    let pods = get(
        &app,
        "/api/v1/namespaces/perf/pods?labelSelector=job-name%3Dcheckout-loadtest",
    )
    .await;
    assert!(names(&pods.body).is_empty());
    let staging = get(
        &app,
        "/api/v1/namespaces/perf-staging/pods?labelSelector=job-name%3Dcheckout-loadtest",
    )
    .await;
    assert_eq!(names(&staging.body).len(), 1);
}

#[tokio::test]
async fn a_reused_job_name_does_not_make_a_different_uid_its_dependent() {
    let world = world_with(HELM, |dataset| {
        let pods = dataset
            .services
            .get_mut("kubernetes")
            .unwrap()
            .collection_mut("pods");
        let mut stale = pods.get("perf/checkout-loadtest-mrbgn").unwrap().clone();
        stale["metadata"]["name"] = json!("checkout-loadtest-stale");
        stale["metadata"]["uid"] = json!("00000000-0000-0000-0000-000000000001");
        stale["metadata"]["ownerReferences"][0]["uid"] =
            json!("00000000-0000-0000-0000-000000000002");
        pods.insert("perf/checkout-loadtest-stale", stale);
    });
    let app = service_app(world.clone(), &Kubernetes);
    let deleted = send(
        &app,
        "DELETE",
        "/apis/batch/v1/namespaces/perf/jobs/checkout-loadtest",
        &[],
        "",
    )
    .await;
    assert_eq!(deleted.status, StatusCode::OK);
    assert_eq!(
        get(&app, "/api/v1/namespaces/perf/pods/checkout-loadtest-stale")
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        get(&app, "/api/v1/namespaces/perf/pods/checkout-loadtest-mrbgn")
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn deployment_scaling_ignores_a_same_named_owner_with_a_different_uid() {
    let world = world_with("scale-checkout-for-sale", |dataset| {
        let sets = dataset
            .services
            .get_mut("kubernetes")
            .unwrap()
            .collection_mut("replicasets.apps");
        let mut stale = sets.get("shop-prod/checkout-tscdrh8t48").unwrap().clone();
        stale["metadata"]["name"] = json!("checkout-stale");
        stale["metadata"]["uid"] = json!("00000000-0000-0000-0000-000000000001");
        stale["metadata"]["ownerReferences"][0]["uid"] =
            json!("00000000-0000-0000-0000-000000000002");
        stale["metadata"]["annotations"]["deployment.kubernetes.io/revision"] = json!("999");
        sets.insert("shop-prod/checkout-stale", stale);
    });
    let app = service_app(world.clone(), &Kubernetes);
    let scaled = send(
        &app,
        "PATCH",
        "/apis/apps/v1/namespaces/shop-prod/deployments/checkout/scale",
        &[("content-type", "application/merge-patch+json")],
        r#"{"spec":{"replicas":8}}"#,
    )
    .await;
    assert_eq!(scaled.status, StatusCode::OK);
    let state = world.lock().unwrap().snapshot();
    let sets = &state["services"]["kubernetes"]["replicasets.apps"];
    assert_eq!(sets["shop-prod/checkout-tscdrh8t48"]["spec"]["replicas"], 8);
    assert_eq!(sets["shop-prod/checkout-stale"]["spec"]["replicas"], 3);
}

#[tokio::test]
async fn noncontrolling_owner_references_do_not_select_the_deployment_replica_set() {
    let world = world_with("scale-checkout-for-sale", |dataset| {
        let sets = dataset
            .services
            .get_mut("kubernetes")
            .unwrap()
            .collection_mut("replicasets.apps");
        let mut unrelated = sets.get("shop-prod/checkout-tscdrh8t48").unwrap().clone();
        unrelated["metadata"]["name"] = json!("checkout-observed");
        unrelated["metadata"]["uid"] = json!("00000000-0000-0000-0000-000000000001");
        unrelated["metadata"]["ownerReferences"][0]["controller"] = json!(false);
        unrelated["metadata"]["annotations"]["deployment.kubernetes.io/revision"] = json!("999");
        sets.insert("shop-prod/checkout-observed", unrelated);
    });
    let app = service_app(world.clone(), &Kubernetes);
    let scaled = send(
        &app,
        "PATCH",
        "/apis/apps/v1/namespaces/shop-prod/deployments/checkout/scale",
        &[("content-type", "application/merge-patch+json")],
        r#"{"spec":{"replicas":8}}"#,
    )
    .await;
    assert_eq!(scaled.status, StatusCode::OK);
    let state = world.lock().unwrap().snapshot();
    let sets = &state["services"]["kubernetes"]["replicasets.apps"];
    assert_eq!(sets["shop-prod/checkout-tscdrh8t48"]["spec"]["replicas"], 8);
    assert_eq!(sets["shop-prod/checkout-observed"]["spec"]["replicas"], 3);
}

#[tokio::test]
async fn typed_quantity_validation_accepts_exact_alternatives_and_rejects_invalid_resources() {
    let path = "/apis/apps/v1/namespaces/analytics/deployments/report-builder";
    for quantity in [
        json!("1024Mi"),
        json!(1073741824),
        json!("1.073741824e9"),
        json!(" 1Gi "),
    ] {
        let world = world("oomkilled-memory-limit");
        let app = service_app(world.clone(), &Kubernetes);
        let body = json!({"spec":{"template":{"spec":{"containers":[{
            "name":"report-builder","resources":{"limits":{"memory":quantity}}
        }]}}}});
        let response = send(
            &app,
            "PATCH",
            path,
            &[("content-type", "application/strategic-merge-patch+json")],
            &body.to_string(),
        )
        .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text);
    }
    for memory in [json!("-1Gi"), json!("1gi"), json!(true), json!("128Mi")] {
        let world = world("oomkilled-memory-limit");
        let app = service_app(world.clone(), &Kubernetes);
        let initial = world.lock().unwrap().snapshot()["services"].clone();
        let body = json!({"spec":{"template":{"spec":{"containers":[{
            "name":"report-builder","resources":{"limits":{"memory":memory}}
        }]}}}});
        let response = send(
            &app,
            "PATCH",
            path,
            &[("content-type", "application/strategic-merge-patch+json")],
            &body.to_string(),
        )
        .await;
        assert_eq!(
            response.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            response.text
        );
        assert_eq!(world.lock().unwrap().snapshot()["services"], initial);
    }
}

#[tokio::test]
async fn deployment_and_scale_writes_honor_supplied_uid_and_version_preconditions() {
    for subresource in ["", "/scale"] {
        for method in ["PUT", "PATCH"] {
            for field in ["uid", "resourceVersion"] {
                let world = world("scale-checkout-for-sale");
                let app = service_app(world.clone(), &Kubernetes);
                let path =
                    format!("/apis/apps/v1/namespaces/shop-prod/deployments/checkout{subresource}");
                let mut candidate = get(&app, &path).await.body;
                candidate["spec"]["replicas"] = json!(8);
                candidate["metadata"][field] = if field == "resourceVersion" {
                    json!("1")
                } else {
                    json!("stale-or-unrelated")
                };
                let initial = world.lock().unwrap().snapshot()["services"].clone();
                let response = send(
                    &app,
                    method,
                    &path,
                    &[("content-type", "application/merge-patch+json")],
                    &candidate.to_string(),
                )
                .await;
                assert_eq!(
                    response.status,
                    if method == "PATCH" && subresource.is_empty() && field == "uid" {
                        StatusCode::UNPROCESSABLE_ENTITY
                    } else {
                        StatusCode::CONFLICT
                    },
                    "{method} {path} {field}: {}",
                    response.text
                );
                assert_eq!(world.lock().unwrap().snapshot()["services"], initial);
            }
        }
    }
}

#[tokio::test]
async fn zero_and_equivalent_numeric_resource_versions_are_valid_unconditional_or_current_updates()
{
    for (slug, path) in [
        (
            "scale-checkout-for-sale",
            "/apis/apps/v1/namespaces/shop-prod/deployments/checkout",
        ),
        (
            "scale-checkout-for-sale",
            "/apis/apps/v1/namespaces/shop-prod/deployments/checkout/scale",
        ),
        (
            HELM,
            "/api/v1/namespaces/perf/secrets/sh.helm.release.v1.checkout-loadtest.v2",
        ),
    ] {
        for version in ["0", "000", "current_alias"] {
            let world = world(slug);
            let app = service_app(world.clone(), &Kubernetes);
            let mut candidate = get(&app, path).await.body;
            let requested = if version == "current_alias" {
                format!(
                    "0{}",
                    candidate["metadata"]["resourceVersion"].as_str().unwrap()
                )
            } else {
                version.to_owned()
            };
            candidate["metadata"]["resourceVersion"] = json!(requested);
            candidate["metadata"]["uid"] = json!("");
            if slug != HELM {
                candidate["spec"]["replicas"] = json!(8);
            }
            let response = send(&app, "PUT", path, &[], &candidate.to_string()).await;
            assert_eq!(
                response.status,
                StatusCode::OK,
                "{path} {version}: {}",
                response.text
            );
            assert!(
                !response.body["metadata"]["uid"]
                    .as_str()
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[tokio::test]
async fn unconditional_version_does_not_disable_an_independent_uid_precondition() {
    for (slug, path) in [
        (
            "scale-checkout-for-sale",
            "/apis/apps/v1/namespaces/shop-prod/deployments/checkout",
        ),
        (
            "scale-checkout-for-sale",
            "/apis/apps/v1/namespaces/shop-prod/deployments/checkout/scale",
        ),
        (
            HELM,
            "/api/v1/namespaces/perf/secrets/sh.helm.release.v1.checkout-loadtest.v2",
        ),
    ] {
        for method in ["PUT", "PATCH"] {
            for wrong_uid in [true, false] {
                let world = world(slug);
                let app = service_app(world.clone(), &Kubernetes);
                let mut candidate = get(&app, path).await.body;
                candidate["metadata"]["uid"] =
                    json!(if wrong_uid { "unrelated-owner" } else { "" });
                candidate["metadata"]["resourceVersion"] = json!(if wrong_uid { "0" } else { "1" });
                if slug != HELM {
                    candidate["spec"]["replicas"] = json!(8);
                }
                let initial = world.lock().unwrap().snapshot()["services"].clone();
                let response = send(
                    &app,
                    method,
                    path,
                    &[("content-type", "application/merge-patch+json")],
                    &candidate.to_string(),
                )
                .await;
                let expected = if wrong_uid && method == "PATCH" && !path.ends_with("/scale") {
                    StatusCode::UNPROCESSABLE_ENTITY
                } else {
                    StatusCode::CONFLICT
                };
                assert_eq!(
                    response.status, expected,
                    "{method} {path}: {}",
                    response.text
                );
                assert_eq!(world.lock().unwrap().snapshot()["services"], initial);
            }
        }
    }
}

#[tokio::test]
async fn deployment_annotations_change_generation_without_faking_a_completed_rollout() {
    let world = world("oomkilled-memory-limit");
    let app = service_app(world.clone(), &Kubernetes);
    let path = "/apis/apps/v1/namespaces/analytics/deployments/report-builder";
    let before = get(&app, path).await.body;
    let response = send(&app, "PATCH", path, &[("content-type","application/merge-patch+json")],
        r#"{"metadata":{"annotations":{"deployment.kubernetes.io/revision":"9","change-ticket":"OPS-100"}}}"#).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.body["metadata"]["generation"],
        before["metadata"]["generation"].as_u64().unwrap() + 1
    );
    assert_eq!(response.body["status"], before["status"]);
}

#[tokio::test]
async fn compressed_helm_secret_updates_preserve_observable_release_identity_and_history() {
    use std::io::Write;
    let world = world(HELM);
    let app = service_app(world.clone(), &Kubernetes);
    let path = "/api/v1/namespaces/perf/secrets/sh.helm.release.v1.checkout-loadtest.v2";
    let mut secret = get(&app, path).await.body;
    let mut release = world.lock().unwrap().snapshot()["services"]["kubernetes"]["helm_releases"]["perf/checkout-loadtest.v2"].clone();
    release["info"]["status"] = json!("uninstalled");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(release.to_string().as_bytes()).unwrap();
    let encoded = mock_kubernetes::util::b64_encode(&encoder.finish().unwrap());
    secret["data"]["release"] = json!(mock_kubernetes::util::b64_encode(encoded.as_bytes()));
    secret["metadata"]["labels"]["status"] = json!("uninstalled");
    let response = send(&app, "PUT", path, &[], &secret.to_string()).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text);
    let state = world.lock().unwrap().snapshot();
    assert_eq!(
        state["services"]["kubernetes"]["helm_releases"]["perf/checkout-loadtest.v2"],
        release
    );
    assert_eq!(
        state["services"]["kubernetes"]["helm_releases"]["perf/checkout-loadtest.v1"]["info"]["status"],
        "superseded"
    );
    let listed = get(
        &app,
        "/api/v1/namespaces/perf/secrets?labelSelector=owner%3Dhelm",
    )
    .await;
    assert_eq!(
        names(&listed.body)
            .iter()
            .filter(|name| *name == "sh.helm.release.v1.checkout-loadtest.v2")
            .count(),
        1
    );
    let deleted = send(&app, "DELETE", path, &[], "").await;
    assert_eq!(deleted.status, StatusCode::OK);
    let state = world.lock().unwrap().snapshot();
    assert!(
        state["services"]["kubernetes"]["helm_releases"]
            .get("perf/checkout-loadtest.v2")
            .is_none()
    );
    assert!(
        state["services"]["kubernetes"]["secrets"]
            .get("perf/sh.helm.release.v1.checkout-loadtest.v2")
            .is_none()
    );
}

fn protobuf_bytes(number: u64, bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for mut value in [(number << 3) | 2, bytes.len() as u64] {
        while value >= 128 {
            output.push((value as u8 & 127) | 128);
            value >>= 7;
        }
        output.push(value as u8);
    }
    output.extend_from_slice(bytes);
    output
}

#[tokio::test]
async fn typed_helm_protobuf_writes_default_namespace_from_the_request_before_decoding_storage() {
    use std::io::Write;
    let world = world(HELM);
    let app = service_app(world.clone(), &Kubernetes);
    let path = "/api/v1/namespaces/perf/secrets/sh.helm.release.v1.checkout-loadtest.v2";
    let mut release = world.lock().unwrap().snapshot()["services"]["kubernetes"]["helm_releases"]["perf/checkout-loadtest.v2"].clone();
    release["info"]["status"] = json!("uninstalling");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(release.to_string().as_bytes()).unwrap();
    let encoded = mock_kubernetes::util::b64_encode(&encoder.finish().unwrap());
    let mut metadata = protobuf_bytes(1, b"sh.helm.release.v1.checkout-loadtest.v2");
    metadata.extend(protobuf_bytes(3, b""));
    for (key, value) in [
        ("owner", "helm"),
        ("name", "checkout-loadtest"),
        ("status", "uninstalling"),
        ("version", "2"),
    ] {
        let pair = [
            protobuf_bytes(1, key.as_bytes()),
            protobuf_bytes(2, value.as_bytes()),
        ]
        .concat();
        metadata.extend(protobuf_bytes(11, &pair));
    }
    let data = [
        protobuf_bytes(1, b"release"),
        protobuf_bytes(2, encoded.as_bytes()),
    ]
    .concat();
    let raw = [
        protobuf_bytes(1, &metadata),
        protobuf_bytes(2, &data),
        protobuf_bytes(3, b"helm.sh/release.v1"),
    ]
    .concat();
    let type_meta = [protobuf_bytes(1, b"v1"), protobuf_bytes(2, b"Secret")].concat();
    let body = [
        b"k8s\0".to_vec(),
        protobuf_bytes(1, &type_meta),
        protobuf_bytes(2, &raw),
    ]
    .concat();
    let response = send_bytes(
        &app,
        "PUT",
        path,
        &[("content-type", "application/vnd.kubernetes.protobuf")],
        &body,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text);
    assert_eq!(response.body["metadata"]["namespace"], "perf");
    assert_eq!(
        world.lock().unwrap().snapshot()["services"]["kubernetes"]["helm_releases"]["perf/checkout-loadtest.v2"],
        release
    );
}

#[tokio::test]
async fn json_helm_writes_accept_absent_or_empty_namespace_but_not_a_different_namespace() {
    for namespace in [Value::Null, json!(""), json!("perf"), json!("perf-staging")] {
        let world = world(HELM);
        let app = service_app(world.clone(), &Kubernetes);
        let path = "/api/v1/namespaces/perf/secrets/sh.helm.release.v1.checkout-loadtest.v2";
        let mut secret = get(&app, path).await.body;
        if namespace.is_null() {
            secret["metadata"]
                .as_object_mut()
                .unwrap()
                .remove("namespace");
        } else {
            secret["metadata"]["namespace"] = namespace.clone();
        }
        secret["metadata"]["resourceVersion"] = json!("");
        secret["metadata"]["uid"] = json!("");
        let initial = world.lock().unwrap().snapshot()["services"].clone();
        let response = send(&app, "PUT", path, &[], &secret.to_string()).await;
        if namespace == "perf-staging" {
            assert_eq!(
                response.status,
                StatusCode::BAD_REQUEST,
                "{}",
                response.text
            );
            assert_eq!(world.lock().unwrap().snapshot()["services"], initial);
        } else {
            assert_eq!(response.status, StatusCode::OK, "{}", response.text);
            assert_eq!(response.body["metadata"]["namespace"], "perf");
        }
    }
}

#[tokio::test]
async fn request_namespace_defaulting_is_shared_by_create_patch_and_apply_create() {
    for (method, path, media_type) in [
        (
            "POST",
            "/api/v1/namespaces/perf/configmaps",
            "application/json",
        ),
        (
            "PATCH",
            "/api/v1/namespaces/perf/configmaps/new-script",
            "application/apply-patch+yaml",
        ),
        (
            "PATCH",
            "/api/v1/namespaces/perf/configmaps/checkout-loadtest-script",
            "application/merge-patch+json",
        ),
    ] {
        for namespace in ["", "perf-staging"] {
            let world = world(HELM);
            let app = service_app(world.clone(), &Kubernetes);
            let name = if path.ends_with("checkout-loadtest-script") {
                "checkout-loadtest-script"
            } else {
                "new-script"
            };
            let body = json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":name,"namespace":namespace},"data":{"script.js":"test"}});
            let initial = world.lock().unwrap().snapshot()["services"].clone();
            let response = send(
                &app,
                method,
                path,
                &[("content-type", media_type)],
                &body.to_string(),
            )
            .await;
            if namespace.is_empty() {
                assert!(response.status.is_success(), "{}", response.text);
                assert_eq!(response.body["metadata"]["namespace"], "perf");
            } else {
                assert_eq!(
                    response.status,
                    StatusCode::BAD_REQUEST,
                    "{}",
                    response.text
                );
                assert_eq!(world.lock().unwrap().snapshot()["services"], initial);
            }
        }
    }
}

#[tokio::test]
async fn namespaces_cannot_delete_default_and_cascade_otherwise() {
    let w = world(HELM);
    let app = service_app(w.clone(), &Kubernetes);
    let forbidden = send(&app, "DELETE", "/api/v1/namespaces/default", &[], "").await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    let del = send(&app, "DELETE", "/api/v1/namespaces/perf-staging", &[], "").await;
    assert_eq!(del.body["status"]["phase"], "Terminating");
    let snap = w.lock().unwrap().snapshot();
    let k = &snap["services"]["kubernetes"];
    assert!(
        k["jobs.batch"]
            .get("perf-staging/checkout-loadtest")
            .is_none()
    );
    assert!(
        k["helm_releases"]
            .get("perf-staging/checkout-loadtest.v1")
            .is_none()
    );
}

#[tokio::test]
async fn scale_counts_pods_with_false_ready_condition_as_unavailable() {
    let w = world(CRASHLOOP);
    let app = service_app(w.clone(), &Kubernetes);
    let snap = w.lock().unwrap().snapshot();
    let pod = snap["services"]["kubernetes"]["pods"]
        .as_object()
        .unwrap()
        .values()
        .find(|p| {
            p["metadata"]["namespace"] == "payments"
                && p["metadata"]["labels"]["app.kubernetes.io/name"] == "payments-api"
                && p["status"]["phase"] == "Running"
        })
        .unwrap()
        .clone();
    let pod_name = pod["metadata"]["name"].as_str().unwrap();
    let r = send(
        &app,
        "PATCH",
        &format!("/api/v1/namespaces/payments/pods/{pod_name}/status"),
        &[("content-type", "application/merge-patch+json")],
        r#"{"status":{"conditions":[{"type":"Ready","status":"False"}]}}"#,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text);
    let ready_before = snap["services"]["kubernetes"]["deployments.apps"]["payments/payments-api"]
        ["status"]["readyReplicas"]
        .as_i64()
        .unwrap();
    let r = send(
        &app,
        "PATCH",
        "/apis/apps/v1/namespaces/payments/deployments/payments-api/scale",
        &[("content-type", "application/merge-patch+json")],
        r#"{"spec":{"replicas":4}}"#,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let snap = w.lock().unwrap().snapshot();
    let dep = &snap["services"]["kubernetes"]["deployments.apps"]["payments/payments-api"];
    assert_eq!(dep["status"]["readyReplicas"], ready_before);
    assert_eq!(dep["status"]["unavailableReplicas"], 1);
}
