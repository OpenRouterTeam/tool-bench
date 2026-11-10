import json
import re
import subprocess
import unittest
from pathlib import Path

from deploy.tests.test_pagerduty_control_admission import metrics as base_metrics


SOURCE = (Path(__file__).resolve().parents[1] / "test-firecrawl-contracts.sh").read_text()
MATCHES = re.findall(
    r'jq -e --arg expected "\$expected" --argjson reward "\$reward" \'([\s\S]*?)\' "\$assessment"',
    SOURCE,
)
if len(MATCHES) != 1:
    raise AssertionError("the actual Firecrawl admission predicate must be unique")
PREDICATE = MATCHES[0]


def metrics(outcome, **changes):
    value = base_metrics(1 if outcome == "pass" else 0, evaluator_version=42,
                         outcome_code={"pass": 0, "fail": 1, "unresolved": 6}[outcome])
    value["adjudication_needed"] = 1 if outcome == "unresolved" else 0
    value["preservation_ok"] = 1
    value.update(changes)
    return value


def admitted(expected, value):
    return subprocess.run(
        ["jq", "-e", "--arg", "expected", expected, "--argjson", "reward", "1" if expected == "pass" else "0",
         PREDICATE], input=json.dumps(value), capture_output=True, text=True,
    ).returncode == 0


class FirecrawlControlAdmissionTests(unittest.TestCase):
    def test_known_negative_may_have_unsafe_but_fully_checked_history(self):
        self.assertTrue(admitted("fail", metrics("fail", history_ok=0, preservation_ok=0)))

    def test_positive_requires_preservation_and_matching_revision(self):
        self.assertTrue(admitted("pass", metrics("pass")))
        for changes in [
            {"history_ok": 0}, {"preservation_ok": 0}, {"evaluator_version": 41},
            {"contract_version": 1}, {"outcome_code": 1},
        ]:
            self.assertFalse(admitted("pass", metrics("pass", **changes)), changes)

    def test_unresolved_is_explicit_not_silently_admitted_as_a_negative(self):
        value = metrics("unresolved")
        self.assertTrue(admitted("unresolved", value))
        self.assertFalse(admitted("fail", value))
        self.assertFalse(admitted("pass", value))
        self.assertFalse(admitted("unresolved", metrics("fail")))
        self.assertFalse(admitted("unknown", metrics("unresolved", adjudication_needed=0)))

    def test_invalid_or_unchecked_evidence_cannot_be_a_business_negative(self):
        for field in [
            "environment_ok", "contract_ok", "integrity_ok", "runner_admission_checked",
            "runner_admission_ok", "preservation_checked", "history_checked",
        ]:
            self.assertFalse(admitted("fail", metrics("fail", **{field: 0})), field)
            value = metrics("fail")
            del value[field]
            self.assertFalse(admitted("fail", value), field)


if __name__ == "__main__":
    unittest.main()
