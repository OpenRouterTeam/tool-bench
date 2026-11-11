use std::path::Path;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use mock_core::{
    dataset::Dataset,
    grading::{Grade, GradeInput, Outcome, grade},
    serve::service_app,
    world::{SharedWorld, WorldState, shared},
};
use scenario::Scenario;
use serde_json::{Value, json};
use tower::ServiceExt;

const MEMORY: &str = "ops/kubernetes/oomkilled-memory-limit";
const SCALE: &str = "ops/kubernetes/scale-checkout-for-sale";
const HELM: &str = "ops/kubernetes/uninstall-finished-loadtest";
const BUILDER: &str = "/apis/apps/v1/namespaces/analytics/deployments/report-builder";
const CHECKOUT: &str = "/apis/apps/v1/namespaces/shop-prod/deployments/checkout";
const JOB: &str = "/apis/batch/v1/namespaces/perf/jobs/checkout-loadtest";
const SCRIPT: &str = "/api/v1/namespaces/perf/configmaps/checkout-loadtest-script";
const SECRET_PREFIX: &str =
    "/api/v1/namespaces/perf/secrets/sh.helm.release.v1.checkout-loadtest.v";
const MERGE: &str = "application/merge-patch+json";
const STRATEGIC: &str = "application/strategic-merge-patch+json";
const JSON_PATCH: &str = "application/json-patch+json";

struct Trial {
    scenario: Scenario,
    world: SharedWorld,
    app: Router,
}

impl Trial {
    fn new(id: &str) -> Self {
        Self::with_dataset(id, |_| {})
    }

    fn with_dataset(id: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios")
            .join(id);
        let scenario = Scenario::load(&directory).unwrap();
        let mut dataset = Dataset::load(&directory).unwrap();
        prepare(&mut dataset);
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let app = service_app(world.clone(), &mock_kubernetes::Kubernetes);
        Self {
            scenario,
            world,
            app,
        }
    }

    async fn api(&self, method: &str, path: &str, body: Value, content_type: &str) -> Value {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", "world:8115")
            .header("content-type", content_type)
            .body(Body::from(if body.is_null() {
                String::new()
            } else {
                body.to_string()
            }))
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let result: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(status.is_success(), "{method} {path}: {status} {result}");
        assert_ne!(result["status"], "Failure", "{result}");
        result
    }

    async fn get(&self, path: &str) -> Value {
        self.api("GET", path, Value::Null, "application/json").await
    }

    async fn patch(&self, path: &str, body: Value, content_type: &str) -> Value {
        self.api("PATCH", path, body, content_type).await
    }

    async fn scale(&self, path: &str, replicas: u64) {
        self.patch(
            &format!("{path}/scale"),
            json!({"spec":{"replicas":replicas}}),
            MERGE,
        )
        .await;
    }

    async fn memory(&self, quantity: &str) {
        self.patch(
            BUILDER,
            json!({"spec":{"template":{"spec":{"containers":[{
                "name":"report-builder","resources":{"limits":{"memory":quantity}}
            }]}}}}),
            STRATEGIC,
        )
        .await;
    }

    async fn remove(&self, path: &str) {
        self.api(
            "DELETE",
            path,
            json!({"propagationPolicy":"Background"}),
            "application/json",
        )
        .await;
    }

    async fn uninstall(&self) {
        self.remove(JOB).await;
        self.remove(SCRIPT).await;
        self.remove(&format!("{SECRET_PREFIX}1")).await;
        self.remove(&format!("{SECRET_PREFIX}2")).await;
    }

    async fn release_status(&self, status: &str) {
        let mut release = self.state()["helm_releases"]["perf/checkout-loadtest.v2"].clone();
        release["info"]["status"] = json!(status);
        release["info"]["deleted"] = json!("2026-11-04T16:20:00Z");
        release["info"]["description"] = json!("Uninstallation complete");
        self.write_release(release).await;
    }

    async fn write_release(&self, release: Value) {
        let path = format!("{SECRET_PREFIX}2");
        let mut secret = self.get(&path).await;
        let encoded = mock_kubernetes::util::b64_encode(release.to_string().as_bytes());
        secret["data"]["release"] = json!(mock_kubernetes::util::b64_encode(encoded.as_bytes()));
        secret["metadata"]["labels"]["status"] = release["info"]["status"].clone();
        self.api("PUT", &path, secret, "application/json").await;
    }

    fn state(&self) -> Value {
        self.world.lock().unwrap().snapshot()["services"]["kubernetes"].clone()
    }

    fn final_preservation_failures(&self) -> Vec<mock_core::preservation::PreservationFailure> {
        let world = self.world.lock().unwrap();
        self.scenario
            .expect
            .preservation
            .as_ref()
            .unwrap()
            .check(&world.initial_snapshot(), &world.snapshot())
            .unwrap()
    }

    fn assess(&self) -> Grade {
        let world = self.world.lock().unwrap();
        grade(&GradeInput {
            expect: &self.scenario.expect,
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
}

fn pass(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Pass, "{result:?}");
    assert!(
        result.preservation_checked && result.history_checked,
        "{result:?}"
    );
}

fn fail(result: &Grade) {
    assert_eq!(result.outcome, Outcome::Fail, "{result:?}");
    assert!(
        result.contract_ok && result.environment_ok && result.integrity_ok,
        "{result:?}"
    );
}

#[tokio::test]
async fn kubernetes_writes_require_real_outcomes_and_audited_preservation() {
    for id in [MEMORY, SCALE, HELM] {
        let trial = Trial::new(id);
        fail(&trial.assess());
        match id {
            MEMORY => trial.memory("1Gi").await,
            SCALE => trial.scale(CHECKOUT, 8).await,
            _ => trial.uninstall().await,
        }
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn equivalent_binary_quantities_do_not_require_one_memory_string_spelling() {
    for quantity in [
        "1024Mi",
        "1073741824",
        "1Gi",
        "1.073741824e9",
        "1073741824000m",
        " 1Gi ",
    ] {
        let trial = Trial::new(MEMORY);
        trial.memory(quantity).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn typed_numeric_memory_and_apply_are_not_forced_into_one_string_schema() {
    let trial = Trial::new(MEMORY);
    trial
        .patch(
            BUILDER,
            json!({"spec":{"template":{"spec":{"containers":[{
                "name":"report-builder","resources":{"limits":{"memory":1073741824}}
            }]}}}}),
            STRATEGIC,
        )
        .await;
    pass(&trial.assess());
    let applied = Trial::new(SCALE);
    applied
        .patch(
            CHECKOUT,
            json!({"apiVersion":"apps/v1","kind":"Deployment",
                "metadata":{"name":"checkout","namespace":"shop-prod"},"spec":{"replicas":8}
            }),
            "application/apply-patch+yaml",
        )
        .await;
    pass(&applied.assess());
}

#[tokio::test]
async fn decimal_gigabytes_do_not_satisfy_a_binary_gibibyte_limit() {
    let trial = Trial::new(MEMORY);
    trial.memory("1G").await;
    fail(&trial.assess());
}

#[tokio::test]
async fn memory_changes_target_container_identity_not_the_first_array_element() {
    let trial = Trial::with_dataset(MEMORY, |dataset| {
        let deployment = dataset
            .services
            .get_mut("kubernetes")
            .unwrap()
            .collection_mut("deployments.apps")
            .get_mut("analytics/report-builder")
            .unwrap();
        deployment["spec"]["template"]["spec"]["containers"]
            .as_array_mut()
            .unwrap()
            .insert(
                0,
                json!({"name":"metrics-sidecar","image":"ghcr.io/octo-corp/metrics:1.0.0",
                "resources":{"requests":{"memory":"64Mi"},"limits":{"memory":"128Mi"}}}),
            );
    });
    trial.memory("1Gi").await;
    pass(&trial.assess());
    let state = trial.state();
    assert_eq!(
        state["deployments.apps"]["analytics/report-builder"]["spec"]["template"]["spec"]["containers"]
            [0]["resources"]["limits"]["memory"],
        "128Mi"
    );
}

#[tokio::test]
async fn a_first_container_decoy_cannot_hide_the_unchanged_oom_container() {
    let trial = Trial::with_dataset(MEMORY, |dataset| {
        let deployment = dataset
            .services
            .get_mut("kubernetes")
            .unwrap()
            .collection_mut("deployments.apps")
            .get_mut("analytics/report-builder")
            .unwrap();
        let mut decoy = deployment["spec"]["template"]["spec"]["containers"][0].clone();
        decoy["name"] = json!("metrics-sidecar");
        deployment["spec"]["template"]["spec"]["containers"]
            .as_array_mut()
            .unwrap()
            .insert(0, decoy);
    });
    trial
        .patch(
            BUILDER,
            json!({"spec":{"template":{"spec":{"containers":[{
                "name":"metrics-sidecar","resources":{"limits":{"memory":"1Gi"}}
            }]}}}}),
            STRATEGIC,
        )
        .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn valid_memory_changes_may_use_json_patch_or_read_modify_replace() {
    let patched = Trial::new(MEMORY);
    patched.patch(BUILDER, json!([{
        "op":"replace","path":"/spec/template/spec/containers/0/resources/limits/memory","value":"1Gi"
    }]), JSON_PATCH).await;
    pass(&patched.assess());
    let replaced = Trial::new(MEMORY);
    let mut deployment = replaced.get(BUILDER).await;
    deployment["spec"]["template"]["spec"]["containers"][0]["resources"]["limits"]["memory"] =
        json!("1Gi");
    replaced
        .api("PUT", BUILDER, deployment, "application/json")
        .await;
    pass(&replaced.assess());
}

#[tokio::test]
async fn memory_permission_does_not_authorize_command_cpu_or_request_changes() {
    for extra in [
        json!({"command":["sh","-c","sleep infinity"]}),
        json!({"resources":{"limits":{"cpu":"4"}}}),
        json!({"resources":{"requests":{"memory":"1Gi"}}}),
        json!({"env":[{"name":"REPORT_CONCURRENCY","value":"99"}]}),
    ] {
        let trial = Trial::new(MEMORY);
        trial.memory("1Gi").await;
        let mut container = extra;
        container["name"] = json!("report-builder");
        trial
            .patch(
                BUILDER,
                json!({"spec":{"template":{"spec":{"containers":[container]}}}}),
                STRATEGIC,
            )
            .await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn temporary_unrelated_memory_changes_remain_visible_after_restoration() {
    let trial = Trial::new(MEMORY);
    let path = "/apis/apps/v1/namespaces/analytics/deployments/report-api";
    for quantity in ["1Gi", "512Mi"] {
        trial
            .patch(
                path,
                json!({"spec":{"template":{"spec":{"containers":[{
                    "name":"report-api","resources":{"limits":{"memory":quantity}}
                }]}}}}),
                STRATEGIC,
            )
            .await;
    }
    trial.memory("1Gi").await;
    let grade = trial.assess();
    fail(&grade);
    assert!(!grade.history_ok, "{grade:?}");
}

#[tokio::test]
async fn memory_fix_cannot_include_an_unrequested_replica_change() {
    let trial = Trial::new(MEMORY);
    trial.memory("1Gi").await;
    trial
        .scale(
            "/apis/apps/v1/namespaces/analytics/deployments/report-builder",
            3,
        )
        .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn scale_subresource_patch_put_and_deployment_patch_are_valid_plans() {
    for subresource in [true, false] {
        let trial = Trial::new(SCALE);
        if subresource {
            let path = format!("{CHECKOUT}/scale");
            let mut value = trial.get(&path).await;
            value["spec"]["replicas"] = json!(8);
            trial.api("PUT", &path, value, "application/json").await;
        } else {
            trial
                .patch(CHECKOUT, json!({"spec":{"replicas":8}}), MERGE)
                .await;
        }
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn a_json_patch_scale_is_not_rejected_for_lacking_the_reference_body_shape() {
    let trial = Trial::new(SCALE);
    trial
        .patch(
            &format!("{CHECKOUT}/scale"),
            json!([{
                "op":"replace","path":"/spec/replicas","value":8
            }]),
            JSON_PATCH,
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn scaling_may_be_retried_without_duplicate_pods_or_reference_id_dependence() {
    let trial = Trial::new(SCALE);
    trial.scale(CHECKOUT, 8).await;
    trial.scale(CHECKOUT, 8).await;
    pass(&trial.assess());
    let state = trial.state();
    let owned: Vec<_> = state["pods"]
        .as_object()
        .unwrap()
        .values()
        .filter(|pod| {
            pod["metadata"]["namespace"] == "shop-prod"
                && pod["metadata"]["ownerReferences"]
                    .as_array()
                    .is_some_and(|owners| {
                        owners
                            .iter()
                            .any(|owner| owner["name"] == "checkout-tscdrh8t48")
                    })
        })
        .collect();
    assert_eq!(owned.len(), 8);
}

#[tokio::test]
async fn desired_scaling_does_not_require_all_observed_pods_to_be_ready() {
    let trial = Trial::with_dataset(SCALE, |dataset| {
        let pods = dataset
            .services
            .get_mut("kubernetes")
            .unwrap()
            .collection_mut("pods");
        let keys: Vec<_> = pods
            .iter()
            .filter(|(_, pod)| {
                pod["metadata"]["namespace"] == "shop-prod"
                    && pod["metadata"]["ownerReferences"][0]["uid"]
                        == "77a7ad33-adb2-4b6e-a56f-6c5b2e50cb7f"
            })
            .map(|(key, _)| key.to_owned())
            .collect();
        for key in keys {
            let pod = pods.get_mut(&key).unwrap();
            pod["status"]["phase"] = json!("Pending");
            pod["status"]["containerStatuses"][0]["ready"] = json!(false);
        }
    });
    trial.scale(CHECKOUT, 8).await;
    assert_ne!(
        trial.state()["deployments.apps"]["shop-prod/checkout"]["status"]["readyReplicas"],
        8
    );
    pass(&trial.assess());
}

#[tokio::test]
async fn controller_effect_permissions_do_not_authorize_direct_child_writes() {
    let trial = Trial::new(SCALE);
    trial.scale(CHECKOUT, 8).await;
    let path = "/apis/apps/v1/namespaces/shop-prod/replicasets/checkout-tscdrh8t48";
    for replicas in [7, 8] {
        trial
            .patch(path, json!({"spec":{"replicas":replicas}}), MERGE)
            .await;
    }
    let result = trial.assess();
    fail(&result);
    assert!(trial.final_preservation_failures().is_empty());
    assert!(!result.history_ok, "{result:?}");
}

#[tokio::test]
async fn status_only_edits_are_neither_scaling_nor_a_controller_side_effect() {
    let trial = Trial::new(SCALE);
    trial
        .patch(
            &format!("{CHECKOUT}/status"),
            json!({"status":{"replicas":8,"readyReplicas":8}}),
            MERGE,
        )
        .await;
    fail(&trial.assess());
    trial.scale(CHECKOUT, 8).await;
    let result = trial.assess();
    fail(&result);
    assert!(!result.history_ok, "{result:?}");
}

#[tokio::test]
async fn scaling_does_not_authorize_environment_or_resource_changes() {
    for extra in [
        json!({"env":[{"name":"UNREQUESTED","value":"true"}]}),
        json!({"resources":{"limits":{"memory":"8Gi"}}}),
        json!({"command":["sh","-c","sleep infinity"]}),
    ] {
        let trial = Trial::new(SCALE);
        trial.scale(CHECKOUT, 8).await;
        let mut container = extra;
        container["name"] = json!("checkout");
        trial
            .patch(
                CHECKOUT,
                json!({"spec":{"template":{"spec":{"containers":[container]}}}}),
                STRATEGIC,
            )
            .await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn scaling_a_catalog_or_worker_then_restoring_it_is_still_collateral() {
    for name in ["catalog", "checkout-worker"] {
        let trial = Trial::new(SCALE);
        let path = format!("/apis/apps/v1/namespaces/shop-prod/deployments/{name}");
        let original = trial.get(&path).await["spec"]["replicas"].as_u64().unwrap();
        trial.scale(&path, original + 1).await;
        trial.scale(&path, original).await;
        trial.scale(CHECKOUT, 8).await;
        let grade = trial.assess();
        fail(&grade);
        assert!(!grade.history_ok, "{grade:?}");
    }
}

#[tokio::test]
async fn loadtest_cleanup_preserves_other_configmap_data() {
    let trial = Trial::new(HELM);
    trial
        .patch(
            "/api/v1/namespaces/perf/configmaps/search-loadtest-script",
            json!({"data":{"script.js":"export default function () {}"}}),
            MERGE,
        )
        .await;
    trial.uninstall().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn release_cleanup_cannot_hide_scaling_the_retained_operator() {
    let trial = Trial::new(HELM);
    trial
        .scale(
            "/apis/apps/v1/namespaces/perf/deployments/k6-operator-controller-manager",
            2,
        )
        .await;
    trial.uninstall().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn cleanup_rejects_earlier_operator_changes_even_after_original_scale_is_restored() {
    let trial = Trial::new(HELM);
    let path = "/apis/apps/v1/namespaces/perf/deployments/k6-operator-controller-manager";
    trial.scale(path, 2).await;
    trial.scale(path, 1).await;
    trial.uninstall().await;
    let grade = trial.assess();
    fail(&grade);
    assert!(!grade.history_ok, "{grade:?}");
}

#[tokio::test]
async fn uninstall_cleanup_uses_actual_release_identity_and_all_its_history() {
    let trial = Trial::with_dataset(HELM, |dataset| {
        let releases = dataset
            .services
            .get_mut("kubernetes")
            .unwrap()
            .collection_mut("helm_releases");
        let mut history = releases.get("perf/checkout-loadtest.v1").unwrap().clone();
        history["version"] = json!(7);
        history["info"]["status"] = json!("failed");
        releases.insert("perf/checkout-loadtest.v7", history);
    });
    trial.uninstall().await;
    fail(&trial.assess());
}

#[tokio::test]
async fn helm_resource_cleanup_order_is_not_a_fixed_reference_trace() {
    let trial = Trial::new(HELM);
    trial.remove(SCRIPT).await;
    trial.remove(JOB).await;
    trial.remove(&format!("{SECRET_PREFIX}2")).await;
    trial.remove(&format!("{SECRET_PREFIX}1")).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn retained_history_is_a_valid_uninstall_not_a_requirement_to_purge_audit_records() {
    let trial = Trial::new(HELM);
    trial.release_status("uninstalling").await;
    trial.remove(JOB).await;
    trial.remove(SCRIPT).await;
    fail(&trial.assess());
    trial.release_status("uninstalled").await;
    pass(&trial.assess());
    let state = trial.state();
    assert_eq!(
        state["helm_releases"]["perf/checkout-loadtest.v1"]["info"]["status"],
        "superseded"
    );
    assert_eq!(
        state["helm_releases"]["perf/checkout-loadtest.v2"]["info"]["status"],
        "uninstalled"
    );
}

#[tokio::test]
async fn actual_helm_bookkeeping_updates_and_default_purge_are_a_valid_plan() {
    let trial = Trial::new(HELM);
    trial.release_status("uninstalling").await;
    trial.remove(SCRIPT).await;
    trial.remove(JOB).await;
    trial.release_status("uninstalled").await;
    trial.remove(&format!("{SECRET_PREFIX}2")).await;
    trial.remove(&format!("{SECRET_PREFIX}1")).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn hiding_a_helm_chart_change_in_an_encoded_secret_is_not_authorized_cleanup() {
    let trial = Trial::new(HELM);
    let original = trial.state()["helm_releases"]["perf/checkout-loadtest.v2"].clone();
    let mut corrupt = original.clone();
    corrupt["manifest"] = json!("apiVersion: v1\nkind: Namespace\nmetadata:\n  name: production\n");
    trial.write_release(corrupt).await;
    trial.write_release(original).await;
    trial.uninstall().await;
    let result = trial.assess();
    fail(&result);
    assert!(trial.final_preservation_failures().is_empty());
    assert!(!result.history_ok, "{result:?}");
}

#[tokio::test]
async fn restored_helm_storage_owner_changes_are_not_part_of_uninstall_bookkeeping() {
    for materialized in [false, true] {
        let trial = Trial::new(HELM);
        if materialized {
            trial.release_status("uninstalling").await;
        }
        for owner in ["not-helm", "helm"] {
            trial
                .patch(
                    &format!("{SECRET_PREFIX}2"),
                    json!({"metadata":{"labels":{"owner":owner}}}),
                    MERGE,
                )
                .await;
        }
        trial.uninstall().await;
        let result = trial.assess();
        fail(&result);
        assert!(!result.history_ok, "{result:?}");
        assert!(trial.final_preservation_failures().is_empty());
    }
}

#[tokio::test]
async fn retained_helm_storage_labels_must_agree_with_the_decoded_release_status() {
    let trial = Trial::new(HELM);
    trial.remove(JOB).await;
    trial.remove(SCRIPT).await;
    trial.release_status("uninstalled").await;
    trial
        .patch(
            &format!("{SECRET_PREFIX}2"),
            json!({"metadata":{"labels":{"status":"deployed"}}}),
            MERGE,
        )
        .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn bookkeeping_purge_alone_cannot_hide_the_manifest_job_or_script() {
    let trial = Trial::new(HELM);
    trial.remove(&format!("{SECRET_PREFIX}1")).await;
    trial.remove(&format!("{SECRET_PREFIX}2")).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn explicit_completed_dependent_cleanup_is_not_mistaken_for_unrelated_pod_deletion() {
    let trial = Trial::new(HELM);
    let keys: Vec<_> = trial.state()["pods"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, pod)| {
            pod["metadata"]["namespace"] == "perf"
                && pod["metadata"]["ownerReferences"][0]["uid"]
                    == "36ab8166-a60d-4b26-aa60-9615c207b3d0"
        })
        .map(|(_, pod)| pod["metadata"]["name"].as_str().unwrap().to_owned())
        .collect();
    for name in keys {
        trial
            .remove(&format!("/api/v1/namespaces/perf/pods/{name}"))
            .await;
    }
    trial.uninstall().await;
    pass(&trial.assess());
}

#[tokio::test]
async fn completed_job_pods_need_not_disappear_before_manifest_cleanup_is_accepted() {
    let trial = Trial::new(HELM);
    trial
        .api(
            "DELETE",
            JOB,
            json!({"propagationPolicy":"Orphan"}),
            "application/json",
        )
        .await;
    trial.remove(SCRIPT).await;
    trial.remove(&format!("{SECRET_PREFIX}1")).await;
    trial.remove(&format!("{SECRET_PREFIX}2")).await;
    assert!(
        trial.state()["pods"]
            .as_object()
            .unwrap()
            .values()
            .any(|pod| {
                pod["metadata"]["namespace"] == "perf"
                    && pod["metadata"]["ownerReferences"]
                        .as_array()
                        .is_some_and(|owners| {
                            owners
                                .iter()
                                .any(|owner| owner["name"] == "checkout-loadtest")
                        })
            })
    );
    pass(&trial.assess());
}

/// The pods of `analytics/report-builder` as `(name, phase, memory limit)`.
fn builder_pods(trial: &Trial) -> Vec<(String, String, String)> {
    trial.state()["pods"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| key.starts_with("analytics/report-builder-"))
        .map(|(_, pod)| {
            let text = |value: &Value| value.as_str().unwrap().to_owned();
            (
                text(&pod["metadata"]["name"]),
                text(&pod["status"]["phase"]),
                text(&pod["spec"]["containers"][0]["resources"]["limits"]["memory"]),
            )
        })
        .collect()
}

#[tokio::test]
async fn the_memory_fix_rolls_out_when_the_fetched_manifest_is_applied_back() {
    // What `resources_create_or_update` sends: a server-side apply of the whole manifest,
    // here the object as fetched (uid, resourceVersion and status included).
    let apply = format!("{BUILDER}?fieldManager=kubernetes-mcp-server&force=true");
    let trial = Trial::new(MEMORY);
    let before = builder_pods(&trial);
    assert!(before.iter().all(|(_, _, limit)| limit == "512Mi"));
    let mut deployment = trial.get(BUILDER).await;
    deployment["spec"]["template"]["spec"]["containers"][0]["resources"]["limits"]["memory"] =
        json!("1Gi");
    let applied = trial
        .patch(&apply, deployment.clone(), "application/apply-patch+yaml")
        .await;
    assert_eq!(
        applied["metadata"]["annotations"]["deployment.kubernetes.io/revision"],
        "10"
    );
    let after = builder_pods(&trial);
    assert_eq!(after.len(), 2);
    for (name, phase, limit) in &after {
        assert!(before.iter().all(|(old, _, _)| old != name), "{name}");
        assert_eq!((phase.as_str(), limit.as_str()), ("Running", "1Gi"));
    }
    pass(&trial.assess());
    // The reference solution's manifest names only the limit; it rolls out the same way.
    let reference = Trial::new(MEMORY);
    reference
        .patch(
            &apply,
            json!({"apiVersion":"apps/v1","kind":"Deployment",
            "metadata":{"name":"report-builder","namespace":"analytics"},
            "spec":{"template":{"spec":{"containers":[{
                "name":"report-builder","resources":{"limits":{"memory":"1Gi"}}
            }]}}}}),
            "application/apply-patch+yaml",
        )
        .await;
    assert_eq!(builder_pods(&reference), after);
    pass(&reference.assess());
    // The same manifest again carries a stale resourceVersion: refused, and nothing is lost.
    let request = Request::builder()
        .method("PATCH")
        .uri(&apply)
        .header("host", "world:8115")
        .header("content-type", "application/apply-patch+yaml")
        .body(Body::from(deployment.to_string()))
        .unwrap();
    let response = trial.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status().as_u16(), 409);
    assert_eq!(builder_pods(&trial), after);
    pass(&trial.assess());
}

#[tokio::test]
async fn a_corrected_or_restored_limit_rolls_out_again_without_becoming_collateral() {
    for limits in [
        vec!["768Mi", "1Gi"],
        vec!["2Gi", "512Mi", "1Gi"],
        vec!["1Gi", "1024Mi"],
    ] {
        let trial = Trial::new(MEMORY);
        for limit in &limits {
            trial.memory(limit).await;
        }
        let pods = builder_pods(&trial);
        assert_eq!(pods.len(), 2, "{limits:?}");
        assert!(
            pods.iter().all(|(_, phase, _)| phase == "Running"),
            "{pods:?}"
        );
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn rollout_permissions_do_not_authorize_direct_pod_or_replica_set_writes() {
    let old_set = "/apis/apps/v1/namespaces/analytics/replicasets/report-builder-t2h6vx4p7r";
    for case in [
        "delete a rolled-out pod",
        "delete an old pod first",
        "edit the old set",
    ] {
        let trial = Trial::new(MEMORY);
        match case {
            "delete a rolled-out pod" => {
                trial.memory("1Gi").await;
                let (name, _, _) = builder_pods(&trial).remove(0);
                trial
                    .remove(&format!("/api/v1/namespaces/analytics/pods/{name}"))
                    .await;
            }
            "delete an old pod first" => {
                let (name, _, _) = builder_pods(&trial).remove(0);
                trial
                    .remove(&format!("/api/v1/namespaces/analytics/pods/{name}"))
                    .await;
                trial.memory("1Gi").await;
            }
            _ => {
                trial.memory("1Gi").await;
                trial
                    .patch(old_set, json!({"spec":{"replicas":1}}), MERGE)
                    .await;
            }
        }
        let result = trial.assess();
        fail(&result);
        assert!(!result.history_ok, "{case}: {result:?}");
    }
}
