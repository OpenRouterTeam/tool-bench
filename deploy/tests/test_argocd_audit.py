import copy
import importlib.util
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "argocd_business_audit", Path(__file__).resolve().parents[1] / "audit-argocd-business.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def workload():
    template = {
        "metadata": {"labels": {"app": "billing-worker"}},
        "spec": {
            "serviceAccountName": "billing-worker",
            "containers": [{"name": "worker", "image": "worker:1.42"}],
        },
    }
    deployment = {
        "uid": "deployment", "name": "billing-worker", "namespace": "billing",
        "cluster": "https://cluster.example", "app": "argocd/billing-worker", "kind": "Deployment",
        "manifest": {
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {"name": "billing-worker", "namespace": "billing", "uid": "deployment", "generation": 2},
            "spec": {"replicas": 1, "template": copy.deepcopy(template)},
            "status": {
                "observedGeneration": 2, "replicas": 1, "readyReplicas": 1,
                "availableReplicas": 1, "updatedReplicas": 1,
            },
        },
    }
    replica_set = {
        "uid": "replica-set", "name": "billing-worker-revision", "namespace": "billing",
        "cluster": deployment["cluster"], "app": deployment["app"], "kind": "ReplicaSet",
        "parentRefs": [{"uid": "deployment", "kind": "Deployment", "name": deployment["name"]}],
        "manifest": {
            "apiVersion": "apps/v1", "kind": "ReplicaSet",
            "metadata": {
                "name": "billing-worker-revision", "namespace": "billing", "uid": "replica-set",
                "ownerReferences": [{
                    "uid": "deployment", "kind": "Deployment", "name": deployment["name"], "controller": True,
                }],
            },
            "spec": {"replicas": 1, "template": copy.deepcopy(template)},
        },
    }
    pod = {
        "uid": "pod", "name": "billing-worker-revision-pod", "namespace": "billing",
        "cluster": deployment["cluster"], "app": deployment["app"], "kind": "Pod",
        "parentRefs": [{"uid": "replica-set", "kind": "ReplicaSet", "name": replica_set["name"]}],
        "manifest": {
            "apiVersion": "v1", "kind": "Pod",
            "metadata": {
                "name": "billing-worker-revision-pod", "namespace": "billing", "uid": "pod",
                "labels": {"app": "billing-worker"},
                "ownerReferences": [{
                    "uid": "replica-set", "kind": "ReplicaSet", "name": replica_set["name"], "controller": True,
                }],
            },
            "spec": copy.deepcopy(template["spec"]) | {"nodeName": "node-1"},
            "status": {
                "phase": "Running", "conditions": [{"type": "Ready", "status": "True"}],
                "containerStatuses": [{
                    "name": "worker", "image": "worker:1.42", "ready": True, "state": {"running": {}},
                }],
            },
        },
    }
    return {
        "resources": {"deployment": deployment, "replica-set": replica_set, "pod": pod},
        "runtime_images": {
            "worker": {"app": deployment["app"], "image": "worker:1.42", "pullable": True, "ready": True},
        },
    }


class ArgoIndependentAuditTests(unittest.TestCase):
    def test_actual_workload_and_observed_readiness_pass(self):
        self.assertTrue(AUDIT.rollout_ok(workload(), "billing"))

    def test_status_and_digest_labels_cannot_replace_a_real_pod(self):
        service = workload()
        service["resources"].pop("pod")
        service["resources"]["deployment"]["health"] = {"status": "Healthy"}
        service["resources"]["deployment"]["content_digest"] = "desired"
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))

    def test_uid_is_not_sufficient_without_real_owner_name_cluster_and_tree_agreement(self):
        for field, replacement in [
            ("name", "another-revision"), ("kind", "Deployment"), ("uid", "another-owner"),
        ]:
            with self.subTest(field=field):
                service = workload()
                service["resources"]["pod"]["manifest"]["metadata"]["ownerReferences"][0][field] = replacement
                self.assertFalse(AUDIT.rollout_ok(service, "billing"))
        service = workload()
        service["resources"]["pod"]["cluster"] = "https://staging.example"
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))
        service = workload()
        service["resources"]["pod"]["parentRefs"] = []
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))

    def test_complete_pod_configuration_and_actual_container_state_are_checked(self):
        for field, replacement in [
            ("serviceAccountName", "admin"),
            ("containers", [{"name": "worker", "image": "worker:1.43"}]),
        ]:
            with self.subTest(field=field):
                service = workload()
                service["resources"]["pod"]["manifest"]["spec"][field] = replacement
                self.assertFalse(AUDIT.rollout_ok(service, "billing"))
        service = workload()
        service["resources"]["pod"]["manifest"]["status"]["containerStatuses"][0]["ready"] = False
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))

    def test_runtime_simulation_facts_are_independent_of_successful_status(self):
        service = workload()
        service["runtime_images"]["worker"]["pullable"] = False
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))

    def test_generation_actual_child_count_and_physical_duplicates_are_checked(self):
        service = workload()
        service["resources"]["deployment"]["manifest"]["status"]["observedGeneration"] = 1
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))
        service = workload()
        service["resources"]["another-key"] = copy.deepcopy(service["resources"]["pod"])
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))
        service = workload()
        service["resources"]["replica-set"]["manifest"]["spec"]["replicas"] = 2
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))

    def test_restart_annotations_must_reach_both_children(self):
        service = workload()
        template = service["resources"]["deployment"]["manifest"]["spec"]["template"]
        template["metadata"]["annotations"] = {"kubectl.kubernetes.io/restartedAt": "2026-10-29T14:05:00Z"}
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))
        service["resources"]["replica-set"]["manifest"]["spec"]["template"]["metadata"]["annotations"] = \
            copy.deepcopy(template["metadata"]["annotations"])
        self.assertFalse(AUDIT.rollout_ok(service, "billing"))
        service["resources"]["pod"]["manifest"]["metadata"]["annotations"] = \
            copy.deepcopy(template["metadata"]["annotations"])
        self.assertTrue(AUDIT.rollout_ok(service, "billing"))

    def test_source_content_checks_do_not_trust_derived_hashes(self):
        service = workload()
        deployment = service["resources"]["deployment"]
        expected = copy.deepcopy(deployment["manifest"])
        expected["spec"]["template"]["spec"]["containers"][0]["image"] = "worker:1.41"
        deployment["content_digest"] = "same-as-desired"
        deployment["template_digest"] = "same-as-desired"
        self.assertFalse(AUDIT.content_ok(deployment, expected))

    def test_existing_successful_history_is_immutable(self):
        record = {"app": "argocd/billing-worker", "sequence": 1, "history": {"revision": "old"}}
        self.assertFalse(AUDIT.permitted_record(
            {}, {}, {}, "sync_history", record, record | {"sequence": 2}, "billing", {},
        ))

    def test_preview_observation_does_not_require_the_persistent_pin_before_it_is_written(self):
        application = {
            "metadata": {"uid": "billing"},
            "spec": {"source": {"targetRevision": "main"}, "syncPolicy": {"automated": {}}},
            "status": {"operationState": {"phase": "Running"}},
        }
        initial = {"applications": {"argocd/billing-worker": copy.deepcopy(application)}}
        observed = copy.deepcopy(application)
        observed["status"]["operationState"]["phase"] = "Succeeded"
        self.assertTrue(AUDIT.permitted_record(
            initial, {}, {}, "applications", application, observed, "billing", {},
        ))
        observed["spec"]["source"]["targetRevision"] = "v1.42.0"
        self.assertTrue(AUDIT.permitted_record(
            initial, {}, {}, "applications", application, observed, "billing", {},
        ))
        observed["spec"]["source"]["targetRevision"] = "arbitrary-sha"
        self.assertFalse(AUDIT.permitted_record(
            initial, {}, {}, "applications", application, observed, "billing", {},
        ))


if __name__ == "__main__":
    unittest.main()
