import contextlib
import copy
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "grafana_business_audit", Path(__file__).resolve().parents[1] / "audit-grafana-business.py",
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)
NOW = "2026-11-12T15:30:00Z"


def service(family="ack"):
    return AUDIT.authored_initial(family)[0]


def matcher(name, value, regex=False, equal=True):
    return {"name": name, "value": value, "isRegex": regex, "isEqual": equal}


def annotation(initial, identity=9007, panel=0):
    targets = AUDIT.identities(initial, "ann")
    dashboard, user = targets["dashboard"], targets["user"]
    return {
        "id": identity, "dashboardId": dashboard["id"], "dashboardUID": dashboard["uid"],
        "panelId": panel, "userId": user["id"], "login": user["login"], "email": user["email"],
        "created": AUDIT.instant_ns(NOW) // 1_000_000, "updated": AUDIT.instant_ns(NOW) // 1_000_000,
        "time": AUDIT.ANNOTATION_TIME, "timeEnd": AUDIT.ANNOTATION_TIME,
        "text": AUDIT.ANNOTATION_TEXT, "tags": ["checkout", "deploy"], "data": None,
    }


def silence(identity="independently-allocated"):
    return {
        "id": identity,
        "matchers": [matcher(name, value) for name, value in AUDIT.SILENCE_LABELS.items()],
        "startsAt": "2026-11-13T02:00:00Z", "endsAt": "2026-11-13T04:00:00Z",
        "createdBy": "maintenance.operator", "comment": "Storage maintenance (CHG-7781).",
        "updatedAt": NOW,
    }


def annotation_body(row, numeric=False):
    result = {field: copy.deepcopy(row[field]) for field in (
        "panelId", "time", "timeEnd", "text", "tags", "data",
    )}
    association = "dashboardId" if numeric else "dashboardUID"
    result[association] = row[association]
    return result


def silence_body(row):
    return {field: copy.deepcopy(row[field]) for field in (
        "matchers", "startsAt", "endsAt", "createdBy", "comment",
    )}


class History:
    def __init__(self, initial):
        self.initial = {"grafana": copy.deepcopy(initial)}
        self.final = copy.deepcopy(self.initial)
        self.events = []
        self.journal = {}

    def call(self, path, method="GET", body=None, changes=(), response=None, status=200):
        identity = len(self.journal) + 100
        operation = {
            "id": identity, "service": "grafana", "method": method, "path": path,
            "source": "native-tools-source",
        }
        headers = [{"name": "authorization", "value": {
            "kind": "redacted", "sha256": "opaque-collected-credential",
        }}]
        raw = {"encoding": "utf8", "data": "" if body is None else json.dumps(body)}
        request = {
            "seq": len(self.journal) + 1, "operation_id": identity, "service": "grafana",
            "method": method, "path": path, "time": NOW, "source": operation["source"],
            "host": "grafana.example", "scheme": "https", "query": None,
            "content_type": None if body is None else "application/json",
            "replay": {"headers": headers, "body": raw},
            "request_body": copy.deepcopy(body), "response_body": response, "status": status,
        }
        self.journal[identity] = request

        def event(kind, **fields):
            record = {
                "seq": len(self.events) + 1, "time": NOW,
                "operation": copy.deepcopy(operation), "kind": kind,
            }
            record.update(copy.deepcopy(fields))
            self.events.append(record)

        event("request_started")
        event("request_head_received", head={
            **{field: request[field] for field in ("host", "scheme", "query", "content_type")},
            "headers": headers,
        })
        event("request_body_received", body=raw)
        if changes:
            records = []
            for collection, record_id, after in changes:
                record_id = str(record_id)
                pointer_id = record_id.replace("~", "~0").replace("/", "~1")
                record = {"pointer": f"/services/grafana/{collection}/{pointer_id}"}
                current = self.final["grafana"].setdefault(collection, {})
                if record_id in current:
                    record["before"] = copy.deepcopy(current[record_id])
                if after is not None:
                    record["after"] = copy.deepcopy(after)
                    current[record_id] = copy.deepcopy(after)
                else:
                    del current[record_id]
                records.append(record)
            event("state_committed", records=records)
        event("response_completed", status=status, body_complete=True)
        return request

    def create_annotation(self, row, numeric=False):
        return self.call(
            "/api/annotations", "POST", annotation_body(row, numeric),
            [("annotations", str(row["id"]), row)], {"id": row["id"]},
        )

    def create_silence(self, row, body=None):
        return self.call(
            AUDIT.SILENCE_PATH, "POST", silence_body(row) if body is None else body,
            [("silences", row["id"], row)], {"silenceID": row["id"]}, 202,
        )

    def acknowledge(self):
        target = AUDIT.identities(self.initial["grafana"], "ack")["group"]
        row = copy.deepcopy(self.final["grafana"]["oncall_alert_groups"][target])
        row.update(state="acknowledged", acknowledged_at=NOW, acknowledged_by=None)
        return self.call(
            f"/oncall/api/v1/alert_groups/{target}/acknowledge", "POST",
            changes=[("oncall_alert_groups", target, row)],
        )

    def audit(self, family):
        return AUDIT.history(self.initial, self.final, self.events, self.journal, family)

    def write_trial(self, root, family):
        verifier = root / "job" / "deep" / "trial" / "verifier"
        verifier.mkdir(parents=True)
        for filename, data in {
            "initial-state.json": {
                "scenario": "ops/grafana/" + AUDIT.SCENARIOS[family], "now": NOW,
                "services": self.initial,
            },
            "state.json": {"services": self.final},
            "events.json": {"events": self.events},
            "journal.json": {"entries": list(self.journal.values())},
            "assessment.json": {"outcome": "deliberately-not-a-verdict"},
        }.items():
            (verifier / filename).write_text(json.dumps(data))
        return verifier


class GrafanaAnswerTests(unittest.TestCase):
    def setUp(self):
        self.initial = service()
        self.targets = AUDIT.identities(self.initial, "ack")
        self.person = self.initial["oncall_users"][next(iter(self.targets["users"]))]

    def test_whole_aliases_and_truthful_combinations(self):
        for text in [
            self.person["username"], self.person["email"], self.person["name"],
            self.person["name"].split()[-1], self.person["name"].upper(),
            f'{self.person["name"]} ({self.person["username"]}, {self.person["email"]})',
            f'{self.person["name"]} <{self.person["email"]}>',
            f'{self.person["name"]} is on call.',
        ]:
            with self.subTest(text=text):
                self.assertTrue(AUDIT.answer_correct(self.initial, text.encode()))

    def test_negated_mention_cannot_validate_the_explicit_wrong_person(self):
        answer = b"Brennan is not on call. The primary on-call person is Mei Chen."
        self.assertFalse(AUDIT.answer_correct(self.initial, answer))
        self.assertFalse(AUDIT.answer_correct(self.initial, b"Mei Chen"))
        self.assertFalse(AUDIT.answer_correct(self.initial, b"Brennan is not on call."))

    def test_unknown_prose_and_a_bare_mention_are_indeterminate_not_model_failures(self):
        for answer in [
            b"Maybe Brennan, but I have not checked.", b"Not Brennan",
            b"Do not pull Brennan in.", b"sbrennan is mentioned in this schedule.",
        ]:
            with self.subTest(answer=answer), self.assertRaises(AUDIT.Indeterminate):
                AUDIT.answer_correct(self.initial, answer)

    def test_globally_ambiguous_surnames_are_not_aliases(self):
        other = next(identity for identity in self.initial["oncall_users"]
                     if identity not in self.targets["users"])
        self.initial["oncall_users"][other]["name"] = "Another Brennan"
        self.assertNotIn("brennan", AUDIT.alias_index(self.initial))
        with self.assertRaises(AUDIT.Indeterminate):
            AUDIT.answer_correct(self.initial, b"Brennan")
        self.assertTrue(AUDIT.answer_correct(self.initial, b"Sam Brennan"))

    def test_empty_duplicate_and_incomplete_current_duty_answers_fail(self):
        self.assertFalse(AUDIT.answer_correct(self.initial, b""))
        self.assertFalse(AUDIT.answer_correct(self.initial, b"[]"))
        user = {field: self.person[field] for field in ("id", "username", "email", "role")}
        self.assertFalse(AUDIT.answer_correct(self.initial, json.dumps([user, user]).encode()))
        schedule = self.initial["oncall_schedules"][self.targets["schedule"]]
        other = next(identity for identity in self.initial["oncall_users"]
                     if identity not in self.targets["users"])
        schedule["on_call_now"].append(other)
        self.assertFalse(AUDIT.answer_correct(self.initial, json.dumps([user]).encode()))
        second = self.initial["oncall_users"][other]
        self.assertTrue(AUDIT.answer_correct(
            self.initial, f'{self.person["username"]}; {second["username"]}'.encode(),
        ))
        self.assertFalse(AUDIT.answer_correct(
            self.initial, f'{self.person["username"]}; {self.person["username"]}'.encode(),
        ))

    def test_structured_arrays_and_schedule_objects_bind_truthful_fields(self):
        user = {field: self.person[field] for field in ("id", "username", "email", "role")}
        schedule = self.initial["oncall_schedules"][self.targets["schedule"]]
        for answer in [
            [user], {"users": [user], "scheduleId": schedule["id"], "scheduleName": schedule["name"]},
            {"users": [user], "schedule": {"id": schedule["id"], "name": schedule["name"]}},
        ]:
            with self.subTest(answer=answer):
                self.assertTrue(AUDIT.answer_correct(self.initial, json.dumps(answer).encode()))
        user["email"] = "untruthful@example.com"
        self.assertFalse(AUDIT.answer_correct(self.initial, json.dumps([user]).encode()))

    def test_known_person_schedule_and_cardinality_errors_are_not_hidden_by_unknown_metadata(self):
        user = {"id": self.person["id"], "unreviewed": "metadata"}
        with self.assertRaises(AUDIT.Indeterminate):
            AUDIT.answer_correct(self.initial, json.dumps([user]).encode())
        for answer in [
            {"scheduleId": "wrong-primary", "users": [user], "unreviewed": "metadata"},
            {"users": [], "unreviewed": "metadata"},
            [{"id": "not-a-user", "unreviewed": "metadata"}],
        ]:
            self.assertFalse(AUDIT.answer_correct(self.initial, json.dumps(answer).encode()))
        user = {"id": self.person["id"], "role": "invented"}
        self.assertFalse(AUDIT.answer_correct(self.initial, json.dumps([user]).encode()))

    def test_source_bindings_follow_the_actual_duty_not_a_seeded_person(self):
        schedule = self.initial["oncall_schedules"][self.targets["schedule"]]
        schedule["on_call_now"] = [next(
            identity for identity, row in self.initial["oncall_users"].items()
            if row["username"] == "mchen"
        )]
        self.assertTrue(AUDIT.answer_correct(self.initial, b"Mei Chen"))
        self.assertFalse(AUDIT.answer_correct(self.initial, b"Sam Brennan"))

    def test_only_canonical_collected_answer_bytes_are_used(self):
        with tempfile.TemporaryDirectory() as directory:
            trial = Path(directory)
            verifier = trial / "verifier"
            verifier.mkdir()
            answer = trial / "artifacts" / "agent" / "answer.md"
            answer.parent.mkdir(parents=True)
            answer.write_bytes(b"Mei Chen")
            (trial / "main-snapshot").mkdir()
            (trial / "main-snapshot" / "answer").write_bytes(b"Sam Brennan")
            manifest = [{
                "source": "/app/answer.md", "destination": "artifacts/agent/answer.md",
                "type": "file", "status": "ok",
            }]
            (trial / "artifacts" / "manifest.json").write_text(json.dumps(manifest))
            self.assertEqual(AUDIT.canonical_answer(verifier), b"Mei Chen")
            self.assertFalse(AUDIT.answer_correct(self.initial, AUDIT.canonical_answer(verifier)))
            answer.unlink()
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.canonical_answer(verifier)


class GrafanaPredicateAndTimeTests(unittest.TestCase):
    def test_offsets_and_fraction_spellings_are_exactly_equivalent(self):
        expected = AUDIT.instant_ns("2026-11-13T02:00:00Z")
        for value in [
            "2026-11-12T20:00:00-06:00", "2026-11-13T07:30:00+05:30",
            "2026-11-13T02:00:00.000000000+00:00",
        ]:
            self.assertEqual(AUDIT.instant_ns(value), expected)
        self.assertEqual(AUDIT.instant_ns("1969-12-31T23:59:59.999999999Z"), -1)

    def test_one_nanosecond_near_misses_are_not_truncated(self):
        row = silence()
        self.assertTrue(AUDIT.silence_correct(row))
        for field in ("startsAt", "endsAt"):
            changed = copy.deepcopy(row)
            changed[field] = changed[field].replace("Z", ".000000001Z")
            self.assertFalse(AUDIT.silence_correct(changed))
        baseline = AUDIT.instant_ns(row["startsAt"])
        self.assertEqual(AUDIT.instant_ns("2026-11-13T02:00:00.000000001Z") - baseline, 1)

    def test_unsupported_or_invalid_time_forms_require_adjudication(self):
        for value in [
            "2026-11-13T02:00:00.0000000001Z", "2026-11-13T02:00:60Z",
            "2026-11-13T02:00:00-00:00", "2026-11-13T02:00:00+24:00",
            "2026-02-30T02:00:00Z",
        ]:
            with self.subTest(value=value), self.assertRaises(AUDIT.Indeterminate):
                AUDIT.instant_ns(value)

    def test_singleton_regex_and_conjunctive_redundancy_prove_the_full_predicate(self):
        ordinary = silence()["matchers"]
        self.assertTrue(AUDIT.predicate_exact(ordinary))
        singleton = copy.deepcopy(ordinary)
        singleton[1] = matcher("instance", "^(?:pg-replica-2)$", regex=True)
        self.assertTrue(AUDIT.predicate_exact(singleton))
        redundant = ordinary + [
            matcher("instance", "^pg-replica-.*$", regex=True),
            matcher("instance", "pg-replica-1", equal=False),
        ]
        self.assertTrue(AUDIT.predicate_exact(redundant))
        self.assertTrue(AUDIT.predicate_exact(list(reversed(redundant))))

    def test_broad_and_narrow_predicates_are_not_proved_by_fixture_samples(self):
        ordinary = silence()["matchers"]
        self.assertFalse(AUDIT.predicate_exact([
            ordinary[0], matcher("instance", "pg-replica-.*", regex=True),
        ]))
        self.assertFalse(AUDIT.predicate_exact(ordinary + [matcher("environment", "prod")]))
        self.assertFalse(AUDIT.predicate_exact(ordinary + [
            matcher("instance", "pg-replica-2", equal=False),
        ]))
        self.assertFalse(AUDIT.predicate_exact([ordinary[1]]))

    def test_unknown_regex_is_never_run_as_python_or_guessed_wrong(self):
        ordinary = silence()["matchers"]
        for pattern in [r"(?i:pg-replica-2)", r"pg-replica-\d", r"(pg-replica-2)", r"\Qpg-replica-2\E"]:
            with self.subTest(pattern=pattern), self.assertRaises(AUDIT.Indeterminate):
                AUDIT.predicate_exact([ordinary[0], matcher("instance", pattern, regex=True)])
        with self.assertRaises(AUDIT.Indeterminate):
            AUDIT.predicate_exact(ordinary + [matcher("instance", "(?i:pg-replica-2)", regex=True)])
        with self.assertRaises(AUDIT.Indeterminate):
            AUDIT.predicate_exact(ordinary + [matcher("unrelated", "\ud800", equal=False)])

    def test_ticket_is_a_complete_token_and_creator_is_not_a_reference_string(self):
        row = silence()
        self.assertTrue(AUDIT.silence_correct(row))
        for ticket in ["CHG-77810", "PRE-CHG-7781", "CHG-7781-suffix", "CHG-7781_extra"]:
            row["comment"] = ticket
            self.assertFalse(AUDIT.silence_correct(row))
        row["comment"] = "Maintenance: CHG-7781."
        row["createdBy"] = "any.actual.author"
        self.assertTrue(AUDIT.silence_correct(row))
        row["createdBy"] = " "
        self.assertFalse(AUDIT.silence_correct(row))


class GrafanaHistoryTests(unittest.TestCase):
    def test_acknowledgment_is_scoped_and_does_not_invent_an_actor(self):
        history = History(service())
        history.acknowledge()
        self.assertEqual(history.audit("ack"), [])
        self.assertTrue(AUDIT.goal(
            history.initial["grafana"], history.final["grafana"], "ack", b"sbrennan",
        ))
        committed = next(event for event in history.events if event["kind"] == "state_committed")
        committed["records"][0]["after"]["acknowledged_by"] = "first-admin"
        target = AUDIT.identities(history.initial["grafana"], "ack")["group"]
        history.final["grafana"]["oncall_alert_groups"][target]["acknowledged_by"] = "first-admin"
        self.assertTrue(history.audit("ack"))

    def test_target_group_is_derived_even_when_authored_ids_change(self):
        initial = service()
        target = AUDIT.identities(initial, "ack")["group"]
        row = initial["oncall_alert_groups"].pop(target)
        row["id"] = "actual-new-group"
        initial["oncall_alert_groups"][row["id"]] = row
        history = History(initial)
        history.acknowledge()
        self.assertEqual(history.audit("ack"), [])

    def test_request_group_identity_mismatch_cannot_authorize_the_actual_group(self):
        history = History(service())
        request = history.acknowledge()
        request["path"] = "/oncall/api/v1/alert_groups/wrong/acknowledge"
        for event in history.events:
            event["operation"]["path"] = request["path"]
        self.assertTrue(history.audit("ack"))

    def test_resolve_and_restore_are_forbidden_in_the_journal_even_without_commits(self):
        history = History(service())
        history.acknowledge()
        target = AUDIT.identities(history.initial["grafana"], "ack")["group"]
        for action in ("resolve", "unresolve"):
            history.call(f"/oncall/api/v1/alert_groups/{target}/{action}/", "POST", status=400)
        self.assertEqual(len(history.audit("ack")), 2)
        history.call("/api/alertmanager/grafana/api/v2/silences/", "POST", {}, status=400)
        self.assertEqual(len(history.audit("ack")), 3)

    def test_committed_resolution_then_exact_restoration_still_fails(self):
        history = History(service())
        history.acknowledge()
        target = AUDIT.identities(history.initial["grafana"], "ack")["group"]
        acknowledged = copy.deepcopy(history.final["grafana"]["oncall_alert_groups"][target])
        resolved = copy.deepcopy(acknowledged)
        resolved.update(state="resolved", resolved_at=NOW)
        history.call(
            f"/oncall/api/v1/alert_groups/{target}/resolve/", "POST",
            changes=[("oncall_alert_groups", target, resolved)],
        )
        history.call(
            f"/oncall/api/v1/alert_groups/{target}/unresolve/", "POST",
            changes=[("oncall_alert_groups", target, acknowledged)],
        )
        self.assertTrue(AUDIT.goal(
            history.initial["grafana"], history.final["grafana"], "ack", b"sbrennan",
        ))
        self.assertEqual(len(history.audit("ack")), 4)

    def test_staging_and_datasource_changes_are_rejected_even_when_restored(self):
        for collection, field, value in [
            ("oncall_alert_groups", "title", "collateral page rewrite"),
            ("datasources", "name", "collateral datasource rewrite"),
        ]:
            with self.subTest(collection=collection):
                history = History(service())
                history.acknowledge()
                key = next(iter(history.initial["grafana"][collection]))
                original = history.initial["grafana"][collection][key]
                changed = copy.deepcopy(original)
                changed[field] = value
                history.call("/api/other", "PUT", changes=[(collection, key, changed)])
                history.call("/api/other", "PUT", changes=[(collection, key, original)])
                self.assertEqual(history.initial["grafana"][collection], history.final["grafana"][collection])
                self.assertEqual(len(history.audit("ack")), 2)

    def test_numeric_dashboard_latency_panel_extra_tags_and_reads_are_legitimate(self):
        history = History(service("ann"))
        history.call("/api/irrelevant-discovery")
        row = annotation(history.initial["grafana"], panel=3)
        row["tags"].append("production")
        row["data"] = {"release": {"version": "4.19.0"}}
        history.create_annotation(row, numeric=True)
        history.call("/api/annotations")
        self.assertEqual(history.audit("ann"), [])
        self.assertTrue(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "ann"))

    def test_dashboard_uid_precedence_does_not_require_the_ignored_numeric_id_to_match(self):
        history = History(service("ann"))
        row = annotation(history.initial["grafana"])
        body = annotation_body(row)
        body["dashboardId"] = row["dashboardId"] + 1
        history.call(
            "/api/annotations", "POST", body,
            [("annotations", str(row["id"]), row)], {"id": row["id"]},
        )
        self.assertEqual(history.audit("ann"), [])
        self.assertTrue(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "ann"))

    def test_annotation_business_relationships_are_not_just_a_successful_post(self):
        changes = [
            ("panelId", 1), ("timeEnd", AUDIT.ANNOTATION_TIME + 60000),
            ("text", "Release checkout v4.19.0; cancel deploy"),
            ("dashboardId", 102), ("dashboardUID", "checkout-staging"),
            ("userId", 99), ("login", "someone-else"), ("tags", ["deploy"]),
        ]
        for field, value in changes:
            with self.subTest(field=field):
                history = History(service("ann"))
                row = annotation(history.initial["grafana"])
                row[field] = value
                history.create_annotation(row)
                self.assertTrue(history.audit("ann"))
                self.assertFalse(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "ann"))

    def test_annotation_request_and_allocated_id_must_match_the_committed_record(self):
        for mutate in [
            lambda request: request["response_body"].update(id=42),
            lambda request: request["request_body"].update(dashboardUID="checkout-staging"),
        ]:
            history = History(service("ann"))
            row = annotation(history.initial["grafana"])
            request = history.create_annotation(row)
            mutate(request)
            raw = {"encoding": "utf8", "data": json.dumps(request["request_body"])}
            request["replay"]["body"] = raw
            for event in history.events:
                if event["kind"] == "request_body_received":
                    event["body"] = raw
            self.assertTrue(history.audit("ann"))

    def test_second_annotation_and_restored_existing_annotation_are_history_failures(self):
        history = History(service("ann"))
        row = annotation(history.initial["grafana"])
        history.create_annotation(row)
        second = annotation(history.initial["grafana"], identity=9999)
        history.create_annotation(second)
        self.assertTrue(history.audit("ann"))
        self.assertFalse(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "ann"))
        history = History(service("ann"))
        history.create_annotation(row)
        key = next(iter(history.initial["grafana"]["annotations"]))
        original = history.initial["grafana"]["annotations"][key]
        changed = copy.deepcopy(original)
        changed["text"] = "rewrite old deployment"
        for version in (changed, original):
            history.call(f"/api/annotations/{key}", "PATCH", version, [("annotations", key, version)])
        self.assertEqual(len(history.audit("ann")), 2)
        self.assertTrue(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "ann"))

    def test_silence_wire_defaults_offsets_and_regex_are_correlated(self):
        history = History(service("sil"))
        row = silence()
        row["matchers"][1] = matcher("instance", "^(?:pg-replica-2)$", regex=True)
        body = silence_body(row)
        body["startsAt"] = "2026-11-12T20:00:00.000000000-06:00"
        body["endsAt"] = "2026-11-12T22:00:00-06:00"
        for term in body["matchers"]:
            del term["isEqual"]
        history.create_silence(row, body)
        self.assertEqual(history.audit("sil"), [])
        self.assertTrue(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "sil"))

    def test_redundant_matchers_are_valid_through_the_actual_history_auditor(self):
        history = History(service("sil"))
        row = silence()
        row["matchers"].extend([
            matcher("instance", "pg-replica-.*", regex=True),
            matcher("instance", "pg-replica-1", equal=False),
        ])
        history.create_silence(row)
        self.assertEqual(history.audit("sil"), [])
        self.assertTrue(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "sil"))

    def test_silence_request_destination_id_and_matchers_are_not_interchangeable(self):
        for field in ("destination", "response_id", "update_id", "request_matchers"):
            with self.subTest(field=field):
                history = History(service("sil"))
                row = silence()
                body = silence_body(row)
                if field == "update_id":
                    body["id"] = next(iter(history.initial["grafana"]["silences"]))
                if field == "request_matchers":
                    body["matchers"][1]["value"] = "pg-replica-1"
                request = history.create_silence(row, body)
                if field == "response_id":
                    request["response_body"]["silenceID"] = "another-allocated-id"
                if field == "destination":
                    request["path"] = "/api/alertmanager/am-prod/api/v2/silences"
                    for event in history.events:
                        event["operation"]["path"] = request["path"]
                self.assertTrue(history.audit("sil"))

    def test_all_existing_silences_are_protected_regardless_of_update_id_behavior(self):
        for clone in (False, True):
            with self.subTest(clone=clone):
                history = History(service("sil"))
                key, original = next(
                    (identity, row) for identity, row in history.initial["grafana"]["silences"].items()
                    if row["comment"] == "staging load test"
                )
                changed = copy.deepcopy(original)
                changed["createdBy"] = "unrequested creator"
                if clone:
                    changed["id"] = "replacement-silence-id"
                body = silence_body(changed)
                body["id"] = key
                history.call(
                    AUDIT.SILENCE_PATH, "POST", body,
                    [("silences", changed["id"], changed)], {"silenceID": changed["id"]}, 202,
                )
                history.create_silence(silence())
                self.assertTrue(history.audit("sil"))

    def test_second_silence_and_delete_attempts_are_not_allowed(self):
        history = History(service("sil"))
        history.create_silence(silence())
        second = silence("second-unrelated")
        second["matchers"].pop()
        history.create_silence(second)
        self.assertTrue(history.audit("sil"))
        self.assertFalse(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "sil"))
        history = History(service("sil"))
        history.create_silence(silence())
        history.call("/api/alertmanager/grafana/api/v2/silence/missing/", "DELETE", status=404)
        self.assertEqual(len(history.audit("sil")), 1)

    def test_restored_rules_routing_and_existing_silences_fail_at_each_commit(self):
        for collection, field, value in [
            ("silences", "createdBy", "rewritten"),
            ("alert_rules", "title", "rewritten rule"),
            ("policies", "receiver", "rewritten routing"),
        ]:
            history = History(service("sil"))
            history.create_silence(silence())
            key = next(iter(history.initial["grafana"][collection]))
            original = history.initial["grafana"][collection][key]
            changed = copy.deepcopy(original)
            changed[field] = value
            history.call("/api/other", "PUT", changes=[(collection, key, changed)])
            history.call("/api/other", "PUT", changes=[(collection, key, original)])
            with self.subTest(collection=collection):
                self.assertEqual(len(history.audit("sil")), 2)
                self.assertTrue(AUDIT.goal(history.initial["grafana"], history.final["grafana"], "sil"))

    def test_forged_before_values_and_wrong_operation_correlations_are_audit_rejections(self):
        for field in ("before", "operation", "uncommitted_final"):
            history = History(service())
            history.acknowledge()
            commit = next(event for event in history.events if event["kind"] == "state_committed")
            if field == "before":
                commit["records"][0]["before"]["title"] = "forged original"
            elif field == "operation":
                commit["operation"]["id"] = 999
            else:
                history.final["grafana"]["user"]["current"]["name"] = "not in committed history"
            with self.subTest(field=field), self.assertRaises(AUDIT.AuditRejected):
                history.audit("ack")

    def test_raw_http_mcp_argument_confusion_and_missing_authentication_are_rejected(self):
        history = History(service("ann"))
        row = annotation(history.initial["grafana"])
        request = history.create_annotation(row)
        request["request_body"] = {"dashboardUid": row["dashboardUID"]}
        with self.assertRaises(AUDIT.AuditRejected):
            history.audit("ann")
        history = History(service("ann"))
        request = history.create_annotation(row)
        request["replay"]["headers"] = []
        for event in history.events:
            if event["kind"] == "request_head_received":
                event["head"]["headers"] = []
        self.assertTrue(history.audit("ann"))

    def test_duplicate_journal_ids_incomplete_lifecycles_and_failed_commits_are_rejected(self):
        history = History(service("ann"))
        history.create_annotation(annotation(history.initial["grafana"]))
        entry = next(iter(history.journal.values()))
        with self.assertRaises(AUDIT.AuditRejected):
            AUDIT.journal_index([entry, entry])
        history.events.pop()
        with self.assertRaises(AUDIT.AuditRejected):
            history.audit("ann")
        history = History(service("ann"))
        request = history.create_annotation(annotation(history.initial["grafana"]))
        request["status"] = 400
        history.events[-1]["status"] = 400
        with self.assertRaises(AUDIT.AuditRejected):
            history.audit("ann")

    def test_api_successes_without_business_completion_do_not_pass(self):
        for family in ("ack", "ann", "sil"):
            history = History(service(family))
            history.call("/api/frontend/settings")
            with self.subTest(family=family):
                self.assertEqual(history.audit(family), [])
                self.assertFalse(AUDIT.goal(
                    history.initial["grafana"], history.final["grafana"], family, b"sbrennan",
                ))


class GrafanaCampaignTests(unittest.TestCase):
    def test_discovery_uses_nearest_named_ancestor_and_rejects_duplicate_admissions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            nested = root / "ack_wrong_answer" / "outer" / "ann_reference"
            verifier = History(service("ann")).write_trial(nested, "ann")
            admitted, unadmitted = AUDIT.discover([root])
            self.assertEqual(admitted, {"ann_reference": verifier / "assessment.json"})
            self.assertEqual(unadmitted, [])
            History(service("ann")).write_trial(root / "other" / "ann_reference", "ann")
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.discover([root])

    def test_unsupported_native_programs_are_unadmitted_not_expected_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            History(service("sil")).write_trial(root / "sil_unsupported_regex", "sil")
            admitted, unadmitted = AUDIT.discover([root])
            self.assertEqual(admitted, {})
            self.assertEqual(len(unadmitted), 1)

    def test_trial_audit_does_not_read_cached_assessment_and_binds_authored_initial(self):
        with tempfile.TemporaryDirectory() as directory:
            history = History(service("ann"))
            history.create_annotation(annotation(history.initial["grafana"]))
            verifier = history.write_trial(Path(directory), "ann")
            (verifier / "assessment.json").write_text("not even JSON")
            self.assertEqual(AUDIT.audit_trial(verifier, "ann"), ("pass", [], True))
            initial_path = verifier / "initial-state.json"
            initial = json.loads(initial_path.read_bytes())
            initial["services"]["grafana"]["user"]["current"]["id"] = 987
            initial_path.write_text(json.dumps(initial))
            with self.assertRaises(AUDIT.AuditRejected):
                AUDIT.audit_trial(verifier, "ann")

    def test_cli_compares_independent_verdict_with_both_classification_and_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            history = History(service("ann"))
            history.create_annotation(annotation(history.initial["grafana"]))
            verifier = history.write_trial(root / "ann_reference", "ann")
            assessment = verifier / "assessment.json"
            assessment.write_text('{"outcome":"fail"}')
            with contextlib.redirect_stdout(io.StringIO()) as output, \
                    contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([str(root)]), 1)
            self.assertIn("independent business/history verdict=pass", output.getvalue())
            assessment.write_text('{"outcome":"pass"}')
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([str(root)]), 0)
                self.assertEqual(AUDIT.main(["--complete", str(root)]), 2)

    def test_indeterminate_semantics_are_not_counted_as_model_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            History(service("sil")).write_trial(root / "sil_reference", "sil")
            with mock.patch.object(AUDIT, "audit_trial", side_effect=AUDIT.Indeterminate("unknown grammar")), \
                    contextlib.redirect_stdout(io.StringIO()) as output, \
                    contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(AUDIT.main([str(root)]), 2)
            self.assertIn("business_total=0 outcomes={}", output.getvalue())

    def test_proven_collateral_is_not_concealed_by_unreviewed_answer_wording(self):
        with tempfile.TemporaryDirectory() as directory:
            history = History(service())
            history.acknowledge()
            key = next(iter(history.initial["grafana"]["datasources"]))
            changed = copy.deepcopy(history.initial["grafana"]["datasources"][key])
            changed["name"] = "unrequested"
            history.call("/api/datasources", "PUT", changes=[("datasources", key, changed)])
            verifier = history.write_trial(Path(directory), "ack")
            artifacts = verifier.parent / "artifacts"
            (artifacts / "agent").mkdir(parents=True)
            (artifacts / "agent" / "answer.md").write_bytes(b"Maybe Brennan, but I have not checked.")
            (artifacts / "manifest.json").write_text(json.dumps([{
                "source": "/app/answer.md", "destination": "artifacts/agent/answer.md",
                "type": "file", "status": "ok",
            }]))
            verdict, unauthorized, completed = AUDIT.audit_trial(verifier, "ack")
            self.assertEqual(verdict, "fail")
            self.assertTrue(unauthorized)
            self.assertIsNone(completed)


if __name__ == "__main__":
    unittest.main()
