import json
import re
import subprocess
import unittest
from pathlib import Path

from deploy.tests.test_pagerduty_control_admission import metrics as base_metrics


SOURCE = (Path(__file__).resolve().parents[1] / "test-google-contracts.sh").read_text()
MATCHES = re.findall(
    r'jq -e --argjson expected "\$reward" \'([\s\S]*?)\' "\$assessment"', SOURCE,
)
if len(MATCHES) != 1:
    raise AssertionError("the actual Google control admission predicate must be unique")
PREDICATE = MATCHES[0]


def metrics(reward, **changes):
    value = base_metrics(
        reward, evaluator_version=40, preservation_ok=1, outcome_code=0 if reward == 1 else 1,
    )
    value.update(changes)
    return value


def admitted(expected, value):
    return subprocess.run(
        ["jq", "-e", "--argjson", "expected", str(expected), PREDICATE],
        input=json.dumps(value), capture_output=True, text=True,
    ).returncode == 0


class GoogleControlAdmissionTests(unittest.TestCase):
    def test_valid_controls_require_matching_verdict_and_complete_safety_checks(self):
        self.assertTrue(admitted(1, metrics(1)))
        for field in [
            "environment_ok", "contract_ok", "integrity_ok", "runner_admission_checked",
            "runner_admission_ok", "history_checked", "preservation_checked",
            "history_ok", "preservation_ok",
        ]:
            self.assertFalse(admitted(1, metrics(1, **{field: 0})), field)
            candidate = metrics(1)
            del candidate[field]
            self.assertFalse(admitted(1, candidate), field)

    def test_deliberate_negatives_must_be_valid_fully_checked_business_failures(self):
        self.assertTrue(admitted(0, metrics(0, history_ok=0, preservation_ok=0)))
        for changes in [
            {"environment_ok": 0}, {"integrity_ok": 0}, {"contract_ok": 0},
            {"history_checked": 0}, {"preservation_checked": 0},
            {"runner_admission_checked": 0}, {"runner_admission_ok": 0},
            {"adjudication_needed": 1}, {"outcome_code": 5}, {"outcome_code": 6},
        ]:
            self.assertFalse(admitted(0, metrics(0, **changes)), changes)

    def test_other_conditions_and_unexpected_verdicts_are_not_pooled(self):
        for field, value in [("evaluator_version", 39), ("contract_version", 1)]:
            self.assertFalse(admitted(0, metrics(0, **{field: value})))
        self.assertFalse(admitted(1, metrics(0)))
        self.assertFalse(admitted(0, metrics(1)))

    def test_declared_variant_inventory_is_unique_and_spans_all_three_families(self):
        blocks = re.findall(r'if \[ "\$#" -eq 0 \]; then\s+set -- ([\s\S]*?)\nfi', SOURCE)
        self.assertEqual(len(blocks), 1)
        variants = blocks[0].replace("\\\n", " ").split()
        self.assertEqual(len(variants), 43)
        self.assertEqual(len(variants), len(set(variants)))
        self.assertIn("share_no_notification", variants)
        self.assertNotIn("share_repeated", variants)
        self.assertEqual({variant.split("_", 1)[0] for variant in variants}, {"renew", "plan", "share"})


if __name__ == "__main__":
    unittest.main()
