from __future__ import annotations

import inspect
from importlib.metadata import version
from types import MethodType

from harbor.agents.computer_1.computer_1 import Computer1, Computer1Recorder
from harbor.agents.computer_1.providers.base import ChatCompletionsProvider
from harbor.llms.base import ContextLengthExceededError

from deploy.runner.admission import AdmissionError, HARBOR_VERSION
from deploy.runner.inference import Computer1InferenceAdapter


class Computer1HostCapture:
    def __init__(self, agent: Computer1, adapter: Computer1InferenceAdapter, *, scenario: str, session_id: str) -> None:
        if version("harbor") != HARBOR_VERSION or type(agent) is not Computer1:
            raise AdmissionError("unsupported host inference producer")
        if type(agent._recorder) is not Computer1Recorder:
            raise AdmissionError("unsupported host inference recorder")
        self._agent = agent
        if not scenario or not session_id:
            raise AdmissionError("host capture lacks trial identity")
        self._scenario = scenario
        self._session_id = session_id
        self._adapter = adapter
        self._recorder = agent._recorder
        self._bindings = []
        self._sealed = None
        self._sealed_json = None
        self._public_response = None
        self._allowed_public_response = None
        self._missing_public_result = False
        self._recorded_turns = 0
        self._execution_failed = False
        self._install()

    def _replace(self, owner, name, expected, replacement) -> None:
        original = getattr(owner, name)
        if not inspect.ismethod(original) or original.__func__ is not expected:
            raise AdmissionError("host inference seam was already replaced")
        bound = MethodType(replacement, owner)
        setattr(owner, name, bound)
        self._bindings.append((owner, name, bound))

    def _install(self) -> None:
        original_step = self._recorder.record_agent_step
        original_parse = self._recorder.record_parse_error_step
        original_final = self._agent._write_final_answer
        original_run = self._agent.run
        capture = self

        def record_step(recorder, episode, llm_response, analysis, plan, action,
                        is_task_complete, observation, screenshot_paths, step_metrics):
            original_step(episode, llm_response, analysis, plan, action,
                          is_task_complete, observation, screenshot_paths, step_metrics)
            capture._observe(llm_response, step_metrics, False)
            explicit = (getattr(action, "result", None) or getattr(action, "text", None)) if action is not None else None
            if isinstance(getattr(capture._agent, "_provider", None), ChatCompletionsProvider):
                capture._allowed_public_response = explicit if is_task_complete else None
            else:
                capture._allowed_public_response = (explicit or llm_response.content) if is_task_complete else None

        def record_parse(recorder, llm_response, next_prompt, step_metrics):
            original_parse(llm_response, next_prompt, step_metrics)
            capture._observe(llm_response, step_metrics, True)
            capture._allowed_public_response = None

        async def write_final(agent, answer):
            if capture._sealed is not None or type(answer) is not str:
                raise AdmissionError("invalid host final-response event")
            if capture._recorded_turns == 0:
                raise AdmissionError("final response has no observed inference turn")
            if capture._allowed_public_response is not None and answer == capture._allowed_public_response:
                capture._public_response = answer
            elif isinstance(getattr(agent, "_provider", None), ChatCompletionsProvider):
                capture._missing_public_result = True
            await original_final(answer)

        async def run(agent, instruction, environment, context):
            try:
                return await original_run(instruction, environment, context)
            except ContextLengthExceededError:
                capture._adapter.observe_context_rejected()
                capture._execution_failed = True
                raise
            except BaseException:
                capture._execution_failed = True
                raise

        self._replace(self._recorder, "record_agent_step", Computer1Recorder.record_agent_step, record_step)
        self._replace(self._recorder, "record_parse_error_step", Computer1Recorder.record_parse_error_step, record_parse)
        self._replace(self._agent, "_write_final_answer", Computer1._write_final_answer, write_final)
        self._replace(self._agent, "run", Computer1.run, run)

    def _observe(self, response, metrics, parse_error: bool) -> None:
        if self._sealed is not None:
            raise AdmissionError("host inference event arrived after sealing")
        observed_model = None
        if isinstance(getattr(self._agent, "_provider", None), ChatCompletionsProvider):
            observed_model = response.model_name
        else:
            payload = (response.extra or {}).get("response_payload")
            if isinstance(payload, dict) and type(payload.get("model")) is str:
                observed_model = payload["model"]
        self._adapter.observe_turn(
            is_terminal=False,
            assistant_text=None,
            input_tokens=metrics.prompt_tokens,
            output_tokens=metrics.completion_tokens,
            cost_usd=metrics.cost_usd,
            model_name=observed_model,
            is_parse_error=parse_error,
            response_id=response.response_id,
        )
        self._recorded_turns += 1

    def _validate_bindings(self) -> None:
        if self._agent._recorder is not self._recorder:
            raise AdmissionError("host inference recorder changed during execution")
        for owner, name, expected in self._bindings:
            if getattr(owner, name) is not expected:
                raise AdmissionError("host inference observer changed during execution")

    def finalize(self, *, interrupted: bool = False):
        self._validate_bindings()
        if self._sealed is not None:
            raise AdmissionError("host inference capture was already sealed")
        reason = self._agent._early_termination_reason
        if reason == "task_complete" and self._public_response is not None:
            self._adapter.observe_final_response(self._public_response)
        elif reason == "task_complete" and self._missing_public_result:
            self._adapter.observe_missing_public_response()
        self._sealed = self._adapter.finalize(
            early_termination_reason=reason,
            interrupted=interrupted or self._execution_failed,
        )
        self._sealed.scenario = self._scenario
        self._sealed.session_id = self._session_id
        self._sealed_json = self._sealed.to_json()
        return self._sealed

    def validate_sealed(self, evidence, *, agent=None) -> None:
        self._validate_bindings()
        if self._sealed is None or self._sealed is not evidence or evidence.to_json() != self._sealed_json:
            raise AdmissionError("inference evidence lacks its host capture authority")
        if agent is not None and agent is not self._agent:
            raise AdmissionError("inference capture belongs to a different host producer")
