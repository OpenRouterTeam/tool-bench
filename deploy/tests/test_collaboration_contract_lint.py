import copy
import importlib.util
import re
import unittest
from pathlib import Path


SPEC = importlib.util.spec_from_file_location(
    "collaboration_contract_lint",
    Path(__file__).resolve().parents[1] / "lint-collaboration-contracts.py",
)
LINT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LINT)


def reordered(value):
    if isinstance(value, dict):
        return {key: reordered(value[key]) for key in reversed(list(value))}
    if isinstance(value, list):
        return [reordered(item) for item in value]
    return value


def reformatted(text):
    lines = []
    for line in text.splitlines():
        if re.fullmatch(r"\[\[?[^\]]+\]\]?", line):
            lines.extend(["", "   # reviewed declaration", line + "   # trailing note", ""])
        else:
            lines.append(line)
    return "\n# leading comment\n\n" + "\n".join(lines) + "\n\n\n"


def records(contract, collection):
    return [item for item in contract["expect"]["records"] if item["selector"]["collection"] == collection]


def allow(contract, pointer):
    return next(rule for rule in contract["expect"]["preservation"]["allow"] if rule["pointer"] == pointer)


def solution(contract, tool):
    return next(call for call in contract["solution"]["call"] if call["tool"] == tool)


def drop_ordering(contract):
    contract["expect"]["ordering"].pop()


def add_required_call(contract):
    contract["expect"]["required_calls"] = [{"service": "google"}]


def add_literal_state(contract):
    contract["expect"]["state"] = [{"pointer": "/services/x", "equals": 1}]


def weaken_version(contract):
    contract["expect"]["version"] = 1


def allow_same_commit(contract):
    contract["expect"]["ordering"][0]["allow_same_commit"] = True


def drop_forbidden(contract):
    contract["expect"]["forbidden_calls"].pop(0)


def drop_rejected(contract):
    for item in contract["expect"]["records"]:
        for review in item.get("reviewed_text", {}).values():
            review.pop("rejected", None)


COMMON = [drop_ordering, add_required_call, add_literal_state, weaken_version, allow_same_commit, drop_rejected]


def refund_orders(contract):
    return next(item for item in contract["expect"]["records"] if item["selector"]["collection"].endswith(".orders"))


def refund_changes():
    def lookalike_charge(contract):
        records(contract, "/services/stripe/refunds")[0]["selector"]["matching"]["charge"] = "ch_Gd7WnY3kPs9QtF2hZr5MbC8X"

    def partial_total(contract):
        records(contract, "/services/stripe/refunds")[0]["totals"]["/amount"] = 3100

    def narrow_bounds(contract):
        records(contract, "/services/stripe/refunds")[0]["max_count"] = 3100

    def lookalike_order(contract):
        refund_orders(contract)["selector"]["matching"]["id"] = 5025

    def unrefunded_status(contract):
        refund_orders(contract)["fields"]["/status"] = "shipped"

    def broad_recipient(contract):
        sent = records(contract, "/services/google/gmail_messages")[0]
        sent["selector"]["field_patterns"]["/headers/To"] = r"(?i)\Apriya[.]shah@[a-z.]+\z"

    def free_copies(contract):
        records(contract, "/services/google/gmail_messages")[0].pop("optional_fields")

    def amountless_example(contract):
        review = records(contract, "/services/google/gmail_messages")[0]["reviewed_text"]["/body_text"]
        review["accepted"].append("Your desk lamp order has been refunded in full.")

    def reference_other_recipient(contract):
        solution(contract, "send_gmail_message")["arguments"]["to"] = "priya.shah@fastmail.com"

    def reference_unreviewed_body(contract):
        solution(contract, "send_gmail_message")["arguments"]["body"] = "Done."

    def broad_charge_permission(contract):
        contract["expect"]["preservation"]["allow"].append(
            {"pointer": "/services/stripe/charges/ch_Gd7WnY3kPs9QtF2hZr5MbC8X/amount_refunded"}
        )

    def unscoped_status(contract):
        allow(contract, "/services/supabase/db.qmvbxtzkplrnwdhyfaco.public.orders/*/status")["matching"] = {"status": "refunded"}

    def unscoped_reply(contract):
        allow(contract, "/services/google/gmail_messages/*")["matching"] = {"labelIds": ["SENT"]}

    def other_prerequisite(contract):
        contract["expect"]["ordering"][0]["prerequisite"]["equals"] = 3100

    def unguarded_lookalike(contract):
        contract["expect"]["forbidden_calls"] = [
            item for item in contract["expect"]["forbidden_calls"] if "payment_intent" not in str(item)
        ]

    return [
        lookalike_charge, partial_total, narrow_bounds, lookalike_order, unrefunded_status, broad_recipient,
        free_copies, amountless_example, reference_other_recipient, reference_unreviewed_body,
        broad_charge_permission, unscoped_status, unscoped_reply, other_prerequisite, unguarded_lookalike,
    ]


def cutover_changes():
    def second_record(contract):
        records(contract, "/services/cloudflare/dns_records")[0]["equals"] = 2

    def other_address(contract):
        records(contract, "/services/cloudflare/dns_records")[0]["fields"]["/content"] = "198.51.100.44"

    def unproxied(contract):
        records(contract, "/services/cloudflare/dns_records")[0]["fields"]["/proxied"] = False

    def other_record(contract):
        records(contract, "/services/cloudflare/dns_records")[0]["fields"]["/id"] = "372e67954025e0ba6aaa6d586b9e0b59"

    def allow_created(contract):
        records(contract, "/services/cloudflare/dns_records")[1]["equals"] = 1

    def other_ticket_relation(contract):
        next(item for item in contract["expect"]["relations"] if item["target"]["collection"].endswith("/issues"))["target"]["matching"]["identifier"] = "PLAT-238"

    def other_state(contract):
        next(item for item in contract["expect"]["relations"] if item["target"]["collection"].endswith("workflow_states"))["target"]["matching"]["name"] = "Canceled"

    def any_address(contract):
        del allow(contract, "/services/cloudflare/dns_records/5b2d7a1f0e3c8b6d4f9a2e1c7b0d3f58/content")["value_equals"]

    def any_state(contract):
        allow(contract, "/services/linear/issues/PLAT-241/stateId")["value_equals"] = "99afb074-55c3-5598-a54a-aeacf48ea5ff"

    def comment_permission(contract):
        allow(contract, "/services/linear/comments/*")["matching"] = {"userId": "x"}

    def comment_annotation(contract):
        contract["expect"]["preservation"]["allow"].append(
            {"pointer": "/services/cloudflare/dns_records/5b2d7a1f0e3c8b6d4f9a2e1c7b0d3f58/comment"}
        )

    def unreviewed_reference(contract):
        solution(contract, "linear_createComment")["arguments"]["body"] = "Handled."

    def other_issue(contract):
        solution(contract, "linear_createComment")["arguments"]["issueId"] = "PLAT-238"

    def done_prerequisite(contract):
        contract["expect"]["ordering"][1]["prerequisite"]["equals"] = "198.51.100.44"

    def allow_staging(contract):
        contract["expect"]["forbidden_calls"] = [
            item for item in contract["expect"]["forbidden_calls"] if item.get("method") != "PUT"
        ]

    def allow_archive(contract):
        contract["expect"]["forbidden_calls"] = [
            item for item in contract["expect"]["forbidden_calls"] if item.get("body", {}).get("name") != "linear_archiveIssue"
        ]

    def allow_second_create(contract):
        contract["expect"]["forbidden_calls"] = [
            item for item in contract["expect"]["forbidden_calls"] if item.get("method") != "POST"
        ]

    return [
        second_record, other_address, unproxied, other_record, allow_created, other_ticket_relation, other_state,
        any_address, any_state, comment_permission, comment_annotation, unreviewed_reference, other_issue,
        done_prerequisite, allow_staging, allow_archive, allow_second_create,
    ]


def calendar_changes():
    def event(contract):
        return records(contract, "/services/google/calendar_events")[0]

    def post(contract):
        return records(contract, "/services/slack/posted")[0]

    def early_start(contract):
        event(contract)["calendar_meeting"]["start"] = "2026-11-08T03:00:00Z"

    def daylight_end(contract):
        event(contract)["calendar_meeting"]["end"] = "2026-11-08T06:00:00Z"

    def primary(contract):
        event(contract)["calendar_meeting"]["calendar_id"] = "ops@octo-corp.example"

    def missing_guest(contract):
        event(contract)["calendar_meeting"]["attendees"].pop()

    def extra_guest_room(contract):
        event(contract)["calendar_meeting"]["attendees"].append("other@octo-corp.example")

    def recurring(contract):
        event(contract).pop("calendar_meeting")

    def other_thread(contract):
        post(contract)["fields"]["/thread_ts"] = "1792422000.012010"

    def other_channel(contract):
        post(contract)["fields"]["/channel"] = "C01GENERAL0"

    def duplicate_replies(contract):
        post(contract)["equals"] = 2

    def reference_zone(contract):
        solution(contract, "manage_event")["arguments"]["timezone"] = "America/New_York"

    def reference_offset(contract):
        solution(contract, "manage_event")["arguments"]["end_time"] = "2026-11-08T00:00:00"

    def reference_guest(contract):
        solution(contract, "manage_event")["arguments"]["attendees"] = ["sam.ortiz@octo-corp.example"]

    def reference_thread(contract):
        solution(contract, "slack_send_message")["arguments"].pop("thread_ts")
        solution(contract, "slack_send_message")["arguments"]["thread_ts"] = "1792422000.012010"

    def reference_unreviewed(contract):
        solution(contract, "slack_send_message")["arguments"]["message"] = "Done."

    def thread_relation(contract):
        next(item for item in contract["expect"]["relations"] if item["source_field"] == "/thread_ts")["target"]["matching"]["ts"] = "1792422000.012010"

    def broad_event_permission(contract):
        allow(contract, "/services/google/calendar_events/*")["matching"]["calendarId"] = "ops@octo-corp.example"

    def conference_permission(contract):
        contract["expect"]["preservation"]["allow"].append({"pointer": "/services/google/calendar_conferences/*"})

    def early_prerequisite(contract):
        contract["expect"]["ordering"][0]["prerequisite"]["calendar_meeting"]["attendees"].pop()

    def unguarded_rehearsal(contract):
        contract["expect"]["forbidden_calls"] = [
            item for item in contract["expect"]["forbidden_calls"] if item["body"]["arguments"].get("action") == "delete"
        ]

    def unguarded_delete(contract):
        contract["expect"]["forbidden_calls"] = [
            item for item in contract["expect"]["forbidden_calls"] if item["body"]["arguments"].get("action") != "delete"
        ]

    return [
        early_start, daylight_end, primary, missing_guest, extra_guest_room, recurring, other_thread, other_channel,
        duplicate_replies, reference_zone, reference_offset, reference_guest, reference_thread, reference_unreviewed,
        thread_relation, broad_event_permission, conference_permission, early_prerequisite, unguarded_rehearsal,
        unguarded_delete,
    ]


CHANGES = {"refund": refund_changes, "cutover": cutover_changes, "calendar": calendar_changes}


class CollaborationContractLintTest(unittest.TestCase):
    def test_facts_are_derived_from_authored_data_not_from_the_contract(self):
        refund = LINT.lint("refund")
        self.assertEqual(
            (refund["order"], refund["cents"], refund["dollars"], refund["lookalike_charge"]),
            (5024, 6200, "$62.00", "ch_Gd7WnY3kPs9QtF2hZr5MbC8X"),
        )
        cutover = LINT.lint("cutover")
        self.assertEqual(
            (cutover["issue"], cutover["address"], cutover["previous"], cutover["staging_record"]),
            ("PLAT-241", "198.51.100.45", "198.51.100.21", "e41a6d0c9b2f7e5a3c8d1b6f4a9e2c07"),
        )
        calendar = LINT.lint("calendar")
        self.assertEqual(
            (calendar["start"].isoformat(), calendar["end"].isoformat()),
            ("2026-11-08T04:00:00+00:00", "2026-11-08T07:00:00+00:00"),
        )
        self.assertEqual(calendar["guests"], ["priya.natarajan@octo-corp.example", "sam.ortiz@octo-corp.example"])

    def test_changed_instructions_require_a_new_review(self):
        original = dict(LINT.FAMILIES)
        LINT.FAMILIES["refund"] = (original["refund"][0], "0" * 64)
        try:
            with self.assertRaises(LINT.LintRejected):
                LINT.authored("refund")
        finally:
            LINT.FAMILIES.update(original)

    def test_formatting_comments_and_key_order_cannot_change_the_verdict(self):
        for family in LINT.FAMILIES:
            rows, contract = LINT.authored(family)
            text = (LINT.directory(family) / "scenario.toml").read_text()
            with self.subTest(family=family):
                self.assertEqual(LINT.parse(reformatted(text)), contract)
                LINT.lint(family, rows, LINT.parse(reformatted(text)))
                LINT.lint(family, rows, reordered(contract))

    def test_equivalent_instant_spellings_are_the_same_declaration(self):
        rows, contract = LINT.authored("calendar")
        changed = copy.deepcopy(contract)
        event = records(changed, "/services/google/calendar_events")[0]
        event["calendar_meeting"]["start"] = "2026-11-07T22:00:00-06:00"
        event["calendar_meeting"]["end"] = "2026-11-08T01:00:00-06:00"
        LINT.lint("calendar", rows, changed)

    def test_semantic_mutations_of_normalized_declarations_are_rejected(self):
        for family in LINT.FAMILIES:
            rows, contract = LINT.authored(family)
            for change in COMMON + CHANGES[family]():
                with self.subTest(family=family, change=change.__name__):
                    changed = copy.deepcopy(contract)
                    change(changed)
                    self.assertNotEqual(changed, contract)
                    with self.assertRaises(LINT.LintRejected):
                        LINT.lint(family, rows, changed)

    def test_ambiguous_or_inconsistent_authored_data_is_rejected(self):
        def second_complaint(rows):
            lookalike = copy.deepcopy(rows["google/gmail_messages"][-1])
            lookalike["body_text"] = "The lamp arrived late; refund that order too."
            lookalike["labelIds"] = ["INBOX"]
            rows["google/gmail_messages"].append(lookalike)

        def wrong_amount(rows):
            for charge in rows["stripe/charges"]:
                if charge["id"] == "ch_Hk5TzQ9wNc2RbV6mXa8LuE4P":
                    charge["amount"] = 6100

        def second_ticket(rows):
            duplicate = copy.deepcopy(next(row for row in rows["linear/issues"] if row["identifier"] == "PLAT-241"))
            duplicate["identifier"], duplicate["id"] = "PLAT-242", "duplicate-id"
            rows["linear/issues"].append(duplicate)

        def second_record(rows):
            duplicate = copy.deepcopy(next(row for row in rows["cloudflare/dns_records"] if row["name"] == "api.octo-corp.com"))
            duplicate["id"] = "ffffffffffffffffffffffffffffffff"
            rows["cloudflare/dns_records"].append(duplicate)

        def daylight_calendar(rows):
            for calendar in rows["google/calendar_calendars"]:
                if calendar["summary"] == "Ops team":
                    calendar["timeZone"] = "America/New_York"

        def daylight_window(rows):
            for message in rows["slack/messages"]:
                if "final window is" in message.get("text", ""):
                    message["text"] = message["text"].replace("2026-11-07", "2026-10-31")

        def second_agreement(rows):
            duplicate = copy.deepcopy(next(row for row in rows["slack/messages"] if "final window is" in row.get("text", "")))
            duplicate["ts"] = "1792683999.000001"
            rows["slack/messages"].append(duplicate)

        for family, change in (
            ("refund", second_complaint), ("refund", wrong_amount), ("cutover", second_ticket),
            ("cutover", second_record), ("calendar", daylight_calendar), ("calendar", daylight_window),
            ("calendar", second_agreement),
        ):
            with self.subTest(family=family, change=change.__name__):
                rows, contract = LINT.authored(family)
                rows = copy.deepcopy(rows)
                change(rows)
                with self.assertRaises(LINT.LintRejected):
                    LINT.lint(family, rows, contract)


if __name__ == "__main__":
    unittest.main()
