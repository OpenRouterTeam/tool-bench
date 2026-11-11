import importlib.util
import json
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "runner_controls_audit", Path(__file__).resolve().parents[1] / "audit-runner-controls.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def message(value):
    return {"result": {"isError": False, "content": [{"type": "text", "text": json.dumps(value)}]}}


class NativeErrorAuditTests(unittest.TestCase):
    def test_pagerduty_nested_errors_require_the_bound_override_tool_not_business_words(self):
        payload = {"status": 400, "errors": ["End must be after start"]}
        for value in [payload, [payload]]:
            self.assertTrue(AUDIT.native_error(message(value), "pagerduty-api-v1", "create_schedule_override"))
            self.assertFalse(AUDIT.native_error(message(value), "mcp-is-error", "create_schedule_override"))
            self.assertFalse(AUDIT.native_error(message(value), "pagerduty-api-v1", "list_incidents"))
        for value in [
            {"status": 201, "errors": []},
            {"status": 400, "errors": []},
            {"status": "400", "errors": ["Rejected"]},
            {"status": True, "errors": ["Rejected"]},
            {"status": 400, "errors": ["Business record"], "id": "ordinary-record"},
            {"content": '{"status":400,"errors":["an error mentioned in the requested note"]}'},
        ]:
            self.assertFalse(AUDIT.native_error(message(value), "pagerduty-api-v1", "create_schedule_override"))
        conflicting = message(payload)
        conflicting["result"]["structuredContent"] = {"result": [{"status": 201, "errors": []}]}
        self.assertFalse(AUDIT.native_error(conflicting, "pagerduty-api-v1", "create_schedule_override"))

    def test_empty_management_is_failed_required_action_evidence_not_an_empty_read_error(self):
        packet = message({"response": [], "response_summary": "No records"})
        self.assertTrue(AUDIT.native_error(packet, "pagerduty-api-v1", "manage_incidents"))
        for tool in [None, "list_incidents", "list_users", "add_note_to_incident"]:
            self.assertFalse(AUDIT.native_error(packet, "pagerduty-api-v1", tool))
        packet["result"]["structuredContent"] = {"response": [{"id": "different-result"}]}
        self.assertFalse(AUDIT.native_error(packet, "pagerduty-api-v1", "manage_incidents"))

    def test_grafana_generic_http_failure_is_explicitly_opt_in_native_error_evidence(self):
        for status in (400, 404, 501, 503):
            packet = message({"status": status, "headers": {}, "data": {"message": "Rejected"}})
            self.assertTrue(AUDIT.native_error(packet, "grafana-api-v1"))
            self.assertFalse(AUDIT.native_error(packet, "mcp-is-error"))
            self.assertFalse(AUDIT.native_error(packet, "okta-api-v1"))

    def test_grafana_business_words_and_unproven_packet_shapes_are_not_error_proof(self):
        for payload in [
            {"status": 200, "data": {"error": "ordinary business content"}},
            {"status": 200, "data": {"status": 400}},
            {"Payload": {"status": 400, "data": "ordinary annotation data"}},
            {"status": "400", "data": {}},
            {"status": True, "data": {}},
            {"status": 400},
            {"status": 400, "data": {}, "ordinary_business_field": True},
        ]:
            self.assertFalse(AUDIT.native_error(message(payload), "grafana-api-v1"), payload)

    def test_mcp_and_json_rpc_errors_are_independent_failure_evidence(self):
        for contract in ["mcp-is-error", "okta-api-v1"]:
            self.assertTrue(AUDIT.native_error({"result": {"isError": True}}, contract))
            self.assertTrue(AUDIT.native_error(
                {"error": {"code": -32602, "message": "Invalid params"}}, contract,
            ))
            self.assertFalse(AUDIT.native_error(
                {"error": {"code": "an ordinary record", "message": "Error word"}}, contract,
            ))

    def test_okta_shapes_are_explicitly_opt_in_not_global_error_word_matching(self):
        for payload in [
            {"error": "SDK request failed"},
            {"exception": "SDK model rejected input"},
            [{"error": "wrapped client failure"}],
            {"ok": False, "error": {"type": "ValueError", "message": "Invalid model"},
             "status_code": None, "raw": {}},
        ]:
            self.assertTrue(AUDIT.native_error(message(payload), "okta-api-v1"))
            self.assertFalse(AUDIT.native_error(message(payload), "mcp-is-error"))

    def test_business_records_and_unproven_malformed_envelopes_are_not_native_error_proof(self):
        for payload in [
            {"id": "record", "error": "customer's recorded error"},
            {"profile": {"exception": "a business label"}},
            {"eventType": "user.authentication.sso", "outcome": {"result": "FAILURE"}},
            {"message": "Error: the requested application log entry"},
            {"error": None},
            {"exception": ""},
            {"ok": True, "error": {"message": "ordinary data"}, "status_code": None, "raw": {}},
        ]:
            self.assertFalse(AUDIT.native_error(message(payload), "okta-api-v1"))


if __name__ == "__main__":
    unittest.main()
