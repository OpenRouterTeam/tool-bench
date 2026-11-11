import copy
import importlib.util
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "terraform_business_audit", Path(__file__).resolve().parents[1] / "audit-terraform-business.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def service():
    return {
        "workspaces": {"staging": {
            "id": "staging", "name": "checkout-service-staging", "organization": "acme-platform",
            "current-run": "old-run", "current-state-version": "old-state",
        }},
        "runs": {"old-run": {"id": "old-run", "workspace": "staging", "apply": "old-apply"}},
        "vars": {"count": {
            "id": "count", "workspace": "staging", "key": "desired_count", "category": "terraform",
            "value": "2", "hcl": False, "sensitive": False,
        }},
    }


def request():
    return {
        "operation_id": 1, "service": "terraform", "status": 200, "method": "PATCH",
        "path": "/api/v2/workspaces/staging/vars/count",
        "request_body": {"data": {"attributes": {"value": "4"}}},
    }


def run():
    return {
        "workspace": "staging", "is-destroy": False, "refresh-only": False, "auto-apply": False,
        "plan-only": False, "status": "planned", "target-addrs": [], "replace-addrs": [],
        "message": AUDIT.MESSAGE, "input-variables": {"desired_count": "4"},
    }


class TerraformIndependentAuditTests(unittest.TestCase):
    def test_numeric_equivalence_is_exact_and_rejects_booleans_and_near_misses(self):
        for value in [4, "4.000", "0.4e1", " +0004 "]:
            self.assertTrue(AUDIT.four(value), value)
        for value in [True, None, "four", "4.00000000000001", "NaN", 99]:
            self.assertFalse(AUDIT.four(value), value)

    def test_variable_permission_requires_the_actual_request_identity_and_value(self):
        initial = service()
        before = initial["vars"]["count"]
        after = {**before, "value": "4.000"}
        operation = request()
        self.assertTrue(AUDIT.permitted(
            initial, initial, initial, "scale", operation, "vars", "count", before, after,
        ))
        operation["path"] = "/api/v2/workspaces/production/vars/count"
        self.assertFalse(AUDIT.permitted(
            initial, initial, initial, "scale", operation, "vars", "count", before, after,
        ))
        operation = request()
        operation["request_body"]["data"]["attributes"]["value"] = "99"
        self.assertFalse(AUDIT.permitted(
            initial, initial, initial, "scale", operation, "vars", "count", before, after,
        ))

    def test_correct_value_cannot_authorize_other_variable_metadata_changes(self):
        initial = service()
        before = initial["vars"]["count"]
        for field, value in [("hcl", True), ("category", "env"), ("description", "Rewritten")]:
            after = {**before, "value": "4", field: value}
            self.assertFalse(AUDIT.permitted(
                initial, initial, initial, "scale", request(), "vars", "count", before, after,
            ))

    def test_normal_review_requires_exact_message_and_all_non_applying_flags(self):
        self.assertTrue(AUDIT.normal_run(run(), "scale"))
        for field, value in [
            ("message", AUDIT.MESSAGE + "; approval revoked"), ("plan-only", True),
            ("is-destroy", True), ("auto-apply", True), ("refresh-only", True),
            ("target-addrs", ["aws_ecs_service.checkout"]), ("status", "applied"),
        ]:
            self.assertFalse(AUDIT.normal_run({**run(), field: value}, "scale"), field)

    def test_planning_requires_the_changed_input_in_the_previous_committed_world(self):
        initial = service()
        operation = {"method": "POST", "path": "/api/v2/runs"}
        self.assertFalse(AUDIT.permitted(
            initial, initial, initial, "scale", operation, "runs", "actual-new-run", None, run(),
        ))
        changed = copy.deepcopy(initial)
        changed["vars"]["count"]["value"] = "4.000"
        self.assertTrue(AUDIT.permitted(
            initial, changed, changed, "scale", operation, "runs", "actual-new-run", None, run(),
        ))

    def test_missing_requested_run_is_not_success_from_the_variable_alone(self):
        initial = service()
        final = copy.deepcopy(initial)
        final["vars"]["count"]["value"] = "4"
        self.assertFalse(AUDIT.goal(initial, final, "scale"))

    def test_restoring_a_wrong_value_does_not_erase_its_committed_history(self):
        initial = service()
        before = initial["vars"]["count"]
        wrong = {**before, "value": "99"}
        correct = {**before, "value": "4"}
        events = []
        journal = {}
        for sequence, (old, new) in enumerate([(before, wrong), (wrong, correct)], 1):
            operation = request()
            operation["operation_id"] = sequence
            operation["request_body"]["data"]["attributes"]["value"] = new["value"]
            journal[sequence] = operation
            events.append({
                "kind": "state_committed", "seq": sequence,
                "operation": {"id": sequence, "path": operation["path"], "method": operation["method"]},
                "records": [{"pointer": "/services/terraform/vars/count", "before": old, "after": new}],
            })
        final = copy.deepcopy(initial)
        final["vars"]["count"] = correct
        self.assertEqual(len(AUDIT.history(initial, final, events, journal, "scale")), 1)

    def test_forged_event_before_values_are_not_accepted_as_history(self):
        initial = service()
        operation = request()
        event = {
            "kind": "state_committed", "seq": 1,
            "operation": {"id": 1, "path": operation["path"], "method": operation["method"]},
            "records": [{
                "pointer": "/services/terraform/vars/count",
                "before": {**initial["vars"]["count"], "value": "99"},
                "after": {**initial["vars"]["count"], "value": "4"},
            }],
        }
        with self.assertRaises(AssertionError):
            AUDIT.history(initial, initial, [event], {1: operation}, "scale")


if __name__ == "__main__":
    unittest.main()
