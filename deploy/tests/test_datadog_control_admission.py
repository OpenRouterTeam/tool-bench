import json
import re
import subprocess
import unittest
from pathlib import Path

from deploy.tests.test_pagerduty_control_admission import metrics as base_metrics


SOURCE = (Path(__file__).resolve().parents[1] / "test-datadog-contracts.sh").read_text()
MATCHES = re.findall(
    r'jq -e --argjson expected "\$reward" \'([\s\S]*?)\' "\$assessment"', SOURCE,
)
if len(MATCHES) != 1:
    raise AssertionError("the actual Datadog control admission predicate must be unique")
PREDICATE = MATCHES[0]


def metrics(reward, **changes):
    value = base_metrics(reward, evaluator_version=38, outcome_code=0 if reward == 1 else 1)
    value.update(changes)
    return value


def admitted(expected, value):
    return subprocess.run(
        ["jq", "-e", "--argjson", "expected", str(expected), PREDICATE],
        input=json.dumps(value), capture_output=True, text=True,
    ).returncode == 0


class DatadogControlAdmissionTests(unittest.TestCase):
    def test_a_proved_business_negative_may_have_unsafe_but_fully_checked_history(self):
        self.assertTrue(admitted(0, metrics(0, history_ok=0)))

    def test_positive_controls_require_the_declared_verdict_and_safe_history(self):
        self.assertTrue(admitted(1, metrics(1)))
        self.assertFalse(admitted(1, metrics(1, history_ok=0)))
        self.assertFalse(admitted(1, metrics(0)))
        self.assertFalse(admitted(0, metrics(1)))

    def test_missing_invalid_or_unresolved_evidence_cannot_be_admitted_as_a_negative(self):
        for changes in [
            {"environment_ok": 0}, {"integrity_ok": 0}, {"contract_ok": 0},
            {"history_checked": 0}, {"preservation_checked": 0},
            {"runner_admission_checked": 0}, {"runner_admission_ok": 0},
            {"adjudication_needed": 1}, {"outcome_code": 5}, {"outcome_code": 6},
        ]:
            self.assertFalse(admitted(0, metrics(0, history_ok=0, **changes)), changes)
        value = metrics(0)
        del value["history_checked"]
        self.assertFalse(admitted(0, value))

    def test_other_evaluator_and_contract_revisions_do_not_silently_mix(self):
        for field, value in [("evaluator_version", 37), ("contract_version", 1)]:
            candidate = metrics(0)
            candidate[field] = value
            self.assertFalse(admitted(0, candidate))


if __name__ == "__main__":
    unittest.main()
