import contextlib
import copy
import importlib.util
import io
import json
import shutil
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "google_business_audit", Path(__file__).resolve().parents[1] / "audit-google-business.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)
URI = "https://meet.google.com/abc-defg-hij"


class Trial:
    def __init__(self, family):
        self.family = family
        baseline, self.now = AUDIT.authored(family)
        self.initial = {"google": baseline}
        self.current = copy.deepcopy(self.initial)
        self.selected = AUDIT.targets(baseline, family)
        self.events, self.journal = [], []
        self.actor = "172.20.0.3"

    def tool(self, name, arguments, mutate=None):
        previous = copy.deepcopy(self.current)
        if mutate:
            mutate()
        changes = []
        for service in sorted(previous.keys() | self.current.keys()):
            for collection in sorted(previous.get(service, {}).keys() | self.current.get(service, {}).keys()):
                old = previous.get(service, {}).get(collection, {})
                new = self.current.get(service, {}).get(collection, {})
                for identity in sorted(old.keys() | new.keys()):
                    if AUDIT.same(old.get(identity), new.get(identity)):
                        continue
                    escaped = identity.replace("~", "~0").replace("/", "~1")
                    record = {"pointer": f"/services/{service}/{collection}/{escaped}"}
                    if identity in old:
                        record["before"] = copy.deepcopy(old[identity])
                    if identity in new:
                        record["after"] = copy.deepcopy(new[identity])
                    changes.append(record)
        identity = len(self.events) + 1
        body = {"name": name, "arguments": arguments}
        raw = {"encoding": "utf8", "data": json.dumps(body)}
        head = {"host": "world:8130", "scheme": "http", "query": None,
                "content_type": "application/json", "headers": []}
        operation = {
            "id": identity, "generation": 1, "parent_id": None, "kind": "http",
            "scenario": "collab/google/" + AUDIT.SCENARIOS[self.family],
            "service": "google", "source": self.actor, "method": "POST", "path": "/_mcp/tools/call",
        }
        self.journal.append({
            "seq": len(self.journal) + 1, "operation_id": identity, "time": self.now,
            "service": "google", "source": self.actor, "method": "POST", "path": "/_mcp/tools/call",
            **{field: head[field] for field in ("host", "scheme", "query", "content_type")},
            "status": 200, "request_body": copy.deepcopy(body),
            "response_body": {"content": [{"type": "text", "text": "Local fixture result"}]},
            "replay": {"parent_operation_id": None, "headers": [], "body": raw},
        })

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
        event("response_completed", status=200, body_complete=True)

    def book(self):
        return self.current["google"]["drive_files"][self.selected["file"]]

    def row(self):
        return self.book()["sheets"][self.selected["tab"]]["values"][self.selected["row"]]

    def renew(self, value="2027-11-30", status="Renewed"):
        def mutate():
            self.row()[2:4] = [value, status]
            self.book()["modifiedTime"] = self.now
            self.book()["lastModifyingUser"] = {"displayName": "Ops Team", "emailAddress": AUDIT.ACTOR}
        self.tool("modify_sheet_values", {
            "spreadsheet_id": self.selected["file"], "range_name": "Renewals!C2:D2",
            "values": [[value, status]],
        }, mutate)

    def share(self, role="reader", recipient="dana.whitfield@contoso.example"):
        def mutate():
            self.book()["permissions"].append({
                "id": "new-direct-permission", "emailAddress": recipient, "type": "user", "role": role,
            })
            self.book()["shared"] = True
        self.tool("manage_drive_access", {
            "file_id": self.selected["file"], "action": "grant", "role": role,
            "share_with": recipient, "share_type": "user",
        }, mutate)

    def meeting(self, identity="actual-event", changes=None, mode="all", meet=True, ledger=True):
        event = {
            "id": identity, "calendarId": AUDIT.ACTOR, "summary": "Q1 planning sync",
            "creator": {"email": AUDIT.ACTOR}, "organizer": {"email": AUDIT.ACTOR},
            "status": "confirmed", "eventType": "default",
            "start": {"dateTime": "2026-11-03T14:00:00-05:00", "timeZone": "America/New_York"},
            "end": {"dateTime": "2026-11-03T14:45:00-05:00", "timeZone": "America/New_York"},
            "attendees": [{"email": guest} for guest in sorted(self.selected["guests"])],
        }
        if changes:
            event.update(copy.deepcopy(changes))

        def mutate():
            self.current["google"]["calendar_events"][identity] = copy.deepcopy(event)
            if meet:
                self.add_conference(identity)
            if ledger:
                self.notification(identity, mode)
        self.tool("manage_event", {"action": "create", "send_updates": mode}, mutate)
        return identity

    def add_conference(self, identity):
        event = self.current["google"]["calendar_events"][identity]
        event["hangoutLink"] = URI
        event["conferenceData"] = {
            "conferenceId": "actual-conference", "entryPoints": [{"entryPointType": "video", "uri": URI}],
        }
        self.current["google"].setdefault("calendar_conferences", {})["actual-conference"] = {
            "id": "actual-conference", "uri": URI, "solution": "hangoutsMeet", "status": "success",
            "created_for": identity, "actor": AUDIT.ACTOR,
        }

    def notification(self, identity, mode):
        event = self.current["google"]["calendar_events"][identity]
        requests = self.current["google"].setdefault("calendar_notification_requests", {})
        request_id = "notification-" + str(len(requests) + 1)
        requests[request_id] = {
            "id": request_id, "eventId": identity, "attendees": sorted({
                AUDIT.principal(guest["email"]) for guest in event["attendees"]
            }), "mode": mode, "delivery": "unobserved", "actor": AUDIT.ACTOR,
            "summary": event["summary"], "start": event["start"]["dateTime"],
            "end": event["end"]["dateTime"], "conferenceUri": event.get("hangoutLink", ""),
        }

    def verdict(self):
        unauthorized = AUDIT.history(
            self.initial, self.current, self.events, self.journal, self.family, self.now, {self.actor},
        )
        complete = AUDIT.goal(self.initial["google"], self.current["google"], self.family)
        return "pass" if complete and not unauthorized else "fail"

    def export(self, directory):
        directory.mkdir(parents=True, exist_ok=True)
        base = {
            "scenario": "collab/google/" + AUDIT.SCENARIOS[self.family], "now": self.now,
            "clock": {"mode": "fixed", "reference": self.now, "step_secs": 0}, "sessions": {},
        }
        for name, value in {
            "initial-state.json": {**base, "services": self.initial},
            "state.json": {**base, "services": self.current},
            "events.json": {"events": self.events},
            "journal.json": {"entries": self.journal},
        }.items():
            (directory / name).write_text(json.dumps(value))


class GoogleSourceAndValueTests(unittest.TestCase):
    def test_three_source_families_are_reviewed_and_keep_authored_time(self):
        for family in AUDIT.SCENARIOS:
            baseline, now = AUDIT.authored(family)
            self.assertEqual(now, "2026-10-29T15:00:00Z")
            self.assertTrue(AUDIT.targets(baseline, family))

    def test_business_input_does_not_read_evaluator_expectations(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / AUDIT.SCENARIOS["renew"]
            shutil.copytree(AUDIT.ROOT / AUDIT.SCENARIOS["renew"], target)
            scenario = target / "scenario.toml"
            scenario.write_text(scenario.read_text().split("\n[expect]", 1)[0] + "\n[expect]\ninvalid = true\n")
            with mock.patch.object(AUDIT, "ROOT", Path(directory)):
                self.assertTrue(AUDIT.targets(AUDIT.authored("renew")[0], "renew"))

    def test_changed_business_instruction_is_not_silently_admitted(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / AUDIT.SCENARIOS["plan"]
            shutil.copytree(AUDIT.ROOT / AUDIT.SCENARIOS["plan"], target)
            (target / "instruction.md").write_text("Email everyone the final link.")
            with mock.patch.object(AUDIT, "ROOT", Path(directory)):
                with self.assertRaises(AUDIT.AuditRejected):
                    AUDIT.authored("plan")

    def test_numeric_serials_and_reviewed_date_text_are_equivalent(self):
        expected = AUDIT.date(2027, 11, 30)
        for value in [46721, Decimal("46721.000"), "2027-11-30", "11/30/2027", "30/11/2027"]:
            self.assertTrue(AUDIT.date_value(value, expected), value)

    def test_raw_numeric_text_formulas_booleans_and_fractional_dates_do_not_pass(self):
        for value in ["46721", '=DATE(2027,11,30)', True, None, Decimal("46721.000000001"),
                      "2027-11-31", "#ERROR!", ""]:
            self.assertFalse(AUDIT.date_value(value, AUDIT.date(2027, 11, 30)), value)

    def test_ambiguous_and_unreviewed_dates_are_indeterminate(self):
        for value in ["11/12/2027", "next Tuesday"]:
            with self.assertRaises(AUDIT.Indeterminate):
                AUDIT.date_value(value, AUDIT.date(2027, 11, 30))

    def test_exact_instants_are_not_microsecond_or_float_rounded(self):
        self.assertEqual(AUDIT.instant_ns("2026-11-03T14:00:00-05:00"),
                         AUDIT.instant_ns("2026-11-03T19:00:00Z"))
        self.assertEqual(AUDIT.instant_ns("2026-11-03T19:00:00.000000001Z")
                         - AUDIT.instant_ns("2026-11-03T19:00:00Z"), 1)

    def test_meet_uri_authorities_codes_and_credentials_are_checked(self):
        self.assertEqual(AUDIT.meet_uri(URI + "?authuser=0"), URI)
        for value in ["http://meet.google.com/abc-defg-hij", URI + "/extra",
                      "https://meet.google.com.attacker.example/abc-defg-hij",
                      "https://user@meet.google.com/abc-defg-hij", "https://meet.google.com:444/abc-defg-hij"]:
            self.assertIsNone(AUDIT.meet_uri(value), value)

    def test_email_case_is_ascii_only_not_unicode_homoglyph_folding(self):
        self.assertEqual(AUDIT.principal("DANA.WHITFIELD@CONTOSO.EXAMPLE"), "dana.whitfield@contoso.example")
        self.assertIsNone(AUDIT.principal("dana.whitfield@contoſo.example"))


class GoogleRenewalHistoryTests(unittest.TestCase):
    def test_normal_and_equivalent_serial_renewals_pass(self):
        for value in ["2027-11-30", 46721, "30/11/2027"]:
            trial = Trial("renew")
            trial.renew(value)
            self.assertEqual(trial.verdict(), "pass")

    def test_blank_then_fill_is_a_valid_staged_plan(self):
        trial = Trial("renew")
        trial.renew("", "")
        trial.renew()
        self.assertEqual(trial.verdict(), "pass")

    def test_a_wrong_intermediate_date_is_not_erased_by_restoration(self):
        trial = Trial("renew")
        trial.renew("2040-01-01")
        trial.renew()
        self.assertEqual(trial.verdict(), "fail")

    def test_a_wrong_intermediate_status_is_not_erased_by_restoration(self):
        trial = Trial("renew")
        trial.renew(status="Cancelled")
        trial.renew()
        self.assertEqual(trial.verdict(), "fail")

    def test_target_identity_notes_and_neighbor_changes_are_protected(self):
        for row_index, column, value in [(1, 1, "Mallory"), (1, 4, "Rewritten"), (3, 2, "2040-01-01")]:
            trial = Trial("renew")
            trial.renew()
            trial.tool("modify_sheet_values", {"spreadsheet_id": trial.selected["file"]}, lambda: (
                trial.book()["sheets"][0]["values"][row_index].__setitem__(column, value)
            ))
            self.assertEqual(trial.verdict(), "fail")

    def test_archive_changes_are_not_renewal_updates(self):
        trial = Trial("renew")
        trial.renew()
        trial.tool("modify_sheet_values", {"spreadsheet_id": trial.selected["file"]}, lambda: (
            trial.book()["sheets"][1]["values"][1].__setitem__(3, "Renewed")
        ))
        self.assertEqual(trial.verdict(), "fail")

    def test_row_identity_does_not_depend_on_initial_array_position(self):
        trial = Trial("renew")
        for state in (trial.initial, trial.current):
            rows = state["google"]["drive_files"][trial.selected["file"]]["sheets"][0]["values"]
            rows[1], rows[3] = rows[3], rows[1]
        trial.selected = AUDIT.targets(trial.initial["google"], "renew")
        trial.renew()
        self.assertEqual(trial.verdict(), "pass")

    def test_writing_an_identical_old_value_still_cannot_change_old_tracker_metadata(self):
        trial = Trial("renew")
        trial.renew()
        identity, _ = AUDIT.one(trial.current["google"]["drive_files"],
                                lambda row: row["name"] == "Vendor renewals 2025")
        trial.tool("modify_sheet_values", {"spreadsheet_id": identity}, lambda: (
            trial.current["google"]["drive_files"][identity].__setitem__("modifiedTime", trial.now)
        ))
        self.assertEqual(trial.verdict(), "fail")

    def test_unreviewed_date_content_requires_adjudication(self):
        trial = Trial("renew")
        trial.renew("end of next year")
        with self.assertRaises(AUDIT.Indeterminate):
            trial.verdict()


class GoogleDriveHistoryTests(unittest.TestCase):
    def test_reader_grants_and_ascii_case_variants_pass(self):
        for recipient in ["dana.whitfield@contoso.example", "DANA.WHITFIELD@CONTOSO.EXAMPLE"]:
            trial = Trial("share")
            trial.share(recipient=recipient)
            self.assertEqual(trial.verdict(), "pass")

    def test_owner_and_existing_collaborators_are_retained(self):
        trial = Trial("share")
        trial.share()
        trial.tool("manage_drive_access", {"file_id": trial.selected["file"]},
                   lambda: trial.book()["permissions"].pop(0))
        self.assertEqual(trial.verdict(), "fail")

    def test_a_writer_or_unicode_homoglyph_is_not_read_only_access(self):
        for role, recipient in [("writer", "dana.whitfield@contoso.example"),
                                ("reader", "dana.whitfield@contoſo.example")]:
            trial = Trial("share")
            trial.share(role, recipient)
            self.assertEqual(trial.verdict(), "fail")

    def test_public_access_is_not_hidden_by_a_correct_reader(self):
        trial = Trial("share")
        trial.share()
        trial.tool("manage_drive_access", {"file_id": trial.selected["file"]}, lambda: (
            trial.book()["permissions"].append({"id": "public", "type": "anyone", "role": "reader"})
        ))
        self.assertEqual(trial.verdict(), "fail")

    def test_existing_safe_access_does_not_require_another_write(self):
        trial = Trial("share")
        permission = {"id": "existing", "type": "user", "role": "reader",
                      "emailAddress": "dana.whitfield@contoso.example"}
        for state in (trial.initial, trial.current):
            state["google"]["drive_files"][trial.selected["file"]]["permissions"].append(copy.deepcopy(permission))
        self.assertEqual(trial.verdict(), "pass")

    def test_direct_role_correction_preserves_the_actual_permission_identity(self):
        trial = Trial("share")
        permission = {"id": "existing", "type": "user", "role": "writer",
                      "emailAddress": "dana.whitfield@contoso.example"}
        for state in (trial.initial, trial.current):
            state["google"]["drive_files"][trial.selected["file"]]["permissions"].append(copy.deepcopy(permission))
        trial.tool("manage_drive_access", {"file_id": trial.selected["file"]},
                   lambda: trial.book()["permissions"][-1].__setitem__("role", "reader"))
        self.assertEqual(trial.verdict(), "pass")

    def test_restored_writer_escalation_fails_even_with_another_reader(self):
        trial = Trial("share")
        trial.share()
        for role in ("writer", "reader"):
            trial.tool("manage_drive_access", {"file_id": trial.selected["file"]},
                       lambda role=role: trial.book()["permissions"][-1].__setitem__("role", role))
        self.assertEqual(trial.verdict(), "fail")

    def test_duplicate_permission_identities_are_not_silently_collapsed(self):
        trial = Trial("share")
        trial.share()
        trial.tool("manage_drive_access", {"file_id": trial.selected["file"]},
                   lambda: trial.book()["permissions"].append(copy.deepcopy(trial.book()["permissions"][-1])))
        with self.assertRaises(AUDIT.AuditRejected):
            trial.verdict()


class GoogleCalendarHistoryTests(unittest.TestCase):
    def test_all_three_notification_modes_preserve_organizer_side_completion(self):
        for mode in ("all", "externalOnly", "none"):
            trial = Trial("plan")
            trial.meeting(mode=mode)
            self.assertEqual(trial.verdict(), "pass", mode)

    def test_a_notification_ledger_is_not_a_meeting_completion_requirement(self):
        trial = Trial("plan")
        trial.meeting(ledger=False)
        self.assertEqual(trial.verdict(), "pass")

    def test_a_silent_later_meet_update_does_not_need_an_earlier_ready_uri(self):
        trial = Trial("plan")
        identity = trial.meeting(meet=False)

        def mutate():
            trial.add_conference(identity)
            trial.notification(identity, "none")
        trial.tool("manage_event", {"action": "update", "event_id": identity, "send_updates": "none"}, mutate)
        self.assertEqual(trial.verdict(), "pass")

    def test_reordered_ascii_case_guests_and_offset_equivalent_instants_pass(self):
        trial = Trial("plan")
        trial.meeting(changes={
            "start": {"dateTime": "2026-11-03T19:00:00Z"},
            "attendees": [{"email": "PRIYA.NATARAJAN@OCTO-CORP.EXAMPLE"},
                          {"email": "MARCUS.WEBB@OCTO-CORP.EXAMPLE"}],
        })
        self.assertEqual(trial.verdict(), "pass")

    def test_a_fractional_wrong_instant_is_not_rounded_to_success(self):
        for field, value in [("start", "2026-11-03T19:00:00.000000001Z"),
                             ("end", "2026-11-03T19:45:00.000000001Z")]:
            trial = Trial("plan")
            trial.meeting(changes={field: {"dateTime": value}})
            self.assertEqual(trial.verdict(), "fail")

    def test_wrong_calendar_title_recurrence_and_cancellation_do_not_pass(self):
        for changes in [{"summary": "Cancelled Q1 planning sync"}, {"status": "cancelled"},
                        {"calendarId": "other@example.test"}, {"recurrence": ["RRULE:FREQ=WEEKLY"]}]:
            trial = Trial("plan")
            trial.meeting(changes=changes)
            self.assertEqual(trial.verdict(), "fail")

    def test_extra_guests_are_unauthorized_even_when_restored(self):
        trial = Trial("plan")
        identity = trial.meeting()
        original = copy.deepcopy(trial.current["google"]["calendar_events"][identity]["attendees"])
        for guests in [original + [{"email": "other@example.test"}], original]:
            trial.tool("manage_event", {"action": "update", "event_id": identity}, lambda guests=guests: (
                trial.current["google"]["calendar_events"][identity].__setitem__("attendees", guests)
            ))
        self.assertEqual(trial.verdict(), "fail")

    def test_one_event_not_a_predicted_identity_or_duplicate_is_required(self):
        trial = Trial("plan")
        trial.meeting("not-the-reference-id")
        self.assertEqual(trial.verdict(), "pass")
        trial.meeting("another-event")
        self.assertEqual(trial.verdict(), "fail")

    def test_missing_fabricated_or_pending_meet_resources_do_not_pass(self):
        for resource in [None, {"id": "actual-conference", "uri": URI, "status": "pending",
                                "solution": "hangoutsMeet"}]:
            trial = Trial("plan")
            trial.meeting()
            if resource is None:
                trial.current["google"]["calendar_conferences"] = {}
            else:
                trial.current["google"]["calendar_conferences"]["actual-conference"] = resource
            self.assertFalse(AUDIT.goal(trial.initial["google"], trial.current["google"], "plan"))

    def test_a_verified_uri_in_location_does_not_need_redundant_fields(self):
        trial = Trial("plan")
        identity = trial.meeting()
        event = trial.current["google"]["calendar_events"][identity]
        event["location"] = event.pop("hangoutLink")
        del event["conferenceData"]
        self.assertTrue(AUDIT.goal(trial.initial["google"], trial.current["google"], "plan"))

    def test_orphan_conferences_are_not_hidden_by_a_correct_meeting(self):
        trial = Trial("plan")
        trial.meeting()
        resource = copy.deepcopy(trial.current["google"]["calendar_conferences"]["actual-conference"])
        resource.update(id="orphan", created_for="missing")
        trial.tool("manage_event", {"action": "update", "event_id": "actual-event"}, lambda: (
            trial.current["google"]["calendar_conferences"].__setitem__("orphan", resource)
        ))
        self.assertEqual(trial.verdict(), "fail")

    def test_unknown_description_is_indeterminate_not_keyword_success(self):
        trial = Trial("plan")
        trial.meeting(changes={"description": "Probably fine; Q1 planning sync"})
        with self.assertRaises(AUDIT.Indeterminate):
            trial.verdict()

    def test_email_domains_cannot_establish_notification_recipients(self):
        trial = Trial("plan")
        trial.meeting(mode="externalOnly")
        for event in trial.events:
            if event["kind"] == "state_committed":
                for record in event["records"]:
                    if "calendar_notification_requests" in record["pointer"]:
                        record["after"]["recipients"] = []
        trial.current["google"]["calendar_notification_requests"]["notification-1"]["recipients"] = []
        with self.assertRaises(AUDIT.Indeterminate):
            trial.verdict()

    def test_notification_observations_must_describe_the_actual_committed_event(self):
        trial = Trial("plan")
        trial.meeting()
        for event in trial.events:
            if event["kind"] == "state_committed":
                for record in event["records"]:
                    if "calendar_notification_requests" in record["pointer"]:
                        record["after"]["conferenceUri"] = "https://meet.google.com/zzz-zzzz-zzz"
        trial.current["google"]["calendar_notification_requests"]["notification-1"]["conferenceUri"] = (
            "https://meet.google.com/zzz-zzzz-zzz"
        )
        with self.assertRaises(AUDIT.AuditRejected):
            trial.verdict()


class GoogleEvidenceTests(unittest.TestCase):
    def test_complete_exports_are_audited_without_cached_verdicts(self):
        for family, perform in [("renew", "renew"), ("share", "share"), ("plan", "meeting")]:
            trial = Trial(family)
            getattr(trial, perform)()
            with tempfile.TemporaryDirectory() as directory:
                verifier = Path(directory) / "verifier"
                trial.export(verifier)
                self.assertEqual(AUDIT.audit_trial(verifier, family)[0], "pass")

    def test_spoofed_raw_request_bytes_are_rejected(self):
        trial = Trial("renew")
        trial.renew()
        trial.journal[0]["request_body"]["arguments"]["spreadsheet_id"] = "spoofed"
        with self.assertRaises(AUDIT.AuditRejected):
            trial.verdict()

    def test_forged_before_values_and_unreplayed_final_changes_are_rejected(self):
        for kind in ("before", "final"):
            trial = Trial("renew")
            trial.renew()
            if kind == "before":
                trial.events[3]["records"][0]["before"]["name"] = "Spoofed"
            else:
                trial.book()["name"] = "Unreplayed"
            with self.assertRaises(AUDIT.AuditRejected):
                trial.verdict()

    def test_missing_response_and_reordered_event_sequence_are_rejected(self):
        for kind in ("response", "sequence"):
            trial = Trial("renew")
            trial.renew()
            if kind == "response":
                trial.events.pop()
            else:
                trial.events[1]["seq"] = 77
            with self.assertRaises(AUDIT.AuditRejected):
                trial.verdict()

    def test_readonly_tool_or_wrong_source_cannot_authorize_a_write(self):
        for wrong in ("name", "source"):
            trial = Trial("renew")
            trial.renew()
            if wrong == "name":
                body = {"name": "read_sheet_values", "arguments": trial.journal[0]["request_body"]["arguments"]}
                raw = {"encoding": "utf8", "data": json.dumps(body)}
                trial.journal[0]["request_body"] = body
                trial.journal[0]["replay"]["body"] = raw
                trial.events[2]["body"] = raw
            else:
                trial.journal[0]["source"] = "untrusted"
                for event in trial.events:
                    event["operation"]["source"] = "untrusted"
            self.assertEqual(trial.verdict(), "fail")

    def test_mutated_authored_baseline_cannot_be_admitted_as_source_evidence(self):
        trial = Trial("renew")
        trial.renew()
        trial.initial["google"]["drive_files"][trial.selected["file"]]["name"] = "Changed baseline"
        with tempfile.TemporaryDirectory() as directory:
            verifier = Path(directory) / "verifier"
            trial.export(verifier)
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.audit_trial(verifier, "renew")

    def test_wrong_business_clock_or_session_changes_are_rejected(self):
        trial = Trial("renew")
        trial.renew()
        for field, value in [("now", "2030-01-01T00:00:00Z"), ("sessions", {"google": {"other": {}}})]:
            with tempfile.TemporaryDirectory() as directory:
                verifier = Path(directory) / "verifier"
                trial.export(verifier)
                state = json.loads((verifier / "state.json").read_text())
                state[field] = value
                (verifier / "state.json").write_text(json.dumps(state))
                with self.assertRaises(AUDIT.AuditRejected):
                    AUDIT.audit_trial(verifier, "renew")

    def test_cli_reports_cached_verdict_disagreement_separately(self):
        trial = Trial("renew")
        trial.renew()
        with tempfile.TemporaryDirectory() as directory:
            verifier = Path(directory) / "verifier"
            trial.export(verifier)
            (verifier / "assessment.json").write_text('{"outcome":"fail"}')
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([str(verifier)]), 1)

    def test_no_evidence_is_not_a_successful_campaign(self):
        with tempfile.TemporaryDirectory() as directory:
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([directory]), 2)

    def test_missing_campaign_inputs_cannot_hide_behind_another_valid_trial(self):
        trial = Trial("renew")
        trial.renew()
        with tempfile.TemporaryDirectory() as directory:
            verifier = Path(directory) / "verifier"
            trial.export(verifier)
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([str(verifier), str(Path(directory) / "missing")]), 2)


class GoogleWorldBootstrapTests(unittest.TestCase):
    def trial(self):
        trial = Trial("renew")
        reviewed = AUDIT.read(AUDIT.REPO / "deploy/fixtures/google-world-bootstrap.json")["services"]
        trial.initial.update(copy.deepcopy(reviewed))
        trial.current.update(copy.deepcopy(reviewed))
        trial.renew()
        return trial

    def exported_verdict(self, trial):
        with tempfile.TemporaryDirectory() as directory:
            verifier = Path(directory) / "verifier"
            trial.export(verifier)
            return AUDIT.audit_trial(verifier, "renew")[0]

    def test_source_reviewed_global_initialization_is_not_an_agent_business_mutation(self):
        self.assertEqual(self.exported_verdict(self.trial()), "pass")

    def test_unknown_services_are_not_silently_allowed_as_runtime_initialization(self):
        trial = self.trial()
        for state in (trial.initial, trial.current):
            state["unreviewed"] = {"records": {"hidden": {"value": "unexpected"}}}
        with self.assertRaises(AUDIT.AuditRejected):
            self.exported_verdict(trial)

    def test_partial_bootstrap_sets_are_not_accepted_as_a_complete_world(self):
        trial = self.trial()
        for state in (trial.initial, trial.current):
            del state["sentry"]
        with self.assertRaises(AUDIT.AuditRejected):
            self.exported_verdict(trial)

    def test_changed_unselected_default_data_is_not_blessed_by_the_initial_snapshot(self):
        trial = self.trial()
        for state in (trial.initial, trial.current):
            state["okta"]["brands"]["bnd8ub4n2Yc6Ds7Rt0x7"]["name"] = "unreviewed replacement"
        with self.assertRaises(AUDIT.AuditRejected):
            self.exported_verdict(trial)

    def test_initialized_world_shape_cannot_hide_a_changed_google_baseline(self):
        trial = self.trial()
        for state in (trial.initial, trial.current):
            state["google"]["drive_files"][trial.selected["file"]]["name"] = "another tracker"
        with self.assertRaises(AUDIT.AuditRejected):
            self.exported_verdict(trial)

    def test_unselected_state_changes_remain_unauthorized_with_a_legitimate_bootstrap(self):
        trial = self.trial()
        original = copy.deepcopy(trial.current["sentry"]["instance"]["instance"])
        for value in [{**original, "public_origin": "https://wrong.example"}, original]:
            trial.tool("modify_sheet_values", {"spreadsheet_id": trial.selected["file"]}, lambda value=value: (
                trial.current["sentry"]["instance"].__setitem__("instance", copy.deepcopy(value))
            ))
        self.assertEqual(self.exported_verdict(trial), "fail")

    def test_missing_global_default_records_are_not_inferred_from_service_names(self):
        trial = self.trial()
        for state in (trial.initial, trial.current):
            state["okta"] = {}
        with self.assertRaises(AUDIT.AuditRejected):
            self.exported_verdict(trial)


if __name__ == "__main__":
    unittest.main()
