import contextlib
import importlib.util
import io
import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("verified_tally", Path(__file__).resolve().parents[1] / "tally-arms.py")
TALLY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TALLY)


class VerifiedTallyTests(unittest.TestCase):
    def test_structured_summary_uses_verified_report_and_preserves_unknown_costs(self):
        payload = {
            "schema_version": 1,
            "format": "verified_trial_summary_v1",
            "trials": [
                {"provenance_checked": True, "inference": {
                    "classification": "later_turn_context_rejection", "cost": {"total_cost_usd": None},
                }},
                {"provenance_checked": True, "inference": {
                    "classification": "completed", "cost": {"total_cost_usd": 0.0},
                }},
                {"provenance_checked": False, "inference": None},
            ],
        }
        output = io.StringIO()
        with patch.object(TALLY.subprocess, "run", return_value=SimpleNamespace(stdout=json.dumps(payload))) as runner:
            with contextlib.redirect_stdout(output):
                TALLY.verified_main(["--report-bin", "/approved/bench-report", "/runs/condition"])
        runner.assert_called_once_with(
            ["/approved/bench-report", "--trials", "/runs/condition", "--json"],
            check=True, capture_output=True, text=True,
        )
        result = json.loads(output.getvalue())
        self.assertEqual(result["classifications"]["later_turn_context_rejection"], 1)
        self.assertEqual(result["attempts"], 3)
        self.assertEqual(result["attempts_with_unknown_host_cost"], 2)
        self.assertEqual(result["attempts_with_known_host_cost"], 1)
        self.assertEqual(result["known_host_cost_usd"], 0.0)

    def test_all_unknown_costs_are_null_not_free(self):
        payload = {"schema_version": 1, "format": "verified_trial_summary_v1",
                   "trials": [{"provenance_checked": False, "inference": None}]}
        output = io.StringIO()
        with patch.object(TALLY.subprocess, "run", return_value=SimpleNamespace(stdout=json.dumps(payload))):
            with contextlib.redirect_stdout(output):
                TALLY.verified_main(["--report-bin", "/approved/bench-report", "/runs/condition"])
        self.assertIsNone(json.loads(output.getvalue())["known_host_cost_usd"])

    def test_wrong_report_format_is_not_treated_as_verified_evidence(self):
        with patch.object(TALLY.subprocess, "run", return_value=SimpleNamespace(stdout="{}")):
            with self.assertRaises(ValueError):
                TALLY.verified_main(["--report-bin", "/approved/bench-report", "/runs/condition"])


if __name__ == "__main__":
    unittest.main()
