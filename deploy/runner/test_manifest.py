import tempfile
import copy
import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from harbor.agents.nop import NopAgent

from deploy.runner.admission import AdmissionError
from deploy.runner.manifest import admit_materialization, digest, execution_context, file_inventory, image_fingerprints
from deploy.runner.strict_docker import StrictDockerEnvironment


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "environment").mkdir()
        (self.root / "environment/Dockerfile").write_text("FROM debian\n")
        (self.root / "tests").mkdir()
        (self.root / "tests/task-source.json").write_text("{}")
        for name in ["profile", "framing"]:
            (self.root / f"tests/{name}.json").write_text("{}\n")
        self.expected = {
            "instruction_sha256": digest(b"ordinary user request\n"),
            "environment_files": file_inventory(self.root / "environment"),
            **{f"{name}_sha256": digest((self.root / f"tests/{name}.json").read_bytes()) for name in ["profile", "framing"]},
        }
        self.trial = SimpleNamespace(
            agent=NopAgent.__new__(NopAgent),
            config=SimpleNamespace(agent=SimpleNamespace(model_name=None, kwargs={}, env={})),
            task=SimpleNamespace(instruction="ordinary user request\n"),
            _agent_timeout_sec=900.0, _verifier_timeout_sec=120.0,
        )

    def test_resolved_scripted_context_records_observed_code_and_budget_without_secrets(self):
        with patch("deploy.runner.manifest.implementation_inventory", return_value=digest(b"harness")):
            result = execution_context(self.trial, self.root, self.expected)
        self.assertEqual(result["agent"]["name"], "nop")
        self.assertEqual(result["execution_kind"], "scripted")
        self.assertEqual(result["agent_timeout_sec"], 900.0)
        self.assertTrue(result["agent"]["code_sha256"].startswith("sha256:"))
        self.assertNotIn("env", result)

    def test_native_or_unapproved_producers_cannot_be_promoted_by_a_familiar_name(self):
        self.trial.agent = SimpleNamespace(name=lambda: "oracle")
        with self.assertRaises(AdmissionError):
            execution_context(self.trial, self.root, self.expected)

    def test_resolved_instruction_and_environment_must_match_the_materialized_context(self):
        self.trial.task.instruction = "extra undeclared instructions"
        with self.assertRaises(AdmissionError):
            execution_context(self.trial, self.root, self.expected)
        self.trial.task.instruction = "ordinary user request\n"
        (self.root / "environment/Dockerfile").write_text("changed\n")
        with self.assertRaises(AdmissionError):
            execution_context(self.trial, self.root, self.expected)

    def test_arbitrary_scripted_settings_and_secret_bearing_environment_are_not_admitted(self):
        self.trial.config.agent.env = {"API_KEY": "synthetic-secret"}
        with self.assertRaises(AdmissionError):
            execution_context(self.trial, self.root, self.expected)

    def test_environment_inventory_never_follows_links(self):
        (self.root / "environment/link").symlink_to("/etc/passwd")
        with self.assertRaises(AdmissionError):
            file_inventory(self.root / "environment")

    def materialized(self):
        (self.root / "tests/task-source.json").write_text('{"materialization":{"schema_version":1}}')
        files = {}
        for name in ["base", "dataset", "manifest", "overlay"]:
            path = self.root / f"tests/materialization/{name}.json"
            path.parent.mkdir(exist_ok=True)
            path.write_text(json.dumps({"fixture": name}))
            files[f"materialization/{name}.json"] = digest(path.read_bytes())
        self.expected["materialization_files"] = files
        return files

    def test_materialized_task_binds_all_four_private_files_before_execution(self):
        self.materialized()
        with patch("deploy.runner.manifest.implementation_inventory", return_value=digest(b"harness")):
            result = execution_context(self.trial, self.root, self.expected)
        self.assertEqual(result["execution_kind"], "scripted")
        self.assertNotIn("materialization", result)

    def test_changed_missing_and_partial_bundles_are_rejected_before_execution(self):
        files = self.materialized()
        for name in files:
            with self.subTest(name=name):
                path = self.root / "tests" / name
                original = path.read_bytes()
                path.write_text("{}")
                with self.assertRaises(AdmissionError):
                    admit_materialization(self.root, self.expected)
                path.unlink()
                with self.assertRaises(AdmissionError):
                    admit_materialization(self.root, self.expected)
                path.write_bytes(original)
                self.expected["materialization_files"] = {key:value for key,value in files.items() if key != name}
                with self.assertRaises(AdmissionError):
                    admit_materialization(self.root, self.expected)
                self.expected["materialization_files"] = files

    def test_bundle_paths_cannot_be_substituted_with_traversal_or_source_links(self):
        files = self.materialized()
        self.expected["materialization_files"] = {"../base.json": digest(b"{}")}
        with self.assertRaises(AdmissionError):
            admit_materialization(self.root, self.expected)
        self.expected["materialization_files"] = files
        path = self.root / "tests/materialization/base.json"
        destination = self.root / "elsewhere.json"
        path.rename(destination)
        path.symlink_to(destination)
        with self.assertRaises(AdmissionError):
            admit_materialization(self.root, self.expected)
        path.unlink()
        destination.rename(path)
        directory = self.root / "tests/materialization"
        directory.rename(self.root / "moved")
        directory.symlink_to(self.root / "moved", target_is_directory=True)
        with self.assertRaises(AdmissionError):
            admit_materialization(self.root, self.expected)

    def test_materialized_and_standalone_sources_cannot_silently_exchange_contexts(self):
        self.materialized()
        for source, files in [
            ({"materialization":{"schema_version":1}}, {}),
            ({}, self.expected["materialization_files"]),
        ]:
            (self.root / "tests/task-source.json").write_text(json.dumps(source))
            self.expected["materialization_files"] = files
            with self.assertRaises(AdmissionError):
                admit_materialization(self.root, self.expected)


class ImageFingerprintTests(unittest.TestCase):
    def test_only_the_compose_project_label_is_cosmetic_not_filesystem_or_runtime_settings(self):
        before = {
            "RootFS": {"Type": "layers", "Layers": ["sha256:original"]},
            "Config": {"Env": ["PATH=/bin"], "User": "root", "Labels": {"com.docker.compose.project": "first", "application-policy": "keep"}},
        }
        after = copy.deepcopy(before)
        after["Config"]["Labels"]["com.docker.compose.project"] = "second"
        original = image_fingerprints(before)
        changed = image_fingerprints(after)
        self.assertEqual(original["filesystem_sha256"], changed["filesystem_sha256"])
        self.assertEqual(original["runtime_config_sha256"], changed["runtime_config_sha256"])
        for path in ["environment", "user", "policy", "layer"]:
            value = copy.deepcopy(before)
            if path == "environment":
                value["Config"]["Env"] = ["PATH=/different"]
            elif path == "user":
                value["Config"]["User"] = "other"
            elif path == "policy":
                value["Config"]["Labels"]["application-policy"] = "changed"
            else:
                value["RootFS"]["Layers"] = ["sha256:changed"]
            self.assertNotEqual(
                (original["filesystem_sha256"], original["runtime_config_sha256"]),
                (image_fingerprints(value)["filesystem_sha256"], image_fingerprints(value)["runtime_config_sha256"]),
            )


class ManifestCollectionTests(unittest.IsolatedAsyncioTestCase):
    def connector_environment(self, actual):
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        expected = "https://gitlab.lumen.internal/api/v4"
        environment._connector_env_sha256 = {"GITLAB_API_URL": digest(expected.encode())}
        environment._run_docker_compose_command = AsyncMock(
            return_value=SimpleNamespace(stdout="container-id")
        )
        environment._engine_json = AsyncMock(side_effect=[
            {"container_id":"container-id","image_id":digest(b"image"),"reference":"tools:fixture","resource_limits":{}},
            {"RootFS":{"Type":"layers","Layers":[]},"Config":{"Env":[f"GITLAB_API_URL={expected}"],"Labels":{}}},
            {"env":actual},
        ])
        return environment

    async def test_connector_settings_are_observed_from_the_container_not_image_defaults(self):
        environment = self.connector_environment(["GITLAB_API_URL=https://gitlab.lumen.internal/api/v4"])
        result = await environment._observe_image("tools")
        self.assertEqual(result["connector_env_sha256"], environment._connector_env_sha256)
        self.assertNotIn("https://", json.dumps(result))
        self.assertEqual(environment._engine_json.await_count,3)

    async def test_changed_missing_or_duplicate_runtime_connection_values_are_not_admitted(self):
        for entries in [
            [],
            ["GITLAB_API_URL=https://unlisted.example/api/v4"],
            ["GITLAB_API_URL=https://gitlab.lumen.internal/api/v4"]*2,
            [None],
        ]:
            environment = self.connector_environment(entries)
            with self.assertRaises(AdmissionError):
                await environment._observe_image("tools")

    async def test_non_tools_observation_does_not_read_or_publish_private_container_environment(self):
        environment = self.connector_environment(["BENCH_ADMIN_TOKEN=not-for-publication"])
        result = await environment._observe_image("world")
        self.assertNotIn("connector_env_sha256", result)
        self.assertEqual(environment._engine_json.await_count,2)

    async def test_replaced_images_or_resource_changes_prevent_manifest_publication(self):
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        environment._execution_manifest = {"images": {"world": {"container_id": "before", "image_id": "original"}}}
        environment._observe_image = AsyncMock(return_value={"container_id": "after", "image_id": "changed"})
        with self.assertRaises(AdmissionError):
            await environment._publish_manifest()

    async def test_missing_observation_is_not_recovered_from_an_agent_written_file(self):
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        environment._execution_manifest = None
        with self.assertRaises(AdmissionError):
            await environment._publish_manifest()
