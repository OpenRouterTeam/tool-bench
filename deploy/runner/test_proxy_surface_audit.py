import json
import asyncio
import tempfile
import unittest
import urllib.error
import urllib.request
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

from harbor.agents.installed.claude_code import ClaudeCode
from harbor.trial.hooks import TrialEvent

from deploy.runner.controller import install_collection_gate
from deploy.runner.proxy_capture import ProviderProxyCapture
from deploy.runner.strict_docker import StrictDockerEnvironment
from deploy.runner.surface_audit import ProxyHarnessMessages, SurfaceAudit
from deploy.runner.test_proxy_capture import CONTEXT, FakeProvider, INSTRUCTION, stream_message


ROOT = Path(__file__).resolve().parents[2]
TASK = ROOT / "tasks/ops__grafana__ack-checkout-page-oncall"
SCENARIO = "ops/grafana/ack-checkout-page-oncall"
TOKEN = "private-audit-credential-should-not-be-written"


class ProxySurfaceAuditTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.provider = FakeProvider()
        self.addCleanup(self.provider.close)
        self.proxy = ProviderProxyCapture(
            instruction=INSTRUCTION, configured_model="fixture", arm="off",
            target=self.provider.url, upstream_key="offline-fixture-credential",
        )
        self.proxy.start()
        self.addCleanup(self.proxy.close)
        self.audit = SurfaceAudit(TASK, SCENARIO, TOKEN)
        self.observer = ProxyHarnessMessages(self.audit, self.proxy)

    def request(self, *, system=None, messages=None, tools=None, response=None):
        response = response or stream_message(
            "offline-reply", [{"type": "text", "text": "Maya"}], "end_turn",
        )
        self.provider.script.append(response)
        body = {
            "model": "fixture", "max_tokens": 128, "stream": True,
            "messages": messages or [{"role": "user", "content": INSTRUCTION}],
        }
        if system is not None:
            body["system"] = system
        if tools is not None:
            body["tools"] = tools
        request = urllib.request.Request(
            self.proxy.local_url + "/v1/messages",
            data=json.dumps(body).encode(),
            headers={
                "content-type": "application/json",
                "x-api-key": self.proxy.agent_env()["ANTHROPIC_API_KEY"],
            },
        )
        try:
            reply = urllib.request.urlopen(request, timeout=20)
        except urllib.error.HTTPError as error:
            reply = error
        with reply:
            self.assertEqual(reply.status, response[0])
            reply.read()

    def seal(self):
        self.proxy.finalize()
        self.observer.finish()
        path = self.directory / "surface-audit.json"
        sealed = self.audit.seal(path, session_id="fixture__env")
        return sealed, path.read_bytes()

    def test_actual_forwarded_prompts_and_schema_descriptions_are_scanned_without_echoing_secrets(self):
        artifact = (TASK / "tests/expect.json").read_text().strip()
        self.request(
            system=[{"type": "text", "text": f"Inspect {TOKEN} for {SCENARIO}."}],
            messages=[{"role": "user", "content": [
                {"type": "text", "text": INSTRUCTION},
                {"type": "text", "text": artifact},
            ]}],
            tools=[{"name": "lookup", "description": "Call /_bench/state for details.",
                    "input_schema": {"type": "object"}}],
        )
        sealed, report = self.seal()
        self.assertIn("harness_messages", sealed["captured"])
        self.assertIn("runtime_processes", sealed["missing"])
        reasons = {finding["ownership_reason"] for finding in sealed["findings"]}
        self.assertLessEqual({
            "evaluator_admin_credential_value", "evaluator_scenario_id",
            "evaluator_scoring_contract", "evaluator_admin_or_health_endpoint",
        }, reasons)
        events = (self.proxy._directory / "events.jsonl").read_bytes()
        private_report = json.loads(report)
        self.assertEqual(private_report.pop("scenario"), SCENARIO)
        redacted_report = json.dumps(private_report).encode()
        for sensitive in [TOKEN.encode(), SCENARIO.encode(), artifact.encode()]:
            self.assertNotIn(sensitive, redacted_report)
            self.assertNotIn(sensitive, json.dumps(sealed).encode())
            self.assertNotIn(sensitive, events)
        forwarded = json.loads(self.provider.seen[0]["body"])
        self.assertEqual(forwarded["system"][0]["text"], f"Inspect {TOKEN} for {SCENARIO}.")

    def test_harness_dates_are_compared_with_the_business_reference_not_the_host_clock(self):
        self.request(system=f"Today's date: {self.audit.reference_time[:10]}.")
        self.request(system="Current date: 2000-01-01.")
        sealed, _ = self.seal()
        self.assertIn("harness_messages", sealed["captured"])
        self.assertEqual(sealed["date_coherence"]["carriers_checked"], 2)
        self.assertEqual(sealed["date_coherence"]["contradictions"], 1)
        self.assertEqual(sealed["date_coherence"]["status"], "contradiction")
        self.assertIn("date_context_contradicts_scenario_reference_time",
                      {finding["ownership_reason"] for finding in sealed["findings"]})

    def test_every_request_including_auxiliary_calls_needs_the_bound_observer(self):
        self.request(system="An ordinary work request.")
        self.observer.path.unlink()
        self.request(system="An auxiliary request without the bound scan.")
        sealed, _ = self.seal()
        self.assertNotIn("harness_messages", sealed["captured"])
        self.assertIn("harness_messages", sealed["missing"])

    def test_changed_or_malformed_scan_configuration_cannot_qualify_inputs(self):
        self.observer.path.write_text("{}")
        self.request(system="An ordinary work request.")
        sealed, _ = self.seal()
        self.assertIn("harness_messages", sealed["missing"])

    def test_no_inference_request_is_missing_evidence_not_an_empty_clean_capture(self):
        sealed, _ = self.seal()
        self.assertIn("harness_messages", sealed["missing"])

    def test_private_reasoning_is_never_stored_or_scanned_as_a_harness_instruction(self):
        self.request(messages=[
            {"role": "user", "content": INSTRUCTION},
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": TOKEN, "signature": "private"},
                {"type": "text", "text": "Checking the requested record."},
            ]},
            {"role": "user", "content": "Continue."},
        ])
        sealed, report = self.seal()
        self.assertIn("harness_messages", sealed["captured"])
        self.assertEqual(sealed["findings"], [])
        self.assertNotIn(TOKEN.encode(), report)
        self.assertNotIn(TOKEN.encode(), (self.proxy._directory / "events.jsonl").read_bytes())

    def test_an_independently_generated_oracle_shaped_reply_is_not_a_harness_artifact_leak(self):
        artifact = next(
            marker.decode()
            for marker, reason, _ in self.audit.markers
            if reason == "evaluator_solution_artifact" and len(marker) >= 32
        )
        self.request(messages=[
            {"role": "user", "content": INSTRUCTION},
            {"role": "assistant", "content": artifact},
            {"role": "user", "content": "Continue."},
        ])
        sealed, _ = self.seal()
        self.assertIn("harness_messages", sealed["captured"])
        self.assertNotIn(
            "evaluator_solution_artifact",
            {finding["ownership_reason"] for finding in sealed["findings"]},
        )

    def test_the_controller_captures_actual_prompts_even_when_the_provider_rejects_context(self):
        self.observer.path.unlink()
        variables = self.proxy.agent_env()
        agent = ClaudeCode(
            logs_dir=self.directory, model_name="fixture",
            extra_env=dict(variables), version="2.1.0-fixture",
        )
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        environment._is_verifier = False
        environment._scenario = SCENARIO
        environment.session_id = "fixture__env"
        environment._observed_agent = agent
        environment._inference_evidence_sha256 = None
        environment._write_receipt = lambda: None
        environment.trial_paths = SimpleNamespace(trial_dir=self.directory)
        environment._controller_registered = False
        environment.finish_agent = AsyncMock()
        environment.observe_execution = AsyncMock()
        environment._surface_audit = self.audit
        callbacks = {}
        trial = SimpleNamespace(
            agent_environment=environment, agent=agent, exception_info=None,
            config=SimpleNamespace(
                artifacts=[], source_trial=None, install_only=False,
                extra_instruction_paths=[], extra_instructions=[], user_agent=None,
                agent=SimpleNamespace(
                    load_trajectory=None, model_name="fixture", kwargs={}, env=variables,
                ),
                environment=SimpleNamespace(mounts=None, extra_docker_compose=[], kwargs={}),
            ),
            task=SimpleNamespace(has_steps=False, instruction=INSTRUCTION),
            add_hook=lambda event, callback: callbacks.setdefault(event, callback),
        )
        install_collection_gate(trial, self.proxy)
        self.request(
            system=f"Today's date: {self.audit.reference_time[:10]}.",
            response=CONTEXT,
        )
        asyncio.run(callbacks[TrialEvent.AGENT_END](None))
        environment.finish_agent.assert_awaited_once()
        evidence = json.loads((self.directory / "inference-evidence.json").read_bytes())
        self.assertEqual(
            evidence["classification"], "first_turn_context_rejection",
            evidence["transport"]["capture_faults"],
        )
        sealed = self.audit.seal(self.directory / "surface-audit.json", session_id="fixture__env")
        self.assertIn("harness_messages", sealed["captured"])
        self.assertEqual(sealed["date_coherence"]["status"], "coherent")
        self.assertIn("runtime_processes", sealed["missing"])


if __name__ == "__main__":
    unittest.main()
