import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

from harbor.agents.oracle import OracleAgent
from harbor.environments.docker.docker import DockerEnvironment
from harbor.models.agent.context import AgentContext
from harbor.models.task.config import TaskOS
from harbor.models.trial.paths import TrialPaths

from deploy.runner.admission import AdmissionError, CollectionGuard
from deploy.runner.controller import install_collection_gate
from deploy.runner.strict_docker import StrictDockerEnvironment


class OracleCompletionTests(unittest.IsolatedAsyncioTestCase):
    def environment(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        environment._is_verifier = False
        environment._scenario = "fixture/oracle/completion"
        environment.session_id = "completion"
        environment._collection_guard = CollectionGuard("completion")
        environment._oracle_command = "(/solution/solve.sh) > /logs/agent/oracle.txt 2>&1"
        environment._oracle_executions = 0
        environment._oracle_completed = False
        environment._receipt_path = root / "runner-admission.json"
        environment._adapter_sha256 = "0" * 64
        environment._contract_sha256 = "sha256:" + "1" * 64
        for field in ["_evidence_sha256", "_answer_sha256", "_main_artifacts_sha256", "_manifest_sha256", "_inference_evidence_sha256"]:
            setattr(environment, field, None)
        return environment

    def oracle_trial(self):
        environment = self.environment()
        root = environment._receipt_path.parent
        task = root / "task"
        for directory in ["solution", "tests"]:
            (task / directory).mkdir(parents=True)
        (task / "task.toml").write_text('schema_version = "1.4"\n')
        (task / "instruction.md").write_text("Complete the fixture.\n")
        (task / "solution/solve.sh").write_text("#!/bin/sh\nexit 0\n")
        (task / "tests/test.sh").write_text("#!/bin/sh\nexit 0\n")
        paths = TrialPaths(root / "trial")
        paths.agent_dir.mkdir(parents=True)
        agent = OracleAgent(logs_dir=paths.agent_dir, task_dir=task, trial_paths=paths)
        environment._controller_registered = False
        environment._oracle_command = None
        environment._enable_egress_control = False
        environment.task_env_config = agent._task.config.environment
        environment.upload_dir = AsyncMock()
        callbacks = {}
        trial = SimpleNamespace(
            agent_environment=environment,
            agent=agent,
            task=agent._task,
            config=SimpleNamespace(
                artifacts=[], source_trial=None, install_only=False,
                extra_instruction_paths=[], extra_instructions=[],
                user_agent=None,
                agent=SimpleNamespace(load_trajectory=None, model_name=None, kwargs={}, env={}),
                environment=SimpleNamespace(mounts=None, extra_docker_compose=[], kwargs={}),
            ),
            add_hook=lambda event, callback: callbacks.setdefault(event, callback),
        )
        return trial

    async def test_controller_registration_matches_the_pinned_oracle_run_command(self):
        trial = self.oracle_trial()
        environment = trial.agent_environment
        install_collection_gate(trial)
        with patch.object(DockerEnvironment, "exec", new_callable=AsyncMock) as execute:
            execute.return_value = SimpleNamespace(return_code=0)
            await trial.agent.run("", environment, AgentContext())
        self.assertEqual(execute.await_count, 2)
        self.assertEqual(execute.await_args_list[0].args, ("chmod +x /solution/solve.sh",))
        self.assertEqual(
            execute.await_args_list[0].kwargs,
            {"user": "root"},
        )
        self.assertEqual(execute.await_args_list[1].args, (environment._oracle_command,))
        self.assertEqual(
            execute.await_args_list[1].kwargs,
            {
                "env": {"DEBIAN_FRONTEND": "noninteractive"},
                "timeout_sec": None,
            },
        )
        self.assertEqual(environment._oracle_executions, 1)
        self.assertTrue(environment._oracle_completed)
        environment.upload_dir.assert_awaited_once()

    async def test_pinned_oracle_cannot_hide_a_nonzero_solution_return(self):
        trial = self.oracle_trial()
        environment = trial.agent_environment
        install_collection_gate(trial)
        with patch.object(DockerEnvironment, "exec", new_callable=AsyncMock) as execute:
            execute.side_effect = [
                SimpleNamespace(return_code=0),
                SimpleNamespace(return_code=17),
            ]
            with self.assertRaisesRegex(AdmissionError, "nonzero"):
                await trial.agent.run("", environment, AgentContext())
        self.assertEqual(environment._oracle_executions, 1)
        self.assertFalse(environment._oracle_completed)
        self.assertEqual(environment._collection_guard.phase, "failed")

    async def test_registration_requires_the_supported_controller_agent_and_os(self):
        for invalid in ["controller", "agent", "os", "verifier", "repeated"]:
            with self.subTest(invalid=invalid):
                trial = self.oracle_trial()
                environment = trial.agent_environment
                environment._controller_registered = invalid != "controller"
                environment._is_verifier = invalid == "verifier"
                if invalid == "repeated":
                    environment.register_oracle_program(trial.agent, TaskOS.LINUX)
                with self.assertRaises(AdmissionError):
                    environment.register_oracle_program(
                        object() if invalid == "agent" else trial.agent,
                        TaskOS.WINDOWS if invalid == "os" else TaskOS.LINUX,
                    )

    async def test_nonzero_execution_is_rejected_despite_a_forged_success_file(self):
        environment = self.environment()
        (environment._receipt_path.parent / "exit-code.txt").write_text("0")
        with patch.object(DockerEnvironment, "exec", new_callable=AsyncMock) as execute:
            execute.return_value = SimpleNamespace(return_code=5)
            with self.assertRaises(AdmissionError):
                await environment.exec(command=environment._oracle_command)
        self.assertEqual(environment._collection_guard.phase, "failed")
        self.assertFalse(environment._oracle_completed)

    async def test_only_a_host_observed_integer_zero_completes_the_registered_program(self):
        environment = self.environment()
        (environment._receipt_path.parent / "exit-code.txt").write_text("255")
        with patch.object(DockerEnvironment, "exec", new_callable=AsyncMock) as execute:
            execute.return_value = SimpleNamespace(return_code=0)
            result = await environment.exec(command=environment._oracle_command)
        self.assertEqual(result.return_code, 0)
        self.assertEqual(environment._oracle_executions, 1)
        self.assertTrue(environment._oracle_completed)

    async def test_setup_commands_and_verifier_commands_are_not_oracle_completion(self):
        for verifier in [False, True]:
            environment = self.environment()
            environment._is_verifier = verifier
            command = environment._oracle_command if verifier else "test -f /unrelated"
            with patch.object(DockerEnvironment, "exec", new_callable=AsyncMock) as execute:
                execute.return_value = SimpleNamespace(return_code=1)
                await environment.exec(command=command)
            self.assertEqual(environment._oracle_executions, 0)
            self.assertFalse(environment._oracle_completed)

    async def test_duplicate_execution_cannot_silently_become_a_replacement_attempt(self):
        environment = self.environment()
        with patch.object(DockerEnvironment, "exec", new_callable=AsyncMock) as execute:
            execute.return_value = SimpleNamespace(return_code=0)
            await environment.exec(command=environment._oracle_command)
            with self.assertRaises(AdmissionError):
                await environment.exec(command=environment._oracle_command)
            self.assertEqual(execute.await_count, 1)
        self.assertEqual(environment._collection_guard.phase, "failed")

    async def test_malformed_return_codes_are_not_successful_completion(self):
        for code in [False, None, "0", 0.0]:
            environment = self.environment()
            with patch.object(DockerEnvironment, "exec", new_callable=AsyncMock) as execute:
                execute.return_value = SimpleNamespace(return_code=code)
                with self.assertRaises(AdmissionError):
                    await environment.exec(command=environment._oracle_command)

    async def test_an_execution_exception_cannot_leave_a_completed_oracle(self):
        environment = self.environment()
        with patch.object(DockerEnvironment, "exec", new_callable=AsyncMock) as execute:
            execute.side_effect = TimeoutError("controlled execution failure")
            with self.assertRaises(TimeoutError):
                await environment.exec(command=environment._oracle_command)
        self.assertFalse(environment._oracle_completed)
        self.assertEqual(environment._collection_guard.phase, "failed")

    async def test_missing_execution_is_rejected_before_main_shutdown_and_collection(self):
        environment = self.environment()
        environment._controller_registered = True
        environment._agent_finished = False
        environment._execution_manifest = {"agent": {"name": "oracle"}}
        with patch.object(DockerEnvironment, "prepare_logs_for_host", new_callable=AsyncMock) as prepare:
            environment.stop_service = AsyncMock()
            with self.assertRaisesRegex(AdmissionError, "oracle solution did not complete"):
                await environment.finish_agent()
            prepare.assert_not_awaited()
            environment.stop_service.assert_not_awaited()
