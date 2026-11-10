import json
import re
import subprocess
import unittest
from pathlib import Path


SOURCE = (Path(__file__).resolve().parents[1] / "test-pagerduty-contracts.sh").read_text()
MATCHES = re.findall(
    r'jq -e --argjson expected "\$reward" \'([\s\S]*?)\' "\$assessment"', SOURCE,
)
if len(MATCHES) != 1:
    raise AssertionError("the actual native control admission predicate must be unique")
PREDICATE = MATCHES[0]


def metrics(reward, **changes):
    return {
        "reward": reward, "evaluator_version": 37, "contract_version": 2,
        "environment_ok": 1, "contract_ok": 1, "integrity_ok": 1,
        "runner_admission_checked": 1, "runner_admission_ok": 1,
        "preservation_checked": 1, "history_checked": 1, "history_ok": 1,
        "adjudication_needed": 0, **changes,
    }


def admitted(expected, value):
    result = subprocess.run(
        ["jq", "-e", "--argjson", "expected", str(expected), PREDICATE],
        input=json.dumps(value), capture_output=True, text=True,
    )
    return result.returncode == 0


class PagerDutyControlAdmissionTests(unittest.TestCase):
    def test_expected_business_failure_may_have_unsafe_but_fully_checked_history(self):
        self.assertTrue(admitted(0, metrics(0, history_ok=0)))

    def test_expected_success_must_still_have_safe_history(self):
        self.assertTrue(admitted(1, metrics(1)))
        self.assertFalse(admitted(1, metrics(1, history_ok=0)))

    def test_missing_or_invalid_execution_history_cannot_masquerade_as_negative_control(self):
        for changes in [
            {"environment_ok": 0}, {"integrity_ok": 0}, {"contract_ok": 0},
            {"history_checked": 0}, {"preservation_checked": 0},
            {"runner_admission_checked": 0}, {"runner_admission_ok": 0},
            {"adjudication_needed": 1}, {"evaluator_version": 36},
        ]:
            self.assertFalse(admitted(0, metrics(0, history_ok=0, **changes)), changes)
        value = metrics(0)
        del value["history_checked"]
        self.assertFalse(admitted(0, value))

    def test_an_unexpected_verdict_cannot_be_admitted(self):
        self.assertFalse(admitted(1, metrics(0)))
        self.assertFalse(admitted(0, metrics(1)))


if __name__ == "__main__":
    unittest.main()
