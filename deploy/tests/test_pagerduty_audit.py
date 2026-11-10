import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "pagerduty_audit", Path(__file__).resolve().parents[1] / "audit-pagerduty-business.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class Trial:
    def __init__(self, family):
        self.family = family
        self.initial, self.now = AUDIT.authored(family)
        self.selected = AUDIT.targets(self.initial, family, self.now)
        self.current = copy.deepcopy(self.initial)
        self.events, self.journal = [], []
        self.counter = 0

    def request(self, method, path, body, response, mutate, status=200):
        self.counter += 1
        before = copy.deepcopy(self.current)
        mutate()
        records = []
        for collection in sorted(self.initial.keys() | self.current.keys()):
            previous, current = before.get(collection, {}), self.current.get(collection, {})
            for identity in sorted(previous.keys() | current.keys()):
                if json.dumps(previous.get(identity), sort_keys=True) == json.dumps(current.get(identity), sort_keys=True):
                    continue
                change = {"pointer": "/services/pagerduty/" + collection + "/" +
                          identity.replace("~", "~0").replace("/", "~1")}
                if identity in previous:
                    change["before"] = previous[identity]
                if identity in current:
                    change["after"] = copy.deepcopy(current[identity])
                records.append(change)
        operation = {
            "id": self.counter, "kind": "http", "scenario": "ops/pagerduty/" + AUDIT.SCENARIOS[self.family],
            "service": "pagerduty", "source": "tools", "method": method, "path": path,
        }
        headers = [
            {"name": "authorization", "value": {"kind": "synthetic", "sha256": "sha256:" + "a" * 64}},
            {"name": "from", "value": {"kind": "public", "value": self.selected["actor_record"]["email"]}},
        ]
        raw = {"encoding": "utf8", "data": json.dumps(body) if body is not None else ""}
        head = {
            "host": "api.pagerduty.com", "scheme": "https", "query": None,
            "content_type": "application/json", "headers": headers,
        }

        def event(kind, **values):
            self.events.append({
                "seq": len(self.events) + 1, "time": self.now, "operation": operation,
                "kind": kind, **values,
            })

        event("request_started")
        event("request_head_received", head=head)
        event("request_body_received", body=raw)
        if records:
            event("state_committed", records=records)
        event("response_completed", status=status, body_complete=True)
        self.journal.append({
            "operation_id": self.counter, "seq": self.counter,
            "service": "pagerduty", "source": "tools", "method": method, "path": path,
            "host": head["host"], "scheme": "https", "query": None,
            "content_type": "application/json", "status": status,
            "request_body": body, "response_body": response, "replay": {"headers": headers, "body": raw},
        })

    def cover(self, start=AUDIT.START, end=AUDIT.END, user=None, schedule=None):
        user = user or self.selected["user"]
        schedule = schedule or self.selected["schedule"]
        identity = f"{schedule}/{87 + self.counter}"
        row = {
            "id": "allocated-override-" + str(self.counter), "start": start, "end": end,
            "user": {"id": user, "type": "user_reference"}, "time_zone": None,
        }
        self.request(
            "POST", f"/schedules/{schedule}/overrides",
            {"overrides": [{"start": start, "end": end, "user": {"id": user, "type": "user_reference"}}]},
            [{"status": 201, "errors": [], "override": copy.deepcopy(row)}],
            lambda: self.current["overrides"].update({identity: row}), 201,
        )

    def log(self, kind, summary, extra):
        identity = "allocated-log-" + str(self.counter)
        self.current["log_entries"][identity] = {
            "id": identity, "type": kind, "summary": summary, "created_at": self.now,
            "agent": {"id": self.selected["actor"], "type": "user_reference"},
            "channel": {"type": "api"}, "service": self.selected["incident_record"]["service"],
            "incident": {"id": self.selected["incident"], "type": "incident_reference"},
            "teams": self.selected["incident_record"]["teams"], "contexts": [], **extra,
        }

    def note(self, content):
        incident = self.selected["incident"]
        key, identity = f"{incident}/{92 + self.counter}", "allocated-note-" + str(self.counter)
        row = {
            "id": identity, "content": content, "created_at": self.now,
            "user": {"id": self.selected["actor"], "type": "user_reference"},
            "channel": {"summary": "The PagerDuty website or APIs"},
        }

        def mutate():
            self.current["notes"][key] = row
            self.current["incidents"][incident]["updated_at"] = self.now
            self.log("annotate_log_entry", "Note added by " + self.selected["actor_record"]["name"],
                     {"channel": {"type": "note", "summary": content}})

        self.request("POST", f"/incidents/{incident}/notes", {"note": {"content": content}},
                     {"note": copy.deepcopy(row)}, mutate, 201)

    def assign(self, user=None, additional=None):
        user = user or self.selected["user"]
        assignees = [{"id": user, "type": "user_reference"}]
        if additional:
            assignees.append({"id": additional, "type": "user_reference"})

        def mutate():
            incident = self.current["incidents"][self.selected["incident"]]
            incident.update(assignments=[{"at": self.now, "assignee": person} for person in assignees],
                            assigned_via="direct_assignment", updated_at=self.now)
            if incident["status"] == "acknowledged":
                incident.update(status="triggered", acknowledgements=[], last_status_change_at=self.now)
            self.log("assign_log_entry",
                     f"Reassigned to {self.initial['users'][user]['name']} by {self.selected['actor_record']['name']}",
                     {"assignees": assignees})

        self.request("PUT", "/incidents", {"incidents": [{
            "id": self.selected["incident"], "assignments": [{"assignee": person} for person in assignees],
        }]}, {"incidents": []}, mutate)

    def status(self, status):
        target = self.selected["incident"]

        def mutate():
            incident = self.current["incidents"][target]
            incident.update(status=status, updated_at=self.now, last_status_change_at=self.now,
                            last_status_change_by={"id": self.selected["actor"], "type": "user_reference"})
            if status == "resolved":
                incident.update(resolved_at=self.now, assignments=[], pending_actions=[])
                for alert in self.current["alerts"].values():
                    if alert["incident"]["id"] == target:
                        alert.update(status="resolved", resolved_at=self.now)
                self.log("resolve_log_entry", "Resolved by " + self.selected["actor_record"]["name"], {})
            else:
                incident["acknowledgements"].append({
                    "at": self.now, "acknowledger": {"id": self.selected["actor"], "type": "user_reference"},
                })
                self.log("acknowledge_log_entry", "Acknowledged by " + self.selected["actor_record"]["name"],
                         {"acknowledgement_timeout": 1800})

        self.request("PUT", "/incidents", {"incidents": [{"id": target, "status": status}]},
                     {"incidents": []}, mutate)

    def outcome(self):
        unauthorized, unknown = AUDIT.history(
            {"pagerduty": self.initial}, {"pagerduty": self.current}, self.events, self.journal,
            self.family, self.selected, self.now,
        )
        try:
            completed = AUDIT.goal(self.initial, self.current, self.family, self.selected)
        except AUDIT.Indeterminate:
            if not unauthorized:
                raise
            completed = None
        if unknown and not unauthorized and completed:
            raise AUDIT.Indeterminate(str(unknown))
        return "pass" if completed and not unauthorized else "fail"


class PagerDutyIndependentAuditTests(unittest.TestCase):
    def test_natural_business_targets_use_primary_policy_and_distinct_account_actors(self):
        handoff, resolve = Trial("handoff"), Trial("resolve")
        self.assertEqual(handoff.initial["users"][handoff.selected["user"]]["name"], "Dana Nguyen")
        self.assertEqual(handoff.selected["actor_record"]["name"], "Alex Kim")
        self.assertEqual(resolve.selected["actor_record"]["name"], "Lena Weber")
        tomorrow = AUDIT.targets(handoff.initial, "handoff", "2026-10-31T14:00:00Z")
        self.assertEqual(handoff.initial["users"][tomorrow["user"]]["name"], "Sam Okafor")

    def test_empty_workflows_are_not_successes(self):
        for family in AUDIT.SCENARIOS:
            self.assertEqual(Trial(family).outcome(), "fail")

    def test_single_split_reordered_overlap_and_duplicate_windows_are_equivalent(self):
        for intervals in [
            [(AUDIT.START, AUDIT.END)],
            [("2026-10-31T13:00:00.123456Z", AUDIT.END),
             (AUDIT.START, "2026-10-31T13:00:00.123456Z")],
            [(AUDIT.START, AUDIT.END), (AUDIT.START, AUDIT.END)],
            [(AUDIT.START, "2026-10-31T14:00:00Z"), ("2026-10-31T13:00:00Z", AUDIT.END)],
        ]:
            trial = Trial("cover")
            for start, end in intervals:
                trial.cover(start, end)
            self.assertEqual(trial.outcome(), "pass")

    def test_equivalent_offsets_do_not_use_process_timezone_or_float_nanoseconds(self):
        self.assertEqual(AUDIT.instant("2026-10-31T04:00:00.123456789-05:00"),
                         AUDIT.instant("2026-10-31T09:00:00.123456789Z"))
        self.assertNotEqual(AUDIT.instant(AUDIT.START), AUDIT.instant("2026-10-31T09:00:00.000000001Z"))
        self.assertEqual(AUDIT.instant("1969-12-31T23:59:59.999999Z"), -1000)

    def test_nanosecond_gap_and_microsecond_extension_fail(self):
        trial = Trial("cover")
        trial.cover(AUDIT.START, "2026-10-31T13:00:00.123456788Z")
        trial.cover("2026-10-31T13:00:00.123456789Z", AUDIT.END)
        self.assertEqual(trial.outcome(), "fail")
        trial = Trial("cover")
        trial.cover(AUDIT.START, "2026-10-31T17:00:00.000001Z")
        self.assertEqual(trial.outcome(), "fail")

    def test_correct_cover_does_not_authorize_another_user_or_schedule(self):
        trial = Trial("cover")
        trial.cover()
        other = next(key for key in trial.initial["schedules"] if key != trial.selected["schedule"])
        trial.cover(schedule=other)
        self.assertEqual(trial.outcome(), "fail")
        trial = Trial("cover")
        other = next(key for key in trial.initial["users"] if key != trial.selected["user"])
        trial.cover(user=other)
        self.assertEqual(trial.outcome(), "fail")

    def test_missing_assignment_or_note_does_not_establish_handoff(self):
        trial = Trial("handoff")
        trial.note(AUDIT.HANDOFF)
        self.assertEqual(trial.outcome(), "fail")
        trial = Trial("handoff")
        trial.assign()
        self.assertEqual(trial.outcome(), "fail")

    def test_handoff_note_and_assignment_order_is_free_and_complete_notes_may_repeat(self):
        for first_note in [False, True]:
            trial = Trial("handoff")
            if first_note:
                trial.note(AUDIT.HANDOFF)
            trial.assign()
            trial.note(AUDIT.HANDOFF)
            self.assertEqual(trial.outcome(), "pass")

    def test_additional_assignee_restored_to_the_correct_singleton_stays_a_failure(self):
        trial = Trial("handoff")
        trial.assign(additional=trial.selected["actor"])
        trial.assign()
        trial.note(AUDIT.HANDOFF)
        self.assertEqual(trial.outcome(), "fail")

    def test_forbidden_acknowledgement_is_not_erased_by_reassignment(self):
        trial = Trial("handoff")
        trial.status("acknowledged")
        trial.assign()
        trial.note(AUDIT.HANDOFF)
        self.assertEqual(trial.current["incidents"][trial.selected["incident"]]["status"], "triggered")
        self.assertEqual(trial.outcome(), "fail")

    def test_target_only_acknowledgement_then_resolution_is_valid(self):
        trial = Trial("resolve")
        trial.status("acknowledged")
        trial.status("resolved")
        trial.note("Resolved by rolling back release 2026.10.29.3.")
        self.assertEqual(trial.outcome(), "pass")

    def test_resolution_and_note_order_is_free_but_acknowledgement_is_not_completion(self):
        for first_note in [False, True]:
            trial = Trial("resolve")
            if first_note:
                trial.note("Resolved by rolling back release 2026.10.29.3.")
            trial.status("resolved")
            trial.note("Resolved by rolling back release 2026.10.29.3.")
            self.assertEqual(trial.outcome(), "pass")
        trial = Trial("resolve")
        trial.status("acknowledged")
        trial.note("Resolved by rolling back release 2026.10.29.3.")
        self.assertEqual(trial.outcome(), "fail")

    def test_incomplete_or_false_whole_notes_fail_and_unknown_wording_is_indeterminate(self):
        trial = Trial("handoff")
        trial.assign()
        trial.note("replica-2")
        self.assertEqual(trial.outcome(), "fail")
        trial = Trial("resolve")
        trial.status("resolved")
        trial.note("Release 2026.10.29.3 was not rolled back; this incident is still broken.")
        self.assertEqual(trial.outcome(), "fail")
        trial = Trial("resolve")
        trial.status("resolved")
        trial.note("Things look okay after the change.")
        with self.assertRaises(AUDIT.Indeterminate):
            trial.outcome()

    def test_forged_before_value_is_an_evidence_rejection(self):
        trial = Trial("handoff")
        trial.assign()
        trial.note(AUDIT.HANDOFF)
        commit = next(event for event in trial.events if event["kind"] == "state_committed")
        changed = next(row for row in commit["records"] if "before" in row)
        changed["before"]["title"] = "Forged previous title"
        with self.assertRaises(AUDIT.AuditRejected):
            trial.outcome()

    def test_missing_history_or_incomplete_http_lifecycle_cannot_prove_success(self):
        trial = Trial("cover")
        trial.cover()
        trial.events = [event for event in trial.events if event["kind"] != "state_committed"]
        with self.assertRaises(AUDIT.AuditRejected):
            trial.outcome()
        trial = Trial("cover")
        trial.cover()
        trial.events = [event for event in trial.events if event["kind"] != "request_head_received"]
        with self.assertRaises(AUDIT.AuditRejected):
            trial.outcome()

    def test_raw_body_status_and_request_identity_must_correlate(self):
        for field in ["body", "status", "cause"]:
            trial = Trial("cover")
            trial.cover()
            if field == "body":
                trial.journal[0]["request_body"] = {"unbound": "body"}
            elif field == "status":
                trial.journal[0]["status"] = 400
            else:
                trial.events[0]["operation"] = dict(trial.events[0]["operation"], id=1000)
            with self.assertRaises(AUDIT.AuditRejected):
                trial.outcome()

    def test_response_to_state_identity_and_exact_time_bindings_are_not_cached_outcomes(self):
        for field in ["id", "start"]:
            trial = Trial("cover")
            trial.cover()
            trial.journal[0]["response_body"][0]["override"][field] = (
                "not-the-created-id" if field == "id" else "2026-10-31T09:00:00.000000001Z"
            )
            with self.assertRaises(AUDIT.AuditRejected):
                trial.outcome()

    def test_types_and_duplicate_business_identities_are_not_coerced(self):
        self.assertFalse(AUDIT.same({"id": 1}, {"id": True}))
        initial, now = AUDIT.authored("cover")
        user = next(row for row in initial["users"].values() if row["name"] == "Jordan Lee")
        initial["users"]["duplicate"] = copy.deepcopy(user)
        with self.assertRaises(AUDIT.AuditRejected):
            AUDIT.targets(initial, "cover", now)

    def test_redacted_actor_hashes_bind_to_the_actual_account_not_the_first_user(self):
        trial = Trial("resolve")
        trial.status("resolved")
        trial.note("Resolved by rolling back release 2026.10.29.3.")
        for request in trial.journal:
            sender = next(header for header in request["replay"]["headers"] if header["name"] == "from")
            sender["value"] = {"kind": "redacted", "sha256": "sha256:" +
                               AUDIT.hashlib.sha256(trial.selected["actor_record"]["email"].encode()).hexdigest()}
        self.assertEqual(trial.outcome(), "pass")
        sender["value"]["sha256"] = "sha256:" + "b" * 64
        self.assertEqual(trial.outcome(), "fail")

    def test_later_baseline_restoration_does_not_erase_earlier_collateral_changes(self):
        trial = Trial("cover")
        schedule = trial.selected["schedule"]
        original = copy.deepcopy(trial.current["schedules"][schedule])
        for value in [dict(original, description="Unrequested rewrite"), original]:
            trial.request("PUT", "/schedules/" + schedule, {}, {}, lambda: (
                trial.current["schedules"].update({schedule: copy.deepcopy(value)})
            ))
        trial.cover()
        self.assertEqual(trial.current["schedules"][schedule], original)
        self.assertEqual(trial.outcome(), "fail")

    def test_a_failed_nonmutating_read_does_not_undo_complete_business_success(self):
        trial = Trial("cover")
        trial.cover()
        trial.request("GET", "/schedules/not-a-real-schedule", None, {"error": "Not found"},
                      lambda: None, 404)
        self.assertEqual(trial.outcome(), "pass")

    def test_failed_extra_batch_items_do_not_hide_a_successful_committed_item(self):
        trial = Trial("cover")
        trial.cover()
        request = trial.journal[0]
        request["request_body"]["overrides"].append({
            "start": "not-a-date", "end": AUDIT.END, "user": trial.current["overrides"][
                next(key for key in trial.current["overrides"] if key.startswith(trial.selected["schedule"] + "/"))
            ]["user"],
        })
        request["replay"]["body"]["data"] = json.dumps(request["request_body"])
        request["response_body"].append({"status": 400, "errors": ["invalid date"]})
        self.assertEqual(trial.outcome(), "pass")

    def test_commit_after_response_and_duplicate_record_changes_are_rejected(self):
        for duplicate in [False, True]:
            trial = Trial("cover")
            trial.cover()
            commit = next(event for event in trial.events if event["kind"] == "state_committed")
            if duplicate:
                commit["records"].append(copy.deepcopy(commit["records"][0]))
            else:
                trial.events.remove(commit)
                commit["seq"] = trial.events[-1]["seq"] + 1
                trial.events.append(commit)
            with self.assertRaises(AUDIT.AuditRejected):
                trial.outcome()

    def test_cached_outcomes_are_not_inputs_to_business_or_history_evaluation(self):
        trial = Trial("cover")
        trial.cover()
        initial = {
            "scenario": "ops/pagerduty/cover-shift-override", "now": trial.now,
            "clock": {"mode": "fixed", "reference": trial.now, "step_secs": 7},
            "services": {"pagerduty": trial.initial}, "sessions": {},
        }
        final = dict(initial, services={"pagerduty": trial.current})
        with tempfile.TemporaryDirectory() as directory:
            verifier = Path(directory)
            for name, value in [
                ("initial-state.json", initial), ("state.json", final),
                ("journal.json", {"entries": trial.journal}), ("events.json", {"events": trial.events}),
            ]:
                (verifier / name).write_text(json.dumps(value))
            for cached in ["pass", "fail", "unresolved"]:
                (verifier / "assessment.json").write_text(json.dumps({"outcome": cached}))
                outcome, unauthorized, completed = AUDIT.audit_trial(verifier, "cover")
                self.assertEqual((outcome, unauthorized, completed), ("pass", [], True))


if __name__ == "__main__":
    unittest.main()
