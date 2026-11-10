import copy
import json
import logging
import tempfile
import unittest
from pathlib import Path, PurePosixPath
from types import SimpleNamespace
from unittest.mock import AsyncMock

from harbor.agents.computer_1.computer_1 import Computer1, Computer1Recorder
from harbor.agents.computer_1.providers.generic import GenericJsonProvider
from harbor.agents.computer_1.runtime import ComputerAction
from harbor.llms.base import ContextLengthExceededError, LLMResponse
from harbor.models.agent.context import AgentContext
from harbor.models.trajectories import Metrics

from deploy.runner.admission import AdmissionError
from deploy.runner.computer1_capture import Computer1HostCapture
from deploy.runner.inference import Computer1InferenceAdapter
from deploy.runner.strict_docker import StrictDockerEnvironment


class HostCaptureTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.agent = Computer1.__new__(Computer1)
        self.agent.logs_dir = self.root
        self.agent.logger = logging.getLogger("offline-computer1-capture")
        self.agent._recorder = Computer1Recorder(self.root, "offline", "computer-1", "fixture", "fixture-model")
        self.agent._early_termination_reason = None
        self.agent._provider = GenericJsonProvider(
            model_name="fixture-model", desktop_width=1, desktop_height=1, enable_images=False,
        )
        self.agent._env_io_dir = PurePosixPath("/logs/agent")
        self.agent._session = SimpleNamespace(
            environment=SimpleNamespace(exec=AsyncMock(return_value=SimpleNamespace(return_code=0))),
            is_session_alive=AsyncMock(return_value=True),
        )
        self.adapter = Computer1InferenceAdapter("fixture-model", "unknown")
        self.capture = Computer1HostCapture(
            self.agent, self.adapter, scenario="diagnostic/host/capture", session_id="offline-session",
        )

    def record(self, *, parse_error=False, tokens=None, cost=None):
        response = LLMResponse(
            content='{"analysis":"PRIVATE_CHAIN","action":{"type":"done","result":"Public answer"}}',
            reasoning_content="PRIVATE_REASONING",
            model_name="observed-fixture",
        )
        metrics = Metrics(prompt_tokens=tokens, completion_tokens=0, cost_usd=cost)
        if parse_error:
            self.agent._recorder.record_parse_error_step(response, "retry", metrics)
        else:
            self.agent._recorder.record_agent_step(
                0, response, "PRIVATE_CHAIN", "", ComputerAction(type="done", result="Public answer"),
                True, "", [], metrics,
            )

    async def test_real_host_recorder_and_final_writer_are_observed_without_actor_files(self):
        self.record(tokens=0, cost=0.0)
        await self.agent._write_final_answer("Public answer")
        self.agent._early_termination_reason = "task_complete"
        evidence = self.capture.finalize()
        self.capture.validate_sealed(evidence)
        self.assertEqual(evidence.final_response.text, "Public answer")
        self.assertEqual(evidence.final_response.turn, 0)
        self.assertEqual(evidence.turns[0].kind, "final_response")
        self.assertEqual(evidence.cost.total_input_tokens, 0)
        self.assertEqual(evidence.cost.total_cost_usd, 0.0)
        self.assertIsNone(evidence.turns[0].retry_count)
        self.assertNotIn("PRIVATE", evidence.to_json())
        self.assertTrue(evidence.validates()[0])
        self.assertFalse(evidence.admits_native())
        with self.assertRaises(AdmissionError):
            self.capture.validate_sealed(copy.deepcopy(evidence))

    async def test_confirmation_candidate_is_not_the_final_answer(self):
        self.record(tokens=10)
        self.agent._early_termination_reason = "max_turns_reached"
        evidence = self.capture.finalize()
        self.assertNotEqual(evidence.classification, "completed")
        self.assertIsNone(evidence.final_response)
        self.assertEqual(evidence.finalization, "no_response")
        self.assertEqual(evidence.turns[0].kind, "tool_use")

    async def test_unobserved_final_writer_cannot_create_native_turns(self):
        with self.assertRaises(AdmissionError):
            await self.agent._write_final_answer("forged")

    async def test_task_complete_without_a_final_write_is_capture_failure_not_completion(self):
        self.record()
        self.agent._early_termination_reason = "task_complete"
        evidence = self.capture.finalize()
        self.assertEqual(evidence.classification, "failed_finalization")
        self.assertEqual(evidence.finalization, "capture_failed")
        self.assertIsNone(evidence.final_response)
        self.assertTrue(evidence.validates()[0])

    async def exercise_context_rejection(self, earlier_turn: bool):
        agent = self.agent
        agent.options = SimpleNamespace(start_url=None)
        agent._llm = SimpleNamespace()
        agent._build_provider = lambda: agent._provider
        agent._capture_screenshot = AsyncMock(return_value="offline.webp")
        agent._compactor = SimpleNamespace(compaction_count=0)
        agent._n_episodes = 0
        agent._api_request_times = []
        agent._maybe_write_final_answer_fallback = AsyncMock()
        agent._recorder.dump_trajectory = lambda *args: None

        async def reject(*args, **kwargs):
            if earlier_turn:
                self.record(tokens=10)
            raise ContextLengthExceededError("offline typed rejection")

        agent._run_loop = reject
        with self.assertRaises(ContextLengthExceededError):
            await agent.run("ordinary request", agent._session.environment, AgentContext())
        evidence = self.capture.finalize()
        self.assertEqual(evidence.classification, "later_turn_context_rejection" if earlier_turn else "first_turn_context_rejection")
        self.assertIsNone(evidence.final_response)
        self.assertTrue(evidence.validates()[0])

    async def test_real_run_context_failure_is_first_turn_without_transcript_grep(self):
        await self.exercise_context_rejection(False)

    async def test_real_run_context_failure_is_later_turn_after_an_observed_response(self):
        await self.exercise_context_rejection(True)

    async def test_configured_vendor_model_echo_is_not_provider_observed_identity(self):
        self.agent._provider = SimpleNamespace()
        self.record()
        await self.agent._write_final_answer("Public answer")
        self.agent._early_termination_reason = "task_complete"
        evidence = self.capture.finalize()
        self.assertIsNone(evidence.identity.observed_model)

    async def test_replaced_observer_and_post_seal_events_fail_closed(self):
        self.record()
        self.agent._recorder.record_agent_step = lambda *args: None
        with self.assertRaises(AdmissionError):
            self.capture.finalize()

    async def test_sealed_memory_document_cannot_be_mutated_before_binding(self):
        self.record()
        await self.agent._write_final_answer("Public answer")
        self.agent._early_termination_reason = "task_complete"
        evidence = self.capture.finalize()
        evidence.identity.configured_model = "substituted"
        with self.assertRaises(AdmissionError):
            self.capture.validate_sealed(evidence)

    async def test_missing_cost_in_one_turn_does_not_become_a_partial_total(self):
        self.record(tokens=10, cost=0.2)
        self.record(tokens=None, cost=None)
        await self.agent._write_final_answer("Public answer")
        self.agent._early_termination_reason = "task_complete"
        evidence = self.capture.finalize()
        self.assertIsNone(evidence.cost.total_input_tokens)
        self.assertIsNone(evidence.cost.total_cost_usd)
        with self.assertRaises(AdmissionError):
            self.record()

    async def test_actor_document_without_host_capture_authority_cannot_be_bound(self):
        self.record()
        await self.agent._write_final_answer("Public answer")
        self.agent._early_termination_reason = "task_complete"
        evidence = self.capture.finalize()
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        environment._is_verifier = False
        environment._scenario = "diagnostic/host/capture"
        environment.session_id = "offline-session"
        environment._observed_agent = self.agent
        environment._inference_evidence_sha256 = None
        environment._write_receipt = lambda: None
        environment.trial_paths = SimpleNamespace(trial_dir=self.root)
        with self.assertRaises(AdmissionError):
            environment.bind_inference_evidence(evidence)
        environment.bind_inference_evidence(evidence, host_capture=self.capture)
        self.assertTrue((self.root / "inference-evidence.json").is_file())

    async def test_capture_from_a_different_session_or_host_producer_cannot_be_reused(self):
        self.record()
        await self.agent._write_final_answer("Public answer")
        self.agent._early_termination_reason = "task_complete"
        evidence = self.capture.finalize()
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        environment._is_verifier = False
        environment._inference_evidence_sha256 = None
        environment._scenario = "diagnostic/host/capture"
        environment.session_id = "another-session"
        environment._observed_agent = self.agent
        with self.assertRaises(AdmissionError):
            environment.bind_inference_evidence(evidence, host_capture=self.capture)
        environment.session_id = "offline-session"
        environment._observed_agent = Computer1.__new__(Computer1)
        with self.assertRaises(AdmissionError):
            environment.bind_inference_evidence(evidence, host_capture=self.capture)

    async def exercise_generic_loop(self, public_result):
        agent = self.agent
        agent.options = SimpleNamespace(enable_episode_logging=False, max_turns=2, llm_call_kwargs=None)
        agent._context = AgentContext()
        agent._pending_completion = False
        agent._screenshot_suffix = "webp"
        agent._n_episodes = 0
        agent._api_request_times = []
        agent._apply_wait_streak = lambda action, complete, observation: observation
        agent._capture_screenshot = AsyncMock(return_value="offline.webp")
        agent._screenshot_ref = AsyncMock(return_value="offline-screenshot")
        agent._compactor = SimpleNamespace(maybe_proactively_compact=AsyncMock(return_value=None))
        agent._provider = GenericJsonProvider(
            model_name="fixture-model", desktop_width=1, desktop_height=1, enable_images=False,
        )
        agent._recorder.publish_snapshot = lambda *args: None
        chat = SimpleNamespace(total_input_tokens=0, total_output_tokens=0, total_cache_tokens=0, total_cost=0.0)

        async def respond(*args, **kwargs):
            chat.total_input_tokens += 10
            chat.total_output_tokens += 5
            chat.total_cost += 0.01
            action = {"type": "done"}
            if public_result is not None:
                action["result"] = public_result
            return LLMResponse(
                content=json.dumps({"analysis": "PRIVATE_CHAIN", "plan": "PRIVATE_PLAN",
                                    "action": action}),
                reasoning_content="PRIVATE_REASONING",
                model_name="observed-fixture",
            )

        chat.chat = respond
        agent._chat = chat
        await agent._run_loop("ordinary request", "offline.webp", original_instruction="ordinary request")
        return self.capture.finalize()

    async def test_actual_generic_episode_loop_captures_only_confirmed_public_result(self):
        evidence = await self.exercise_generic_loop("Confirmed public result")
        agent = self.agent
        self.assertEqual(agent._early_termination_reason, "task_complete")
        self.assertEqual(len(evidence.turns), 2)
        self.assertEqual([turn.kind for turn in evidence.turns], ["tool_use", "final_response"])
        self.assertEqual(evidence.final_response.text, "Confirmed public result")
        self.assertEqual(evidence.cost.total_input_tokens, 20)
        self.assertNotIn("PRIVATE", evidence.to_json())

    async def test_actual_generic_loop_raw_json_fallback_is_not_a_public_answer(self):
        evidence = await self.exercise_generic_loop(None)
        self.assertEqual(evidence.classification, "no_public_response")
        self.assertEqual(evidence.finalization, "no_response")
        self.assertIsNone(evidence.final_response)
        self.assertNotIn("PRIVATE", evidence.to_json())
        self.assertTrue(evidence.validates()[0])


if __name__ == "__main__":
    unittest.main()
