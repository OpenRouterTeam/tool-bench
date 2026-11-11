"""Trusted-host resolved-surface audit (improvement plan items 7 and 9).

The strict runner captures the bytes a trial actually exposed to the agent and scans
them on the host for evaluator-owned identifiers:

* the resolved instruction Harbor hands the agent;
* the files really present in the agent container's workspace and home;
* the agent container's environment, argv, mounts and DNS-visible hostnames;
* the MCP ``initialize``/``tools/list`` responses and tool result/error bodies the
  tools container really served, from its passive transcript.

A required surface that was not captured makes the audit ``unqualified``; it is never
counted as passed. Findings are redacted locations, hashes and an ownership reason.
Marker text, file contents and environment values are never written anywhere.

The full report stays in the trial directory on the host, outside every agent-writable
mount. Its hash and a summary are bound into the runner receipt, which the verifier and
report read through ``mock_core::surface_audit``.

A clean result says the captured bytes held none of the known markers. It does not
say, and must never be worded as saying, that the model was unaware of being evaluated.
"""

import asyncio
import base64
import hashlib
import io
import json
import re
import tarfile
import tomllib
from datetime import datetime, timezone
from pathlib import Path


REQUIRED_SURFACES = [
    "resolved_instruction",
    "workspace_files",
    "home_files",
    "agent_environment",
    "agent_argv",
    "agent_mounts",
    "agent_hostnames",
    "mcp_initialize",
    "mcp_tool_definitions",
    "mcp_tool_results",
    "mcp_tool_errors",
    "harness_messages",
    "runtime_processes",
]
CONTAINER_SURFACES = ["agent_environment", "agent_argv", "agent_mounts", "agent_hostnames"]
MCP_SURFACES = ["mcp_initialize", "mcp_tool_definitions", "mcp_tool_results", "mcp_tool_errors"]
LIMITS = [
    "lexical_marker_scan_of_captured_bytes_only",
    "does_not_establish_model_unawareness",
    "uncaptured_surfaces_are_unqualified_not_passed",
    "authentic_upstream_wording_is_not_treated_as_a_leak",
    "harness_input_capture_requires_the_host_provider_proxy",
    "private_reasoning_blocks_are_not_captured",
    "network_topology_does_not_establish_control_route_reachability",
]
TRANSCRIPT_DIR = "/var/lib/tool-bench/surface"
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024
MAX_SEALED_FINDINGS = 25
MAX_MATCHES_PER_MARKER = 20

# Evaluator-owned names, matched case-insensitively. Generic words an authentic vendor
# tool may legitimately use (test, fixture, reward, oracle) are deliberately absent.
STATIC_MARKERS = [
    (b"_bench", "evaluator_admin_or_health_endpoint"),
    (b"bench_admin_token", "evaluator_admin_credential_name"),
    (b"harbor_trial_admin_token", "evaluator_admin_credential_name"),
    (b"bench_api_caller_host", "evaluator_control_setting"),
    (b"bench-verify", "evaluator_verifier_binary"),
    (b"tool-bench", "evaluator_product_identifier"),
    (b"tool_bench", "evaluator_product_identifier"),
    (b"toolbench", "evaluator_product_identifier"),
    (b"expect.json", "evaluator_scoring_contract"),
    (b"result-contracts", "evaluator_scoring_contract"),
    (b"reward.json", "evaluator_scoring_signal"),
    (b"/logs/verifier", "evaluator_scoring_signal"),
    (b"task-source.json", "evaluator_private_manifest"),
    (b"task-context.json", "evaluator_private_manifest"),
    (b"runner-admission", "evaluator_private_manifest"),
    (b"realized-manifest", "evaluator_private_manifest"),
    (b"solve.sh", "evaluator_solution_artifact"),
]
DATE_ENVIRONMENT_NAMES = {
    "TOOL_BENCH_SCENARIO_NOW", "SOURCE_DATE_EPOCH", "FAKETIME", "CURRENT_DATE", "TODAY", "DATE",
}
DATE_PHRASE = re.compile(
    rb"(?i)\b(?:today(?:'s date)?(?: is|:)|(?:the )?current date(?: is|:))\s*(\d{4}-\d{2}-\d{2})"
)
SESSION_SETTINGS = ".config/workspace/session.json"


def digest(content: bytes) -> str:
    return "sha256:" + hashlib.sha256(content).hexdigest()


def _instant(value) -> datetime | None:
    """Parses an RFC 3339 instant, a calendar date, or epoch seconds, in UTC."""
    try:
        if isinstance(value, (int, float)) or str(value).isdigit():
            return datetime.fromtimestamp(int(value), timezone.utc)
        text = str(value).strip().lstrip("@")
        parsed = datetime.fromisoformat(text.replace("Z", "+00:00"))
        return parsed.astimezone(timezone.utc) if parsed.tzinfo else parsed.replace(tzinfo=timezone.utc)
    except (ValueError, OverflowError, OSError):
        return None


class SurfaceAudit:
    """Accumulates captured surfaces for one trial, then seals the audit once."""

    def __init__(self, task_root: Path, scenario: str, admin_token: str | None):
        task = tomllib.loads((task_root / "task.toml").read_text())
        self.scenario = scenario
        self.reference_time = task["metadata"]["business_reference_time"]
        self.registered = [server["name"] for server in task["environment"]["mcp_servers"]]
        exposure = task_root / "tests/exposure.json"
        self.exposure_catalog_sha256 = (
            json.loads(exposure.read_bytes())["catalog_sha256"] if exposure.is_file() else None
        )
        # (marker bytes, ownership reason, case-insensitive)
        self.markers = [(marker, reason, True) for marker, reason in STATIC_MARKERS]
        for value in {scenario, scenario.replace("/", "__")}:
            self.markers.append((value.encode(), "evaluator_scenario_id", False))
        if admin_token:
            self.markers.append((admin_token.encode(), "evaluator_admin_credential_value", False))
        private = [(task_root / "tests/expect.json", "evaluator_scoring_contract")]
        solution = task_root / "solution"
        if solution.is_dir():
            private += [
                (path, "evaluator_solution_artifact")
                for path in sorted(solution.iterdir())
                if path.name == "answer.md" or path.name.startswith("call-")
            ]
        for path, reason in private:
            content = path.read_bytes().strip() if path.is_file() else b""
            if len(content) >= 32:
                self.markers.append((content, reason, False))
        self._surfaces: dict[str, list[dict]] = {}
        self._missing: dict[str, str] = {}
        self._findings: list[dict] = []
        self._carriers = 0
        self._contradictions = 0
        self._listed: dict[str, list] = {}
        self._sealed = False

    @classmethod
    def create(cls, task_root: Path, scenario: str, admin_token: str | None):
        """The audit for a task, or ``None`` when its sources cannot seed one.

        Task admission is decided elsewhere. A trial without an audit simply carries no
        sealed record, which the verifier reports as unqualified.
        """
        try:
            return cls(task_root, scenario, admin_token)
        except Exception:
            return None

    # ── capture ────────────────────────────────────────────────────────────────

    def uncaptured(self, surfaces, reason: str) -> None:
        for surface in [surfaces] if isinstance(surfaces, str) else surfaces:
            if surface not in self._surfaces:
                self._missing.setdefault(surface, reason)

    def capture(self, surface: str, items: list[tuple[str, bytes]], *, hash_items: bool = True) -> None:
        """Records and scans one surface. Item names stay in the private host report."""
        if self._sealed or surface in self._surfaces or surface not in REQUIRED_SURFACES:
            raise ValueError("surface was already captured, sealed, or is not audited")
        self._missing.pop(surface, None)
        records = []
        for index, (name, content) in enumerate(items):
            item_sha256 = digest(content) if hash_items else None
            records.append({"index": index, "name": name, "sha256": item_sha256, "bytes": len(content)})
            lowered = content.lower()
            for marker, reason, insensitive in self.markers:
                haystack, needle = (lowered, marker.lower()) if insensitive else (content, marker)
                start = 0
                for _ in range(MAX_MATCHES_PER_MARKER):
                    offset = haystack.find(needle, start)
                    if offset < 0:
                        break
                    self._find(surface, index, item_sha256, offset, len(needle), reason)
                    start = offset + 1
        self._surfaces[surface] = records

    def _find(self, surface, item, item_sha256, offset, length, reason) -> None:
        self._findings.append({
            "surface": surface, "item": item, "item_sha256": item_sha256,
            "offset": offset, "length": length, "ownership_reason": reason,
        })

    def _date(self, surface: str, item: int, value, *, date_only: bool = False) -> None:
        """Checks one agent-visible date carrier against the scenario reference time."""
        self._carriers += 1
        reference, observed = _instant(self.reference_time), _instant(value)
        agrees = observed is not None and reference is not None and (
            observed.date() == reference.date() if date_only else observed == reference
        )
        if not agrees:
            self._contradictions += 1
            self._find(surface, item, None, 0, 0, "date_context_contradicts_scenario_reference_time")

    def capture_instruction(self, instruction: str) -> None:
        content = instruction.encode()
        self.capture("resolved_instruction", [("instruction", content)])
        for match in DATE_PHRASE.finditer(content):
            self._date("resolved_instruction", 0, match.group(1).decode(), date_only=True)

    def capture_archive(self, surface: str, archive: bytes | None) -> None:
        """Scans the files of a container directory from its ``docker cp`` tar stream."""
        if archive is None:
            self.capture(surface, [])
            return
        items = []
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r|") as bundle:
            for member in bundle:
                if member.isfile():
                    items.append((member.name, bundle.extractfile(member).read()))
                elif member.issym() or member.islnk():
                    items.append((member.name, member.linkname.encode()))
                # Names are agent-visible too, including directories.
                items.append((member.name + "#name", member.name.encode()))
        self.capture(surface, items)
        for index, (name, content) in enumerate(items):
            if name.endswith(SESSION_SETTINGS):
                try:
                    self._date(surface, index, json.loads(content).get("data_as_of"))
                except (ValueError, AttributeError):
                    self._date(surface, index, None)
            elif not name.endswith("#name"):
                for match in DATE_PHRASE.finditer(content):
                    self._date(surface, index, match.group(1).decode(), date_only=True)

    def capture_container(self, main: dict, peers: dict[str, dict]) -> None:
        """Scans the agent container's own ``docker inspect`` and its network peers."""
        config = main.get("Config") or {}
        environment = [entry for entry in config.get("Env") or [] if isinstance(entry, str)]
        # Values are scanned but never hashed or stored: only the variable name is kept.
        self.capture(
            "agent_environment",
            [(entry.partition("=")[0], entry.encode()) for entry in environment],
            hash_items=False,
        )
        for index, entry in enumerate(environment):
            name, _, value = entry.partition("=")
            if name in DATE_ENVIRONMENT_NAMES:
                self._date("agent_environment", index, value, date_only=True)
        argv = [main.get("Path"), *(main.get("Args") or []), *(config.get("Entrypoint") or []),
                *(config.get("Cmd") or [])]
        self.capture("agent_argv", [("argv", str(value).encode()) for value in argv if value])
        mounts = [
            f"{mount.get('Type')}:{mount.get('Source')}:{mount.get('Destination')}"
            for mount in main.get("Mounts") or []
        ] + list((main.get("HostConfig") or {}).get("Binds") or [])
        self.capture("agent_mounts", [("mount", value.encode()) for value in mounts])
        networks = (main.get("NetworkSettings") or {}).get("Networks") or {}
        names = [("hostname", config.get("Hostname")), ("domainname", config.get("Domainname"))]
        for settings in networks.values():
            names += [("self", alias) for key in ("Aliases", "DNSNames") for alias in (settings or {}).get(key) or []]
        for role, peer_networks in sorted(peers.items()):
            for network, settings in sorted((peer_networks or {}).items()):
                if network in networks:
                    names += [(role, alias) for key in ("Aliases", "DNSNames") for alias in (settings or {}).get(key) or []]
        items = [(kind, str(value).encode()) for kind, value in names if value]
        self.capture("agent_hostnames", items)

    def capture_transcript(self, archive: bytes | None) -> None:
        """Scans what the tools container served, from its passive per-endpoint transcript."""
        if archive is None:
            self.uncaptured(MCP_SURFACES, "the tools container kept no served-response transcript")
            return
        files = {}
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r|") as bundle:
            for member in bundle:
                if member.isfile() and member.name.endswith(".jsonl"):
                    files[Path(member.name).stem] = bundle.extractfile(member).read()
        surfaces = {surface: [] for surface in MCP_SURFACES}
        initialized, incomplete = set(), []
        for endpoint, content in sorted(files.items()):
            try:
                records = [json.loads(line) for line in content.splitlines() if line.strip()]
            except ValueError:
                incomplete.append(endpoint)
                continue
            if not records or records[0].get("registered") is not True or any(
                record.get("overflow") for record in records
            ):
                incomplete.append(endpoint)
            for record in records:
                method, body = record.get("method"), record.get("body")
                if not isinstance(body, str):
                    continue
                try:
                    response = json.loads(body)
                except ValueError:
                    response = {}
                failed = "error" in response or (response.get("result") or {}).get("isError") is True
                if method == "initialize":
                    surface = "mcp_initialize"
                    if not failed:
                        initialized.add(endpoint)
                elif method == "tools/list":
                    surface = "mcp_tool_definitions"
                    if not failed and isinstance(record.get("tools"), list):
                        self._listed[endpoint] = record["tools"]
                elif method == "tools/call":
                    surface = "mcp_tool_errors" if failed else "mcp_tool_results"
                else:
                    continue
                surfaces[surface].append((endpoint, body.encode()))
        absent = [endpoint for endpoint in self.registered if endpoint not in files]
        if incomplete or absent:
            self.uncaptured(MCP_SURFACES, "a registered endpoint's transcript is missing or incomplete")
            return
        for surface, served in [("mcp_initialize", initialized), ("mcp_tool_definitions", self._listed)]:
            if any(endpoint not in served for endpoint in self.registered):
                self.uncaptured(surface, "a registered endpoint never served this response to the agent")
        for surface, items in surfaces.items():
            if surface not in self._missing:
                self.capture(surface, items)

    # ── sealing ────────────────────────────────────────────────────────────────

    def _catalog(self) -> dict:
        listed = [endpoint for endpoint in self.registered if endpoint in self._listed]
        complete = len(listed) == len(self.registered) and "mcp_tool_definitions" in self._surfaces
        catalog_sha256 = None
        if complete:
            hasher = hashlib.sha256()
            for endpoint in self.registered:
                for name, definition in self._listed[endpoint]:
                    for part in (endpoint, name, definition):
                        hasher.update(str(part).encode() + b"\0")
            catalog_sha256 = "sha256:" + hasher.hexdigest()
        return {
            "registered_endpoints": len(self.registered),
            "listed_endpoints": len(listed),
            "listed_tools": sum(len(self._listed[endpoint]) for endpoint in listed) if complete else None,
            "listed_catalog_sha256": catalog_sha256,
            "matches_exposure_manifest": (
                None if self.exposure_catalog_sha256 is None or not complete
                else catalog_sha256 == self.exposure_catalog_sha256
            ),
        }

    def seal(self, path: Path, *, session_id: str) -> dict:
        """Writes the private report once and returns the summary bound into the receipt."""
        if self._sealed:
            raise ValueError("surface audit was already sealed")
        self._sealed = True
        for surface in REQUIRED_SURFACES:
            if surface not in self._surfaces:
                self._missing.setdefault(surface, "the runner did not capture this surface")
        captured = [surface for surface in REQUIRED_SURFACES if surface in self._surfaces]
        missing = [surface for surface in REQUIRED_SURFACES if surface not in self._surfaces]
        catalog = self._catalog()
        date_coherence = {
            "status": "contradiction" if self._contradictions else "coherent" if self._carriers else "not_supplied",
            "reference_time": self.reference_time,
            "carriers_checked": self._carriers,
            "contradictions": self._contradictions,
        }
        if missing:
            outcome = "unqualified"
        elif self._findings or self._contradictions or catalog["matches_exposure_manifest"] is False:
            outcome = "findings_present"
        else:
            outcome = "clean"
        report = {
            "schema_version": 1,
            "scenario": self.scenario,
            "session_id": session_id,
            "outcome": outcome,
            "surfaces": {
                surface: {
                    "captured": surface in self._surfaces,
                    "reason": self._missing.get(surface),
                    "items": self._surfaces.get(surface, []),
                }
                for surface in REQUIRED_SURFACES
            },
            "findings": self._findings,
            "date_coherence": date_coherence,
            "catalog": catalog,
            "limits": LIMITS,
        }
        content = (json.dumps(report, indent=2) + "\n").encode()
        if path.exists() or path.is_symlink():
            raise ValueError("surface audit output already exists")
        with path.open("xb") as output:
            output.write(content)
        return {
            "schema_version": 1,
            "outcome": outcome,
            "report_sha256": digest(content),
            "required": REQUIRED_SURFACES,
            "captured": captured,
            "missing": missing,
            "finding_count": len(self._findings),
            "findings": self._findings[:MAX_SEALED_FINDINGS],
            "date_coherence": date_coherence,
            "catalog": catalog,
            "limits": LIMITS,
        }


class ProxyHarnessMessages:
    def __init__(self, audit: SurfaceAudit, capture):
        from deploy.runner.proxy_capture import ProviderProxyCapture

        if type(capture) is not ProviderProxyCapture or capture._directory is None:
            raise ValueError("harness input audit requires the running host provider proxy")
        self.audit = audit
        self.capture = capture
        self.path = capture._directory / "surface-audit-config.json"
        reference = _instant(audit.reference_time)
        if reference is None:
            raise ValueError("the harness audit requires a valid reference time")
        configuration = {
            "schema_version": 1,
            "reference_date": reference.date().isoformat(),
            "markers": [
                {"base64": base64.b64encode(marker).decode(), "ownership_reason": reason,
                 "case_insensitive": insensitive}
                for marker, reason, insensitive in audit.markers
            ],
        }
        content = (json.dumps(configuration, sort_keys=True) + "\n").encode()
        self.configuration_sha256 = digest(content)
        with self.path.open("xb") as output:
            output.write(content)
        self.path.chmod(0o600)

    @classmethod
    def create(cls, audit, capture):
        if audit is None:
            return None
        try:
            return cls(audit, capture)
        except Exception:
            audit.uncaptured("harness_messages", "the host proxy input observer could not be installed")
            return None

    def finish(self) -> None:
        try:
            self._finish()
        except Exception:
            self.audit.uncaptured("harness_messages", "the host proxy input capture is incomplete or unbound")

    def _finish(self) -> None:
        from deploy.runner.proxy_capture import read_events

        if self.path.is_symlink() or digest(self.path.read_bytes()) != self.configuration_sha256:
            raise ValueError("harness input audit configuration changed")
        events, faults = read_events(self.path.parent / "events.jsonl")
        if faults:
            raise ValueError("harness input capture event chain is incomplete")
        requests = [event for event in events if event.get("kind") == "request" and event.get("inference")]
        reports = [event for event in events if event.get("kind") == "surface_audit"]
        if not requests or len(requests) != len(reports):
            raise ValueError("not every inference request was scanned")
        records, findings = [], []
        carriers = contradictions = 0
        reasons = {reason for _, reason, _ in self.audit.markers}
        reasons.add("date_context_contradicts_scenario_reference_time")
        seen = set()
        for request, report in zip(requests, reports, strict=True):
            size = report.get("payload_bytes")
            content_hash = report.get("payload_sha256")
            if (
                request.get("wire") != "anthropic_messages"
                or request.get("id") in seen
                or report.get("id") != request.get("id")
                or report.get("request_sha256") != request.get("request_sha256")
                or report.get("config_sha256") != self.configuration_sha256
                or report.get("fault")
                or type(size) is not int or not 0 < size <= MAX_ARCHIVE_BYTES
                or not isinstance(content_hash, str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", content_hash)
                or type(report.get("date_carriers_checked")) is not int
                or type(report.get("date_contradictions")) is not int
                or not 0 <= report["date_contradictions"] <= report["date_carriers_checked"]
                or not isinstance(report.get("findings"), list)
            ):
                raise ValueError("harness input scan is not bound to the actual request")
            seen.add(request["id"])
            index = len(records)
            records.append({"index": index, "name": f"provider-request-{request['id']}",
                            "sha256": content_hash, "bytes": size})
            for finding in report["findings"]:
                if (
                    not isinstance(finding, dict)
                    or finding.get("ownership_reason") not in reasons
                    or type(finding.get("offset")) is not int
                    or type(finding.get("length")) is not int
                    or not 0 <= finding["offset"] < size
                    or not 0 < finding["length"] <= size - finding["offset"]
                ):
                    raise ValueError("invalid redacted harness finding")
                findings.append((index, content_hash, finding))
            carriers += report["date_carriers_checked"]
            contradictions += report["date_contradictions"]
        if self.audit._sealed or "harness_messages" in self.audit._surfaces:
            raise ValueError("harness messages were already captured or sealed")
        self.audit._surfaces["harness_messages"] = records
        self.audit._missing.pop("harness_messages", None)
        for index, content_hash, finding in findings:
            self.audit._find("harness_messages", index, content_hash, finding["offset"],
                             finding["length"], finding["ownership_reason"])
        self.audit._carriers += carriers
        self.audit._contradictions += contradictions


# ── strict-runner seam ─────────────────────────────────────────────────────────


async def _container(environment, role: str) -> str:
    result = await environment._run_docker_compose_command(
        ["ps", "--all", "--quiet", role], timeout_sec=20
    )
    containers = (result.stdout or "").split()
    if len(containers) != 1:
        raise RuntimeError(f"expected exactly one {role} container")
    return containers[0]


async def _archive(environment, container: str, path: str) -> bytes | None:
    """``docker cp`` of a container path as a bounded tar stream; ``None`` if absent."""
    process = await asyncio.create_subprocess_exec(
        *type(environment).runtime().engine_cmd("cp", f"{container}:{path}", "-"),
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    try:
        content = bytearray()

        async def receive() -> None:
            while chunk := await process.stdout.read(64 * 1024):
                content.extend(chunk)
                if len(content) > MAX_ARCHIVE_BYTES:
                    raise RuntimeError("captured surface exceeds the audit bound")

        async def diagnostics() -> bytes:
            return (await process.stderr.read(64 * 1024))

        async with asyncio.TaskGroup() as transfers:
            transfers.create_task(receive())
            error = transfers.create_task(diagnostics())
            await asyncio.wait_for(process.wait(), timeout=60)
        if process.returncode != 0:
            if b"Could not find the file" in error.result() and not content:
                return None
            raise RuntimeError("host engine failed to copy an audited surface")
        return bytes(content)
    finally:
        if process.returncode is None:
            process.kill()
        await process.wait()


async def capture_pre_agent(environment, trial) -> None:
    """Captures what the agent container holds before the agent starts.

    A capture failure never fails the trial; the surface stays uncaptured and the audit
    is reported as unqualified.
    """
    audit = environment._surface_audit
    try:
        audit.capture_instruction(trial.task.instruction)
    except Exception:
        audit.uncaptured("resolved_instruction", "the resolved instruction could not be read")
    main = None
    try:
        main = await _container(environment, "main")
        inspected = await environment._engine_json("inspect", "--format", "{{json .}}", main)
        peers = {
            role: await environment._engine_json(
                "inspect", "--format", "{{json .NetworkSettings.Networks}}",
                await _container(environment, role),
            )
            for role in ["world", "tools"]
        }
        audit.capture_container(inspected, peers)
    except Exception:
        audit.uncaptured(CONTAINER_SURFACES, "the agent container could not be inspected")
    for surface, path in [("workspace_files", "/app"), ("home_files", "/root")]:
        try:
            if main is None:
                raise RuntimeError("no agent container")
            audit.capture_archive(surface, await _archive(environment, main, path))
        except Exception:
            audit.uncaptured(surface, "the container files could not be captured within bounds")


async def seal_post_stop(environment) -> None:
    """Collects the served-response transcript after the agent stopped and seals the audit."""
    audit = getattr(environment, "_surface_audit", None)
    if audit is None or getattr(environment, "_surface_audit_sealed", None) is not None:
        return
    try:
        tools = await _container(environment, "tools")
        audit.capture_transcript(await _archive(environment, tools, TRANSCRIPT_DIR))
    except Exception:
        audit.uncaptured(MCP_SURFACES, "the served-response transcript could not be collected")
    try:
        environment._surface_audit_sealed = audit.seal(
            environment.trial_paths.trial_dir / "surface-audit.json",
            session_id=environment.session_id,
        )
    except Exception:
        # Without a sealed record the verifier reports the audit as unqualified.
        environment._surface_audit_sealed = None
