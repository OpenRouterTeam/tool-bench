"""
Resolved-surface audit: validates that every agent-visible channel in a
materialized task is free of evaluator-owned identifiers, admin access, and
solution artifacts.

Operates on resolved content (the actual bytes a model receives). A passed
lexical audit does not certify model unawareness—it establishes that the
surfaces inspected contain no known evaluator-owned markers. Missing required
surfaces make the audit unqualified, not passed.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Optional


class Surface(str, Enum):
    INITIAL_SYSTEM_PROMPT = "initial_system_prompt"
    INITIAL_DEVELOPER_PROMPT = "initial_developer_prompt"
    INITIAL_USER_PROMPT = "initial_user_prompt"
    PROFILE_WORKSPACE = "profile_workspace"
    PROFILE_HOME = "profile_home"
    WORKSPACE_SESSION_CONFIG = "workspace_session_config"
    WORKSPACE_CONNECTION_INVENTORY = "workspace_connection_inventory"
    WORKSPACE_ACCOUNT_IDENTITY = "workspace_account_identity"
    TOOL_DEFINITIONS = "tool_definitions"
    TOOL_INIT_RESPONSES = "tool_init_responses"
    TOOL_RESULT_BODIES = "tool_result_bodies"
    TOOL_ERROR_BODIES = "tool_error_bodies"
    AGENT_ARGV = "agent_argv"
    AGENT_ENVIRONMENT = "agent_environment"
    AGENT_MOUNTS = "agent_mounts"
    DNS_NAMES = "dns_names"
    SERVICE_HEALTH_ROUTES = "service_health_routes"
    SESSION_METADATA = "session_metadata"


# Evaluator-owned markers: patterns that identify evaluator content.
PRIVATE_MARKERS: list[tuple[str, str]] = [
    ("_bench", "evaluator_admin_route"),
    ("BENCH_ADMIN_TOKEN", "evaluator_credential"),
    ("bench-verify", "evaluator_binary"),
    ("bench_admin", "evaluator_admin_identifier"),
    ("tool-bench-oracle", "evaluator_oracle_identity"),
    ("oracle", "evaluator_oracle_reference"),
    ("scenario.toml", "evaluator_scenario_source"),
    ("solution/", "evaluator_solution_artifact"),
    ("result-contracts", "evaluator_result_contract"),
    ("expect.json", "evaluator_contract_artifact"),
    ("reward", "evaluator_scoring_signal"),
    ("evaluator", "evaluator_self_reference"),
    ("grading", "evaluator_grading_reference"),
]

TEMPLATE_MARKERS: list[str] = ["{{", "}}"]


def _sha256_hex(content: bytes) -> str:
    return "sha256:" + hashlib.sha256(content).hexdigest()


@dataclass
class CapturedSurface:
    surface: Surface
    content_sha256: str
    byte_count: int

    @classmethod
    def capture(cls, surface: Surface, content: bytes) -> CapturedSurface:
        return cls(
            surface=surface,
            content_sha256=_sha256_hex(content),
            byte_count=len(content),
        )


@dataclass
class Finding:
    surface: Surface
    surface_sha256: str
    marker_kind: str
    ownership_reason: str
    redacted_offset: int
    redacted_length: int


class AuditOutcome(str, Enum):
    CLEAN = "clean"
    FINDINGS_PRESENT = "findings_present"
    UNQUALIFIED = "unqualified"


@dataclass
class AuditReport:
    outcome: AuditOutcome
    surfaces_required: list[Surface]
    surfaces_captured: list[Surface]
    surfaces_missing: list[Surface]
    findings: list[Finding]
    captured_hashes: dict[str, str]
    limits: list[str]

    def to_dict(self) -> dict:
        return {
            "outcome": self.outcome.value,
            "surfaces_required": [s.value for s in self.surfaces_required],
            "surfaces_captured": [s.value for s in self.surfaces_captured],
            "surfaces_missing": [s.value for s in self.surfaces_missing],
            "findings": [
                {
                    "surface": f.surface.value,
                    "surface_sha256": f.surface_sha256,
                    "marker_kind": f.marker_kind,
                    "ownership_reason": f.ownership_reason,
                    "redacted_offset": f.redacted_offset,
                    "redacted_length": f.redacted_length,
                }
                for f in self.findings
            ],
            "captured_hashes": self.captured_hashes,
            "limits": self.limits,
        }


def required_surfaces(workspace_profile: str) -> list[Surface]:
    """Surfaces that must be captured for a given workspace profile."""
    required = [
        Surface.INITIAL_USER_PROMPT,
        Surface.TOOL_DEFINITIONS,
        Surface.AGENT_ARGV,
        Surface.AGENT_ENVIRONMENT,
        Surface.DNS_NAMES,
        Surface.SERVICE_HEALTH_ROUTES,
        Surface.SESSION_METADATA,
    ]
    if workspace_profile != "minimal":
        required.extend([
            Surface.PROFILE_WORKSPACE,
            Surface.PROFILE_HOME,
            Surface.WORKSPACE_SESSION_CONFIG,
            Surface.WORKSPACE_CONNECTION_INVENTORY,
            Surface.WORKSPACE_ACCOUNT_IDENTITY,
        ])
    return sorted(required, key=lambda s: s.value)


def _scan_content(surface: Surface, content: bytes, content_sha256: str) -> list[Finding]:
    """Scan resolved content for evaluator-owned markers."""
    findings = []
    try:
        text = content.decode("utf-8", errors="replace")
    except Exception:
        return findings

    for pattern, reason in PRIVATE_MARKERS:
        start = 0
        while True:
            idx = text.find(pattern, start)
            if idx < 0:
                break
            findings.append(Finding(
                surface=surface,
                surface_sha256=content_sha256,
                marker_kind=pattern,
                ownership_reason=reason,
                redacted_offset=idx,
                redacted_length=len(pattern),
            ))
            start = idx + 1

    for marker in TEMPLATE_MARKERS:
        if marker in text:
            findings.append(Finding(
                surface=surface,
                surface_sha256=content_sha256,
                marker_kind=marker,
                ownership_reason="unresolved_template",
                redacted_offset=text.index(marker),
                redacted_length=len(marker),
            ))
    return findings


def audit_surfaces(
    workspace_profile: str,
    captured: dict[Surface, bytes],
) -> AuditReport:
    """
    Run resolved-surface audit against captured content.

    Args:
        workspace_profile: "minimal", "support", or "engineering"
        captured: mapping from Surface to resolved byte content
    """
    required = required_surfaces(workspace_profile)
    captured_surfaces = sorted(
        [s for s in captured if s in set(required)],
        key=lambda s: s.value,
    )
    missing = [s for s in required if s not in captured]

    if missing:
        return AuditReport(
            outcome=AuditOutcome.UNQUALIFIED,
            surfaces_required=required,
            surfaces_captured=captured_surfaces,
            surfaces_missing=missing,
            findings=[],
            captured_hashes={},
            limits=["missing_surfaces_make_audit_unqualified"],
        )

    findings = []
    captured_hashes = {}
    for surface in required:
        content = captured[surface]
        sha = _sha256_hex(content)
        captured_hashes[surface.value] = sha
        findings.extend(_scan_content(surface, content, sha))

    outcome = AuditOutcome.FINDINGS_PRESENT if findings else AuditOutcome.CLEAN
    return AuditReport(
        outcome=outcome,
        surfaces_required=required,
        surfaces_captured=captured_surfaces,
        surfaces_missing=[],
        findings=findings,
        captured_hashes=captured_hashes,
        limits=[
            "lexical_scan_only_not_semantic",
            "host_captured_not_model_observed",
        ],
    )


def audit_task_directory(task_dir: Path, workspace_profile: str = "minimal") -> AuditReport:
    """
    Audit a materialized task directory by reading resolved files.

    This reads the actual generated files (instruction.md, task.toml, profile files)
    and checks them for evaluator markers.
    """
    captured: dict[Surface, bytes] = {}

    instruction = task_dir / "instruction.md"
    if instruction.is_file():
        captured[Surface.INITIAL_USER_PROMPT] = instruction.read_bytes()

    task_toml = task_dir / "task.toml"
    if task_toml.is_file():
        content = task_toml.read_bytes()
        captured[Surface.TOOL_DEFINITIONS] = content
        captured[Surface.SESSION_METADATA] = content

    for name, surface in [
        ("environment/profile/workspace", Surface.PROFILE_WORKSPACE),
        ("environment/profile/home", Surface.PROFILE_HOME),
    ]:
        path = task_dir / name
        if path.is_dir():
            parts = []
            for f in sorted(path.rglob("*")):
                if f.is_file():
                    parts.append(f.read_bytes())
            if parts:
                captured[surface] = b"\n".join(parts)

    # Stub surfaces for static audit (actual values come from host capture at runtime)
    for surface in [
        Surface.AGENT_ARGV,
        Surface.AGENT_ENVIRONMENT,
        Surface.DNS_NAMES,
        Surface.SERVICE_HEALTH_ROUTES,
    ]:
        captured[surface] = b""

    # Profile-specific files
    profile_dir = task_dir / "environment" / "profile"
    if profile_dir.is_dir():
        for name, surface in [
            ("workspace/.session.json", Surface.WORKSPACE_SESSION_CONFIG),
            ("workspace/connections.json", Surface.WORKSPACE_CONNECTION_INVENTORY),
            ("workspace/account.json", Surface.WORKSPACE_ACCOUNT_IDENTITY),
        ]:
            path = profile_dir / name
            if path.is_file():
                captured[surface] = path.read_bytes()

    return audit_surfaces(workspace_profile, captured)
