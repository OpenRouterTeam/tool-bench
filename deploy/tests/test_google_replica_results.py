import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "google_result_replay", Path(__file__).resolve().parents[1] / "check-google-replica-results.py",
)
REPLAY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPLAY)


def entry():
    text = ("Successfully updated range 'Renewals!C2:D2' in spreadsheet actual-book "
            "for ops@octo-corp.example. Updated: 2 cells, 1 rows, 2 columns.")
    return {
        "operation_id": 1, "service": "google", "method": "POST", "path": "/_mcp/tools/call",
        "status": 200, "request_body": {"name": "modify_sheet_values", "arguments": {
            "spreadsheet_id": "actual-book", "range_name": "Renewals!C2:D2",
            "values": [["2027-11-30", "Renewed"]],
        }}, "response_body": {
            "content": [{"type": "text", "text": text}], "structuredContent": {"result": text},
        },
    }


def fixture(directory, entries):
    directory = Path(directory)
    (directory / "result-contracts.json").write_text(json.dumps({
        "schema_version": 1, "protocol": "oracle-tool-results-v1",
        "profiles": {"7130": {"modify_sheet_values": "google_workspace_replica_v1"}},
        "default_profiles": {}, "allowed_aws_api_errors": [],
    }))
    verifier = directory / "control" / "verifier"
    verifier.mkdir(parents=True)
    (verifier / "journal.json").write_text(json.dumps({"entries": entries}))
    return directory


class GoogleReplicaResultReplayTests(unittest.TestCase):
    def test_actual_result_adapter_is_exercised_without_reading_a_business_verdict(self):
        with tempfile.TemporaryDirectory() as directory:
            root = fixture(directory, [entry()])
            (root / "control/verifier/assessment.json").write_text('{"outcome":"fail"}')
            self.assertEqual(REPLAY.check(root), 1)

    def test_wrong_resources_and_conflicting_text_envelopes_are_rejected(self):
        for kind in ("resource", "structured", "error"):
            changed = entry()
            if kind == "resource":
                changed["request_body"]["arguments"]["spreadsheet_id"] = "wrong-book"
            elif kind == "structured":
                changed["response_body"]["structuredContent"]["result"] = "different"
            else:
                changed["response_body"]["isError"] = True
            with tempfile.TemporaryDirectory() as directory:
                root = fixture(directory, [changed])
                with self.assertRaises(ValueError):
                    REPLAY.check(root)

    def test_invalid_http_context_cannot_be_promoted_to_a_replica_result(self):
        for field, value in [("service", "other"), ("method", "GET"),
                             ("path", "/v3/other"), ("status", 404)]:
            changed = entry()
            changed[field] = value
            with tempfile.TemporaryDirectory() as directory:
                root = fixture(directory, [changed])
                with self.assertRaises(ValueError):
                    REPLAY.check(root)

    def test_duplicate_or_missing_operation_identity_is_rejected(self):
        for entries in [[entry(), copy.deepcopy(entry())], [{**entry(), "operation_id": None}]]:
            with tempfile.TemporaryDirectory() as directory:
                root = fixture(directory, entries)
                with self.assertRaises(ValueError):
                    REPLAY.check(root)

    def test_empty_or_unreviewed_only_result_sets_do_not_pass_vacuously(self):
        unreviewed = entry()
        unreviewed["request_body"]["name"] = "search_gmail_messages"
        for entries in [[], [unreviewed]]:
            with tempfile.TemporaryDirectory() as directory:
                root = fixture(directory, entries)
                with self.assertRaises(ValueError):
                    REPLAY.check(root)

    def test_an_adapter_cannot_return_altered_data_even_when_its_exit_code_is_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            root = fixture(directory, [entry()])
            with mock.patch.object(REPLAY.subprocess, "run", return_value=SimpleNamespace(
                returncode=0, stdout='{"different":"payload"}', stderr="",
            )):
                with self.assertRaises(ValueError):
                    REPLAY.check(root)

    def test_duplicate_json_registry_members_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = fixture(directory, [entry()])
            (root / "result-contracts.json").write_text('{"profiles":{},"profiles":{}}')
            with self.assertRaises(ValueError):
                REPLAY.check(root)


if __name__ == "__main__":
    unittest.main()
