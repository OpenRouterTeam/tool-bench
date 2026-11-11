import copy
import importlib.util
import json
import unittest
from decimal import Decimal
from pathlib import Path
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "ops_remediation_audit", Path(__file__).resolve().parents[1] / "audit-ops-remediation.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class MemoryTrial:
    def __init__(self):
        self.scenario, self.now, self.initial = AUDIT.authored("memory")
        self.current = copy.deepcopy(self.initial)
        self.events, self.entries = [], []

    def call(self, service, path, body, mutate):
        previous = copy.deepcopy(self.current)
        mutate(self.current)
        changes = []
        for provider in previous.keys() | self.current.keys():
            old, new = previous.get(provider, {}), self.current.get(provider, {})
            for collection in old.keys() | new.keys():
                before, after = old.get(collection, {}), new.get(collection, {})
                for identity in before.keys() | after.keys():
                    if identity in before and identity in after and AUDIT.same(before[identity], after[identity]):
                        continue
                    escaped = identity.replace("~", "~0").replace("/", "~1")
                    change = {"pointer": f"/services/{provider}/{collection}/{escaped}"}
                    if identity in before:
                        change["before"] = before[identity]
                    if identity in after:
                        change["after"] = copy.deepcopy(after[identity])
                    changes.append(change)
        identity = len(self.events) + 1
        operation = {
            "id": identity, "kind": "http", "scenario": self.scenario, "generation": 0,
            "parent_id": None, "service": service, "method": "PUT", "path": path, "source": "unknown",
        }
        head = {
            "host": {"kubernetes": "world:8115", "sentry": "world:8113"}[service],
            "scheme": "http", "query": None, "content_type": "application/json", "headers": [],
        }
        raw = {"encoding": "utf8", "data": json.dumps(body)}
        phases = [
            {"kind": "request_started"}, {"kind": "request_head_received", "head": head},
            {"kind": "request_body_received", "body": raw},
        ]
        if changes:
            phases.append({"kind": "state_committed", "records": changes})
        phases.append({"kind": "response_completed", "status": 200, "body_complete": True})
        for phase in phases:
            self.events.append(dict(phase, seq=len(self.events) + 1, time=self.now, operation=operation))
        self.entries.append({
            "seq": len(self.entries) + 1, "operation_id": identity, "time": self.now,
            **{key: operation[key] for key in ("service", "source", "method", "path")},
            **{key: head[key] for key in ("host", "scheme", "query", "content_type")},
            "status": 200, "request_body": body, "response_body": {},
            "replay": {"parent_operation_id": None, "headers": head["headers"], "body": raw},
        })

    def memory(self, quantity="1024Mi", collateral=False):
        def mutate(state):
            deployment = state["kubernetes"]["deployments.apps"]["analytics/report-builder"]
            AUDIT.container(deployment)["resources"]["limits"]["memory"] = quantity
            if collateral:
                deployment["metadata"]["annotations"]["unrequested"] = "changed"
        self.call("kubernetes", "/apis/apps/v1/namespaces/analytics/deployments/report-builder",
                  {"memory": quantity}, mutate)

    def resolve(self):
        def mutate(state):
            issue = state["sentry"]["issues"][AUDIT.ISSUE]
            issue.update(status="resolved", substatus=None, resolvedAt=self.now)
            state["sentry"].setdefault("activities", {})["6184422318/opaque"] = {
                "id": "opaque", "issue": "6184422318", "type": "set_resolved",
                "data": {}, "user": "2841306", "dateCreated": self.now,
            }
        self.call("sentry", "/api/0/organizations/lumen-labs/issues/6184422318/",
                  {"status": "resolved"}, mutate)

    def files(self):
        common = {
            "scenario": self.scenario, "now": self.now,
            "clock": {"mode": "fixed", "reference": self.now, "step_secs": 60},
        }
        return {
            "initial-state.json": dict(common, services=self.initial),
            "state.json": dict(common, services=self.current),
            "events.json": {"events": self.events}, "journal.json": {"entries": self.entries},
        }

    def assess(self, files=None, allow_direct=True):
        files = files or self.files()
        original = AUDIT.read
        def read(source):
            source = Path(source)
            return copy.deepcopy(files[source.name]) if source.parent == Path("/probe") else original(source)
        with mock.patch.object(AUDIT, "read", side_effect=read):
            return AUDIT.audit_trial("/probe", "memory", allow_direct=allow_direct)


class OperationalAuditTests(unittest.TestCase):
    def test_authored_instructions_and_complete_selected_baselines_are_reviewed(self):
        for family in AUDIT.FAMILIES:
            scenario, now, baseline = AUDIT.authored(family)
            self.assertTrue(scenario.startswith("ops/multi/"))
            self.assertTrue(now.endswith("Z"))
            self.assertIn("kubernetes", baseline)

    def test_exact_quantity_equivalence_uses_independent_rational_arithmetic(self):
        for value in ["1Gi", "1024Mi", 1073741824, Decimal("1073741824.0"),
                      "1073741824000m", "1.073741824e9", "+1Gi", " 1Gi "]:
            self.assertEqual(AUDIT.quantity(value), 2**30, value)
        for value in ["1G", "512Mi", "2Gi", "1073741823.999999999"]:
            self.assertNotEqual(AUDIT.quantity(value), 2**30, value)

    def test_invalid_quantities_do_not_compare_as_equal_parse_errors(self):
        for value in [None, True, {}, [], "nonsense", "1gi", "1MiB", "NaN"]:
            self.assertIsNone(AUDIT.quantity(value), value)
        for value in ["1e1000001", "1" * 4097]:
            with self.assertRaises(AUDIT.Indeterminate):
                AUDIT.quantity(value)

    def test_quantity_rounding_and_binary_saturation_are_not_byte_rounding(self):
        self.assertEqual(AUDIT.quantity("3.001n"), AUDIT.quantity("4n"))
        self.assertEqual(AUDIT.quantity("9Ei"), 2**63 - 1)
        self.assertEqual(AUDIT.quantity("-9Ei"), -(2**63 - 1))

    def test_known_notes_and_unknown_wording_have_distinct_dispositions(self):
        for content in AUDIT.NOTES:
            self.assertTrue(AUDIT.note_meaning(content))
        for content in AUDIT.WRONG_NOTES:
            self.assertFalse(AUDIT.note_meaning(content))
        with self.assertRaises(AUDIT.Indeterminate):
            AUDIT.note_meaning("The checkout remediation has been completed.")

    def test_native_opaque_note_id_is_not_a_storage_counter(self):
        _scenario, now, initial = AUDIT.authored("scale")
        actor = initial["pagerduty"]["account"]["account"]["user_id"]
        content = next(iter(AUDIT.NOTES))
        note = {
            "id": "opaque-generated-id", "content": content, "created_at": now,
            "user": AUDIT.reference(actor), "channel": {"summary": "The PagerDuty website or APIs"},
        }
        request = {
            "service": "pagerduty", "method": "POST", "path": f"/incidents/{AUDIT.INCIDENT}/notes",
            "request_body": {"note": {"content": content}}, "response_body": {"note": note},
            "replay": {"headers": [{"name": "from", "value": {
                "kind": "public", "value": initial["pagerduty"]["users"][actor]["email"],
            }}]},
        }
        self.assertTrue(AUDIT.pagerduty_change(
            initial, "notes", AUDIT.INCIDENT + "/17", None, note, request, now,
        ))
        with self.assertRaises(AUDIT.AuditRejected):
            forged = copy.deepcopy(request)
            forged["response_body"]["note"]["id"] = "not-the-committed-id"
            AUDIT.pagerduty_change(initial, "notes", AUDIT.INCIDENT + "/17", None, note, forged, now)

    def test_desired_and_ready_replica_counts_do_not_substitute_for_owned_ready_pods(self):
        _scenario, _now, state = AUDIT.authored("scale")
        deployment = state["kubernetes"]["deployments.apps"]["shop-prod/checkout"]
        deployment["spec"]["replicas"] = 9
        deployment["status"]["readyReplicas"] = 9
        state["kubernetes"]["replicasets.apps"][AUDIT.RS]["spec"]["replicas"] = 9
        self.assertFalse(AUDIT.ready(state))
        sibling = copy.deepcopy(AUDIT.owned_pods(state)[0])
        for index in range(6):
            candidate = copy.deepcopy(sibling)
            candidate["metadata"].update(name=f"new-{index}", uid=f"new-uid-{index}")
            state["kubernetes"]["pods"][f"shop-prod/new-{index}"] = candidate
        self.assertTrue(AUDIT.ready(state))
        candidate = state["kubernetes"]["pods"]["shop-prod/new-0"]
        for condition in candidate["status"]["conditions"]:
            if condition["type"] == "Ready":
                condition["status"] = "False"
        self.assertFalse(AUDIT.ready(state))
        candidate["status"] = copy.deepcopy(sibling["status"])
        candidate["metadata"]["ownerReferences"][0]["uid"] = "different-replicaset"
        self.assertFalse(AUDIT.ready(state))

    def test_kubernetes_permission_does_not_authorize_neighbor_or_restored_metadata_writes(self):
        _scenario, _now, initial = AUDIT.authored("memory")
        identity = "analytics/report-builder"
        before = initial["kubernetes"]["deployments.apps"][identity]
        after = copy.deepcopy(before)
        AUDIT.container(after)["resources"]["limits"]["memory"] = "1024Mi"
        request = {
            "service": "kubernetes", "method": "PUT",
            "path": "/apis/apps/v1/namespaces/analytics/deployments/report-builder",
        }
        self.assertTrue(AUDIT.kubernetes_change(
            initial, "memory", "deployments.apps", identity, before, after, request,
        ))
        after["metadata"]["annotations"]["unrequested"] = "changed"
        self.assertFalse(AUDIT.kubernetes_change(
            initial, "memory", "deployments.apps", identity, before, after, request,
        ))
        self.assertFalse(AUDIT.kubernetes_change(
            initial, "memory", "deployments.apps", "analytics/report-api", before, after, request,
        ))

    def test_reference_and_equivalent_quantities_pass_complete_replay(self):
        for quantity in ["1Gi", "1024Mi", "1.073741824e9"]:
            trial = MemoryTrial()
            trial.memory(quantity)
            trial.resolve()
            self.assertEqual(trial.assess()[0], "pass")

    def test_unsafe_ordering_and_collateral_changes_fail(self):
        trial = MemoryTrial()
        trial.resolve()
        trial.memory()
        self.assertEqual(trial.assess()[0], "fail")
        trial = MemoryTrial()
        trial.memory(collateral=True)
        trial.resolve()
        self.assertEqual(trial.assess()[0], "fail")

    def test_noop_and_wrong_allocation_fail(self):
        self.assertEqual(MemoryTrial().assess()[0], "fail")
        trial = MemoryTrial()
        trial.memory("2Gi")
        trial.resolve()
        self.assertEqual(trial.assess()[0], "fail")

    def test_missing_or_forged_event_and_request_evidence_is_rejected(self):
        trial = MemoryTrial()
        trial.memory()
        trial.resolve()
        for component in ["before", "request", "missing", "clock", "source", "terminal"]:
            files = copy.deepcopy(trial.files())
            if component == "before":
                commit = next(event for event in files["events.json"]["events"]
                              if event["kind"] == "state_committed")
                commit["records"][0]["before"]["metadata"]["uid"] = "forged"
            elif component == "request":
                files["journal.json"]["entries"][0]["request_body"] = {"forged": True}
            elif component == "missing":
                files["events.json"]["events"].pop(0)
            elif component == "clock":
                files["state.json"]["now"] = "2026-10-22T03:15:00Z"
            elif component == "source":
                files["initial-state.json"]["services"]["sentry"]["users"]["2841306"]["email"] = "other"
            else:
                files["state.json"]["services"]["sentry"]["issues"][AUDIT.ISSUE]["priority"] = "low"
            with self.assertRaises(AUDIT.AuditRejected, msg=component):
                trial.assess(files)

    def test_unsealed_probes_cannot_be_silently_admitted_as_runtime_evidence(self):
        with self.assertRaises(AUDIT.AuditRejected):
            MemoryTrial().assess(allow_direct=False)


if __name__ == "__main__":
    unittest.main()
