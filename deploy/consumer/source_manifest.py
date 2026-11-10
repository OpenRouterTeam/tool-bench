"""
Source manifest plumbing: reads materialized task directories and validates
that all resolved surfaces, templates, and evaluator markers are accounted for.

This is a static qualification step—it validates source bytes, not runtime
capture. Runtime surfaces are captured by the trusted host (another delegate).
"""

from __future__ import annotations

import hashlib
import json
import tomllib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from deploy.consumer.audit import (
    AuditOutcome,
    AuditReport,
    Surface,
    audit_task_directory,
)


@dataclass
class TaskSource:
    task_name: str
    scenario_id: str
    workspace_profile: str
    framing: str
    instruction_sha256: str
    task_toml_sha256: str
    has_profile_files: bool
    audit: AuditReport


@dataclass
class SourceManifest:
    tasks: list[TaskSource]
    qualified: int
    unqualified: int
    findings_present: int
    clean: int

    def to_dict(self) -> dict:
        return {
            "qualified": self.qualified,
            "unqualified": self.unqualified,
            "findings_present": self.findings_present,
            "clean": self.clean,
            "tasks": [
                {
                    "task_name": t.task_name,
                    "scenario_id": t.scenario_id,
                    "workspace_profile": t.workspace_profile,
                    "framing": t.framing,
                    "instruction_sha256": t.instruction_sha256,
                    "task_toml_sha256": t.task_toml_sha256,
                    "has_profile_files": t.has_profile_files,
                    "audit_outcome": t.audit.outcome.value,
                }
                for t in self.tasks
            ],
        }


def _sha256(content: bytes) -> str:
    return "sha256:" + hashlib.sha256(content).hexdigest()


def read_task_source(task_dir: Path) -> TaskSource:
    """Read and audit a single materialized task directory."""
    task_toml_path = task_dir / "task.toml"
    if not task_toml_path.is_file():
        raise FileNotFoundError(f"missing task.toml in {task_dir}")

    task_toml_bytes = task_toml_path.read_bytes()
    task = tomllib.loads(task_toml_bytes.decode())

    metadata = task.get("metadata", {})
    scenario_id = metadata.get("scenario", "")
    workspace_profile = metadata.get("workspace_profile", "minimal")
    framing = metadata.get("framing", "neutral")

    instruction_path = task_dir / "instruction.md"
    instruction_sha256 = ""
    if instruction_path.is_file():
        instruction_sha256 = _sha256(instruction_path.read_bytes())

    profile_dir = task_dir / "environment" / "profile"
    has_profile = profile_dir.is_dir() and any(profile_dir.iterdir())

    audit = audit_task_directory(task_dir, workspace_profile)

    return TaskSource(
        task_name=task_dir.name,
        scenario_id=scenario_id,
        workspace_profile=workspace_profile,
        framing=framing,
        instruction_sha256=instruction_sha256,
        task_toml_sha256=_sha256(task_toml_bytes),
        has_profile_files=has_profile,
        audit=audit,
    )


def build_source_manifest(tasks_root: Path, task_names: Optional[list[str]] = None) -> SourceManifest:
    """
    Build source manifest for all (or selected) tasks under a root directory.
    """
    tasks = []
    if task_names:
        dirs = [tasks_root / name for name in task_names]
    else:
        dirs = sorted(
            d for d in tasks_root.iterdir()
            if d.is_dir() and (d / "task.toml").is_file()
        )

    for task_dir in dirs:
        tasks.append(read_task_source(task_dir))

    clean = sum(1 for t in tasks if t.audit.outcome == AuditOutcome.CLEAN)
    findings = sum(1 for t in tasks if t.audit.outcome == AuditOutcome.FINDINGS_PRESENT)
    unqualified = sum(1 for t in tasks if t.audit.outcome == AuditOutcome.UNQUALIFIED)

    return SourceManifest(
        tasks=tasks,
        qualified=clean + findings,
        unqualified=unqualified,
        findings_present=findings,
        clean=clean,
    )
