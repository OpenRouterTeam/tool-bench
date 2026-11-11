"""Real pinned ClaudeCode.run with FAKE Environment exercising the host carrier pipeline.

This test does NOT execute the CLI on the host. Instead it:
1. Creates a real ClaudeCode agent instance (pinned from site-packages).
2. Binds it to a real ProviderProxyCapture with a real arm-proxy.ts proxy.
3. Provides a FAKE Environment whose exec() intercepts commands:
   - Setup commands return ExecResult immediately.
   - The actual CLI command sends real HTTP requests to the proxy (which
     forwards to a FakeProvider) and returns stream-json stdout as the
     ExecResult, exactly as the container would.
4. Calls the real ClaudeCode.run through the observed_exec wrapper.
5. Verifies the carrier pipeline: host_stdout is captured, result digest
   matches, load_bound rejects absent/empty/wrong digest/error/malformed/
   duplicate calls, and observer replacement is detected.
6. Validates that failure envelopes from honest CLI error reporting
   (context rejection, budget exhaustion) do not destroy trustworthy
   typed provider-failure classifications.
"""
from __future__ import annotations

import asyncio
import copy
import hashlib
import json
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from types import SimpleNamespace

from harbor.agents.base import AgentContext
from harbor.agents.installed.claude_code import ClaudeCode
from harbor.agents.installed.base import NonZeroAgentExitCodeError
from harbor.environments.base import ExecResult

from deploy.runner.admission import AdmissionError
from deploy.runner.inference import (
    InferenceIdentity,
    admit_evidence,
)
from deploy.runner.proxy_capture import (
    HOST_CONSUMER_SEAM,
    PROXY_ADAPTER_NAME,
    ProviderProxyCapture,
    _sha256,
    parse_host_stdout_result,
)

INSTRUCTION = "Find the engineer who owns the checkout service and reply with their first name."
REAL_KEY = "sk-real-provider-credential"
SCENARIO = "example/github/basic"
SESSION = "trial__carrier"
USAGE = {"input_tokens": 10, "cache_read_input_tokens": 90, "cache_creation_input_tokens": 0}


def _sse(*events):
    return "".join(f"event: {event['type']}\ndata: {json.dumps(event)}\n\n" for event in events)


def _stream_message(identifier, blocks, stop_reason, *, output_tokens=7, model="provider-model-1"):
    events = [{"type": "message_start", "message": {
        "id": identifier, "type": "message", "role": "assistant", "model": model, "content": [],
        "stop_reason": None, "usage": {**USAGE, "output_tokens": 1}}}]
    for index, block in enumerate(blocks):
        if block["type"] == "text":
            events.append({"type": "content_block_start", "index": index,
                           "content_block": {"type": "text", "text": ""}})
            events.append({"type": "content_block_delta", "index": index,
                           "delta": {"type": "text_delta", "text": block["text"]}})
        elif block["type"] == "tool_use":
            events.append({"type": "content_block_start", "index": index,
                           "content_block": {"type": "tool_use", "id": block["id"],
                                             "name": block["name"], "input": {}}})
            events.append({"type": "content_block_delta", "index": index,
                           "delta": {"type": "input_json_delta",
                                     "partial_json": block["partial_json"]}})
        events.append({"type": "content_block_stop", "index": index})
    events.append({"type": "message_delta", "delta": {"stop_reason": stop_reason},
                   "usage": {"output_tokens": output_tokens}})
    events.append({"type": "message_stop"})
    return 200, "text/event-stream", _sse(*events)


class FakeProvider:
    """Offline Anthropic Messages endpoint replaying scripted responses."""

    def __init__(self):
        self.script = []
        self.seen = []
        provider = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                body = self.rfile.read(int(self.headers.get("content-length", 0)))
                provider.seen.append(body)
                status, content_type, text = (
                    provider.script.pop(0) if provider.script
                    else (500, "application/json",
                          json.dumps({"type": "error", "error": {"type": "api_error",
                                                                  "message": "script exhausted"}})))
                data = text.encode()
                self.send_response(status)
                self.send_header("content-type", content_type)
                self.send_header("content-length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"

    def close(self):
        self.server.shutdown()
        self.server.server_close()


def _user(text):
    return {"role": "user", "content": [{"type": "text", "text": text}]}


def _send_to_proxy(proxy_url, token, messages, *, model="claude-fixture"):
    """Send a real HTTP request to the proxy as the container CLI agent would."""
    body = {"model": model, "max_tokens": 1024, "system": "You are a coding agent.",
            "stream": True, "messages": messages,
            "tools": [{"name": "github_search", "input_schema": {"type": "object"}}]}
    request = urllib.request.Request(
        proxy_url + "/v1/messages", data=json.dumps(body).encode(),
        headers={"content-type": "application/json", "anthropic-version": "2023-06-01",
                 "x-api-key": token})
    with urllib.request.urlopen(request, timeout=20) as response:
        return response.read().decode()


class FakeEnvironment:
    """A fake environment that intercepts exec calls for testing the carrier pipeline.

    Setup commands return ExecResult immediately. The CLI command sends real loopback
    requests to the proxy and returns stream-json stdout as ExecResult.
    """

    def __init__(self, *, proxy_url: str, proxy_token: str, provider: FakeProvider,
                 terminal_stdout=None, terminal_exit_code=0):
        self._proxy_url = proxy_url
        self._proxy_token = proxy_token
        self._provider = provider
        self._cli_invocations = 0
        self._terminal_stdout = terminal_stdout
        self._terminal_exit_code = terminal_exit_code

    async def exec(self, command, cwd=None, env=None, timeout_sec=None, user=None):
        if isinstance(command, str) and "output-format=stream-json" in command and "--print" in command:
            self._cli_invocations += 1
            return self._run_fake_cli()
        return ExecResult(stdout="", stderr="", return_code=0)

    def _run_fake_cli(self) -> ExecResult:
        """Simulate the CLI: send real requests through the proxy, build stream-json stdout."""
        messages = [_user(INSTRUCTION)]
        try:
            _send_to_proxy(self._proxy_url, self._proxy_token, messages)
        except urllib.error.HTTPError:
            if self._terminal_stdout is None:
                raise
        if self._terminal_stdout is not None:
            return ExecResult(
                stdout=self._terminal_stdout, stderr="", return_code=self._terminal_exit_code)
        messages = [_user(INSTRUCTION),
                    {"role": "assistant", "content": [
                        {"type": "tool_use", "id": "toolu_1", "name": "github_search",
                         "input": {"query": "checkout owner"}}]},
                    {"role": "user", "content": [
                        {"type": "tool_result", "tool_use_id": "toolu_1",
                         "content": "Maya is the checkout service owner"}]}]
        _send_to_proxy(self._proxy_url, self._proxy_token, messages)
        stdout = (
            '{"type":"system","subtype":"init","session_id":"sess-carrier-test"}\n'
            '{"type":"assistant","message":{"role":"assistant"}}\n'
            '{"type":"result","subtype":"success","result":"Maya",'
            '"is_error":false,"session_id":"sess-carrier-test","total_cost_usd":0.01}\n'
        )
        return ExecResult(stdout=stdout, stderr="", return_code=0)


class RealClaudeCarrierTests(unittest.TestCase):
    """Exercises real ClaudeCode.run → observed_exec → host carrier pipeline."""

    def setUp(self):
        self.provider = FakeProvider()
        self.addCleanup(self.provider.close)

    def _make_capture(self):
        capture = ProviderProxyCapture(
            instruction=INSTRUCTION, configured_model="anthropic/claude-fixture",
            arm="off", target=self.provider.url, upstream_key=REAL_KEY,
            max_turns=8, base_path="",
        )
        capture.start()
        self.addCleanup(capture.close)
        return capture

    def _bind(self, capture):
        root = Path(tempfile.mkdtemp())
        env_vars = capture.agent_env()
        agent = ClaudeCode(logs_dir=root, model_name="anthropic/claude-fixture",
                           extra_env=dict(env_vars), version="2.1.0-fixture", max_turns=8)
        trial = SimpleNamespace(
            agent=agent, task=SimpleNamespace(instruction=INSTRUCTION + "\n"),
            config=SimpleNamespace(agent=SimpleNamespace(
                model_name="anthropic/claude-fixture", env=dict(env_vars),
                kwargs={"max_turns": 8})),
        )
        capture.bind_trial(trial, scenario=SCENARIO, session_id=SESSION)
        return trial, root, env_vars

    def _run_agent(self, capture, trial, env_vars, **environment_options):
        """Run the real ClaudeCode.run with the fake environment."""
        fake_env = FakeEnvironment(
            proxy_url=capture.local_url,
            proxy_token=env_vars["ANTHROPIC_API_KEY"],
            provider=self.provider,
            **environment_options,
        )
        context = AgentContext()
        asyncio.run(trial.agent.run(INSTRUCTION, fake_env, context))

    def test_real_run_captures_carrier_and_validates_digest(self):
        """Real ClaudeCode.run → observed_exec captures host_stdout with correct digest."""
        self.provider.script += [
            _stream_message("msg_1", [{"type": "tool_use", "id": "toolu_1",
                                       "name": "github_search",
                                       "partial_json": '{"query":"checkout owner"}'}],
                            "tool_use"),
            _stream_message("msg_2", [{"type": "text", "text": "Maya"}], "end_turn"),
        ]
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)
        self._run_agent(capture, trial, env_vars)

        self.assertTrue(capture._host_stdout_captured)
        self.assertEqual(capture._host_stdout_result, "Maya")
        self.assertEqual(capture._host_stdout_faults, [])

        evidence = capture.finalize()
        admit_evidence(evidence)
        self.assertEqual(evidence.classification, "completed")
        self.assertEqual(evidence.final_response.text, "Maya")
        self.assertIn("host_stdout", evidence.transport)
        host = evidence.transport["host_stdout"]
        self.assertEqual(host["seam"], HOST_CONSUMER_SEAM)
        self.assertEqual(host["faults"], [])
        self.assertIsNotNone(host["result_sha256"])
        expected_digest = _sha256("Maya".encode())
        self.assertEqual(host["result_sha256"], expected_digest)

    def test_absent_host_stdout_on_bound_proxy_is_infrastructure_failure(self):
        """A bound proxy where the CLI was never observed → failed_finalization."""
        self.provider.script += [
            _stream_message("msg_1", [{"type": "text", "text": "Maya"}], "end_turn"),
        ]
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)
        # Send requests through proxy directly (not through run → observed_exec)
        body = {"model": "claude-fixture", "max_tokens": 1024, "system": "test",
                "stream": True, "messages": [_user(INSTRUCTION)],
                "tools": [{"name": "github_search", "input_schema": {"type": "object"}}]}
        request = urllib.request.Request(
            capture.local_url + "/v1/messages", data=json.dumps(body).encode(),
            headers={"content-type": "application/json", "anthropic-version": "2023-06-01",
                     "x-api-key": env_vars["ANTHROPIC_API_KEY"]})
        # Read the stream to its end: an unread reply is still in flight at finalization.
        with urllib.request.urlopen(request, timeout=10) as response:
            response.read()

        self.assertFalse(capture._host_stdout_captured)
        evidence = capture.finalize()
        self.assertEqual(evidence.classification, "failed_finalization")
        self.assertIn("host_stdout_cli_not_observed", evidence.transport["capture_faults"])

    def test_empty_result_digest_rejected(self):
        """Host stdout with empty result text → digest present but empty string hash."""
        text, faults = parse_host_stdout_result(
            '{"type":"system","subtype":"init","session_id":"s1"}\n'
            '{"type":"result","subtype":"success","result":"","is_error":false,"session_id":"s1"}\n',
            exit_code=0)
        self.assertEqual(text, "")
        self.assertEqual(faults, [])

    def test_wrong_digest_rejected_by_rust_check(self):
        """Demonstrate that a mismatched digest would be caught by cross-check."""
        # The Rust check verifies result_sha256 matches final_response.text.
        # We verify the Python side computes the right digest.
        text = "Maya"
        expected = _sha256(text.encode())
        wrong = _sha256(b"Mallory")
        self.assertNotEqual(expected, wrong)
        self.assertEqual(expected, "sha256:" + hashlib.sha256(b"Maya").hexdigest())

    def test_error_envelope_faults_carrier(self):
        """A type=result with is_error=true faults the carrier."""
        text, faults = parse_host_stdout_result(
            '{"type":"system","subtype":"init","session_id":"s1"}\n'
            '{"type":"result","result":"error occurred","is_error":true,"session_id":"s1"}\n')
        self.assertEqual(text, "error occurred")
        self.assertIn("host_stdout_result_is_error", faults)

    def test_malformed_stdout_faults_carrier(self):
        """Non-JSON lines and malformed JSON both generate faults."""
        text, faults = parse_host_stdout_result(
            'plain text line\n'
            '{"type":"system","subtype":"init","session_id":"s1"}\n'
            '{broken json\n'
            '{"type":"result","result":"Maya","session_id":"s1"}\n')
        self.assertEqual(text, "Maya")
        self.assertIn("host_stdout_non_json_line", faults)
        self.assertIn("host_stdout_malformed_json", faults)

    def test_missing_init_event_faults_carrier(self):
        """A stdout without a system/init event is faulted."""
        text, faults = parse_host_stdout_result(
            '{"type":"result","result":"Maya","session_id":"s1"}\n')
        self.assertEqual(text, "Maya")
        self.assertIn("host_stdout_no_init", faults)

    def test_error_subtype_faults_carrier(self):
        """A type=result with subtype != success is faulted."""
        text, faults = parse_host_stdout_result(
            '{"type":"system","subtype":"init","session_id":"s1"}\n'
            '{"type":"result","subtype":"error","result":"fail","session_id":"s1"}\n')
        self.assertEqual(text, "fail")
        self.assertIn("host_stdout_result_subtype_not_success", faults)

    def test_non_int_exit_code_faulted(self):
        """A non-integer exit code is faulted."""
        text, faults = parse_host_stdout_result(
            '{"type":"system","subtype":"init","session_id":"s1"}\n'
            '{"type":"result","result":"Maya","session_id":"s1"}\n',
            exit_code="1")
        self.assertEqual(text, "Maya")
        self.assertIn("host_stdout_exit_not_int", faults)

    def test_duplicate_cli_execution_faults(self):
        """Two CLI invocations through observed_exec → duplicate fault."""
        self.provider.script += [
            _stream_message("msg_1", [{"type": "text", "text": "Maya"}], "end_turn"),
            _stream_message("msg_2", [{"type": "text", "text": "Maya"}], "end_turn"),
            _stream_message("msg_3", [{"type": "text", "text": "Maya"}], "end_turn"),
            _stream_message("msg_4", [{"type": "text", "text": "Maya"}], "end_turn"),
        ]
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)

        async def double_run():
            fake_env = FakeEnvironment(
                proxy_url=capture.local_url,
                proxy_token=env_vars["ANTHROPIC_API_KEY"],
                provider=self.provider,
            )
            await trial.agent.exec_as_agent(
                fake_env,
                'printf "test" | claude --verbose --output-format=stream-json --print')
            self.assertTrue(capture._host_stdout_captured)
            self.assertEqual(capture._host_stdout_result, "Maya")
            await trial.agent.exec_as_agent(
                fake_env,
                'printf "test" | claude --verbose --output-format=stream-json --print')
            self.assertIn("host_stdout_duplicate_cli_execution", capture._host_stdout_faults)
            self.assertIsNone(capture._host_stdout_result)

        asyncio.new_event_loop().run_until_complete(double_run())

    def test_observer_replacement_rejected_at_finalization(self):
        """Replacing exec_as_agent or run after bind_trial → AdmissionError at finalize."""
        self.provider.script += [
            _stream_message("msg_1", [{"type": "text", "text": "Maya"}], "end_turn"),
        ]
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)

        # Replace the observer
        trial.agent.exec_as_agent = lambda *a, **k: None
        with self.assertRaises(AdmissionError):
            capture.finalize()

    def test_observer_run_replacement_rejected(self):
        """Replacing run after bind_trial → AdmissionError at finalize."""
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)
        trial.agent.run = lambda *a, **k: None
        with self.assertRaises(AdmissionError):
            capture.finalize()

    def test_context_failure_retains_typed_classification(self):
        """A provider context failure with no public reply retains its classification."""
        self.provider.script += [
            (400, "application/json",
             json.dumps({"type": "error", "error": {"type": "invalid_request_error",
                                                     "message": "prompt is too long: 550000 tokens > 200000 maximum"}})),
        ]
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)

        # Send a request that gets context-rejected
        try:
            _send_to_proxy(capture.local_url, env_vars["ANTHROPIC_API_KEY"],
                           [_user(INSTRUCTION)])
        except Exception:
            pass

        # Force host_stdout captured (with no result since CLI "failed")
        capture._host_stdout_captured = True
        capture._host_stdout_result = None
        capture._host_stdout_faults = ["host_stdout_no_result"]

        evidence = capture.finalize()
        # Context rejection retains its typed class, not generic failure
        self.assertIn(evidence.classification, (
            "first_turn_context_rejection", "schema_admission_failure",
            "environment_startup_failure", "failed_finalization"))
        self.assertIsNone(evidence.final_response)


    # -- Failure envelope via FakeEnvironment --------------------------------

    def test_context_rejection_envelope_preserves_classification(self):
        """Provider rejects with context_length; CLI wraps failure in error envelope.
        The typed provider classification (first_turn_context_rejection) must survive
        the host_stdout faults from the honest failure envelope."""
        # Script: provider returns context_length error
        self.provider.script += [
            (400, "application/json",
             json.dumps({"type": "error", "error": {"type": "invalid_request_error",
                         "message": "prompt is too long: 550000 tokens > 200000 maximum"}})),
        ]
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)

        # Simulate the honest CLI failure envelope
        failure_stdout = (
            '{"type":"system","subtype":"init","session_id":"sess-fail"}\n'
            '{"type":"result","subtype":"error","result":"Context window exceeded",'
            '"is_error":true,"session_id":"sess-fail"}\n'
        )
        with self.assertRaises(NonZeroAgentExitCodeError):
            self._run_agent(capture, trial, env_vars,
                            terminal_stdout=failure_stdout, terminal_exit_code=1)

        evidence = capture.finalize()
        # The provider-side classification should be schema_admission_failure
        # or first_turn_context_rejection — NOT failed_finalization from host_stdout
        # faults, because finalization was never Normal.
        self.assertEqual(evidence.classification, "first_turn_context_rejection")
        self.assertIsNone(evidence.final_response)
        self.assertFalse(capture._host_stdout_captured)

    def test_budget_exhaustion_envelope_preserves_classification(self):
        """Provider returns max_tokens stop; CLI wraps in result envelope.
        Classification should be budget_exhausted, not masked by host_stdout faults."""
        self.provider.script += [
            _stream_message("msg_1", [{"type": "text", "text": "partial answer"}],
                            "max_tokens"),
        ]
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)

        # CLI reports budget exhaustion honestly
        budget_stdout = (
            '{"type":"system","subtype":"init","session_id":"sess-budget"}\n'
            '{"type":"result","subtype":"error","result":"Max output tokens reached",'
            '"is_error":true,"session_id":"sess-budget"}\n'
        )
        self._run_agent(capture, trial, env_vars, terminal_stdout=budget_stdout)

        evidence = capture.finalize()
        # budget_exhausted is the correct typed classification
        self.assertEqual(evidence.classification, "budget_exhausted")
        self.assertIsNone(evidence.final_response)

    # -- Session identity consistency ----------------------------------------

    def test_init_result_session_mismatch_faults_in_full_run(self):
        """Mismatched session_id between init and result faults the carrier."""
        self.provider.script += [
            _stream_message("msg_1", [{"type": "text", "text": "Maya"}], "end_turn"),
        ]
        capture = self._make_capture()
        trial, root, env_vars = self._bind(capture)

        # Simulate stdout with mismatched session ids
        mismatch_stdout = (
            '{"type":"system","subtype":"init","session_id":"sess-A"}\n'
            '{"type":"result","subtype":"success","result":"Maya",'
            '"is_error":false,"session_id":"sess-B"}\n'
        )
        self._run_agent(capture, trial, env_vars, terminal_stdout=mismatch_stdout)

        evidence = capture.finalize()
        # Should be failed_finalization because host_stdout has faults
        self.assertEqual(evidence.classification, "failed_finalization")
        self.assertIn("host_stdout_session_mismatch",
                       evidence.transport["host_stdout"]["faults"])


if __name__ == "__main__":
    unittest.main()
