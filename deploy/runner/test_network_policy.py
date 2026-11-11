import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from harbor.environments.docker.docker import DockerEnvironment
from harbor.models.task.config import EnvironmentConfig, NetworkMode, NetworkPolicy, TaskOS
from harbor.models.trial.paths import TrialPaths

from deploy.runner.admission import AdmissionError, HARBOR_VERSION, RUNNER_PROTOCOL, VERIFICATION_PROTOCOL, adapter_sha256
from deploy.runner.strict_docker import StrictDockerEnvironment


class OfflineVerifierNetworkTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for role in ["environment", "tests", "trial"]:
            directory = self.root / role
            directory.mkdir()
            (directory / "Dockerfile").write_text("FROM debian:bookworm-slim\n")
        (self.root / "tests/expect.json").write_text("{}\n")
        (self.root / "tests/task-context.json").write_text("{}\n")
        (self.root / "tests/runner-policy.json").write_text(json.dumps({
            "schema_version": 1,
            "runner_protocol": RUNNER_PROTOCOL,
            "verification_protocol": VERIFICATION_PROTOCOL,
            "harbor_version": HARBOR_VERSION,
            "adapter_sha256": adapter_sha256(),
        }))
        (self.root / "task.toml").write_text(
            '[[verifier.collect]]\nservice = "world"\ncommand = "collect"\n'
        )

    def environment(self, role="tests", policy=None, phases=None, task_os=TaskOS.LINUX):
        policy = policy or NetworkPolicy(network_mode=NetworkMode.NO_NETWORK)
        with patch("deploy.runner.strict_docker.admit_task", return_value="fixture/network"), \
                patch.object(DockerEnvironment, "_egress_control_kernel_support", return_value=False) as probe:
            environment = StrictDockerEnvironment(
                environment_dir=self.root / role,
                environment_name="offline-verifier",
                session_id="offline-verifier",
                trial_paths=TrialPaths(trial_dir=self.root / "trial"),
                task_env_config=EnvironmentConfig(os=task_os),
                network_policy=policy,
                phase_network_policies=phases if phases is not None else (policy,),
            )
            probe.assert_not_called()
        if environment._offline_network_directory is not None:
            self.addCleanup(environment._offline_network_directory.cleanup)
        return environment

    def test_offline_verifier_does_not_depend_on_the_nftables_probe(self):
        environment = self.environment()
        self.assertTrue(environment.capabilities.disable_internet)
        self.assertFalse(environment.capabilities.network_allowlist)
        self.assertFalse(environment.capabilities.dynamic_network_policy)
        self.assertFalse(environment._enable_egress_control)
        self.assertEqual(
            environment._docker_compose_paths[-1].read_text(),
            "services:\n  main:\n    network_mode: none\n",
        )

    def test_other_network_modes_or_phases_cannot_weaken_the_offline_verifier(self):
        public = NetworkPolicy(network_mode=NetworkMode.PUBLIC)
        with self.assertRaises(AdmissionError):
            self.environment(policy=public)
        with self.assertRaises(AdmissionError):
            self.environment(phases=(public,))
        environment = self.environment()
        with self.assertRaises(AdmissionError):
            environment.validate_network_policy_support(public)

    def test_agent_network_capabilities_are_not_promoted_by_the_verifier_adapter(self):
        public = NetworkPolicy(network_mode=NetworkMode.PUBLIC)
        environment = self.environment(role="environment", policy=public)
        self.assertFalse(environment.capabilities.disable_internet)
        self.assertFalse(environment.capabilities.network_allowlist)
        self.assertIsNone(environment._offline_network_override)

    def test_windows_cannot_claim_the_linux_offline_enforcement(self):
        with self.assertRaises(AdmissionError):
            self.environment(task_os=TaskOS.WINDOWS)


class OfflineVerifierObservationTests(unittest.IsolatedAsyncioTestCase):
    def environment(self):
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        environment._is_verifier = True
        environment._offline_network_override = Path("private-network.yaml")
        environment._run_docker_compose_command = AsyncMock(
            return_value=SimpleNamespace(stdout="actual-verifier-container\n")
        )
        environment._engine_json = AsyncMock(
            return_value={"network_mode": "none", "networks": {"none": {}}}
        )
        return environment

    async def test_observed_none_mode_is_required_before_start_returns(self):
        environment = self.environment()
        with patch.object(DockerEnvironment, "start", new_callable=AsyncMock):
            await environment.start(False)
        environment._engine_json.assert_awaited_once()

    async def test_configured_none_or_missing_metadata_does_not_prove_actual_isolation(self):
        for observed in [
            {},
            {"network_mode": "bridge", "networks": {"bridge": {}}},
            {"network_mode": "none", "networks": {"backend": {}}},
            {"network_mode": "none", "networks": None},
        ]:
            environment = self.environment()
            environment._engine_json.return_value = observed
            with patch.object(DockerEnvironment, "start", new_callable=AsyncMock), \
                    patch.object(DockerEnvironment, "stop", new_callable=AsyncMock) as stop:
                with self.assertRaises(AdmissionError):
                    await environment.start(False)
                stop.assert_awaited_once_with(delete=True)

    async def test_engine_inspection_failure_cannot_be_rehabilitated_by_a_network_claim(self):
        environment = self.environment()
        environment._engine_json.side_effect = AdmissionError("engine unavailable")
        with self.assertRaises(AdmissionError):
            await environment._require_offline_network()
