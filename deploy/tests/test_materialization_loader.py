"""Organization-base materialization through the real Rust dataset loader.

`release-manifest --dataset-source` calls `mock_core::dataset::Dataset::load`,
the loader the world, task generator and verifier use. These tests drive it on
the committed diagnostic scenarios and on edited temporary copies, and compare
its render with an independent reference materializer written here.
"""

import copy
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCENARIOS = REPO / "scenarios"
BASE = "_organizations/synth-ops/base.json"
NO_ACTION = "diagnostic/stripe/no-action-already-refunded"
CLARIFY = "diagnostic/stripe/clarify-ambiguous-refund"
STANDALONE = "example/github/basic"


def reference_materialize(base: dict, overlay: dict) -> dict:
    """Independent statement of the documented base + overlay semantics."""
    assert overlay["base_id"] == base["id"]
    assert overlay["base_revision"] == base["revision"]

    def keyed(document: dict):
        document = dict(document)
        key = document.pop("_key", document.get("id"))
        return str(key), document

    stores: dict = {}
    for service, collections in base["datasets"].items():
        for collection, documents in collections.items():
            target = stores.setdefault(service, {}).setdefault(collection, {})
            for document in documents:
                key, value = keyed(document)
                assert key not in target
                target[key] = value
    for service, collections in overlay.get("removes", {}).items():
        for collection, keys in collections.items():
            for key in keys:
                del stores[service][collection][key]
    for service, collections in overlay.get("inserts", {}).items():
        for collection, documents in collections.items():
            target = stores.setdefault(service, {}).setdefault(collection, {})
            for document in documents:
                key, value = keyed(document)
                target[key] = value  # replacement keeps position, new keys append
    return stores


class MaterializationLoaderTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(
            ["cargo", "build", "--quiet", "--locked", "-p", "scenario", "--bin", "release-manifest"],
            cwd=REPO,
            check=True,
        )
        metadata = json.loads(
            subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
                cwd=REPO,
                check=True,
                capture_output=True,
                text=True,
            ).stdout
        )
        cls.binary = Path(metadata["target_directory"]) / "debug" / "release-manifest"

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name) / "scenarios"
        for part in ("_organizations", "diagnostic", "example"):
            shutil.copytree(SCENARIOS / part, self.root / part)

    def tearDown(self):
        self.temporary.cleanup()

    def load(self, scenario: str, root: Path | None = None) -> dict:
        result = subprocess.run(
            [str(self.binary), "--dataset-source", str((root or self.root) / scenario)],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            raise AssertionError(result.stderr)
        return json.loads(result.stdout)

    def fails(self, scenario: str) -> str:
        result = subprocess.run(
            [str(self.binary), "--dataset-source", str(self.root / scenario)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        return result.stderr

    def tree_bytes(self) -> dict:
        return {
            str(path.relative_to(self.root)): path.read_bytes()
            for path in sorted(self.root.rglob("*"))
            if path.is_file()
        }

    def test_committed_diagnostics_load_and_match_the_reference_render(self):
        base = json.loads((SCENARIOS / BASE).read_text())
        for scenario in (NO_ACTION, CLARIFY):
            loaded = self.load(scenario, SCENARIOS)
            manifest = loaded["source"]["materialization"]
            self.assertEqual(manifest["base_id"], "synth-ops")
            self.assertEqual(manifest["task_id"], scenario)
            self.assertEqual(manifest["source_files"]["base"]["path"], BASE)
            overlay = json.loads((SCENARIOS / scenario / "overlay.json").read_text())
            self.assertEqual(loaded["materialized"], reference_materialize(base, overlay))
        # The overlays produce the business difference each task is about.
        charges = self.load(NO_ACTION, SCENARIOS)["materialized"]["stripe"]["charges"]
        refunded = [key for key, charge in charges.items() if charge["refunded"]]
        self.assertEqual(refunded, ["ch_wkPmIRePDHbjF1ZhUb3Jf9T3"])
        other = self.load(CLARIFY, SCENARIOS)["materialized"]["stripe"]
        self.assertNotIn("refunds", other)
        self.assertFalse(any(charge["refunded"] for charge in other["charges"].values()))
        self.assertIsNotNone(self.load(CLARIFY, SCENARIOS)["scripted_user"])
        self.assertIsNone(self.load(NO_ACTION, SCENARIOS)["scripted_user"])

    def test_loading_is_hash_stable_and_never_writes_sources(self):
        before = self.tree_bytes()
        first = [self.load(scenario) for scenario in (NO_ACTION, CLARIFY, STANDALONE)]
        second = [self.load(scenario) for scenario in (NO_ACTION, CLARIFY, STANDALONE)]
        self.assertEqual(first, second)
        self.assertEqual(before, self.tree_bytes())
        self.assertNotEqual(
            first[0]["source"]["dataset_sha256"], first[1]["source"]["dataset_sha256"]
        )
        self.assertNotIn("materialization", first[2]["source"])

    def test_a_shared_base_edit_invalidates_every_dependent_task_hash(self):
        original = {name: self.load(name) for name in (NO_ACTION, CLARIFY, STANDALONE)}
        base_path = self.root / BASE

        # Whitespace only: identities change, renders do not.
        base_path.write_bytes(base_path.read_bytes() + b"\n")
        for name in (NO_ACTION, CLARIFY):
            changed = self.load(name)
            self.assertNotEqual(
                changed["source"]["dataset_sha256"], original[name]["source"]["dataset_sha256"]
            )
            self.assertEqual(changed["materialized"], original[name]["materialized"])

        # Semantic: the render of both dependents changes too.
        base = json.loads(base_path.read_text())
        base["datasets"]["stripe"]["customers"][0]["name"] = "River Electronics Ltd"
        base_path.write_text(json.dumps(base, indent=2) + "\n")
        for name in (NO_ACTION, CLARIFY):
            changed = self.load(name)
            before = original[name]["source"]["materialization"]
            after = changed["source"]["materialization"]
            self.assertNotEqual(after["base_sha256"], before["base_sha256"])
            self.assertNotEqual(after["materialized_sha256"], before["materialized_sha256"])
            self.assertEqual(after["overlay_sha256"], before["overlay_sha256"])
        # A standalone dataset does not depend on the base.
        self.assertEqual(self.load(STANDALONE), original[STANDALONE])

    def test_an_overlay_edit_invalidates_only_its_own_task(self):
        original = {name: self.load(name) for name in (NO_ACTION, CLARIFY)}
        overlay_path = self.root / CLARIFY / "overlay.json"
        overlay = json.loads(overlay_path.read_text())
        overlay["inserts"]["stripe"]["customers"][0]["description"] = "Called twice"
        overlay_path.write_text(json.dumps(overlay, indent=2) + "\n")
        self.assertEqual(self.load(NO_ACTION), original[NO_ACTION])
        self.assertNotEqual(
            self.load(CLARIFY)["source"]["dataset_sha256"],
            original[CLARIFY]["source"]["dataset_sha256"],
        )

    def test_inconsistent_or_unsafe_sources_fail_loading(self):
        base_path = self.root / BASE
        pristine = base_path.read_bytes()
        base = json.loads(pristine)

        revised = copy.deepcopy(base)
        revised["revision"] = 2
        base_path.write_text(json.dumps(revised))
        self.assertIn("overlay references base", self.fails(NO_ACTION))

        duplicate = copy.deepcopy(base)
        duplicate["datasets"]["stripe"]["customers"].append(
            duplicate["datasets"]["stripe"]["customers"][0]
        )
        base_path.write_text(json.dumps(duplicate))
        self.assertIn("key collision", self.fails(CLARIFY))

        base_path.write_bytes(pristine)
        overlay_path = self.root / NO_ACTION / "overlay.json"
        overlay = json.loads(overlay_path.read_text())
        overlay["task_id"] = CLARIFY
        overlay_path.write_text(json.dumps(overlay))
        self.assertIn("belongs to another task", self.fails(NO_ACTION))

        # A materialized scenario cannot also carry standalone data.
        (self.root / CLARIFY / "dataset" / "stripe").mkdir(parents=True)
        self.assertIn("cannot also declare standalone dataset", self.fails(CLARIFY))


if __name__ == "__main__":
    unittest.main()
