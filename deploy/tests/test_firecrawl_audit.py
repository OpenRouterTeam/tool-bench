import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock
from uuid import NAMESPACE_URL, uuid5


SPEC = importlib.util.spec_from_file_location(
    "firecrawl_business_audit", Path(__file__).resolve().parents[1] / "audit-firecrawl-business.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def canonical(value):
    return "sha256:" + hashlib.sha256(json.dumps(
        value, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False,
    ).encode()).hexdigest()


class Trial:
    def __init__(self, family, runtime=False):
        self.family, self.runtime = family, runtime
        baseline, self.now, self.source = AUDIT.authored(family)
        self.baseline = copy.deepcopy(baseline)
        self.source["contract_sha256"] = "sha256:" + "0" * 64
        fixed = {
            "every 6 hours": "0 */6 * * *", "every 12 hours": "0 */12 * * *",
            "daily at 9am": "0 9 * * *",
        }
        for identity, record in baseline.get("monitors", {}).items():
            text, zone = record["schedule"]["text"], record["schedule"]["timezone"]
            cron = fixed.get(text, "0 7 * * *" if zone == "Europe/Berlin" else "0 8 * * *")
            record["schedule"] = {"cron": cron, "timezone": zone}
            record.setdefault("judgeEnabled", bool(record.get("goal")))
            record.setdefault("notification", {})
            for index, target in enumerate(record["targets"]):
                target.setdefault("id", str(uuid5(NAMESPACE_URL, f"{family}/{identity}/{index}")))
        self.initial = {"firecrawl": baseline}
        if runtime:
            defaults = AUDIT.read(AUDIT.REPO / "deploy/fixtures/google-world-bootstrap.json")["services"]
            self.initial = {**{key: value for key, value in defaults.items() if key != "firecrawl"},
                            "google": {}, "firecrawl": baseline}
        self.current = copy.deepcopy(self.initial)
        self.initial_sessions = {key: {} for key in self.initial}
        if runtime:
            stamp = self.now.removesuffix("Z") + ".000Z"
            self.initial_sessions["mongodb"] = {
                "_meta": {"init": {"initializedAt": stamp}},
                "connections": {"preconfigured": {
                    "connectionId": "preconfigured", "name": "preconfigured",
                    "source": "preconfigured", "state": "disconnected",
                    "connectionString": "mongodb+srv://ac-octo-shard-00-00.x7kq2.mongodb.net/",
                    "createdAt": stamp, "lastUsedAt": stamp,
                }},
            }
        self.sessions = copy.deepcopy(self.initial_sessions)
        self.events, self.entries, self.startup = [], [], []
        self.actor = "172.29.0.3" if runtime else "unknown"
        self.secret = AUDIT.credentials() if runtime else "Bearer local-firecrawl"
        self.selected = AUDIT.targets(baseline, family, self.now)
        self.identity = 0

    def call(self, method, path, body=None, mutate=None, response=None, status=200, service="firecrawl"):
        before = copy.deepcopy(self.current)
        returned = mutate() if mutate else {"success": True}
        response = copy.deepcopy(response if response is not None else returned)
        changes = []
        for provider in sorted(before.keys() | self.current.keys()):
            old_provider, new_provider = before.get(provider, {}), self.current.get(provider, {})
            for collection in sorted(old_provider.keys() | new_provider.keys()):
                old, new = old_provider.get(collection, {}), new_provider.get(collection, {})
                for identity in sorted(old.keys() | new.keys()):
                    if (identity in old) == (identity in new) and AUDIT.same(old.get(identity), new.get(identity)):
                        continue
                    escaped = identity.replace("~", "~0").replace("/", "~1")
                    record = {"pointer": f"/services/{provider}/{collection}/{escaped}"}
                    if identity in old:
                        record["before"] = copy.deepcopy(old[identity])
                    if identity in new:
                        record["after"] = copy.deepcopy(new[identity])
                    changes.append(record)
        identity = len(self.events) + 1
        raw = {"encoding": "utf8", "data": json.dumps(body) if body is not None else ""}
        head = {
            "host": "api.firecrawl.dev" if self.runtime else "world:8102",
            "scheme": "https" if self.runtime else "http", "query": None,
            "content_type": "application/json",
            "headers": [{"name": "authorization", "value": {
                "kind": "redacted", "sha256": "sha256:" + hashlib.sha256(self.secret.encode()).hexdigest(),
            }}],
        }
        operation = {
            "id": identity, "generation": 1, "parent_id": None, "kind": "http",
            "scenario": "search/firecrawl/" + AUDIT.SCENARIOS[self.family],
            "service": service, "source": self.actor, "method": method, "path": path,
        }
        self.entries.append({
            "seq": len(self.entries) + 1, "operation_id": identity, "time": self.now,
            **{field: operation[field] for field in ("service", "source", "method", "path")},
            **{field: head[field] for field in ("host", "scheme", "query", "content_type")},
            "status": status, "request_body": copy.deepcopy(body), "response_body": response,
            "replay": {"parent_operation_id": None, "headers": head["headers"], "body": raw},
        })
        def event(kind, **fields):
            self.events.append({"seq": len(self.events) + 1, "time": self.now,
                                "operation": copy.deepcopy(operation), "kind": kind, **copy.deepcopy(fields)})
        event("request_started")
        event("request_head_received", head=head)
        event("request_body_received", body=raw)
        if changes:
            event("state_committed", records=changes)
        event("response_completed", status=status, body_complete=True)

    def next_id(self, kind):
        self.identity += 1
        return f"actual-{kind}-{self.identity}"

    def crawl(self, changes=None, identity=None):
        identity = identity or self.next_id("crawl")
        pages = sorted(url for url in self.baseline["pages"]
                       if url.startswith("https://docs.lumenpay.dev/api/"))
        record = {
            "id": identity, "url": "https://docs.lumenpay.dev/api/overview", "status": "queued",
            "plannedPages": pages, "pages": [], "total": len(pages), "completed": 0,
            "scrapeIds": [], "creditsUsed": 0,
        }
        record.update(copy.deepcopy(changes or {}))
        def mutate():
            self.current["firecrawl"].setdefault("crawls", {})[identity] = copy.deepcopy(record)
            return {"success": True, "id": identity, "url": "https://api.firecrawl.dev/v2/crawl/" + identity}
        self.call("POST", "/v2/crawl", {"url": record["url"]}, mutate)
        return identity

    def monitor(self, changes=None, identity=None):
        identity = identity or self.next_id("monitor")
        record = {
            "id": identity, "name": "Fabrikam pricing", "status": "active",
            "schedule": {"cron": "0 0 * * *", "timezone": "UTC"},
            "targets": [{"type": "scrape", "urls": ["https://www.fabrikam.io/pricing"],
                         "id": "actual-target", "scrapeOptions": {}}],
            "notification": {"email": {"enabled": True, "recipients": ["pricing-intel@contoso.example"],
                                       "includeDiffs": False}},
            "goal": "Alert when Fabrikam changes any plan price or plan limit.", "judgeEnabled": True,
            "createdAt": self.now, "updatedAt": self.now, "retentionDays": 30,
            "nextRunAt": "2026-10-29T00:00:00.000Z", "lastRunAt": None, "lastCheckId": None,
        }
        record.update(copy.deepcopy(changes or {}))
        body = {key: copy.deepcopy(value) for key, value in record.items()
                if key in {"name", "schedule", "targets", "notification", "goal", "judgeEnabled", "retentionDays"}}
        def mutate():
            self.current["firecrawl"]["monitors"][identity] = copy.deepcopy(record)
            return {"success": True, "data": copy.deepcopy(record)}
        self.call("POST", "/v2/monitor", body, mutate)
        return identity

    def update(self, identity, body):
        def mutate():
            record = self.current["firecrawl"]["monitors"][identity]
            record.update(copy.deepcopy(body))
            record["updatedAt"] = self.now
            if "schedule" in body:
                record["nextRunAt"] = "2026-10-29T13:00:00.000Z"
            return {"success": True, "data": copy.deepcopy(record)}
        self.call("PATCH", "/v2/monitor/" + identity, body, mutate)

    def run(self, parent, identity=None):
        identity = identity or self.next_id("check")
        record = {"id": identity, "monitorId": parent, "status": "queued", "trigger": "manual",
                  "createdAt": self.now, "startedAt": None, "completedAt": None,
                  "summary": {"totalPages": 0}, "pages": []}
        def mutate():
            self.current["firecrawl"].setdefault("monitor_checks", {})[identity] = copy.deepcopy(record)
            monitor = self.current["firecrawl"]["monitors"][parent]
            monitor["lastCheckId"], monitor["lastRunAt"] = identity, self.now
            return {"success": True, "id": identity, "data": copy.deepcopy(record)}
        self.call("POST", "/v2/monitor/" + parent + "/run", None, mutate)
        return identity

    def snapshot(self, services, sessions):
        return {"scenario": "search/firecrawl/" + AUDIT.SCENARIOS[self.family], "now": self.now,
                "clock": {"mode": "fixed", "reference": self.now, "step_secs": 7},
                "services": services, "sessions": sessions}

    def write(self, root):
        verifier = root / "verifier"
        verifier.mkdir()
        initial = self.snapshot(self.initial, self.initial_sessions)
        final = self.snapshot(self.current, self.sessions)
        for name, value in [
            ("initial-state.json", initial), ("state.json", final),
            ("events.json", {"events": self.events}), ("journal.json", {"entries": self.entries}),
        ]:
            (verifier / name).write_text(json.dumps(value))
        if self.runtime:
            target = root / "artifacts/world/evidence.json"
            target.parent.mkdir(parents=True)
            target.write_text(json.dumps({
                "schema_version": 5, "scenario": initial["scenario"], "source": self.source,
                "initial_state_sha256": canonical(initial), "initial_state": initial, "state": final,
                "events": self.events, "journal": self.entries,
                "tools_ips": [self.actor], "origin_error": None, "startup": self.startup,
                "finalization": {"phase": "sealed", "pending": [], "failure": None,
                                 "event_count": len(self.events), "generation": 1},
            }))
        return verifier

    def audit(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = self.write(Path(temporary))
            return AUDIT.audit_trial(directory, self.family, direct=not self.runtime)


class SourceBindingTests(unittest.TestCase):
    def test_declared_dataset_digests_match_generated_sources_without_reading_predicates(self):
        for family, slug in AUDIT.SCENARIOS.items():
            with self.subTest(family=family):
                _, _, descriptor = AUDIT.authored(family)
                generated = AUDIT.read(AUDIT.REPO / "tasks" / ("search__firecrawl__" + slug)
                                       / "tests/task-source.json")
                self.assertEqual(descriptor, {key: generated[key] for key in descriptor})

    def test_canonical_normalization_and_opaque_target_id_variations_are_admitted(self):
        for family in AUDIT.SCENARIOS:
            trial = Trial(family)
            normalized = AUDIT.check_initial_services(trial.initial, trial.baseline, direct=True)
            self.assertTrue(AUDIT.same(normalized, trial.initial["firecrawl"]))

    def test_undeclared_extra_record_and_already_paused_overlay_are_rejected_not_scored(self):
        trial = Trial("pause")
        trial.initial["firecrawl"]["monitors"][trial.selected["broken"]]["status"] = "paused"
        trial.current = copy.deepcopy(trial.initial)
        with self.assertRaises(AUDIT.AuditRejected):
            trial.audit()
        trial = Trial("crawl")
        trial.initial["firecrawl"]["crawls"]["unreviewed"] = {"id": "unreviewed"}
        trial.current = copy.deepcopy(trial.initial)
        with self.assertRaises(AUDIT.AuditRejected):
            trial.audit()

    def test_generated_target_ids_must_be_unique_and_canonical_but_not_predicted(self):
        trial = Trial("create")
        records = list(trial.initial["firecrawl"]["monitors"].values())
        records[1]["targets"][0]["id"] = records[0]["targets"][0]["id"]
        with self.assertRaises(AUDIT.AuditRejected):
            AUDIT.check_initial_services(trial.initial, trial.baseline, direct=True)

    def test_initial_schedule_and_neighbor_fields_cannot_be_arbitrarily_rebased(self):
        for field, value in [("schedule", {"cron": "0 10 * * *", "timezone": "UTC"}),
                             ("retentionDays", 1)]:
            trial = Trial("create")
            next(iter(trial.initial["firecrawl"]["monitors"].values()))[field] = value
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.check_initial_services(trial.initial, trial.baseline, direct=True)

    def test_runtime_requires_complete_reviewed_world_not_a_selected_service_subset(self):
        trial = Trial("crawl")
        with self.assertRaises(AUDIT.AuditRejected):
            AUDIT.check_initial_services(trial.initial, trial.baseline)
        trial = Trial("crawl", runtime=True)
        AUDIT.check_initial_services(trial.initial, trial.baseline)
        trial.initial["hidden"] = {}
        with self.assertRaises(AUDIT.AuditRejected):
            AUDIT.check_initial_services(trial.initial, trial.baseline)

    def test_bootstrap_parent_is_byte_bound(self):
        trial = Trial("crawl", runtime=True)
        original = AUDIT.sha256
        with mock.patch.object(AUDIT, "sha256", side_effect=lambda value: "wrong" if value.startswith(b"{") else original(value)):
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.check_initial_services(trial.initial, trial.baseline)

    def test_full_world_default_sessions_are_exact_and_business_time_bound(self):
        trial = Trial("create", runtime=True)
        AUDIT.check_initial_sessions(trial.initial_sessions, trial.initial, trial.now)
        for field, value in [
            ("state", "connected"), ("createdAt", "2026-10-28T16:29:59.000Z"),
            ("connectionString", "mongodb+srv://unreviewed.example.test/"),
        ]:
            captured = copy.deepcopy(trial.initial_sessions)
            captured["mongodb"]["connections"]["preconfigured"][field] = value
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.check_initial_sessions(captured, trial.initial, trial.now)
        captured = copy.deepcopy(trial.initial_sessions)
        captured["mongodb"]["connections"]["unreviewed"] = {"connectionId": "unreviewed"}
        with self.assertRaises(AUDIT.AuditRejected):
            AUDIT.check_initial_sessions(captured, trial.initial, trial.now)

    def test_unselected_session_changes_cannot_be_hidden_after_source_normalization(self):
        trial = Trial("create", runtime=True)
        trial.monitor()
        trial.sessions["mongodb"]["connections"]["preconfigured"]["state"] = "connected"
        with self.assertRaises(AUDIT.AuditRejected):
            trial.audit()

    def test_source_instruction_changes_require_review(self):
        with mock.patch.object(AUDIT, "INSTRUCTIONS", {**AUDIT.INSTRUCTIONS, "crawl": "0" * 64}):
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.authored("crawl")

    def test_declared_already_paused_source_variant_needs_matching_producer_binding(self):
        trial = Trial("pause")
        broken = trial.selected["broken"]
        trial.initial["firecrawl"]["monitors"][broken]["status"] = "paused"
        trial.current = copy.deepcopy(trial.initial)
        trial.run(trial.selected["healthy"])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            shutil.copytree(AUDIT.ROOT / AUDIT.SCENARIOS["pause"], source)
            target = source / "dataset/firecrawl/monitors.json"
            records = json.loads(target.read_text())
            next(record for record in records if record["id"] == broken)["status"] = "paused"
            target.write_text(json.dumps(records))
            _, descriptor = AUDIT.source_descriptor(source, "pause")
            verifier = trial.write(root)
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.audit_trial(verifier, "pause", direct=True, source_directory=source)
            binding = {
                "schema_version": 1, "scope": "source_bound_direct_firecrawl_fixture_not_runtime",
                "source": {**descriptor, "contract_sha256": "sha256:" + "0" * 64},
                "initial_state_sha256": canonical(trial.snapshot(trial.initial, trial.initial_sessions)),
            }
            (verifier / "direct-source.json").write_text(json.dumps(binding))
            self.assertEqual(AUDIT.audit_trial(
                verifier, "pause", direct=True, source_directory=source,
            )[0], "pass")
            binding["source"]["dataset_sha256"] = "sha256:" + "0" * 64
            (verifier / "direct-source.json").write_text(json.dumps(binding))
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.audit_trial(verifier, "pause", direct=True, source_directory=source)


class BusinessGoalTests(unittest.TestCase):
    def test_all_three_reference_outcomes_pass_without_prescribing_discovery(self):
        for family in AUDIT.SCENARIOS:
            trial = Trial(family)
            if family == "crawl":
                trial.crawl()
            elif family == "create":
                trial.monitor()
            else:
                trial.update(trial.selected["broken"], {"status": "paused"})
                trial.run(trial.selected["healthy"])
            self.assertEqual(trial.audit()[0], "pass")

    def test_no_action_is_a_known_business_failure_not_audit_rejection(self):
        for family in AUDIT.SCENARIOS:
            self.assertEqual(Trial(family).audit()[0], "fail")

    def test_crawl_rejects_mixed_missing_duplicate_and_marketing_resources(self):
        for planned in [
            ["https://docs.lumenpay.dev/api/overview", "https://docs.lumenpay.dev/guides/testing"],
            ["https://docs.lumenpay.dev/api/overview"] * 5,
            ["https://lumenpay.dev/api/overview"],
        ]:
            trial = Trial("crawl")
            trial.crawl({"plannedPages": planned})
            self.assertEqual(trial.audit()[0], "fail")

    def test_crawl_accepts_opaque_identity_and_case_equivalent_seed(self):
        trial = Trial("crawl")
        trial.crawl({"url": "https://DOCS.LUMENPAY.DEV:443/api/overview#reference"}, "opaque~identity/path")
        self.assertEqual(trial.audit()[0], "pass")

    def test_crawl_admission_and_observed_complete_collection_are_both_valid(self):
        trial = Trial("crawl")
        identity = trial.crawl()
        self.assertEqual(trial.audit()[0], "pass")
        def mutate():
            record = trial.current["firecrawl"]["crawls"][identity]
            record.update(status="completed", pages=record["plannedPages"], completed=record["total"])
            return {"success": True, "status": "completed"}
        trial.call("GET", "/v2/crawl/" + identity, mutate=mutate)
        self.assertEqual(trial.audit()[0], "pass")

    def test_false_terminal_collection_and_string_counts_do_not_pass(self):
        for changes in [{"status": "completed"}, {"total": "5"}, {"completed": 1}, {"status": "cancelled"}]:
            trial = Trial("crawl")
            trial.crawl(changes)
            self.assertEqual(trial.audit()[0], "fail")

    def test_extra_crawl_and_neighbor_damage_are_failures_even_when_restored(self):
        trial = Trial("crawl")
        trial.crawl()
        trial.crawl()
        self.assertEqual(trial.audit()[0], "fail")
        trial = Trial("pause")
        trial.update(trial.selected["broken"], {"goal": "Changed without permission"})
        trial.update(trial.selected["broken"], {"goal": trial.baseline["monitors"][trial.selected["broken"]]["goal"]})
        trial.update(trial.selected["broken"], {"status": "paused"})
        trial.run(trial.selected["healthy"])
        self.assertEqual(trial.audit()[0], "fail")

    def test_broad_unjudged_tracking_and_email_case_are_valid(self):
        trial = Trial("create")
        trial.monitor({"goal": None, "judgeEnabled": False, "notification": {
            "email": {"enabled": True, "recipients": ["PRICING-INTEL@CONTOSO.EXAMPLE"], "includeDiffs": False},
        }})
        self.assertEqual(trial.audit()[0], "pass")

    def test_notification_and_target_scope_are_exact(self):
        changes = [
            {"notification": {"email": {"enabled": False, "recipients": ["pricing-intel@contoso.example"]}}},
            {"notification": {"email": {"enabled": True, "recipients": ["pricing-intel@contoso.example", "other@example.test"]}}},
            {"notification": {"email": {"enabled": True, "recipients": ["pricing-intel@contoso.example"]},
                              "slack": {"enabled": True}}},
            {"targets": [{"type": "scrape", "urls": ["https://www.fabrikam.io/pricing-backup"]}]},
            {"webhook": {"url": "https://other.example.test/notify"}},
        ]
        for change in changes:
            trial = Trial("create")
            trial.monitor(change)
            self.assertEqual(trial.audit()[0], "fail")

    def test_staged_notifications_and_safe_paused_filter_correction_are_valid(self):
        trial = Trial("create")
        identity = trial.monitor({"notification": {}, "judgeEnabled": False, "goal": None})
        trial.update(identity, {"notification": {"email": {
            "enabled": True, "recipients": ["pricing-intel@contoso.example"],
        }}})
        self.assertEqual(trial.audit()[0], "pass")
        trial = Trial("create")
        identity = trial.monitor()
        trial.update(identity, {"status": "paused"})
        trial.update(identity, {"goal": "Unreviewed inactive setup criterion"})
        trial.update(identity, {"goal": "Notify when prices or plan limits change."})
        trial.update(identity, {"status": "active"})
        self.assertEqual(trial.audit()[0], "pass")

    def test_contradictory_active_filter_fails_and_unknown_active_filter_is_unresolved(self):
        trial = Trial("create")
        trial.monitor({"goal": "Only alert about prices, never plan limits."})
        self.assertEqual(trial.audit()[0], "fail")
        trial = Trial("create")
        trial.monitor({"goal": "Watch meaningful price and limit revisions."})
        self.assertEqual(trial.audit()[0], "unresolved")

    def test_safe_daily_clock_change_on_the_new_monitor_is_a_valid_plan(self):
        trial = Trial("create")
        identity = trial.monitor()
        trial.update(identity, {"schedule": {"cron": "0 9 * * *", "timezone": "America/New_York"}})
        self.assertEqual(trial.audit()[0], "pass")

    def test_inactive_frequency_edits_are_valid_when_corrected_before_reactivation(self):
        trial = Trial("create")
        identity = trial.monitor()
        trial.update(identity, {"status": "paused"})
        trial.update(identity, {"schedule": {"cron": "0 0,12 * * *", "timezone": "UTC"}})
        trial.update(identity, {"schedule": {"cron": "0 7 * * *", "timezone": "Europe/Berlin"}})
        trial.update(identity, {"status": "active"})
        self.assertEqual(trial.audit()[0], "pass")

    def test_multiple_manual_checks_and_unrelated_monitor_runs_are_failures(self):
        trial = Trial("create")
        identity = trial.monitor()
        trial.run(identity)
        trial.run(identity)
        self.assertEqual(trial.audit()[0], "fail")
        trial = Trial("create")
        trial.monitor()
        trial.run(next(iter(trial.initial["firecrawl"]["monitors"])))
        self.assertEqual(trial.audit()[0], "fail")

    def test_pause_then_run_is_required_not_the_reverse(self):
        trial = Trial("pause")
        trial.run(trial.selected["healthy"])
        trial.update(trial.selected["broken"], {"status": "paused"})
        self.assertEqual(trial.audit()[0], "fail")

    def test_pause_does_not_allow_extra_target_configuration(self):
        trial = Trial("pause")
        trial.update(trial.selected["broken"], {"status": "paused", "retentionDays": 1})
        trial.run(trial.selected["healthy"])
        self.assertEqual(trial.audit()[0], "fail")

    def test_optional_new_monitor_check_and_observation_are_valid(self):
        trial = Trial("create")
        parent = trial.monitor()
        identity = trial.run(parent)
        def mutate():
            record = trial.current["firecrawl"]["monitor_checks"][identity]
            record.update(status="completed", completedAt=trial.now)
            return {"success": True, "data": copy.deepcopy(record)}
        trial.call("GET", f"/v2/monitor/{parent}/checks/{identity}", mutate=mutate)
        self.assertEqual(trial.audit()[0], "pass")

    def test_unknown_extraction_is_not_guessed_to_be_correct(self):
        trial = Trial("create")
        trial.monitor({"scrapeOptions": {"formats": ["summary"]}})
        self.assertEqual(trial.audit()[0], "unresolved")


class ProtocolTests(unittest.TestCase):
    def test_schema_six_without_faults_preserves_the_frozen_audit_condition(self):
        trial = Trial("create", runtime=True)
        trial.monitor()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            verifier = trial.write(root)
            path = root / "artifacts/world/evidence.json"
            evidence = json.loads(path.read_text())
            evidence["schema_version"] = 6
            path.write_text(json.dumps(evidence))
            self.assertEqual(AUDIT.audit_trial(verifier, "create")[0], "pass")
            evidence["fault_records"] = [{"decision": "response_lost"}]
            path.write_text(json.dumps(evidence))
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.audit_trial(verifier, "create")

    def test_sealed_runtime_transport_and_full_world_baseline_pass(self):
        trial = Trial("create", runtime=True)
        trial.monitor()
        self.assertEqual(trial.audit()[0], "pass")

    def test_direct_source_evidence_requires_explicit_opt_in(self):
        trial = Trial("crawl")
        trial.crawl()
        with tempfile.TemporaryDirectory() as temporary:
            verifier = trial.write(Path(temporary))
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.audit_trial(verifier, "crawl")

    def test_unbound_auth_or_origin_is_not_an_ordinary_business_failure(self):
        for field in ["auth", "host", "source"]:
            trial = Trial("create", runtime=True)
            trial.monitor()
            if field == "auth":
                trial.entries[0]["replay"]["headers"][0]["value"]["sha256"] = "sha256:" + "0" * 64
                trial.events[1]["head"]["headers"] = copy.deepcopy(trial.entries[0]["replay"]["headers"])
            elif field == "host":
                trial.entries[0]["host"] = "world:8102"
                trial.events[1]["head"]["host"] = "world:8102"
            else:
                for event in trial.events:
                    event["operation"]["source"] = "172.29.0.99"
                trial.entries[0]["source"] = "172.29.0.99"
            with self.assertRaises(AUDIT.AuditRejected):
                trial.audit()

    def test_raw_request_and_replay_before_values_cannot_be_forged(self):
        for field in ["body", "before", "duplicate", "status"]:
            trial = Trial("pause")
            trial.update(trial.selected["broken"], {"status": "paused"})
            trial.run(trial.selected["healthy"])
            if field == "body":
                trial.entries[0]["request_body"] = {"status": "active"}
            elif field == "before":
                next(event for event in trial.events if event["kind"] == "state_committed")["records"][0]["before"]["forged"] = True
            elif field == "duplicate":
                records = next(event for event in trial.events if event["kind"] == "state_committed")["records"]
                records.append(copy.deepcopy(records[0]))
            else:
                trial.entries[0]["status"] = 500
                next(event for event in trial.events if event["kind"] == "response_completed")["status"] = 500
            with self.assertRaises(AUDIT.AuditRejected):
                trial.audit()

    def test_unrecorded_final_business_and_session_state_are_rejected(self):
        for field in ["business", "session", "clock"]:
            trial = Trial("crawl")
            trial.crawl()
            if field == "business":
                trial.current["unselected"] = {"records": {"injected": {"id": "injected"}}}
            elif field == "session":
                trial.sessions["firecrawl"]["injected"] = {}
            with tempfile.TemporaryDirectory() as temporary:
                verifier = trial.write(Path(temporary))
                if field == "clock":
                    value = json.loads((verifier / "initial-state.json").read_text())
                    value["clock"]["step_secs"] = 8
                    (verifier / "initial-state.json").write_text(json.dumps(value))
                with self.assertRaises(AUDIT.AuditRejected):
                    AUDIT.audit_trial(verifier, "crawl", direct=True)

    def test_producer_source_and_initial_digest_are_not_self_asserted(self):
        for field in ["dataset_sha256", "initial_state_sha256", "generation", "pending"]:
            trial = Trial("crawl", runtime=True)
            trial.crawl()
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                verifier = trial.write(root)
                target = root / "artifacts/world/evidence.json"
                evidence = json.loads(target.read_text())
                if field == "dataset_sha256":
                    evidence["source"][field] = "sha256:" + "0" * 64
                elif field == "initial_state_sha256":
                    evidence[field] = "sha256:" + "0" * 64
                elif field == "generation":
                    evidence["finalization"][field] = 999
                else:
                    evidence["finalization"][field] = [1]
                target.write_text(json.dumps(evidence))
                with self.assertRaises(AUDIT.AuditRejected):
                    AUDIT.audit_trial(verifier, "crawl")

    def test_successful_creation_needs_the_actual_response_identity(self):
        trial = Trial("crawl")
        trial.crawl()
        trial.entries[0]["response_body"]["id"] = "not-the-created-crawl"
        with self.assertRaises(AUDIT.AuditRejected):
            trial.audit()

    def test_cross_service_records_are_not_silently_ignored(self):
        trial = Trial("create")
        trial.monitor()
        def mutate():
            trial.current["unselected"] = {"records": {"extra": {"id": "extra"}}}
            return {"success": True}
        trial.call("POST", "/other", mutate=mutate, service="unselected")
        self.assertEqual(trial.audit()[0], "fail")

    def test_startup_effects_are_not_attributed_to_task_work(self):
        trial = Trial("create", runtime=True)
        trial.monitor()
        trial.startup = [{"service": "firecrawl", "method": "POST", "path": "/v2/monitor", "query": None}]
        with self.assertRaises(AUDIT.AuditRejected):
            trial.audit()

    def test_cached_verdicts_and_expectation_files_are_not_audit_inputs(self):
        trial = Trial("create")
        trial.monitor()
        with tempfile.TemporaryDirectory() as temporary:
            verifier = trial.write(Path(temporary))
            (verifier / "assessment.json").write_text('{"outcome":"fail"}')
            (verifier / "expect.json").write_text('{"invented":"pass"}')
            self.assertEqual(AUDIT.audit_trial(verifier, "create", direct=True)[0], "pass")

    def test_bound_scrape_session_is_a_valid_read_not_collateral_business_damage(self):
        trial = Trial("create")
        identity = trial.monitor()
        scrape_id, url = "actual-scrape", "https://www.fabrikam.io/pricing"
        def mutate():
            trial.sessions["firecrawl"]["scrapes"] = {
                scrape_id: {"id": scrape_id, "url": url, "createdAt": trial.now},
            }
            return {"success": True, "data": {"metadata": {
                "scrapeId": scrape_id, "url": url, "sourceURL": url,
            }}}
        trial.call("POST", "/v2/scrape", {"url": url}, mutate)
        self.assertEqual(trial.audit()[0], "pass")
        self.assertEqual(trial.current["firecrawl"]["monitors"][identity]["name"], "Fabrikam pricing")

    def test_cli_does_not_turn_audit_rejection_into_a_numeric_business_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            verifier = Trial("create").write(root)
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([str(verifier)]), 2)


class ScheduleTests(unittest.TestCase):
    def test_daily_is_semantic_not_a_literal_cron_or_1440_elapsed_minutes(self):
        for cron in ["0 9 * * *", "15 17 1-31 1-12 0-7", "0,0 9,9 * * *", "0/5 9 * * *"]:
            self.assertTrue(AUDIT.daily({"cron": cron, "timezone": "America/New_York"}))
        for cron in ["0 0,12 * * *", "0 9 * * 1-5", "0 9 1 * *", "0 9 * 1 *", "0,1 9 * * *"]:
            self.assertFalse(AUDIT.daily({"cron": cron, "timezone": "Europe/Berlin"}))

    def test_unknown_grammar_and_invalid_bounds_do_not_gain_an_invented_schedule(self):
        for schedule in [
            {"cron": "0 9 * * MON", "timezone": "UTC"},
            {"cron": "0 25 * * *", "timezone": "UTC"},
            {"cron": "0 9 * * *", "timezone": "Mars/Olympus"},
            {"cron": "0 9 * * *", "timezone": "UTC", "ignored": True},
        ]:
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.daily(schedule)

class ResourceTests(unittest.TestCase):
    def test_reviewed_url_equivalences_match_the_native_platform_parser(self):
        cases = [
            "https://WWW.FABRIKAM.IO:443/pricing#limits",
            "https://www.fabrikam.io/./pricing",
            "https://www.fabrikam.io/reference/../pricing",
            "https://www.fabrikam.io/%2e/pricing",
            "https://www.fabrikam.io/reference/.%2e/pricing",
            "https://@www.fabrikam.io/pricing",
            "https://:@www.fabrikam.io/pricing",
            " https://www.fabrikam.io/pricing ",
            "https://www.fabrikam.io/pricing?",
            "https://www.fabrikam.io/reference//../pricing",
        ]
        output = subprocess.run(
            ["node", "-e", "process.stdout.write(JSON.stringify(JSON.parse(process.argv[1]).map(value=>{const url=new URL(value);url.hash='';return url.href;})))",
             json.dumps(cases)],
            check=True, text=True, capture_output=True,
        )
        self.assertEqual([AUDIT.resource(value) for value in cases], json.loads(output.stdout))

    def test_credentials_and_unqualified_backslashes_do_not_gain_a_business_identity(self):
        self.assertIsNone(AUDIT.resource("https://operator:password@www.fabrikam.io/pricing"))
        with self.assertRaises(AUDIT.Indeterminate):
            AUDIT.resource("https://www.fabrikam.io\\pricing")


if __name__ == "__main__":
    unittest.main()
