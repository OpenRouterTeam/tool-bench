import asyncio
import json
import tempfile
import unittest
from contextlib import asynccontextmanager, nullcontext
from datetime import datetime, timezone
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from harbor.environments.docker.docker import DockerEnvironment
from harbor.agents.nop import NopAgent
from harbor.models.trial.config import ArtifactConfig
from harbor.trial.artifact_handler import ArtifactHandler
from harbor.trial.hooks import TrialEvent
from harbor.trial.errors import AgentTimeoutError
from harbor.trial.trial import Trial

from deploy.runner.admission import AdmissionError, CollectionGuard
from deploy.runner.controller import install_collection_gate
from deploy.runner.strict_docker import StrictDockerEnvironment
from deploy.runner.test_admission import stopped


def answer_entry(manifest):
    return next(entry for entry in manifest.entries if entry.source == "/app/answer.md")


class MainCollectionTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        preparation = patch.object(
            DockerEnvironment, "prepare_logs_for_host", new_callable=AsyncMock
        )
        self.prepare_logs = preparation.start()
        self.addCleanup(preparation.stop)
        self.reset_attempt()

    def reset_attempt(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        self.environment = environment
        environment._is_verifier = False
        environment._enable_egress_control = False
        environment._collection_guard = CollectionGuard("trial")
        environment._controller_registered = False
        environment._agent_finished = False
        environment._snapshot_root = root / "main-snapshot"
        environment._mounted_publish = root / "artifacts/logs/artifacts"
        environment._mounted_publish.mkdir(parents=True)
        environment._publish_sha256 = None
        environment._answer_sha256 = None
        environment._answer_snapshot_loaded = False
        environment._answer_present = False
        environment._answer_copied = False
        environment._main_artifacts_sha256 = None
        environment._write_receipt = lambda: None
        environment.observe_execution = AsyncMock()
        environment.trial_paths = SimpleNamespace(
            trial_dir=root, artifacts_dir=root / "artifacts"
        )
        self.actions = []

        async def inspect_main():
            self.actions.append("inspect")
            return stopped()

        async def compose(arguments, **kwargs):
            self.assertEqual(arguments, ["stop", "--timeout", "10", "main"])
            self.actions.append("stop")

        async def copy_archive(source, target):
            self.actions.append("copy")
            target.write_text("Maya")

        environment._inspect_main = inspect_main
        environment._run_docker_compose_command = compose
        environment._copy_main_archive = copy_archive
        self.handler = ArtifactHandler(
            artifacts=[
                ArtifactConfig(source="/app/answer.md", destination="agent/answer.md")
            ],
            logger=SimpleNamespace(debug=lambda *args, **kwargs: None),
        )
        self.callbacks = {}
        self.trial = SimpleNamespace(
            agent_environment=environment,
            agent=NopAgent.__new__(NopAgent),
            config=SimpleNamespace(
                artifacts=[], source_trial=None, install_only=False,
                extra_instruction_paths=[], extra_instructions=[],
                user_agent=None, agent=SimpleNamespace(load_trajectory=None, model_name=None, kwargs={}, env={}),
                environment=SimpleNamespace(mounts=None, extra_docker_compose=[], kwargs={}),
            ),
            task=SimpleNamespace(has_steps=False),
            add_hook=lambda event, callback: self.callbacks.setdefault(event, callback),
        )

    async def finish(self):
        install_collection_gate(self.trial)
        await self.callbacks[TrialEvent.AGENT_END](None)

    async def collect(self):
        return await self.handler.download_artifacts(
            self.environment,
            self.environment.trial_paths.artifacts_dir,
            source_artifacts_dir="/logs/artifacts",
            services={"main"},
        )

    async def test_controller_stops_before_all_main_probes_and_copies(self):
        await self.finish()
        manifest = await self.collect()
        self.environment._verify_main_artifacts()
        self.assertEqual(self.actions[:2], ["stop", "inspect"])
        self.assertGreater(self.actions.index("copy"), self.actions.index("inspect"))
        self.assertEqual(answer_entry(manifest).status, "ok")
        self.assertTrue(self.environment._main_artifacts_sha256.startswith("sha256:"))
        self.assertTrue(self.environment._answer_sha256.startswith("sha256:"))
        await self.environment.prepare_logs_for_host()
        self.assertEqual(self.prepare_logs.await_count, 1)

    async def test_stock_collection_without_agent_end_is_permanently_inadmissible(self):
        manifest = await self.collect()
        self.assertEqual(answer_entry(manifest).status, "failed")
        self.assertNotIn("copy", self.actions)
        self.assertEqual(self.environment._collection_guard.phase, "failed")
        with self.assertRaises(AdmissionError):
            await self.environment.stop_service("main")

    async def test_actual_directory_type_is_used_instead_of_filename_suffix(self):
        async def copy_directory(source, target):
            target.mkdir()
            (target / "note").write_text("Maya")

        self.environment._copy_main_archive = copy_directory
        await self.finish()
        manifest = await self.collect()
        self.environment._verify_main_artifacts()
        self.assertEqual(answer_entry(manifest).type, "directory")
        self.assertIsNone(self.environment._answer_sha256)

    async def test_missing_answer_is_distinct_from_a_failed_copy(self):
        self.environment._copy_main_archive = AsyncMock()
        await self.finish()
        manifest = await self.collect()
        self.environment._verify_main_artifacts()
        self.assertEqual(answer_entry(manifest).status, "failed")
        self.assertEqual(self.environment._collection_guard.phase, "stopped")
        self.assertIsNone(self.environment._answer_sha256)

    async def test_failed_or_cancelled_copy_poison_collection(self):
        for failure in [OSError("engine failed"), asyncio.CancelledError()]:
            with self.subTest(failure=failure):
                self.reset_attempt()
                await self.finish()
                self.environment._copy_main_archive = AsyncMock(side_effect=failure)
                try:
                    await self.collect()
                except asyncio.CancelledError:
                    pass
                self.assertEqual(self.environment._collection_guard.phase, "failed")
                with self.assertRaises(AdmissionError):
                    await self.environment._admit_collection()

    async def test_false_successful_stop_prevents_main_and_sidecar_collection(self):
        self.environment._inspect_main = AsyncMock(
            return_value=stopped(status="running", running=True)
        )
        with self.assertRaises(AdmissionError):
            await self.finish()
        self.assertNotIn("copy", self.actions)
        with self.assertRaises(AdmissionError):
            await self.environment._admit_collection()

    async def test_cancellation_in_agent_end_cannot_be_recovered_as_success(self):
        self.prepare_logs.side_effect = asyncio.CancelledError()
        with self.assertRaises(asyncio.CancelledError):
            await self.finish()
        with self.assertRaises(AdmissionError):
            await self.environment._admit_collection()

    async def test_restart_or_replacement_between_stop_and_copy_is_rejected(self):
        await self.finish()
        self.environment._inspect_main = AsyncMock(
            return_value=stopped("replacement")
        )
        manifest = await self.collect()
        self.assertEqual(answer_entry(manifest).status, "failed")
        self.assertEqual(self.environment._collection_guard.phase, "failed")

    async def test_mounted_symlink_prevents_authoritative_collection(self):
        (self.environment._mounted_publish / "outside").symlink_to("/etc/passwd")
        with self.assertRaises(AdmissionError):
            await self.finish()
        self.assertEqual(self.environment._collection_guard.phase, "failed")

    async def test_changed_mount_or_answer_and_missing_manifest_are_not_admitted(self):
        for mutation in ["publish", "answer", "manifest"]:
            with self.subTest(mutation=mutation):
                self.reset_attempt()
                await self.finish()
                await self.collect()
                if mutation == "publish":
                    (self.environment._mounted_publish / "later").write_text("changed")
                elif mutation == "answer":
                    (self.environment.trial_paths.artifacts_dir / "agent/answer.md").write_text("changed")
                else:
                    (self.environment.trial_paths.artifacts_dir / "manifest.json").unlink()
                with self.assertRaises((AdmissionError, OSError)):
                    self.environment._verify_main_artifacts()
                self.assertEqual(self.environment._collection_guard.phase, "failed")

    async def test_ok_manifest_without_completed_copy_cannot_admit_a_deferred_download(self):
        await self.finish()
        await self.environment._snapshot_answer("/app/answer.md")
        artifacts = self.environment.trial_paths.artifacts_dir
        (artifacts / "manifest.json").write_text(json.dumps([
            {
                "source": "/app/answer.md", "status": "ok",
                "destination": "artifacts/agent/answer.md",
            },
            {
                "source": "/logs/artifacts", "status": "empty",
                "destination": "artifacts/logs/artifacts", "type": "directory",
            },
        ]))
        with self.assertRaises(AdmissionError):
            self.environment._verify_main_artifacts()

    async def test_repeated_hook_and_unsupported_controller_conditions_are_rejected(self):
        await self.finish()
        with self.assertRaises(AdmissionError):
            await self.callbacks[TrialEvent.AGENT_END](None)
        self.assertEqual(self.environment._collection_guard.phase, "failed")
        self.trial.config.source_trial = object()
        with self.assertRaises(AdmissionError):
            install_collection_gate(self.trial)

    async def run_harbor_agent_phase(self, run, timeout):
        install_collection_gate(self.trial)

        @asynccontextmanager
        async def network_policy(*args, **kwargs):
            yield

        async def emit(event):
            if event in self.callbacks:
                await self.callbacks[event](None)

        self.trial._emit = emit
        self.trial.user_agent = None
        self.trial.agent = SimpleNamespace(run=run, extra_env={})
        self.trial._now = lambda: datetime.now(timezone.utc)
        self.trial._network_plan = lambda step: SimpleNamespace(
            agent_env_baseline=None, agent_phase=None
        )
        self.trial._phase_network_policy = network_policy
        self.trial._log_context = lambda *args: nullcontext()
        self.environment.with_default_user = lambda user: nullcontext()
        self.environment.scoped_exec_env = lambda environment: nullcontext()
        await Trial._run_agent_phase(
            self.trial, target=SimpleNamespace(), instruction="ordinary request",
            timeout_sec=timeout, user=None,
        )

    async def test_real_harbor_finally_hook_stops_main_after_agent_failure(self):
        async def fail(**kwargs):
            raise RuntimeError("injected native invocation failure")

        with self.assertRaisesRegex(RuntimeError, "native invocation"):
            await self.run_harbor_agent_phase(fail, 1)
        self.assertEqual(self.environment._collection_guard.phase, "stopped")
        self.assertEqual(self.actions[:2], ["stop", "inspect"])

    async def test_real_harbor_timeout_stops_main_before_recovery_collection(self):
        async def wait(**kwargs):
            await asyncio.Event().wait()

        with self.assertRaises(AgentTimeoutError):
            await self.run_harbor_agent_phase(wait, 0.001)
        await self.collect()
        self.environment._verify_main_artifacts()
        self.assertLess(self.actions.index("stop"), self.actions.index("copy"))

    async def test_real_harbor_cancellation_still_runs_the_stop_hook(self):
        entered = asyncio.Event()

        async def wait(**kwargs):
            entered.set()
            await asyncio.Event().wait()

        task = asyncio.create_task(self.run_harbor_agent_phase(wait, None))
        await entered.wait()
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await task
        self.assertEqual(self.environment._collection_guard.phase, "stopped")
        self.assertEqual(self.actions[:2], ["stop", "inspect"])


if __name__ == "__main__":
    unittest.main()
