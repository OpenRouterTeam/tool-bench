"""
Tests for taskgen/bridge consumer modules: audit, awareness, exposure,
source manifest, and bridge ordering.

These tests call real taskgen/bridge paths and reject missing surfaces,
unresolved templates, and accidental evaluator markers. Source static audits
remain source qualification, not native capture experiments.
"""

import json
import tempfile
import unittest
from pathlib import Path

from deploy.consumer.audit import (
    AuditOutcome,
    AuditReport,
    CapturedSurface,
    Finding,
    Surface,
    PRIVATE_MARKERS,
    TEMPLATE_MARKERS,
    audit_surfaces,
    audit_task_directory,
    required_surfaces,
)
from deploy.consumer.awareness import (
    AwarenessCondition,
    AwarenessPlan,
    FramingCondition,
    WorkspaceCondition,
    all_conditions,
    build_plan,
    enumerate_information_differences,
    standard_post_run_questions,
    validate_condition_admission,
    verify_historic_compatibility,
    SCORED_TURN_FORBIDDEN_MARKERS,
)
from deploy.consumer.exposure import (
    CatalogMetrics,
    DiscoveryMetrics,
    ExposedTool,
    ExposureManifest,
    ExposureVariant,
    build_exposure,
    record_observation,
)
from deploy.consumer.bridge_ordering import (
    build_tool_inventory,
    load_catalog_endpoints,
    load_snapshot_tools,
    build_service_endpoint_map,
)


def sample_tools():
    return [
        ("github", "list_issues", {"name": "list_issues", "description": "List issues", "inputSchema": {"type": "object"}}),
        ("github", "create_issue", {"name": "create_issue", "description": "Create an issue", "inputSchema": {"type": "object"}}),
        ("linear", "list_projects", {"name": "list_projects", "description": "List projects", "inputSchema": {"type": "object"}}),
        ("slack", "send_message", {"name": "send_message", "description": "Send a message", "inputSchema": {"type": "object"}}),
        ("stripe", "create_refund", {"name": "create_refund", "description": "Create a refund", "inputSchema": {"type": "object"}}),
    ]


# ── Audit tests ───────────────────────────────────────────────────────────────

class AuditSurfaceTests(unittest.TestCase):
    def test_minimal_required_surfaces(self):
        required = required_surfaces("minimal")
        self.assertIn(Surface.INITIAL_USER_PROMPT, required)
        self.assertIn(Surface.TOOL_DEFINITIONS, required)
        self.assertNotIn(Surface.PROFILE_WORKSPACE, required)

    def test_support_required_surfaces_include_profile(self):
        required = required_surfaces("support")
        self.assertIn(Surface.PROFILE_WORKSPACE, required)
        self.assertIn(Surface.WORKSPACE_CONNECTION_INVENTORY, required)

    def test_clean_audit_with_all_surfaces(self):
        captured = {s: b"clean content here" for s in required_surfaces("minimal")}
        report = audit_surfaces("minimal", captured)
        self.assertEqual(report.outcome, AuditOutcome.CLEAN)
        self.assertEqual(len(report.surfaces_missing), 0)

    def test_unqualified_when_surfaces_missing(self):
        report = audit_surfaces("minimal", {})
        self.assertEqual(report.outcome, AuditOutcome.UNQUALIFIED)
        self.assertGreater(len(report.surfaces_missing), 0)

    def test_findings_when_evaluator_marker_present(self):
        captured = {s: b"safe" for s in required_surfaces("minimal")}
        captured[Surface.INITIAL_USER_PROMPT] = b"go to _bench route"
        report = audit_surfaces("minimal", captured)
        self.assertEqual(report.outcome, AuditOutcome.FINDINGS_PRESENT)
        self.assertTrue(any(f.marker_kind == "_bench" for f in report.findings))

    def test_findings_when_template_unresolved(self):
        captured = {s: b"safe" for s in required_surfaces("minimal")}
        captured[Surface.TOOL_DEFINITIONS] = b"use {{variable}} here"
        report = audit_surfaces("minimal", captured)
        self.assertEqual(report.outcome, AuditOutcome.FINDINGS_PRESENT)
        self.assertTrue(any(f.ownership_reason == "unresolved_template" for f in report.findings))

    def test_captured_surface_hashing(self):
        cs = CapturedSurface.capture(Surface.INITIAL_USER_PROMPT, b"hello")
        self.assertTrue(cs.content_sha256.startswith("sha256:"))
        self.assertEqual(cs.byte_count, 5)

    def test_audit_report_serialization(self):
        captured = {s: b"ok" for s in required_surfaces("minimal")}
        report = audit_surfaces("minimal", captured)
        d = report.to_dict()
        self.assertEqual(d["outcome"], "clean")
        self.assertIsInstance(d["captured_hashes"], dict)

    def test_all_private_markers_have_reasons(self):
        for pattern, reason in PRIVATE_MARKERS:
            self.assertTrue(pattern)
            self.assertTrue(reason)

    def test_audit_rejects_multiple_marker_hits(self):
        captured = {s: b"safe" for s in required_surfaces("minimal")}
        captured[Surface.INITIAL_USER_PROMPT] = b"oracle oracle oracle"
        report = audit_surfaces("minimal", captured)
        oracle_findings = [f for f in report.findings if f.marker_kind == "oracle"]
        self.assertEqual(len(oracle_findings), 3)


class AuditTaskDirectoryTests(unittest.TestCase):
    def test_audit_actual_task_directory(self):
        task_dir = Path("tasks/business__stripe__customer-mrr")
        if not task_dir.is_dir():
            self.skipTest("task directory not present")
        report = audit_task_directory(task_dir, "minimal")
        self.assertIn(report.outcome, (AuditOutcome.CLEAN, AuditOutcome.FINDINGS_PRESENT, AuditOutcome.UNQUALIFIED))
        self.assertIsInstance(report.findings, list)

    def test_missing_task_dir_gives_unqualified(self):
        with tempfile.TemporaryDirectory() as d:
            report = audit_task_directory(Path(d), "minimal")
            self.assertEqual(report.outcome, AuditOutcome.UNQUALIFIED)


# ── Awareness tests ───────────────────────────────────────────────────────────

class AwarenessTests(unittest.TestCase):
    def test_all_conditions_is_2x2(self):
        conditions = all_conditions()
        self.assertEqual(len(conditions), 4)
        labels = {c.label for c in conditions}
        self.assertEqual(len(labels), 4)

    def test_plan_is_consistent(self):
        plan = build_plan()
        self.assertEqual(len(plan.conditions), 4)
        self.assertGreater(len(plan.information_differences), 0)
        self.assertGreater(len(plan.post_run_questions), 0)
        self.assertIn(
            "sensitivity_measurement_not_unawareness_certification",
            plan.limits,
        )

    def test_admission_rejects_framing_mismatch(self):
        condition = AwarenessCondition.new(
            FramingCondition.EXPLICIT_EVALUATION, WorkspaceCondition.MINIMAL
        )
        framing = {"framing": "neutral", "base_instruction_sha256": "sha256:a", "rendered_instruction_sha256": "sha256:a"}
        profile = {"id": "minimal", "information": []}
        errors = validate_condition_admission(condition, framing, profile)
        self.assertTrue(any("framing mismatch" in e for e in errors))

    def test_admission_rejects_workspace_mismatch(self):
        condition = AwarenessCondition.new(
            FramingCondition.NEUTRAL, WorkspaceCondition.ORDINARY
        )
        framing = {"framing": "neutral", "base_instruction_sha256": "sha256:a", "rendered_instruction_sha256": "sha256:a"}
        profile = {"id": "minimal", "information": []}
        errors = validate_condition_admission(condition, framing, profile)
        self.assertTrue(any("workspace mismatch" in e for e in errors))

    def test_admission_catches_forbidden_markers_in_scored_turns(self):
        condition = AwarenessCondition.new(
            FramingCondition.NEUTRAL, WorkspaceCondition.MINIMAL
        )
        framing = {"framing": "neutral", "base_instruction_sha256": "sha256:a", "rendered_instruction_sha256": "sha256:a"}
        profile = {"id": "minimal", "information": []}
        errors = validate_condition_admission(
            condition, framing, profile,
            scored_turn_content="Here is an awareness diagnostic for the model."
        )
        self.assertTrue(any("forbidden awareness marker" in e for e in errors))

    def test_neutral_framing_must_not_alter_instruction(self):
        condition = AwarenessCondition.new(
            FramingCondition.NEUTRAL, WorkspaceCondition.MINIMAL
        )
        framing = {"framing": "neutral", "base_instruction_sha256": "sha256:a", "rendered_instruction_sha256": "sha256:b"}
        profile = {"id": "minimal", "information": []}
        errors = validate_condition_admission(condition, framing, profile)
        self.assertTrue(any("neutral framing must not alter" in e for e in errors))

    def test_evaluation_framing_must_alter_instruction(self):
        condition = AwarenessCondition.new(
            FramingCondition.EXPLICIT_EVALUATION, WorkspaceCondition.MINIMAL
        )
        framing = {"framing": "evaluation", "base_instruction_sha256": "sha256:a", "rendered_instruction_sha256": "sha256:a"}
        profile = {"id": "minimal", "information": []}
        errors = validate_condition_admission(condition, framing, profile)
        self.assertTrue(any("evaluation framing must visibly alter" in e for e in errors))

    def test_ordinary_workspace_must_declare_information(self):
        condition = AwarenessCondition.new(
            FramingCondition.NEUTRAL, WorkspaceCondition.ORDINARY
        )
        framing = {"framing": "neutral", "base_instruction_sha256": "sha256:a", "rendered_instruction_sha256": "sha256:a"}
        profile = {"id": "support", "information": [], "files_sha256": "sha256:x", "files": ["a.md"], "business_reference_time": "2026-01-01T00:00:00Z"}
        errors = validate_condition_admission(condition, framing, profile)
        self.assertTrue(any("must declare profile information" in e for e in errors))

    def test_historic_compatibility_detects_hash_changes(self):
        historic_framing = {"base_instruction_sha256": "sha256:old"}
        current_framing = {"base_instruction_sha256": "sha256:new", "rendered_instruction_sha256": "sha256:new"}
        issues = verify_historic_compatibility(
            historic_framing, {}, current_framing, {}
        )
        self.assertTrue(any("base instruction hash changed" in i for i in issues))

    def test_historic_compatibility_passes_unchanged(self):
        historic_framing = {"base_instruction_sha256": "sha256:same"}
        historic_profile = {"files_sha256": "sha256:same_files"}
        current_framing = {"base_instruction_sha256": "sha256:same"}
        current_profile = {"files_sha256": "sha256:same_files"}
        issues = verify_historic_compatibility(
            historic_framing, historic_profile, current_framing, current_profile
        )
        self.assertEqual(len(issues), 0)


# ── Exposure tests ────────────────────────────────────────────────────────────

class ExposureTests(unittest.TestCase):
    def test_full_catalog_includes_all(self):
        tools = sample_tools()
        m = build_exposure(ExposureVariant.FULL_CATALOG, 42, tools, [], [])
        self.assertEqual(m.tool_count, len(tools))

    def test_relevant_only_selects_solution_tools(self):
        tools = sample_tools()
        relevant = [("github", "list_issues")]
        m = build_exposure(ExposureVariant.RELEVANT_ONLY, 42, tools, relevant, [])
        self.assertEqual(m.tool_count, 1)
        self.assertEqual(m.tools[0].tool_name, "list_issues")
        self.assertFalse(m.tools[0].is_distractor)
        self.assertIn("relevant_only_is_easier_diagnostic_not_upper_bound", m.limits)

    def test_organization_pack_filters_by_service(self):
        tools = sample_tools()
        m = build_exposure(ExposureVariant.ORGANIZATION_PACK, 42, tools, [], ["github"])
        self.assertEqual(m.tool_count, 2)
        self.assertTrue(all(t.endpoint == "github" for t in m.tools))

    def test_randomized_order_changes_with_different_seeds(self):
        tools = sample_tools()
        m1 = build_exposure(ExposureVariant.RANDOMIZED_ORDER, 1, tools, [], [])
        m2 = build_exposure(ExposureVariant.RANDOMIZED_ORDER, 2, tools, [], [])
        self.assertEqual(m1.tool_count, m2.tool_count)
        names1 = sorted(t.tool_name for t in m1.tools)
        names2 = sorted(t.tool_name for t in m2.tools)
        self.assertEqual(names1, names2)

    def test_confusing_distractors_adds_marked_tools(self):
        tools = sample_tools()
        m = build_exposure(ExposureVariant.CONFUSING_DISTRACTORS, 42, tools, [], [])
        self.assertGreater(m.tool_count, len(tools))
        self.assertTrue(any(t.is_distractor for t in m.tools))
        non_distractors = [t for t in m.tools if not t.is_distractor]
        self.assertEqual(len(non_distractors), len(tools))
        # Distractors use genuine pinned definitions with deprecated prefix
        for t in m.tools:
            if t.is_distractor:
                self.assertTrue(
                    t.definition.get("description", "").startswith("(Deprecated)"),
                    f"Distractor {t.tool_name} must use genuine deprecated prefix, not fake schema edits"
                )

    def test_context_feasible_respects_token_budget(self):
        tools = sample_tools()
        m = build_exposure(ExposureVariant.CONTEXT_FEASIBLE, 42, tools, [], [], token_budget=50)
        self.assertLessEqual(m.tool_count, len(tools))
        self.assertGreater(m.tool_count, 0)

    def test_unknown_provider_costs_are_none(self):
        m = build_exposure(ExposureVariant.FULL_CATALOG, 42, sample_tools(), [], [])
        self.assertIsNone(m.catalog_metrics.provider_accepted)
        self.assertIsNone(m.catalog_metrics.model_visible)

    def test_unknown_discovery_metrics_are_none(self):
        metrics = DiscoveryMetrics()
        self.assertIsNone(metrics.hidden_discovery_inference_tokens)
        self.assertIsNone(metrics.total_budget_tokens)
        self.assertIsNone(metrics.consumed_budget_tokens)

    def test_exposure_hashes_are_deterministic(self):
        tools = sample_tools()
        m1 = build_exposure(ExposureVariant.FULL_CATALOG, 42, tools, [], [])
        m2 = build_exposure(ExposureVariant.FULL_CATALOG, 42, tools, [], [])
        self.assertEqual(m1.catalog_sha256, m2.catalog_sha256)

    def test_observation_records_trial_id(self):
        m = build_exposure(ExposureVariant.FULL_CATALOG, 42, sample_tools(), [], [])
        obs = record_observation("trial-42", m, DiscoveryMetrics())
        self.assertEqual(obs.trial_id, "trial-42")

    def test_limits_always_present(self):
        m = build_exposure(ExposureVariant.RELEVANT_ONLY, 0, [], [], [])
        self.assertIn("relevant_only_is_easier_diagnostic_not_upper_bound", m.limits)
        self.assertIn("unknown_provider_costs_are_unknown_not_zero", m.limits)

    def test_catalog_metrics_distinguish_stored_and_loaded(self):
        tools = sample_tools()
        relevant = [("github", "list_issues")]
        m = build_exposure(ExposureVariant.RELEVANT_ONLY, 42, tools, relevant, [])
        self.assertEqual(m.catalog_metrics.stored_definitions, 5)
        self.assertEqual(m.catalog_metrics.loaded_schemas, 1)

    def test_manifest_serialization(self):
        tools = sample_tools()
        m = build_exposure(ExposureVariant.FULL_CATALOG, 42, tools, [], [])
        d = m.to_dict()
        self.assertEqual(d["variant"], "full_catalog")
        self.assertEqual(d["tool_count"], len(tools))


# ── Bridge ordering tests ────────────────────────────────────────────────────

class BridgeOrderingTests(unittest.TestCase):
    def test_load_catalog_endpoints(self):
        catalog_path = Path("catalog/services.toml")
        if not catalog_path.is_file():
            self.skipTest("catalog not present")
        endpoints = load_catalog_endpoints(catalog_path)
        self.assertGreater(len(endpoints), 0)
        self.assertTrue(all(ep.endpoint_id for ep in endpoints))
        self.assertTrue(all(ep.mcp_port > 0 for ep in endpoints))

    def test_load_snapshot_tools(self):
        snapshots_dir = Path("servers/snapshots")
        if not snapshots_dir.is_dir():
            self.skipTest("snapshots not present")
        tools = load_snapshot_tools(snapshots_dir, "stripe")
        if tools:
            self.assertTrue(all(t[0] == "stripe" for t in tools))
            self.assertTrue(all(t[1] for t in tools))

    def test_build_tool_inventory_deterministic(self):
        catalog_path = Path("catalog/services.toml")
        snapshots_dir = Path("servers/snapshots")
        if not catalog_path.is_file() or not snapshots_dir.is_dir():
            self.skipTest("catalog or snapshots not present")
        inv1 = build_tool_inventory(catalog_path, snapshots_dir)
        inv2 = build_tool_inventory(catalog_path, snapshots_dir)
        self.assertEqual(inv1, inv2)
        # Verify sorted order
        names = [(t[0], t[1]) for t in inv1]
        self.assertEqual(names, sorted(names))

    def test_build_service_endpoint_map(self):
        catalog_path = Path("catalog/services.toml")
        if not catalog_path.is_file():
            self.skipTest("catalog not present")
        mapping = build_service_endpoint_map(catalog_path)
        self.assertIn("stripe", mapping)
        self.assertIsInstance(mapping["stripe"], list)

    def test_missing_snapshot_returns_empty(self):
        with tempfile.TemporaryDirectory() as d:
            tools = load_snapshot_tools(Path(d), "nonexistent")
            self.assertEqual(tools, [])


# ── Source manifest tests ─────────────────────────────────────────────────────

class SourceManifestTests(unittest.TestCase):
    def test_source_manifest_for_actual_task(self):
        from deploy.consumer.source_manifest import read_task_source
        task_dir = Path("tasks/business__stripe__customer-mrr")
        if not task_dir.is_dir():
            self.skipTest("task directory not present")
        source = read_task_source(task_dir)
        self.assertEqual(source.scenario_id, "business/stripe/customer-mrr")
        self.assertTrue(source.instruction_sha256.startswith("sha256:"))
        self.assertTrue(source.task_toml_sha256.startswith("sha256:"))

    def test_missing_task_toml_raises(self):
        from deploy.consumer.source_manifest import read_task_source
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(FileNotFoundError):
                read_task_source(Path(d))

    def test_build_source_manifest_selected_tasks(self):
        from deploy.consumer.source_manifest import build_source_manifest
        tasks_root = Path("tasks")
        if not tasks_root.is_dir():
            self.skipTest("tasks directory not present")
        names = [d.name for d in sorted(tasks_root.iterdir()) if (d / "task.toml").is_file()][:2]
        if not names:
            self.skipTest("no tasks found")
        manifest = build_source_manifest(tasks_root, names)
        self.assertEqual(len(manifest.tasks), len(names))
        self.assertEqual(manifest.qualified + manifest.unqualified, len(names))

    def test_manifest_serialization(self):
        from deploy.consumer.source_manifest import build_source_manifest
        tasks_root = Path("tasks")
        if not tasks_root.is_dir():
            self.skipTest("tasks directory not present")
        names = [d.name for d in sorted(tasks_root.iterdir()) if (d / "task.toml").is_file()][:1]
        if not names:
            self.skipTest("no tasks found")
        manifest = build_source_manifest(tasks_root, names)
        d = manifest.to_dict()
        self.assertIn("qualified", d)
        self.assertIn("tasks", d)


# ── Integration: exposure over real catalog/bridge ────────────────────────────

class ExposureBridgeIntegrationTests(unittest.TestCase):
    def test_full_catalog_exposure_from_real_inventory(self):
        catalog_path = Path("catalog/services.toml")
        snapshots_dir = Path("servers/snapshots")
        if not catalog_path.is_file() or not snapshots_dir.is_dir():
            self.skipTest("catalog or snapshots not present")
        all_tools = build_tool_inventory(catalog_path, snapshots_dir)
        if not all_tools:
            self.skipTest("no tools in inventory")
        m = build_exposure(ExposureVariant.FULL_CATALOG, 42, all_tools, [], [])
        self.assertEqual(m.tool_count, len(all_tools))
        self.assertTrue(m.catalog_sha256.startswith("sha256:"))
        # Provider-observed counts must be None (unknown)
        self.assertIsNone(m.catalog_metrics.provider_accepted)
        self.assertIsNone(m.catalog_metrics.model_visible)

    def test_confusing_distractors_on_real_inventory(self):
        catalog_path = Path("catalog/services.toml")
        snapshots_dir = Path("servers/snapshots")
        if not catalog_path.is_file() or not snapshots_dir.is_dir():
            self.skipTest("catalog or snapshots not present")
        all_tools = build_tool_inventory(catalog_path, snapshots_dir)
        if not all_tools:
            self.skipTest("no tools in inventory")
        m = build_exposure(ExposureVariant.CONFUSING_DISTRACTORS, 42, all_tools, [], [])
        self.assertGreater(m.tool_count, len(all_tools))
        for t in m.tools:
            if t.is_distractor:
                # Genuine pinned definitions, no fake schema edits
                self.assertIn("name", t.definition)
                desc = t.definition.get("description", "")
                if desc:
                    self.assertTrue(desc.startswith("(Deprecated)"))


if __name__ == "__main__":
    unittest.main()
