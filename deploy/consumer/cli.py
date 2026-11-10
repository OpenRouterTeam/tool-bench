"""
CLI entry point for consumer audit/exposure/awareness operations.

Usage:
    python -m deploy.consumer.cli audit-task <task-dir> [--profile minimal|support|engineering]
    python -m deploy.consumer.cli source-manifest <tasks-root> [--task <name>...]
    python -m deploy.consumer.cli exposure <variant> --catalog <path> --snapshots <dir> [--seed N] [--budget N]
    python -m deploy.consumer.cli awareness-plan
    python -m deploy.consumer.cli validate-condition <condition-label> --framing <path> --profile <path>
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def cmd_audit_task(args: argparse.Namespace) -> int:
    from deploy.consumer.audit import audit_task_directory
    report = audit_task_directory(Path(args.task_dir), args.profile)
    json.dump(report.to_dict(), sys.stdout, indent=2)
    print()
    return 0 if report.outcome.value != "findings_present" else 1


def cmd_source_manifest(args: argparse.Namespace) -> int:
    from deploy.consumer.source_manifest import build_source_manifest
    manifest = build_source_manifest(Path(args.tasks_root), args.task or None)
    json.dump(manifest.to_dict(), sys.stdout, indent=2)
    print()
    return 0


def cmd_exposure(args: argparse.Namespace) -> int:
    from deploy.consumer.exposure import ExposureVariant, build_exposure
    from deploy.consumer.bridge_ordering import build_tool_inventory

    all_tools = build_tool_inventory(Path(args.catalog), Path(args.snapshots))
    manifest = build_exposure(
        variant=ExposureVariant(args.variant),
        seed=args.seed,
        all_tools=all_tools,
        relevant_tools=[],
        active_services=[],
        token_budget=args.budget,
    )
    json.dump(manifest.to_dict(), sys.stdout, indent=2)
    print()
    return 0


def cmd_awareness_plan(args: argparse.Namespace) -> int:
    from deploy.consumer.awareness import build_plan
    plan = build_plan()
    output = {
        "conditions": [
            {"label": c.label, "framing": c.framing.value, "workspace": c.workspace.value}
            for c in plan.conditions
        ],
        "information_differences": [
            {"axis": d.axis, "description": d.description}
            for d in plan.information_differences
        ],
        "post_run_questions": [
            {"id": q.id, "question": q.question, "response_type": q.response_type.value}
            for q in plan.post_run_questions
        ],
        "forbidden_markers": plan.scored_turn_forbidden_markers,
        "limits": plan.limits,
    }
    json.dump(output, sys.stdout, indent=2)
    print()
    return 0


def cmd_validate_condition(args: argparse.Namespace) -> int:
    from deploy.consumer.awareness import (
        AwarenessCondition,
        FramingCondition,
        WorkspaceCondition,
        validate_condition_admission,
    )

    framing_manifest = json.loads(Path(args.framing).read_text())
    profile_manifest = json.loads(Path(args.profile).read_text())

    # Parse condition label
    parts = args.label.split("_", 1)
    if len(parts) != 2:
        print(f"Invalid condition label: {args.label}", file=sys.stderr)
        return 1

    framing_str, workspace_str = parts
    framing = (
        FramingCondition.NEUTRAL
        if framing_str == "neutral"
        else FramingCondition.EXPLICIT_EVALUATION
    )
    workspace = (
        WorkspaceCondition.MINIMAL
        if workspace_str == "minimal"
        else WorkspaceCondition.ORDINARY
    )
    condition = AwarenessCondition.new(framing, workspace)
    errors = validate_condition_admission(condition, framing_manifest, profile_manifest)

    if errors:
        json.dump({"valid": False, "errors": errors}, sys.stdout, indent=2)
        print()
        return 1
    else:
        json.dump({"valid": True, "errors": []}, sys.stdout, indent=2)
        print()
        return 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="consumer", description="Task consumer operations")
    sub = parser.add_subparsers(dest="command")

    audit = sub.add_parser("audit-task")
    audit.add_argument("task_dir")
    audit.add_argument("--profile", default="minimal")

    source = sub.add_parser("source-manifest")
    source.add_argument("tasks_root")
    source.add_argument("--task", action="append", default=[])

    exp = sub.add_parser("exposure")
    exp.add_argument("variant")
    exp.add_argument("--catalog", required=True)
    exp.add_argument("--snapshots", required=True)
    exp.add_argument("--seed", type=int, default=42)
    exp.add_argument("--budget", type=int, default=None)

    sub.add_parser("awareness-plan")

    validate = sub.add_parser("validate-condition")
    validate.add_argument("label")
    validate.add_argument("--framing", required=True)
    validate.add_argument("--profile", required=True)

    args = parser.parse_args()
    if not args.command:
        parser.print_help()
        return 1

    dispatch = {
        "audit-task": cmd_audit_task,
        "source-manifest": cmd_source_manifest,
        "exposure": cmd_exposure,
        "awareness-plan": cmd_awareness_plan,
        "validate-condition": cmd_validate_condition,
    }
    return dispatch[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
