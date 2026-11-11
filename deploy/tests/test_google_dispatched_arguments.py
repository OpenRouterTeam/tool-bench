import base64
import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "google_dispatched_arguments",
    Path(__file__).resolve().parents[1] / "check-google-dispatched-arguments.py",
)
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
RAW = '{"name":"modify_sheet_values","arguments":{"spreadsheet_id":"actual","range_name":"C2","values":"[[46721]]"}}'


def entry(content=RAW):
    return {
        "operation_id": 1, "service": "google", "method": "POST", "path": "/_mcp/tools/call",
        "query": None, "request_body": CHECKER.decode(content),
        "replay": {"body": {"encoding": "utf8", "data": content}},
    }


def checked(command, **options):
    requests = sorted(Path(command[-1]).glob("*.json"))
    digest = "sha256:" + hashlib.sha256(
        (CHECKER.ROOT / "servers/snapshots/google-workspace.json").read_bytes(),
    ).hexdigest()
    return SimpleNamespace(returncode=0, stderr="", stdout=json.dumps({
        "scope": "locked_schema_checks_of_retained_static_request_files",
        "validated_request_files": len(requests),
        "requests": [
            {"path": str(source), "validation": {
                "endpoint": "google-workspace", "tool": json.loads(source.read_bytes())["params"]["name"],
                "arguments": "static_validated", "deferred_paths": [],
                "snapshot_path": "servers/snapshots/google-workspace.json", "snapshot_sha256": digest,
            }} for source in requests
        ],
    }))


def check(entries, result=None):
    with tempfile.TemporaryDirectory() as directory:
        (Path(directory) / "journal.json").write_text(json.dumps({"entries": entries}))
        with mock.patch.object(CHECKER.subprocess, "run", side_effect=result or checked):
            return CHECKER.check(Path(directory), Path("/frozen/programcheck"))


class GoogleDispatchedArgumentTests(unittest.TestCase):
    def test_exact_request_bytes_and_all_realized_calls_are_checked(self):
        second = entry()
        second["operation_id"] = 5
        result = check([entry(), second])
        self.assertEqual(result["validated_operations"], 2)
        self.assertEqual(result["requests"][0]["request_sha256"],
                         "sha256:" + hashlib.sha256(RAW.encode()).hexdigest())
        self.assertFalse(result["runtime_schema_enforcement_claimed"])
        self.assertFalse(result["model_receipt_claimed"])

    def test_base64_and_arbitrary_precision_request_bytes_are_not_reformatted(self):
        original = entry('{"name":"manage_event","arguments":{"probe":0.12345678901234567890123456789}}')
        raw = original["replay"]["body"]["data"].encode()
        original["replay"]["body"] = {"encoding": "base64", "data": base64.b64encode(raw).decode()}
        self.assertEqual(CHECKER.request_bytes(original), raw)

    def test_changed_or_missing_replay_bytes_and_wrong_request_context_reject(self):
        for field, value in [("method", "GET"), ("path", "/other"), ("query", "extra=1")]:
            changed = entry()
            changed[field] = value
            with self.assertRaises(ValueError):
                CHECKER.request_bytes(changed)
        for body in [None, {"encoding": "utf8", "data": "{}"},
                     {"encoding": "unknown", "data": RAW}, {"encoding": "base64", "data": "!!"}]:
            changed = entry()
            changed["replay"]["body"] = body
            with self.assertRaises(ValueError):
                CHECKER.request_bytes(changed)

    def test_duplicate_members_and_nonfinite_json_are_not_accepted(self):
        for content in ['{"arguments":{},"arguments":{}}', '{"value":NaN}', '{"value":Infinity}']:
            with self.assertRaises(ValueError):
                CHECKER.decode(content)

    def test_boolean_numeric_conflation_cannot_hide_disagreement_with_raw_bytes(self):
        changed = entry('{"name":"manage_event","arguments":{"add_google_meet":true}}')
        changed["request_body"]["arguments"]["add_google_meet"] = 1
        with self.assertRaises(ValueError):
            CHECKER.request_bytes(changed)

    def test_duplicate_or_missing_operation_identities_reject(self):
        for entries in [[entry(), entry()], [{**entry(), "operation_id": None}]]:
            with self.assertRaises(ValueError):
                check(entries)

    def test_empty_or_unrelated_only_evidence_does_not_pass(self):
        for entries in [[], [{**entry(), "service": "other"}]]:
            with self.assertRaises(ValueError):
                check(entries)

    def test_schema_rejection_cannot_be_promoted_to_valid_argument_evidence(self):
        with self.assertRaises(ValueError):
            check([entry()], lambda *arguments, **options: SimpleNamespace(
                returncode=1, stdout="", stderr="wrong values list type",
            ))

    def test_checker_coverage_locked_snapshot_and_actual_tool_binding_are_required(self):
        changes = [
            ("arguments", "partially_deferred"), ("deferred_paths", ["/values"]),
            ("tool", "other"), ("endpoint", "other"), ("snapshot_sha256", "sha256:wrong"),
            ("snapshot_path", "other.json"),
        ]
        for field, value in changes:
            def altered(*arguments, **options):
                result = checked(*arguments, **options)
                contents = json.loads(result.stdout)
                contents["requests"][0]["validation"][field] = value
                result.stdout = json.dumps(contents)
                return result

            with self.subTest(field=field), self.assertRaises(ValueError):
                check([entry()], altered)

    def test_duplicate_or_omitted_schema_checker_rows_reject(self):
        for kind in ("omitted", "duplicate", "wrong_path"):
            def altered(*arguments, **options):
                result = checked(*arguments, **options)
                contents = json.loads(result.stdout)
                if kind == "omitted":
                    contents["requests"] = []
                elif kind == "duplicate":
                    contents["requests"] *= 2
                    contents["validated_request_files"] = 2
                else:
                    contents["requests"][0]["path"] = "unused.json"
                result.stdout = json.dumps(contents)
                return result

            with self.subTest(kind=kind), self.assertRaises(ValueError):
                check([entry()], altered)


if __name__ == "__main__":
    unittest.main()
