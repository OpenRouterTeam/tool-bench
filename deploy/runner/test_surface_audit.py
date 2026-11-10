import asyncio
import io
import json
import os
import shutil
import tarfile
import tempfile
import tomllib
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from deploy.runner import surface_audit
from deploy.runner.admission import CollectionGuard
from deploy.runner.strict_docker import StrictDockerEnvironment
from deploy.runner.surface_audit import (
    MCP_SURFACES,
    REQUIRED_SURFACES,
    SurfaceAudit,
    capture_pre_agent,
    seal_post_stop,
)

ROOT = Path(__file__).resolve().parents[2]
TASK = ROOT / "tasks/ops__grafana__ack-checkout-page-oncall"
SCENARIO = "ops/grafana/ack-checkout-page-oncall"
TOKEN = "f3a9" * 16
FIXTURE = ROOT / "crates/mock-core/tests/fixtures/sealed-surface-audit.json"
DEFINITION = "sha256:" + "1" * 64


def tar(files: dict[str, bytes]) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as bundle:
        for name, content in files.items():
            member = tarfile.TarInfo(name)
            member.size = len(content)
            bundle.addfile(member, io.BytesIO(content))
    return buffer.getvalue()


def inspect(environment=("PATH=/usr/bin", "HOME=/root"), networks=("trial_default",)):
    return {
        "Path": "sleep", "Args": ["infinity"],
        "Config": {"Env": list(environment), "Hostname": "4f2a9c01d7e3", "Domainname": "",
                   "Cmd": ["sleep", "infinity"], "Entrypoint": None},
        "Mounts": [{"Type": "bind", "Source": "/srv/trials/4f2a/agent", "Destination": "/logs/agent"}],
        "HostConfig": {"Binds": ["/srv/trials/4f2a/agent:/logs/agent:rw"]},
        "NetworkSettings": {"Networks": {name: {"Aliases": ["main"], "DNSNames": ["main"]} for name in networks}},
    }


PEERS = {
    "tools": {"trial_default": {"Aliases": ["tools"]}, "trial_backend": {"Aliases": ["tools"]}},
    "world": {"trial_backend": {"Aliases": ["world", "api.stripe.com"]}},
}


def transcript(endpoints, *, calls=(), omit_list=(), tools=(("list_alert_groups", DEFINITION),)) -> bytes:
    files = {}
    for endpoint in endpoints:
        lines = [{"endpoint": endpoint, "registered": True}]
        lines.append({"endpoint": endpoint, "method": "initialize", "body": json.dumps(
            {"jsonrpc": "2.0", "id": 1, "result": {"serverInfo": {"name": endpoint, "version": "1"}}})})
        if endpoint not in omit_list:
            lines.append({"endpoint": endpoint, "method": "tools/list", "tools": [list(tool) for tool in tools],
                          "body": json.dumps({"jsonrpc": "2.0", "id": 2, "result": {"tools": [
                              {"name": name, "description": "Lists alert groups."} for name, _ in tools]}})})
        for body in calls if endpoint == endpoints[0] else ():
            lines.append({"endpoint": endpoint, "method": "tools/call", "body": json.dumps(body)})
        files[f"surface/{endpoint}.jsonl"] = "".join(json.dumps(line) + "\n" for line in lines).encode()
    return tar(files)


class SurfaceAuditTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.task = tomllib.loads((TASK / "task.toml").read_text())
        self.reference = self.task["metadata"]["business_reference_time"]

    def audit(self, task_root=TASK) -> SurfaceAudit:
        return SurfaceAudit(task_root, SCENARIO, TOKEN)

    def complete(self, audit, *, workspace=None, environment=None, peers=PEERS, served=None, include_runtime=True):
        audit.capture_instruction((TASK / "instruction.md").read_text())
        audit.capture_archive("workspace_files", tar(workspace or {"app/notes.md": b"Checkout runbook."}))
        audit.capture_archive("home_files", None)
        audit.capture_container(inspect(*([environment] if environment else [])), peers)
        audit.capture_transcript(served if served is not None else transcript(audit.registered))
        if include_runtime:
            audit.capture("harness_messages", [("synthetic-system-message", b"Help the user with their request.")])
            audit.capture("runtime_processes", [("synthetic-agent-process", b"agent --model fixture")])
        return audit.seal(self.directory / "surface-audit.json", session_id="trial__env")

    def test_missing_harness_messages_and_actual_agent_processes_prevent_qualification(self):
        sealed = self.complete(self.audit(), include_runtime=False)
        self.assertEqual(sealed["outcome"], "unqualified")
        self.assertEqual(sealed["missing"], ["harness_messages", "runtime_processes"])

    def test_complete_capture_of_a_generated_task_is_clean_and_claims_no_unawareness(self):
        audit = self.audit()
        self.assertEqual(len(audit.registered), 32)
        sealed = self.complete(audit)
        self.assertEqual(sealed["outcome"], "clean")
        self.assertEqual(sealed["captured"], REQUIRED_SURFACES)
        self.assertEqual((sealed["missing"], sealed["finding_count"]), ([], 0))
        self.assertEqual(sealed["catalog"]["listed_endpoints"], 32)
        self.assertIsNone(sealed["catalog"]["matches_exposure_manifest"])
        self.assertIn("does_not_establish_model_unawareness", sealed["limits"])
        report = (self.directory / "surface-audit.json").read_bytes()
        self.assertEqual(sealed["report_sha256"], surface_audit.digest(report))
        with self.assertRaises(ValueError):
            audit.seal(self.directory / "again.json", session_id="trial__env")

    def test_an_uncaptured_required_surface_is_unqualified_never_passed(self):
        for name, mutate, missing in [
            ("no transcript", lambda audit: audit.capture_transcript(None), MCP_SURFACES),
            ("unlisted endpoint", lambda audit: audit.capture_transcript(
                transcript(audit.registered, omit_list=[audit.registered[3]])), ["mcp_tool_definitions"]),
            ("absent endpoint", lambda audit: audit.capture_transcript(
                transcript(audit.registered[:-1])), MCP_SURFACES),
            ("nothing captured", lambda audit: None, REQUIRED_SURFACES[1:]),
        ]:
            with self.subTest(name):
                audit = self.audit()
                audit.capture_instruction("Acknowledge the checkout page.")
                if name != "nothing captured":
                    audit.capture_archive("workspace_files", None)
                    audit.capture_archive("home_files", None)
                    audit.capture_container(inspect(), PEERS)
                    audit.capture("harness_messages", [])
                    audit.capture("runtime_processes", [])
                mutate(audit)
                sealed = audit.seal(self.directory / f"{name}.json", session_id="trial__env")
                self.assertEqual(sealed["outcome"], "unqualified")
                self.assertEqual(sealed["missing"], missing)
                self.assertIsNone(sealed["catalog"]["listed_tools"])

    def test_evaluator_owned_content_is_located_and_attributed_without_being_written(self):
        expected_answer = (TASK / "tests/expect.json").read_bytes().strip()
        audit = self.audit()
        sealed = self.complete(
            audit,
            workspace={
                "app/README.md": f"Ticket for {SCENARIO} is open.".encode(),
                "app/hints.json": expected_answer,
            },
            environment=("PATH=/usr/bin", f"UPSTREAM_KEY={TOKEN}", "API_SECRET=sk-live-never-log-this"),
            served=transcript(
                audit.registered,
                calls=[
                    {"jsonrpc": "2.0", "id": 3, "result": {"content": [{"type": "text", "text": "ok"}]}},
                    {"jsonrpc": "2.0", "id": 4, "result": {"isError": True, "content": [
                        {"type": "text", "text": "GET http://world:9000/_bench/health refused"}]}},
                ],
            ),
        )
        self.assertEqual(sealed["outcome"], "findings_present")
        reasons = {(finding["surface"], finding["ownership_reason"]) for finding in sealed["findings"]}
        self.assertLessEqual({
            ("workspace_files", "evaluator_scenario_id"),
            ("workspace_files", "evaluator_scoring_contract"),
            ("agent_environment", "evaluator_admin_credential_value"),
            ("mcp_tool_errors", "evaluator_admin_or_health_endpoint"),
        }, reasons)
        environment = [f for f in sealed["findings"] if f["surface"] == "agent_environment"]
        self.assertTrue(all(finding["item_sha256"] is None for finding in environment))
        self.assertEqual({finding["item"] for finding in environment}, {1})
        report = json.loads((self.directory / "surface-audit.json").read_bytes())
        # The private report names its own trial; nothing captured may be echoed anywhere.
        self.assertEqual(report.pop("scenario"), SCENARIO)
        written = json.dumps([sealed, report]).encode()
        for secret in [TOKEN.encode(), b"sk-live-never-log-this", SCENARIO.encode(), b"_bench", expected_answer[:40]]:
            self.assertFalse(secret in written, "captured content was written to an audit output")
        self.assertEqual(set(sealed["findings"][0]),
                         {"surface", "item", "item_sha256", "offset", "length", "ownership_reason"})

    def test_authentic_upstream_wording_is_not_a_finding(self):
        sealed = self.complete(self.audit(), served=transcript(
            self.audit().registered,
            calls=[{"jsonrpc": "2.0", "id": 3, "result": {"content": [{"type": "text", "text":
                    "Oracle database test fixture: reward points graded by the evaluator team"}]}}],
        ))
        self.assertEqual((sealed["outcome"], sealed["finding_count"]), ("clean", 0))

    def test_shared_network_does_not_fabricate_control_route_reachability(self):
        peers = {**PEERS, "world": {"trial_default": {"Aliases": ["world"]}}}
        sealed = self.complete(self.audit(), peers=peers)
        self.assertEqual(sealed["findings"], [])
        self.assertIn("network_topology_does_not_establish_control_route_reachability",
                      sealed["limits"])

    def test_date_context_must_agree_with_the_scenario_reference_time(self):
        session = lambda value: {"app/.config/workspace/session.json": json.dumps(
            {"data_as_of": value, "timezone": "UTC"}).encode()}
        for name, kwargs, status, outcome in [
            ("not supplied", {}, "not_supplied", "clean"),
            ("rendered session agrees", {"workspace": session(self.reference)}, "coherent", "clean"),
            ("session contradicts", {"workspace": session("2024-01-01T00:00:00Z")}, "contradiction", "findings_present"),
            ("environment contradicts", {"environment": ("PATH=/usr/bin", "CURRENT_DATE=2024-01-01")},
             "contradiction", "findings_present"),
            ("document contradicts", {"workspace": {"app/AGENTS.md": b"Today is 2024-01-01."}},
             "contradiction", "findings_present"),
            ("document agrees", {"workspace": {"app/AGENTS.md": f"Today is {self.reference[:10]}.".encode()}},
             "coherent", "clean"),
            ("harness-style date agrees", {"workspace": {"app/AGENTS.md": f"Today's date: {self.reference[:10]}.".encode()}},
             "coherent", "clean"),
            ("equivalent offset context", {"environment": (
                "PATH=/usr/bin", "CURRENT_DATE=" + datetime.fromisoformat(
                    self.reference.replace("Z", "+00:00")
                ).astimezone(timezone(timedelta(hours=14))).isoformat(),
            )}, "coherent", "clean"),
        ]:
            with self.subTest(name):
                self.directory = Path(tempfile.mkdtemp(dir=self.temporary.name))
                sealed = self.complete(self.audit(), **kwargs)
                self.assertEqual(sealed["date_coherence"]["status"], status)
                self.assertEqual(sealed["date_coherence"]["reference_time"], self.reference)
                self.assertEqual(sealed["outcome"], outcome)
                if status == "contradiction":
                    self.assertIn("date_context_contradicts_scenario_reference_time",
                                  [finding["ownership_reason"] for finding in sealed["findings"]])

    def exposed_task(self, tools) -> Path:
        """A copy of the generated task registering one endpoint under a private manifest."""
        root = self.directory / "task"
        shutil.copytree(TASK, root)
        text = (root / "task.toml").read_text()
        head, *servers = text.split("[[environment.mcp_servers]]")
        kept = [server for server in servers if 'name = "grafana"' in server]
        (root / "task.toml").write_text(head + "[[environment.mcp_servers]]" + kept[0])
        import hashlib
        hasher = hashlib.sha256()
        for name, definition in tools:
            for part in ("grafana", name, definition):
                hasher.update(part.encode() + b"\0")
        (root / "tests/exposure.json").write_text(json.dumps(
            {"variant": "relevant_only", "catalog_sha256": "sha256:" + hasher.hexdigest()}))
        return root

    def test_served_registration_is_compared_with_the_private_exposure_manifest(self):
        pinned = (("list_alert_groups", DEFINITION), ("update_alert_group", "sha256:" + "2" * 64))
        root = self.exposed_task(pinned)
        for name, served, matches, outcome in [
            ("as registered", pinned, True, "clean"),
            ("reordered", pinned[::-1], False, "findings_present"),
            ("extra tool", pinned + (("delete_alert_group", "sha256:" + "3" * 64),), False, "findings_present"),
            ("edited definition", (pinned[0], ("update_alert_group", "sha256:" + "9" * 64)), False, "findings_present"),
        ]:
            with self.subTest(name):
                audit = self.audit(root)
                self.assertEqual(audit.registered, ["grafana"])
                self.directory = Path(tempfile.mkdtemp(dir=self.temporary.name))
                sealed = self.complete(audit, served=transcript(["grafana"], tools=served))
                self.assertEqual(sealed["catalog"]["matches_exposure_manifest"], matches)
                self.assertEqual(sealed["catalog"]["listed_tools"], len(served))
                self.assertEqual(sealed["outcome"], outcome)

    def test_sealed_record_is_the_fixture_the_rust_verifier_reads(self):
        """Keeps the Python writer and `mock_core::surface_audit` reader on one schema."""
        sealed = self.complete(
            self.audit(),
            workspace={"app/.config/workspace/session.json": json.dumps({"data_as_of": self.reference}).encode(),
                       "app/README.md": b"See /_bench/state for details."},
        )
        sealed["report_sha256"] = "sha256:" + "a" * 64
        rendered = json.dumps(sealed, indent=2, sort_keys=True) + "\n"
        if os.environ.get("UPDATE_SURFACE_AUDIT_FIXTURE"):
            FIXTURE.parent.mkdir(parents=True, exist_ok=True)
            FIXTURE.write_text(rendered)
        self.assertEqual(FIXTURE.read_text(), rendered)


class StrictRunnerSeamTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        environment = StrictDockerEnvironment.__new__(StrictDockerEnvironment)
        environment._is_verifier = False
        environment._scenario = SCENARIO
        environment.session_id = "trial__env"
        environment._collection_guard = CollectionGuard("trial__env")
        environment._adapter_sha256 = "0" * 64
        environment._contract_sha256 = "sha256:" + "0" * 64
        for name in ["evidence", "main_artifacts", "answer", "manifest", "inference_evidence"]:
            setattr(environment, f"_{name}_sha256", None)
        environment._receipt_path = root / "runner-admission.json"
        environment.trial_paths = SimpleNamespace(trial_dir=root)
        environment._surface_audit = SurfaceAudit(TASK, SCENARIO, TOKEN)
        environment._surface_audit_sealed = None
        self.environment, self.root = environment, root
        self.archives = {
            "main:/app": tar({"app/notes.md": b"Checkout runbook."}),
            "main:/root": None,
            f"tools:{surface_audit.TRANSCRIPT_DIR}": transcript(environment._surface_audit.registered),
        }

        async def container(_, role):
            return role

        async def archive(_, container_id, path):
            content = self.archives[f"{container_id}:{path}"]
            if isinstance(content, Exception):
                raise content
            return content

        async def engine_json(*arguments):
            return inspect() if arguments[-1] == "main" else PEERS[arguments[-1]]

        environment._engine_json = engine_json
        for name, replacement in [("_container", container), ("_archive", archive)]:
            patcher = patch.object(surface_audit, name, replacement)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.trial = SimpleNamespace(task=SimpleNamespace(instruction=(TASK / "instruction.md").read_text()))

    def receipt(self) -> dict:
        self.environment._write_receipt()
        return json.loads(self.environment._receipt_path.read_text())

    async def test_runner_captures_before_the_agent_and_binds_the_sealed_audit_into_its_receipt(self):
        self.assertNotIn("surface_audit", self.receipt())
        await capture_pre_agent(self.environment, self.trial)
        await seal_post_stop(self.environment)
        receipt = self.receipt()
        self.assertEqual(receipt["surface_audit"]["outcome"], "unqualified")
        self.assertEqual(receipt["surface_audit"]["missing"], ["harness_messages", "runtime_processes"])
        sealed = self.root / "surface-audit.json"
        self.assertEqual(receipt["surface_audit"]["report_sha256"], surface_audit.digest(sealed.read_bytes()))
        self.assertEqual(json.loads(sealed.read_bytes())["session_id"], "trial__env")
        await seal_post_stop(self.environment)
        self.assertEqual(self.receipt()["surface_audit"], receipt["surface_audit"])

    async def test_capture_failures_leave_the_trial_running_and_the_audit_unqualified(self):
        self.archives["main:/root"] = RuntimeError("captured surface exceeds the audit bound")
        self.archives[f"tools:{surface_audit.TRANSCRIPT_DIR}"] = RuntimeError("engine failed")
        await capture_pre_agent(self.environment, self.trial)
        await seal_post_stop(self.environment)
        sealed = self.receipt()["surface_audit"]
        self.assertEqual(sealed["outcome"], "unqualified")
        self.assertEqual(sealed["missing"], ["home_files", *MCP_SURFACES, "harness_messages", "runtime_processes"])

    async def test_a_trial_that_never_reaches_sealing_carries_no_audit(self):
        await capture_pre_agent(self.environment, self.trial)
        self.assertNotIn("surface_audit", self.receipt())

    async def test_an_existing_report_is_never_overwritten(self):
        (self.root / "surface-audit.json").write_text("{}")
        await capture_pre_agent(self.environment, self.trial)
        await seal_post_stop(self.environment)
        self.assertNotIn("surface_audit", self.receipt())
        self.assertEqual((self.root / "surface-audit.json").read_text(), "{}")


if __name__ == "__main__":
    unittest.main()
