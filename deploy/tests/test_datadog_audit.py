import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "datadog_business_audit", Path(__file__).resolve().parents[1] / "audit-datadog-business.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def dumps(value):
    if isinstance(value, Decimal):
        return str(value)
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(key) + ":" + dumps(item) for key, item in value.items()) + "}"
    if isinstance(value, list):
        return "[" + ",".join(dumps(item) for item in value) + "]"
    return json.dumps(value)


class Trial:
    def __init__(self, family):
        self.family = family
        baseline, self.now = AUDIT.authored(family)
        self.selected = AUDIT.targets(baseline, family)
        self.initial = {"datadog": baseline}
        self.current = copy.deepcopy(self.initial)
        self.events, self.journal, self.startup = [], [], []
        self.source = "172.20.0.3"

    def request(self, method, path, body=None, response=None, mutate=None, status=200, service="datadog"):
        previous = copy.deepcopy(self.current)
        if mutate is not None:
            mutate()
        changes = []
        for service_name in sorted(previous.keys() | self.current.keys()):
            old_service, new_service = previous.get(service_name, {}), self.current.get(service_name, {})
            for collection in sorted(old_service.keys() | new_service.keys()):
                old, new = old_service.get(collection, {}), new_service.get(collection, {})
                for key in sorted(old.keys() | new.keys()):
                    if AUDIT.same(old.get(key), new.get(key)):
                        continue
                    escaped = key.replace("~", "~0").replace("/", "~1")
                    change = {"pointer": f"/services/{service_name}/{collection}/{escaped}"}
                    if key in old:
                        change["before"] = copy.deepcopy(old[key])
                    if key in new:
                        change["after"] = copy.deepcopy(new[key])
                    changes.append(change)
        identity = len(self.events) + 1
        operation = {
            "id": identity, "generation": 1, "parent_id": None, "kind": "http",
            "scenario": "ops/datadog/" + AUDIT.SCENARIOS[self.family],
            "service": service, "source": self.source, "method": method, "path": path,
        }
        headers = [{
            "name": name,
            "value": {
                "kind": "synthetic", "reference": "fixture." + name,
                "sha256": "sha256:" + hashlib.sha256(value.encode()).hexdigest(),
            },
        } for name, value in AUDIT.pinned_credentials().items()]
        raw = {"encoding": "utf8", "data": "" if body is None else dumps(body)}
        head = {
            "host": "api.datadoghq.com", "scheme": "https", "query": None,
            "content_type": "application/json" if body is not None else None, "headers": headers,
        }
        request = {
            "seq": len(self.journal) + 1, "operation_id": identity, "time": self.now,
            "service": service, "source": self.source, "method": method, "path": path,
            **{field: head[field] for field in ("host", "scheme", "query", "content_type")},
            "status": status, "request_body": copy.deepcopy(body), "response_body": copy.deepcopy(response),
            "replay": {"parent_operation_id": None, "headers": headers, "body": raw},
        }
        self.journal.append(request)

        def event(kind, **fields):
            self.events.append({
                "seq": len(self.events) + 1, "time": self.now,
                "operation": copy.deepcopy(operation), "kind": kind, **copy.deepcopy(fields),
            })

        event("request_started")
        event("request_head_received", head=head)
        event("request_body_received", body=raw)
        if changes:
            event("state_committed", records=changes)
        event("response_completed", status=status, body_complete=True)
        return request

    def logs(self, rows=None, query="", status=200):
        if rows is None:
            rows = [copy.deepcopy(next(iter(self.selected["logs"].values())))]
        return self.request("POST", AUDIT.LOG_PATH, {
            "filter": {"query": query, "from": "2026-10-30T12:00:00Z", "to": "1793370600000"},
            "sort": "-timestamp", "page": {"limit": 1},
        }, {"data": rows, "meta": {"status": "done", "request_id": "local-http-only"}}, status=status)

    def cancel(self, key=None):
        key = key or self.selected["downtime"]

        def mutate():
            row = self.current["datadog"]["downtimes"][key]
            if row["canceled"] is None:
                row.update(canceled=AUDIT.epoch(self.now), disabled=True, active=False, updater_id=1445416)

        return self.request("DELETE", f"/api/v1/downtime/{key}", mutate=mutate, status=204)

    def schedule(self, key="9007", body=None, updates=None):
        body = copy.deepcopy(body) if body is not None else {
            "scope": ["host:pg-prod-replica-2"], "start": 1793412000, "end": 1793419200,
            "monitor_id": 7301, "message": "Replica rebuild (CHG-5012).",
        }
        body.update(updates or {})
        row = {
            "id": int(key), "active": False, "active_child": None, "canceled": None,
            "creator_id": 1445416, "disabled": False, "downtime_type": 2 if body.get("monitor_id") else 0,
            "end": body.get("end"), "message": body.get("message"), "monitor_id": body.get("monitor_id"),
            "monitor_tags": body.get("monitor_tags", ["*"]),
            "mute_first_recovery_notification": body.get("mute_first_recovery_notification", False),
            "notify_end_states": body.get("notify_end_states", ["alert", "no data", "warn"]),
            "notify_end_types": body.get("notify_end_types", ["expired"]),
            "parent_id": None, "recurrence": None, "scope": [
                term.strip() for item in body["scope"] for term in item.split(",") if term.strip()
            ],
            "start": body.get("start", AUDIT.epoch(self.now)), "timezone": body.get("timezone", "UTC"),
            "updater_id": None,
        }
        if body.get("recurrence") is not None:
            recurrence = body["recurrence"]
            row["recurrence"] = {
                "type": recurrence.get("type"), "period": recurrence.get("period", 1),
                "rrule": recurrence.get("rrule"), "until_date": recurrence.get("until_date"),
                "until_occurrences": recurrence.get("until_occurrences"), "week_days": recurrence.get("week_days"),
            }
        response = copy.deepcopy(row)
        response["active"] = (
            AUDIT.numeric(row["start"]) is not None and AUDIT.numeric(row["start"]) <= AUDIT.epoch(self.now)
            and (row["end"] is None or AUDIT.numeric(row["end"]) > AUDIT.epoch(self.now))
        )
        return self.request("POST", "/api/v1/downtime", body, response,
                            lambda: self.current["datadog"].setdefault("downtimes", {}).update({key: row}))

    def mute(self, host=None, updates=None):
        host = host or self.selected["host"]
        body = {"end": 1793383200, "message": "Draining for replacement: INC-2291.", "override": False}
        body.update(updates or {})

        def mutate():
            self.current["datadog"]["hosts"][host].update(is_muted=True, mute_timeout=body["end"])
            self.current["datadog"].setdefault("host_mutes", {})[host] = {
                "hostname": host, "end": body["end"], "message": body["message"], "muted_at": self.now,
            }

        return self.request("POST", f"/api/v1/host/{host}/mute", body, {
            "action": "Muted", "hostname": host, "end": body["end"], "message": body["message"],
        }, mutate)

    def change_committed(self, collection, key, field, value, response=True):
        for event in reversed(self.events):
            if event["kind"] != "state_committed":
                continue
            for change in event["records"]:
                if change["pointer"] == f"/services/datadog/{collection}/{key}":
                    change["after"][field] = copy.deepcopy(value)
                    self.current["datadog"][collection][key][field] = copy.deepcopy(value)
                    if response and self.journal[-1]["response_body"] is not None:
                        self.journal[-1]["response_body"][field] = copy.deepcopy(value)
                    return
        raise AssertionError("missing committed record")

    def renumber(self):
        identities = {
            event["operation"]["id"]: index
            for index, event in enumerate(self.events, 1) if event["kind"] == "request_started"
        }
        for index, event in enumerate(self.events, 1):
            event["seq"] = index
            event["operation"]["id"] = identities[event["operation"]["id"]]
        for request in self.journal:
            request["operation_id"] = identities[request["operation_id"]]

    def outcome(self):
        unauthorized = AUDIT.history(
            self.initial, self.current, self.events, self.journal, self.family, self.now,
            startup=self.startup,
        )
        completed = AUDIT.goal(self.initial["datadog"], self.current["datadog"], self.family)
        return "pass" if completed and not unauthorized else "fail"

    def write(self, root, envelope=False):
        verifier = root / "trial" / "verifier"
        verifier.mkdir(parents=True)
        initial = {
            "scenario": "ops/datadog/" + AUDIT.SCENARIOS[self.family], "now": self.now,
            "clock": {"mode": "fixed", "reference": self.now, "step_secs": 0},
            "services": self.initial, "sessions": {},
        }
        final = copy.deepcopy(initial)
        final["services"] = self.current
        documents = {
            "initial-state.json": initial, "state.json": final,
            "events.json": {"events": self.events}, "journal.json": {"entries": self.journal},
            "assessment.json": {"outcome": "poisoned-cache"},
        }
        for name, value in documents.items():
            (verifier / name).write_text(dumps(value))
        if envelope:
            destination = verifier.parent / "artifacts" / "world"
            destination.mkdir(parents=True)
            (destination / "evidence.json").write_text(dumps({
                "scenario": initial["scenario"], "initial_state": initial, "state": final,
                "journal": self.journal, "events": self.events, "startup": self.startup,
                "tools_ips": [self.source], "origin_error": None,
                "finalization": {
                    "phase": "sealed", "pending": [], "failure": None,
                    "event_count": len(self.events), "generation": 1,
                },
            }))
        return verifier


class DatadogSourceAndGoalTests(unittest.TestCase):
    def test_business_identities_and_clocks_come_from_source(self):
        cancel, mute, schedule = Trial("cancel"), Trial("mute"), Trial("schedule")
        self.assertEqual(cancel.selected["downtime"], "2207731")
        self.assertEqual(cancel.now, "2026-10-30T10:00:00Z")
        self.assertEqual(mute.selected["host"], "checkout-api-prod-1b")
        self.assertEqual(mute.now, "2026-10-30T14:30:00Z")
        self.assertEqual(len(mute.selected["logs"]), 12)
        self.assertEqual(schedule.selected["monitor"], 7301)
        self.assertEqual(schedule.selected["tags"]["service"], {"postgres"})
        self.assertEqual(schedule.selected["tags"]["role"], {"replica"})
        self.assertEqual(schedule.now, "2026-10-30T16:00:00Z")

    def test_empty_workflows_fail_without_claiming_completion(self):
        for family in AUDIT.SCENARIOS:
            with self.subTest(family=family):
                self.assertEqual(Trial(family).outcome(), "fail")

    def test_all_three_local_references_pass(self):
        for family in AUDIT.SCENARIOS:
            with self.subTest(family=family):
                trial = Trial(family)
                if family == "mute":
                    trial.logs()
                getattr(trial, family)()
                self.assertEqual(trial.outcome(), "pass")

    def test_cancel_retains_original_metadata_and_every_neighbor(self):
        trial = Trial("cancel")
        trial.cancel()
        self.assertEqual(trial.outcome(), "pass")
        original = trial.initial["datadog"]["downtimes"]
        actual = trial.current["datadog"]["downtimes"]
        for key in original:
            if key != trial.selected["downtime"]:
                self.assertTrue(AUDIT.same(original[key], actual[key]))
        self.assertEqual(actual[trial.selected["downtime"]]["creator_id"], 1445400)
        self.assertEqual(actual[trial.selected["downtime"]]["updater_id"], 1445416)

    def test_idempotent_cancel_does_not_require_a_second_commit(self):
        trial = Trial("cancel")
        trial.cancel()
        trial.cancel()
        self.assertEqual(trial.outcome(), "pass")

    def test_wrong_cancellation_identity_fails_even_when_restored(self):
        trial = Trial("cancel")
        trial.cancel("2207752")
        trial.request("GET", "/api/v1/downtime", mutate=lambda: trial.current["datadog"]["downtimes"].update({
            "2207752": copy.deepcopy(trial.initial["datadog"]["downtimes"]["2207752"]),
        }))
        trial.cancel()
        self.assertEqual(trial.outcome(), "fail")

    def test_each_target_metadata_corruption_fails(self):
        for field, value in {
            "creator_id": 1445416, "message": "overwritten", "start": 1793484001,
            "scope": ["env:staging"], "monitor_id": 8802, "parent_id": 2207702,
            "notify_end_types": [], "canceled": Decimal("1793354400.000000001"), "updater_id": 1445400,
        }.items():
            with self.subTest(field=field):
                trial = Trial("cancel")
                trial.cancel()
                trial.change_committed("downtimes", trial.selected["downtime"], field, value, response=False)
                self.assertEqual(trial.outcome(), "fail")

    def test_removing_canceled_record_is_not_identity_preserving(self):
        trial = Trial("cancel")
        trial.request("DELETE", f"/api/v1/downtime/{trial.selected['downtime']}", status=204,
                      mutate=lambda: trial.current["datadog"]["downtimes"].pop(trial.selected["downtime"]))
        self.assertEqual(trial.outcome(), "fail")

    def test_wrong_monitor_or_missing_direct_monitor_selector_fails(self):
        for monitor in [7302, 7303, 7304, 7305, None, Decimal("7301.000000000000000001")]:
            with self.subTest(monitor=monitor):
                trial = Trial("schedule")
                trial.schedule(updates={"monitor_id": monitor})
                self.assertEqual(trial.outcome(), "fail")

    def test_exact_numeric_request_and_state_windows_have_no_epsilon(self):
        for field in ["start", "end"]:
            for delta in [Decimal("0.000000000000000001"), Decimal("-0.000000000000000001")]:
                with self.subTest(field=field, delta=delta):
                    trial = Trial("schedule")
                    wanted = 1793412000 if field == "start" else 1793419200
                    trial.schedule(updates={field: Decimal(wanted) + delta})
                    self.assertEqual(trial.outcome(), "fail")

    def test_numeric_strings_and_booleans_are_not_epoch_numbers(self):
        trial = Trial("schedule")
        trial.schedule(updates={"start": "1793412000"})
        self.assertFalse(AUDIT.goal(trial.initial["datadog"], trial.current["datadog"], "schedule"))
        with self.assertRaises(AUDIT.AuditRejected):
            trial.outcome()
        self.assertIsNone(AUDIT.numeric(True))
        self.assertFalse(AUDIT.same(True, 1))

    def test_decimal_integral_wire_values_are_exactly_equivalent(self):
        trial = Trial("schedule")
        trial.schedule(updates={"start": Decimal("1793412000.0"), "end": Decimal("1793419200.000")})
        self.assertEqual(trial.outcome(), "pass")

    def test_recurring_schedule_is_not_a_one_time_window(self):
        trial = Trial("schedule")
        trial.schedule(updates={"recurrence": {"type": "days", "period": 1}})
        self.assertEqual(trial.outcome(), "fail")

    def test_created_id_is_actual_key_and_response_not_allocator_guess(self):
        for key in ["17", "9007", "999999999999999999"]:
            with self.subTest(key=key):
                trial = Trial("schedule")
                trial.schedule(key=key)
                self.assertEqual(trial.outcome(), "pass")
                self.assertIn(key, trial.current["datadog"]["downtimes"])

    def test_second_created_identity_fails_even_if_correct(self):
        trial = Trial("schedule")
        trial.schedule("9007")
        trial.schedule("9008")
        self.assertEqual(trial.outcome(), "fail")

    def test_two_creates_in_one_transaction_are_not_one_downtime(self):
        trial = Trial("schedule")
        trial.schedule()
        first = copy.deepcopy(trial.current["datadog"]["downtimes"]["9007"])
        second = dict(first, id=9008)
        trial.current["datadog"]["downtimes"]["9008"] = second
        commit = next(event for event in trial.events if event["kind"] == "state_committed")
        commit["records"].append({
            "pointer": "/services/datadog/downtimes/9008", "after": copy.deepcopy(second),
        })
        self.assertEqual(trial.outcome(), "fail")

    def test_complete_message_identifier_not_prefix_or_embedded_token(self):
        for family, token in [("schedule", "CHG-5012"), ("mute", "INC-2291")]:
            for message in [token + "0", "X" + token, token + "suffix", token[:-1], "", None]:
                with self.subTest(family=family, message=message):
                    trial = Trial(family)
                    if family == "mute":
                        trial.logs()
                    getattr(trial, family)(updates={"message": message})
                    self.assertEqual(trial.outcome(), "fail")

    def test_identifier_policy_does_not_impose_unrequested_exact_prose(self):
        for family, message in [
            ("schedule", "Maintenance — [CHG-5012]"), ("mute", "Replacement (INC-2291)."),
            ("schedule", "Maintenance — [chg-5012]"), ("mute", "Replacement (inc-2291)."),
            ("schedule", "https://ops.example/changes/CHG-5012-rebuild"),
            ("mute", "https://ops.example/incidents/INC-2291-postmortem"),
        ]:
            with self.subTest(family=family):
                trial = Trial(family)
                if family == "mute":
                    trial.logs()
                getattr(trial, family)(updates={"message": message})
                self.assertEqual(trial.outcome(), "pass")

    def test_timezone_display_does_not_reinterpret_numeric_window(self):
        trial = Trial("schedule")
        trial.schedule(updates={"timezone": "America/Chicago"})
        self.assertEqual(trial.outcome(), "pass")


class DatadogIndependentScopeTests(unittest.TestCase):
    def test_positive_redundant_terms_commas_and_order_are_equivalent(self):
        for scope in [
            ["host:pg-prod-replica-2"],
            ["*", "host:pg-prod-replica-2", "env:prod", "service:post*", "role:replica"],
            ["service:postgres,host:pg-prod-replica-2,env:pr*,host:pg-prod-replica-2,*"],
            ["host:pg-prod-replica-2,*", "host:pg-prod-*", "role:r*", "env:*"],
        ]:
            with self.subTest(scope=scope):
                trial = Trial("schedule")
                trial.schedule(updates={"scope": scope})
                self.assertEqual(trial.outcome(), "pass")

    def test_wrong_contradictory_incompatible_and_broad_scopes_fail(self):
        for scope in [
            ["host:pg-prod-replica-1"], ["*"], ["env:prod"], ["host:pg-prod-replica-*"],
            ["host:pg-prod-replica-2", "host:pg-prod-replica-1"],
            ["host:pg-prod-replica-2", "env:staging"],
            ["host:pg-prod-replica-2", "role:primary"],
            ["host:pg-prod-replica-2", "service:mysql"],
            ["host:pg-prod-replica-2", "missing:value"],
        ]:
            with self.subTest(scope=scope):
                trial = Trial("schedule")
                trial.schedule(updates={"scope": scope})
                self.assertEqual(trial.outcome(), "fail")

    def test_matching_only_seeded_host_is_not_an_all_values_proof(self):
        trial = Trial("schedule")
        scope = ["host:pg-prod-replica-2*", "host:*pg-prod-replica-2"]
        selected_seeded = [
            host for host in trial.initial["datadog"]["hosts"]
            if all(AUDIT.glob_matches(term.split(":", 1)[1], host) for term in scope)
        ]
        self.assertEqual(selected_seeded, ["pg-prod-replica-2"])
        trial.schedule(updates={"scope": scope})
        self.assertEqual(trial.outcome(), "fail")
        doubled = "pg-prod-replica-2pg-prod-replica-2"
        self.assertTrue(all(AUDIT.glob_matches(term.split(":", 1)[1], doubled) for term in scope))

    def test_unreviewed_syntax_is_indeterminate_not_guessed_failure(self):
        for scope in [
            ["host:pg-prod-replica-2 OR host:pg-prod-replica-1"],
            ["host:pg-prod-replica-2 env:prod"],
            ["host:pg-prod-replica-?"], ["!env:staging"], ["(host:pg-prod-replica-2)"],
            ["host:[pg-prod-replica-2]"], ["ho*:pg-prod-replica-2"], ['host:"pg-prod-replica-2"'],
        ]:
            with self.subTest(scope=scope):
                trial = Trial("schedule")
                trial.schedule(updates={"scope": scope})
                with self.assertRaises(AUDIT.Indeterminate):
                    trial.outcome()

    def test_nontrivial_combined_monitor_selector_stays_unreviewed(self):
        trial = Trial("schedule")
        trial.schedule(updates={"monitor_tags": ["env:prod", "service:postgres"]})
        with self.assertRaises(AUDIT.Indeterminate):
            trial.outcome()


class DatadogMuteEvidenceTests(unittest.TestCase):
    def test_one_complete_matching_log_is_sufficient_with_broad_query(self):
        trial = Trial("mute")
        trial.logs(query="")
        trial.mute()
        self.assertEqual(trial.outcome(), "pass")

    def test_query_order_limit_and_bound_representation_are_not_reference_requirements(self):
        for body in [
            {"filter": {"query": '@env:prod AND service:checkout-api', "from": "1793365200000",
                        "to": "1793370600000"}, "page": {"limit": 3}, "sort": "-timestamp"},
            {"filter": {"query": "*", "from": "2026-10-30T08:00:00-05:00",
                        "to": "2026-10-30T14:30:00Z"}, "page": {"limit": 37}, "sort": "-timestamp"},
        ]:
            with self.subTest(body=body):
                trial = Trial("mute")
                response = {"data": [copy.deepcopy(next(iter(trial.selected["logs"].values())))]}
                trial.request("POST", AUDIT.LOG_PATH, body, response)
                trial.mute()
                self.assertEqual(trial.outcome(), "pass")

    def test_broader_complete_source_candidate_set_is_allowed(self):
        trial = Trial("mute")
        trial.logs(rows=list(copy.deepcopy(trial.initial["datadog"]["logs"]).values()))
        trial.mute()
        self.assertEqual(trial.outcome(), "pass")

    def test_no_log_read_is_a_business_failure_when_complete_trace_proves_its_absence(self):
        trial = Trial("mute")
        trial.mute()
        self.assertEqual(trial.outcome(), "fail")

    def test_empty_error_or_nonmatching_log_results_do_not_identify_target(self):
        for rows, status in [([], 200), ([], 403), ([], 500)]:
            with self.subTest(rows=rows, status=status):
                trial = Trial("mute")
                trial.logs(rows=rows, status=status)
                trial.mute()
                self.assertEqual(trial.outcome(), "fail")

    def test_missing_log_response_and_missing_data_array_are_rejected(self):
        for response in [None, {}, {"data": None}, {"data": "Logs data: []"}]:
            with self.subTest(response=response):
                trial = Trial("mute")
                request = trial.logs()
                request["response_body"] = response
                trial.mute()
                with self.assertRaisesRegex(AUDIT.AuditRejected, "complete data array"):
                    trial.outcome()

    def test_failed_target_mute_admission_also_requires_prior_log_evidence(self):
        trial = Trial("mute")
        trial.request("POST", f"/api/v1/host/{trial.selected['host']}/mute",
                      {"end": 1793383200}, {"errors": ["Forbidden"]}, status=403)
        self.assertEqual(trial.outcome(), "fail")

    def test_paginated_response_with_one_complete_matching_log_is_sufficient(self):
        trial = Trial("mute")
        request = trial.logs()
        request["response_body"]["meta"]["page"] = {"after": "cursor-not-exposed-by-native-tool"}
        request["response_body"]["links"] = {"next": "https://api.datadoghq.com/next"}
        trial.mute()
        self.assertEqual(trial.outcome(), "pass")

    def test_pre_onset_wrong_service_staging_and_nonerror_logs_do_not_suffice(self):
        trial = Trial("mute")
        negatives = [
            row for row in trial.initial["datadog"]["logs"].values()
            if row["id"] not in trial.selected["logs"]
        ]
        self.assertTrue(negatives)
        for row in negatives:
            with self.subTest(log=row["id"]):
                candidate = Trial("mute")
                candidate.logs(rows=[row])
                candidate.mute()
                self.assertEqual(candidate.outcome(), "fail")

    def test_partial_source_log_is_rejected_even_if_id_and_host_match(self):
        trial = Trial("mute")
        source = next(iter(trial.selected["logs"].values()))
        partial = {"id": source["id"], "type": "log", "attributes": {
            field: source["attributes"][field] for field in ("host", "service", "status", "timestamp", "message")
        }}
        trial.logs(rows=[partial])
        trial.mute()
        with self.assertRaisesRegex(AUDIT.AuditRejected, "partial, spoofed"):
            trial.outcome()

    def test_spoofed_source_content_cannot_borrow_a_real_log_id(self):
        for field, value in [
            ("host", "checkout-api-prod-1a"), ("service", "payments-api"),
            ("status", "warn"), ("timestamp", "2026-10-30T14:30:00Z"), ("message", "healthy"),
        ]:
            with self.subTest(field=field):
                trial = Trial("mute")
                row = copy.deepcopy(next(iter(trial.selected["logs"].values())))
                row["attributes"][field] = value
                trial.logs(rows=[row])
                trial.mute()
                with self.assertRaises(AUDIT.AuditRejected):
                    trial.outcome()

    def test_log_evidence_interval_is_half_open_without_truncation(self):
        contract = AUDIT.source_contract("mute")
        row = copy.deepcopy(next(iter(Trial("mute").selected["logs"].values())))
        for timestamp, expected in [
            ("2026-10-30T12:59:59.999999999Z", False), ("2026-10-30T13:00:00Z", True),
            ("2026-10-30T14:29:59.999999999Z", True), ("2026-10-30T14:30:00Z", False),
        ]:
            with self.subTest(timestamp=timestamp):
                row["attributes"]["timestamp"] = timestamp
                self.assertEqual(AUDIT.matching_log(row, contract), expected)

    def test_read_after_mute_is_too_late(self):
        trial = Trial("mute")
        trial.mute()
        trial.logs()
        self.assertEqual(trial.outcome(), "fail")

    def test_response_start_journal_is_not_response_completion(self):
        trial = Trial("mute")
        log = trial.logs()
        completion = trial.events.pop()
        trial.mute()
        trial.events.append(completion)
        trial.renumber()
        self.assertEqual(log["seq"], 1)
        self.assertEqual(trial.outcome(), "fail")

    def test_completed_log_after_mute_admission_but_before_write_still_too_late(self):
        trial = Trial("mute")
        trial.logs()
        completion = trial.events.pop()
        trial.mute()
        trial.events.insert(5, completion)
        trial.renumber()
        self.assertEqual(trial.outcome(), "fail")

    def test_incomplete_response_is_rejected_even_with_matching_response_body(self):
        trial = Trial("mute")
        trial.logs()
        trial.events[-1]["body_complete"] = False
        trial.mute()
        with self.assertRaisesRegex(AUDIT.AuditRejected, "incomplete"):
            trial.outcome()

    def test_startup_does_not_satisfy_the_captured_request_prerequisite(self):
        trial = Trial("mute")
        trial.logs()
        trial.startup = [{"service": "datadog", "method": "POST", "path": AUDIT.LOG_PATH, "query": None}]
        trial.mute()
        self.assertEqual(trial.outcome(), "fail")

    def test_repeated_read_after_one_startup_budget_is_legitimate(self):
        trial = Trial("mute")
        trial.logs()
        trial.startup = [{"service": "datadog", "method": "POST", "path": AUDIT.LOG_PATH, "query": None}]
        trial.logs()
        trial.mute()
        self.assertEqual(trial.outcome(), "pass")

    def test_wrong_host_upstream_alias_and_preexisting_host_are_not_target(self):
        for host in ["checkout-api-prod-1a", "checkout-api-staging-1b", "payments-api-prod-1a"]:
            with self.subTest(host=host):
                trial = Trial("mute")
                trial.logs()
                trial.mute(host=host, updates={"override": True})
                self.assertEqual(trial.outcome(), "fail")

    def test_mute_end_near_misses_are_not_truncated_or_epsilon_equal(self):
        for end in [1793383199, 1793383201, Decimal("1793383200.000000000000000001"), None, "1793383200"]:
            with self.subTest(end=end):
                trial = Trial("mute")
                trial.logs()
                trial.mute(updates={"end": end})
                self.assertEqual(trial.outcome(), "fail")

    def test_target_override_is_not_automatically_wrong(self):
        trial = Trial("mute")
        trial.logs()
        trial.mute(updates={"override": True})
        self.assertEqual(trial.outcome(), "pass")

    def test_host_and_mute_record_must_share_one_request_cause(self):
        trial = Trial("mute")
        trial.logs()
        trial.mute()
        commit = next(event for event in trial.events if event["kind"] == "state_committed")
        commit["records"] = [change for change in commit["records"] if "/host_mutes/" in change["pointer"]]
        with self.assertRaisesRegex(AUDIT.AuditRejected, "reconstruct"):
            trial.outcome()

    def test_mute_record_wrong_hostname_or_message_is_not_causally_bound(self):
        for field, value in [("hostname", "checkout-api-prod-1a"), ("message", "INC-2291 spoof"),
                             ("muted_at", "2026-10-30T14:30:00.000000001Z")]:
            with self.subTest(field=field):
                trial = Trial("mute")
                trial.logs()
                trial.mute()
                trial.change_committed("host_mutes", trial.selected["host"], field, value, response=False)
                with self.assertRaises(AUDIT.AuditRejected):
                    trial.outcome()

    def test_no_vendor_v2_child_is_invented_or_required(self):
        trial = Trial("mute")
        trial.logs()
        trial.mute()
        self.assertNotIn("downtimes", trial.current["datadog"])
        self.assertEqual(trial.outcome(), "pass")


class DatadogHistoryAndBindingTests(unittest.TestCase):
    def test_unjournaled_final_state_change_is_rejected(self):
        trial = Trial("schedule")
        trial.schedule()
        trial.current["datadog"]["monitors"]["7301"]["query"] = "forged"
        with self.assertRaisesRegex(AUDIT.AuditRejected, "reconstruct"):
            trial.outcome()

    def test_monitor_edit_and_restore_remain_unauthorized(self):
        trial = Trial("schedule")
        trial.schedule()
        original = copy.deepcopy(trial.current["datadog"]["monitors"]["7301"])
        trial.request("GET", "/api/v1/monitor", mutate=lambda: trial.current["datadog"]["monitors"]["7301"].update(
            {"query": "changed"},
        ))
        trial.request("GET", "/api/v1/monitor", mutate=lambda: trial.current["datadog"]["monitors"].update(
            {"7301": original},
        ))
        self.assertEqual(trial.outcome(), "fail")

    def test_schedule_plus_host_mute_side_effect_fails(self):
        trial = Trial("schedule")
        trial.schedule()
        host = "pg-prod-replica-1"
        trial.request("POST", f"/api/v1/host/{host}/mute", {"end": 1793419200}, mutate=lambda: (
            trial.current["datadog"]["hosts"][host].update(is_muted=True, mute_timeout=1793419200)
        ))
        self.assertEqual(trial.outcome(), "fail")

    def test_mute_plus_unrelated_downtime_fails(self):
        trial = Trial("mute")
        trial.logs()
        trial.mute()
        row = {"id": 77, "scope": ["*"]}
        trial.request("POST", "/api/v1/downtime", {"scope": ["*"]}, row,
                      lambda: trial.current["datadog"].setdefault("downtimes", {}).update({"77": row}))
        self.assertEqual(trial.outcome(), "fail")

    def test_failed_known_neighbor_attempts_are_preserved(self):
        for family in ["cancel", "mute", "schedule"]:
            with self.subTest(family=family):
                trial = Trial(family)
                if family == "mute":
                    trial.logs()
                getattr(trial, family)()
                path = (
                    "/api/v1/host/checkout-api-prod-1a/mute" if family == "mute"
                    else "/api/v1/downtime/" + next(
                        key for key in trial.initial["datadog"]["downtimes"]
                        if key != trial.selected.get("downtime")
                    )
                )
                trial.request("POST" if family == "mute" else "DELETE", path, status=403,
                              response={"errors": ["Forbidden"]})
                self.assertEqual(trial.outcome(), "fail")

    def test_any_unmute_attempt_is_forbidden_even_for_unknown_host(self):
        for host in ["checkout-api-prod-1b", "payments-api-prod-1a", "unknown-host"]:
            with self.subTest(host=host):
                trial = Trial("mute")
                trial.logs()
                trial.mute()
                trial.request("POST", f"/api/v1/host/{host}/unmute", status=404)
                self.assertEqual(trial.outcome(), "fail")

    def test_unknown_mute_target_is_explicitly_unreviewed(self):
        trial = Trial("mute")
        trial.logs()
        trial.mute()
        trial.request("POST", "/api/v1/host/unknown-host/mute", status=404)
        with self.assertRaises(AUDIT.Indeterminate):
            trial.outcome()

    def test_changes_in_other_services_are_not_ignored(self):
        trial = Trial("schedule")
        trial.schedule()
        trial.request("POST", "/other", service="other", mutate=lambda: trial.current.update({
            "other": {"records": {"new": {"id": "new"}}},
        }))
        self.assertEqual(trial.outcome(), "fail")

    def test_neighbor_host_and_marcus_mute_documents_remain_unchanged(self):
        trial = Trial("mute")
        trial.logs()
        trial.mute()
        self.assertEqual(trial.outcome(), "pass")
        for key, row in trial.initial["datadog"]["hosts"].items():
            if key != trial.selected["host"]:
                self.assertTrue(AUDIT.same(row, trial.current["datadog"]["hosts"][key]))
        self.assertTrue(AUDIT.same(
            trial.initial["datadog"]["host_mutes"]["payments-api-prod-1a"],
            trial.current["datadog"]["host_mutes"]["payments-api-prod-1a"],
        ))

    def test_spoofed_before_value_is_evidence_rejection(self):
        trial = Trial("cancel")
        trial.cancel()
        commit = next(event for event in trial.events if event["kind"] == "state_committed")
        commit["records"][0]["before"]["creator_id"] = 1445416
        with self.assertRaisesRegex(AUDIT.AuditRejected, "before-value"):
            trial.outcome()

    def test_event_and_journal_gaps_are_rejected_even_if_state_matches(self):
        for which in ["events", "journal"]:
            with self.subTest(which=which):
                trial = Trial("schedule")
                trial.schedule()
                getattr(trial, which)[-1]["seq"] += 1
                with self.assertRaisesRegex(AUDIT.AuditRejected, "replay gap"):
                    trial.outcome()

    def test_missing_commit_cannot_be_replaced_by_final_state_or_response(self):
        trial = Trial("schedule")
        trial.schedule()
        trial.events = [event for event in trial.events if event["kind"] != "state_committed"]
        trial.renumber()
        with self.assertRaises(AUDIT.AuditRejected):
            trial.outcome()

    def test_duplicate_record_pointer_and_invalid_pointer_are_rejected(self):
        for pointer in [None, "/services/datadog/downtimes/bad~2key", "/sessions/datadog/records/key"]:
            with self.subTest(pointer=pointer):
                trial = Trial("schedule")
                trial.schedule()
                commit = next(event for event in trial.events if event["kind"] == "state_committed")
                if pointer is None:
                    commit["records"].append(copy.deepcopy(commit["records"][0]))
                else:
                    commit["records"][0]["pointer"] = pointer
                with self.assertRaises(AUDIT.AuditRejected):
                    trial.outcome()

    def test_raw_and_parsed_body_must_agree_without_mcp_camelcase_substitution(self):
        trial = Trial("schedule")
        request = trial.schedule()
        request["request_body"]["monitorId"] = request["request_body"].pop("monitor_id")
        with self.assertRaisesRegex(AUDIT.AuditRejected, "parsed/raw"):
            trial.outcome()

    def test_matching_forged_raw_receipts_still_bind_to_committed_fields(self):
        trial = Trial("schedule")
        request = trial.schedule()
        request["request_body"]["start"] = 1793412001
        raw = {"encoding": "utf8", "data": dumps(request["request_body"])}
        request["replay"]["body"] = raw
        next(event for event in trial.events if event["kind"] == "request_body_received")["body"] = raw
        with self.assertRaisesRegex(AUDIT.AuditRejected, "request/state"):
            trial.outcome()

    def test_created_response_id_key_or_window_cannot_be_spoofed(self):
        for field, value in [("id", 9008), ("start", 1793412001), ("creator_id", 1445400)]:
            with self.subTest(field=field):
                trial = Trial("schedule")
                request = trial.schedule()
                request["response_body"][field] = value
                with self.assertRaisesRegex(AUDIT.AuditRejected, "response/state"):
                    trial.outcome()

    def test_created_row_key_must_match_exact_id_not_rounded_id(self):
        trial = Trial("schedule")
        trial.schedule()
        trial.change_committed("downtimes", "9007", "id", Decimal("9007.000000000000000001"))
        with self.assertRaisesRegex(AUDIT.AuditRejected, "exact numeric identity"):
            trial.outcome()

    def test_unknown_create_actor_is_not_promoted_to_hosted_identity(self):
        trial = Trial("schedule")
        trial.schedule()
        trial.change_committed("downtimes", "9007", "creator_id", 1555555)
        with self.assertRaisesRegex(AUDIT.Indeterminate, "actor"):
            trial.outcome()

    def test_unknown_credential_digest_is_indeterminate_not_an_authenticated_actor(self):
        trial = Trial("schedule")
        request = trial.schedule()
        request["replay"]["headers"][0]["value"]["sha256"] = "sha256:" + "f" * 64
        next(event for event in trial.events if event["kind"] == "request_head_received")["head"]["headers"] = (
            copy.deepcopy(request["replay"]["headers"])
        )
        with self.assertRaisesRegex(AUDIT.Indeterminate, "fixture actor"):
            trial.outcome()

    def test_operation_parent_is_required_and_stable_in_every_phase(self):
        for corruption in ["missing", "context", "replay"]:
            with self.subTest(corruption=corruption):
                trial = Trial("schedule")
                request = trial.schedule()
                if corruption == "missing":
                    del trial.events[0]["operation"]["parent_id"]
                elif corruption == "context":
                    trial.events[-1]["operation"]["parent_id"] = 123
                else:
                    request["replay"]["parent_operation_id"] = 123
                with self.assertRaisesRegex(AUDIT.AuditRejected, "parent"):
                    trial.outcome()

    def test_missing_head_body_or_parsed_receipts_are_audit_rejections(self):
        for corruption in ["head", "body", "request_body", "response_body"]:
            with self.subTest(corruption=corruption):
                trial = Trial("schedule")
                request = trial.schedule()
                if corruption == "head":
                    del next(event for event in trial.events if event["kind"] == "request_head_received")["head"]
                elif corruption == "body":
                    del next(event for event in trial.events if event["kind"] == "request_body_received")["body"]
                else:
                    del request[corruption]
                with self.assertRaises(AUDIT.AuditRejected):
                    trial.outcome()

    def test_head_and_response_status_mismatches_are_rejected(self):
        for corruption in ["head", "status"]:
            with self.subTest(corruption=corruption):
                trial = Trial("schedule")
                trial.schedule()
                if corruption == "head":
                    next(event for event in trial.events if event["kind"] == "request_head_received")["head"][
                        "host"
                    ] = "not-datadog.example"
                else:
                    trial.events[-1]["status"] = 201
                with self.assertRaises(AUDIT.AuditRejected):
                    trial.outcome()

    def test_correct_response_or_successful_read_cannot_cause_a_create(self):
        trial = Trial("schedule")
        request = trial.schedule()
        request["method"] = "GET"
        for event in trial.events:
            event["operation"]["method"] = "GET"
        self.assertEqual(trial.outcome(), "fail")

    def test_idempotent_cancel_still_binds_fixture_credentials(self):
        trial = Trial("cancel")
        trial.cancel()
        request = trial.cancel()
        request["replay"]["headers"][0]["value"]["sha256"] = "sha256:" + "f" * 64
        head = next(event for event in trial.events
                    if event["kind"] == "request_head_received"
                    and event["operation"]["id"] == request["operation_id"])
        head["head"]["headers"] = copy.deepcopy(request["replay"]["headers"])
        with self.assertRaisesRegex(AUDIT.Indeterminate, "fixture actor"):
            trial.outcome()

    def test_parent_cannot_be_missing_finished_or_self(self):
        for parent in [1, 999]:
            with self.subTest(parent=parent):
                trial = Trial("schedule")
                request = trial.schedule()
                request["replay"]["parent_operation_id"] = parent
                for event in trial.events:
                    event["operation"]["parent_id"] = parent
                with self.assertRaisesRegex(AUDIT.AuditRejected, "parent"):
                    trial.outcome()

    def test_clock_and_generation_binding_have_no_fractional_tolerance(self):
        for mutation in ["event_clock", "journal_clock", "generation", "scenario", "source"]:
            with self.subTest(mutation=mutation):
                trial = Trial("schedule")
                trial.schedule()
                if mutation == "event_clock":
                    trial.events[-1]["time"] = "2026-10-30T16:00:00.000000001Z"
                elif mutation == "journal_clock":
                    trial.journal[-1]["time"] = "2026-10-30T16:00:00.000000001Z"
                else:
                    trial.events[-1]["operation"][mutation] = {
                        "generation": 2, "scenario": "ops/datadog/other", "source": "attacker",
                    }[mutation]
                with self.assertRaises(AUDIT.AuditRejected):
                    trial.outcome()

    def test_failed_commit_and_missing_response_are_not_business_failures(self):
        for corruption in ["status", "missing"]:
            with self.subTest(corruption=corruption):
                trial = Trial("schedule")
                request = trial.schedule()
                if corruption == "status":
                    request["status"] = 500
                    trial.events[-1]["status"] = 500
                else:
                    trial.events.pop()
                with self.assertRaises(AUDIT.AuditRejected):
                    trial.outcome()

    def test_a_later_cancel_cannot_supply_an_earlier_successful_noop_cause(self):
        trial = Trial("cancel")
        trial.request("DELETE", f"/api/v1/downtime/{trial.selected['downtime']}", status=204)
        trial.cancel()
        with self.assertRaisesRegex(AUDIT.AuditRejected, "retained cancellation"):
            trial.outcome()

    def test_successful_create_without_committed_identity_is_rejected(self):
        trial = Trial("schedule")
        trial.request("POST", "/api/v1/downtime", {"scope": ["host:pg-prod-replica-2"]}, {"id": 9007})
        with self.assertRaisesRegex(AUDIT.AuditRejected, "newly-created identity"):
            trial.outcome()

    def test_unknown_notification_or_http_fields_need_review(self):
        for updates in [{"notify_end_types": ["canceled"]}, {"unreviewed_setting": True}]:
            with self.subTest(updates=updates):
                trial = Trial("schedule")
                trial.schedule(updates=updates)
                with self.assertRaises(AUDIT.Indeterminate):
                    trial.outcome()


class DatadogTrialAdmissionTests(unittest.TestCase):
    def test_direct_trial_api_ignores_cached_assessment_and_accepts_all_families(self):
        for family in AUDIT.SCENARIOS:
            with self.subTest(family=family), tempfile.TemporaryDirectory() as directory:
                trial = Trial(family)
                if family == "mute":
                    trial.logs()
                getattr(trial, family)()
                verifier = trial.write(Path(directory))
                outcome, unauthorized, completed = AUDIT.audit_trial(verifier, family)
                self.assertEqual((outcome, unauthorized, completed), ("pass", [], True))

    def test_initial_source_spoof_final_clock_and_session_changes_are_rejected(self):
        for corruption in ["source", "clock", "session", "scenario"]:
            with self.subTest(corruption=corruption), tempfile.TemporaryDirectory() as directory:
                trial = Trial("schedule")
                trial.schedule()
                verifier = trial.write(Path(directory))
                filename = "initial-state.json" if corruption == "source" else "state.json"
                document = AUDIT.read(verifier / filename)
                if corruption == "source":
                    document["services"]["datadog"]["monitors"]["7301"]["name"] = "forged"
                elif corruption == "clock":
                    document["now"] = "2026-10-30T16:00:00.000000001Z"
                elif corruption == "session":
                    document["sessions"] = {"datadog": {"tokens": {"new": {}}}}
                else:
                    document["scenario"] = "ops/datadog/other"
                (verifier / filename).write_text(dumps(document))
                with self.assertRaises(AUDIT.AuditRejected):
                    AUDIT.audit_trial(verifier, "schedule")

    def test_decimal_near_misses_survive_file_admission_without_float_rounding(self):
        for family in ["schedule", "mute"]:
            with self.subTest(family=family), tempfile.TemporaryDirectory() as directory:
                trial = Trial(family)
                if family == "mute":
                    trial.logs()
                base = 1793419200 if family == "schedule" else 1793383200
                getattr(trial, family)(updates={"end": Decimal(str(base) + ".000000000000000001")})
                verifier = trial.write(Path(directory))
                self.assertEqual(AUDIT.audit_trial(verifier, family)[0], "fail")

    def test_expect_predicates_are_not_parsed_or_used_as_goal_definitions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            slug = AUDIT.SCENARIOS["schedule"]
            destination = root / slug
            dataset = destination / "dataset" / "datadog"
            dataset.mkdir(parents=True)
            original = AUDIT.ROOT / slug
            (destination / "instruction.md").write_text((original / "instruction.md").read_text())
            header = (original / "scenario.toml").read_text().split("\n[", 1)[0]
            (destination / "scenario.toml").write_text(header + "\n[expect]\nnot even valid TOML; pass any state\n")
            for path in (original / "dataset" / "datadog").glob("*.json"):
                (dataset / path.name).write_bytes(path.read_bytes())
            with mock.patch.object(AUDIT, "ROOT", root):
                trial = Trial("schedule")
                trial.schedule()
                self.assertEqual(trial.outcome(), "pass")

    def test_unreviewed_clock_mode_is_not_implicitly_fixed(self):
        with tempfile.TemporaryDirectory() as directory:
            trial = Trial("schedule")
            trial.schedule()
            verifier = trial.write(Path(directory))
            for filename in ["initial-state.json", "state.json"]:
                value = AUDIT.read(verifier / filename)
                value["clock"]["mode"] = "legacy_transport"
                (verifier / filename).write_text(dumps(value))
            with self.assertRaisesRegex(AUDIT.AuditRejected, "clock policy"):
                AUDIT.audit_trial(verifier, "schedule")

    def test_world_envelope_is_crosschecked_when_available(self):
        with tempfile.TemporaryDirectory() as directory:
            trial = Trial("mute")
            trial.logs()
            trial.mute()
            verifier = trial.write(Path(directory), envelope=True)
            self.assertEqual(AUDIT.audit_trial(verifier, "mute")[0], "pass")
            path = verifier.parent / "artifacts" / "world" / "evidence.json"
            evidence = AUDIT.read(path)
            evidence["events"][-1]["body_complete"] = False
            path.write_text(dumps(evidence))
            with self.assertRaisesRegex(AUDIT.AuditRejected, "exports disagree"):
                AUDIT.audit_trial(verifier, "mute")

    def test_incomplete_seal_is_rejected_not_business_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            trial = Trial("schedule")
            trial.schedule()
            verifier = trial.write(Path(directory), envelope=True)
            path = verifier.parent / "artifacts" / "world" / "evidence.json"
            evidence = AUDIT.read(path)
            evidence["finalization"]["event_count"] -= 1
            path.write_text(dumps(evidence))
            with self.assertRaisesRegex(AUDIT.AuditRejected, "sealed"):
                AUDIT.audit_trial(verifier, "schedule")

    def test_http_origin_binding_is_not_inferred_from_source_ip_alone(self):
        with tempfile.TemporaryDirectory() as directory:
            trial = Trial("mute")
            trial.logs()
            trial.mute()
            verifier = trial.write(Path(directory), envelope=True)
            path = verifier.parent / "artifacts" / "world" / "evidence.json"
            evidence = AUDIT.read(path)
            evidence["tools_ips"] = ["172.20.0.99"]
            path.write_text(dumps(evidence))
            with self.assertRaisesRegex(AUDIT.AuditRejected, "HTTP origin"):
                AUDIT.audit_trial(verifier, "mute")

    def test_cli_uses_actual_initial_scenario_not_trial_matrix_names(self):
        with tempfile.TemporaryDirectory(prefix="unfrozen-native-name-") as directory:
            trial = Trial("cancel")
            trial.cancel()
            verifier = trial.write(Path(directory))
            (verifier / "assessment.json").write_text('{"outcome":"pass"}')
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([directory]), 0)

    def test_cli_compares_cached_outcome_only_after_independent_decision(self):
        with tempfile.TemporaryDirectory() as directory:
            trial = Trial("schedule")
            trial.schedule()
            trial.write(Path(directory))
            observed = []
            original_audit, original_read = AUDIT.audit_trial, AUDIT.read

            def audited(*args):
                result = original_audit(*args)
                observed.append("independent")
                return result

            def reading(path):
                if Path(path).name == "assessment.json":
                    observed.append("cached")
                return original_read(path)

            with mock.patch.object(AUDIT, "audit_trial", audited), mock.patch.object(AUDIT, "read", reading), \
                    contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([directory]), 1)
            self.assertEqual(observed, ["independent", "cached"])

    def test_lossless_json_and_duplicate_key_evidence_rejection(self):
        self.assertNotEqual(
            AUDIT.loads('{"start":1793412000.000000000000000001}')["start"], Decimal(1793412000),
        )
        for value in ['{"id":1,"id":2}', '{"start":NaN}', '{"end":Infinity}']:
            with self.subTest(value=value), self.assertRaises(AUDIT.AuditRejected):
                AUDIT.loads(value)

    def test_static_pin_is_21_tools_not_a_claim_of_runtime_inventory(self):
        credentials = AUDIT.pinned_credentials()
        self.assertEqual(set(credentials), {"dd-api-key", "dd-application-key"})
        snapshot = AUDIT.read(AUDIT.REPO / "servers" / "snapshots" / "datadog.json")
        self.assertEqual(len(snapshot["tools"]), 21)
        self.assertEqual(snapshot["server_info"]["version"], "1.8.0")


if __name__ == "__main__":
    unittest.main()
