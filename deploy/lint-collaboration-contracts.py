"""Static source-fact contract lint for three remaining collaboration scenarios.

This is NOT an independent business, history or runtime auditor. It parses each
scenario.toml with tomllib into typed declarations and checks that their
meaning (selectors, relations, permissions, ordering, reviewed examples, forbidden
requests and reference arguments) agrees with business facts derived from the
reviewed instruction and authored dataset. Whitespace, comments and key order
cannot change its verdict. Behavioral evidence remains the executed Rust
contract tests and any separately qualified runtime. It does not establish
information acquisition, provider/hosted conformance or native client/model
behavior. Ambiguous authored evidence is rejected rather than guessed.
Requires Python 3.11 or newer.
"""

import argparse
import copy
import hashlib
import json
import re
import sys
import tomllib
from datetime import datetime, timedelta, timezone
from decimal import Decimal
from pathlib import Path
from zoneinfo import ZoneInfo


REPO = Path(__file__).resolve().parents[1]
FAMILIES = {
    "refund": (
        "google-refund-damaged-order",
        "0f4f6c08873ea443076f9b9c77800000f81d8a8f39a429f30a115823ee2522c1",
    ),
    "cutover": (
        "linear-cloudflare-api-origin-cutover",
        "95f9e60d11504c6b0fdfc290e38de5cf729cd4c170d2530daec1ee3e36651527",
    ),
    "calendar": (
        "slack-google-cutover-calendar",
        "247331a91e3a1e0e8bb53fe86622c769586b435cc383347b97b3d5e53ef48051",
    ),
}


class LintRejected(ValueError):
    """Missing, ambiguous or inconsistent source evidence."""


def require(condition, message):
    if not condition:
        raise LintRejected(message)


def read(path):
    return json.loads(Path(path).read_text(), parse_float=Decimal)


def only(items, message):
    items = list(items)
    require(len(items) == 1, message)
    return items[0]


def fold(text):
    return " ".join(text.split())


def instant(text):
    return datetime.fromisoformat(text.replace("Z", "+00:00")).astimezone(timezone.utc)


def directory(family):
    require(family in FAMILIES, "unknown collaboration family")
    return REPO / "scenarios/collab/multi" / FAMILIES[family][0]


def authored(family):
    base = directory(family)
    require(
        hashlib.sha256((base / "instruction.md").read_bytes()).hexdigest() == FAMILIES[family][1],
        "instruction changed; independent business review required",
    )
    rows = {}
    for provider in sorted((base / "dataset").iterdir()):
        for source in sorted(provider.glob("*.json")):
            rows[provider.name + "/" + source.stem] = read(source)
    return rows, parse((base / "scenario.toml").read_text())


def parse(text):
    return tomllib.loads(text)


def cents(amount):
    scaled = Decimal(amount) * 100
    require(scaled == scaled.to_integral_value(), "amount is not a whole number of cents")
    return int(scaled)


def derive_refund(rows):
    complaint = only(
        (
            message
            for message in rows["google/gmail_messages"]
            if "refund" in message["body_text"].lower()
            and "arrived" in message["body_text"].lower()
            and "INBOX" in message["labelIds"]
        ),
        "the complaint email is missing or ambiguous",
    )
    display, sender = re.fullmatch(r"(.*?)\s*<([^>]+)>", complaint["headers"]["From"]).groups()
    day = re.search(r"order on (October \d+)", complaint["body_text"])
    year = re.search(r"\b(20\d\d)\b", complaint["headers"]["Date"])
    require(day is not None and year is not None, "the complaint does not date the order")
    date = datetime.strptime(f"{day.group(1)} {year.group(1)}", "%B %d %Y").date().isoformat()
    project = only(
        (row for row in rows["supabase/projects"] if row["name"] == "storefront-prod"),
        "the production project is missing or ambiguous",
    )["id"]
    customers = rows[f"supabase/db.{project}.public.customers"]
    orders = rows[f"supabase/db.{project}.public.orders"]
    customer = only((row for row in customers if row["email"].lower() == sender.lower()), "the complaining customer is missing or ambiguous")
    order = only(
        (row for row in orders if row["customer_id"] == customer["id"] and row["created_at"].startswith(date)),
        "the complained order is missing or ambiguous",
    )
    charge = only((row for row in rows["stripe/charges"] if row["id"] == order["stripe_charge_id"]), "the order's payment is missing")
    require(charge["amount"] == cents(order["total"]), "order total and payment disagree")
    require(charge["amount_refunded"] == 0, "payment is not refundable as authored")
    lookalikes = [
        row
        for row in orders
        if row["customer_id"] != customer["id"]
        and row["created_at"].startswith(date)
        and row["total"] == order["total"]
        and row["stripe_charge_id"]
    ]
    other_customer = only((row for row in customers if row["id"] == only(lookalikes, "the look-alike order is missing or ambiguous")["customer_id"]), "the look-alike customer is missing")
    only(
        (
            row
            for row in rows["google/gmail_messages"]
            if row["headers"]["From"].startswith(display) and other_customer["email"].lower() in row["headers"]["From"].lower()
        ),
        "the look-alike sender is missing or ambiguous",
    )
    other_charge = only((row for row in rows["stripe/charges"] if row["id"] == lookalikes[0]["stripe_charge_id"]), "the look-alike payment is missing")
    return {
        "project": project,
        "recipient": sender,
        "lookalike_recipient": other_customer["email"],
        "thread": complaint["threadId"],
        "order": order["id"],
        "charge": charge["id"],
        "intent": charge["payment_intent"],
        "lookalike_charge": other_charge["id"],
        "lookalike_intent": other_charge["payment_intent"],
        "cents": charge["amount"],
        "dollars": f"${Decimal(charge['amount']) / 100:.2f}",
    }


def derive_cutover(rows):
    team = only((row for row in rows["linear/teams"] if row["key"] == "PLAT"), "the Platform team is missing")
    states = [row for row in rows["linear/workflow_states"] if row["teamId"] == team["id"]]
    todo = {row["id"] for row in states if row["type"] == "unstarted"}
    done = only((row for row in states if row["type"] == "completed"), "the Done state is missing or ambiguous")
    # Picking the ticket up before closing it is ordinary workflow, so the team's In Progress
    # state may be passed through on the way to Done.
    in_progress = only(
        (row for row in states if row["type"] == "started" and row["name"] == "In Progress"),
        "the In Progress state is missing or ambiguous",
    )
    ticket = only(
        (row for row in rows["linear/issues"] if row["teamId"] == team["id"] and row["stateId"] in todo and "origin-3" in row["title"]),
        "the open origin-3 ticket is missing or ambiguous",
    )
    request = re.search(
        r"update the existing A record for ([a-z0-9.-]+) to (\d+\.\d+\.\d+\.\d+) and keep it proxied",
        ticket["description"],
    )
    require(request is not None, "the ticket does not state the requested change")
    host, address = request.groups()
    zone = only((row for row in rows["cloudflare/zones"] if host.endswith("." + row["name"])), "the zone is missing or ambiguous")
    record = only(
        (row for row in rows["cloudflare/dns_records"] if row["zone_id"] == zone["id"] and row["name"] == host and row["type"] == "A"),
        "the existing A record is missing or ambiguous",
    )
    require(record["content"] != address and record["proxied"] is True, "the record already satisfies the request")
    label = host.split(".")[0]
    staging = only(
        (row for row in rows["cloudflare/dns_records"] if row["zone_id"] != zone["id"] and row["name"].split(".")[0] == label),
        "the staging look-alike record is missing or ambiguous",
    )
    return {
        "issue": ticket["identifier"],
        "issue_id": ticket["id"],
        "team": team["id"],
        "done": done["id"],
        "done_name": done["name"],
        "in_progress": in_progress["id"],
        "zone": zone["id"],
        "record": record["id"],
        "staging_zone": staging["zone_id"],
        "staging_record": staging["id"],
        "host": host,
        "address": address,
        "previous": record["content"],
    }


def derive_calendar(rows):
    instruction = (directory("calendar") / "instruction.md").read_text()
    title = re.search(r'titled "([^"]+)"', instruction)
    require(title is not None, "the requested event title is missing")
    channel = only((row for row in rows["slack/channels"] if row.get("name") == "db-migration"), "the migration channel is missing")["id"]
    thread = [row for row in rows["slack/messages"] if row["channel"] == channel]
    root = only((row for row in thread if "thread_ts" not in row and "production cutover window" in row["text"]), "the thread root is missing or ambiguous")
    replies = sorted((row for row in thread if row.get("thread_ts") == root["ts"]), key=lambda row: row["ts"])
    agreed = only((row for row in replies if "final window is" in row["text"]), "the final agreement is missing or ambiguous")
    window = re.search(r"(\d{4}-\d\d-\d\d), (\d\d):(\d\d)-(\d\d):(\d\d) CT", agreed["text"])
    require(window is not None, "the agreed window is not stated")
    calendar = only((row for row in rows["google/calendar_calendars"] if row["summary"] == "Ops team"), "the Ops team calendar is missing")
    zone = ZoneInfo(calendar["timeZone"])
    day, start_hour, start_minute, end_hour, end_minute = window.groups()
    start = datetime.fromisoformat(day).replace(hour=int(start_hour), minute=int(start_minute), tzinfo=zone)
    end = datetime.fromisoformat(day).replace(hour=int(end_hour), minute=int(end_minute), tzinfo=zone)
    if end <= start:
        end = (datetime.fromisoformat(day) + timedelta(days=1)).replace(hour=int(end_hour), minute=int(end_minute), tzinfo=zone)
    require(start.strftime("%z") == "-0600", "the agreed window is not on standard time")
    request = only((row for row in replies if "Ops team calendar" in row["text"]), "the calendar request is missing or ambiguous")
    users = {row["id"]: row for row in rows["slack/users"]}
    priya = only(
        (users[identity] for identity in {row["user"] for row in replies} if users[identity]["real_name"].startswith("Priya")),
        "Priya is missing or ambiguous",
    )
    existing = [row for row in rows["google/calendar_events"] if row["calendarId"] == calendar["id"] and "PG16" in row["summary"]]
    return {
        "summary": title.group(1),
        "calendar": calendar["id"],
        "calendar_zone": calendar["timeZone"],
        "channel": channel,
        "thread": root["ts"],
        "start": start.astimezone(timezone.utc),
        "end": end.astimezone(timezone.utc),
        "guests": sorted([users[request["user"]]["email"], priya["email"]]),
        "rehearsals": sorted(row["id"] for row in existing),
    }


def expectation(contract):
    return contract.get("expect", {})


def checks(contract, kind, collection=None):
    found = expectation(contract).get(kind, [])
    if collection is None:
        return list(found)
    return [item for item in found if item["selector"]["collection"] == collection]


def single_check(contract, collection, message, **selector):
    candidates = [
        item
        for item in checks(contract, "records", collection)
        if all(item["selector"].get(key) == value for key, value in selector.items())
    ]
    return only(candidates, message)


def allowed(contract):
    return expectation(contract).get("preservation", {}).get("allow", [])


def permitted_pointers(contract, required, optional=()):
    pointers = {rule["pointer"] for rule in allowed(contract)}
    missing = set(required) - pointers
    extra = pointers - set(required) - set(optional)
    require(not missing, f"missing permission {sorted(missing)}")
    require(not extra, f"unreviewed permission {sorted(extra)}")


def permission(contract, pointer):
    return only((rule for rule in allowed(contract) if rule["pointer"] == pointer), f"permission {pointer} is missing or ambiguous")


def forbidden_requests(contract):
    return expectation(contract).get("forbidden_calls", [])


def reviewed_examples(check, field, must_contain):
    text = check["reviewed_text"][field]
    accepted = [fold(example) for example in text["accepted"]]
    rejected = [fold(example) for example in text.get("rejected", [])]
    require(accepted and rejected, "reviewed text needs accepted and rejected examples")
    require(not set(accepted) & set(rejected), "an example is both accepted and rejected")
    for example in accepted:
        for needed in must_contain:
            require(needed in example, f"accepted example lacks {needed!r}")
    return set(accepted)


def reference_calls(contract, tool):
    return [call for call in contract.get("solution", {}).get("call", []) if call["tool"] == tool]


def common_shape(contract, orderings):
    expected = expectation(contract)
    require(expected.get("version") == 2 and expected.get("audit_history") is True, "contract is not an audited version 2 contract")
    for kind in ("required_calls", "state"):
        require(not expected.get(kind), f"contract keeps route or literal requirement {kind}")
    require(len(expected.get("ordering", [])) == orderings, "causal requirements differ from the reviewed set")
    for ordering in expected.get("ordering", []):
        require(not ordering.get("allow_same_commit") and not ordering.get("require_transition"), "ordering weakens or forces a transition")


def lint_refund(facts, contract):
    common_shape(contract, 2)
    base = f"/services/supabase/db.{facts['project']}.public"
    refunds = single_check(contract, "/services/stripe/refunds", "refund outcome is missing or ambiguous", created=True)
    require(refunds["selector"]["matching"] == {"charge": facts["charge"]}, "refund is not bound to the order's actual charge")
    require(refunds["fields"].get("/payment_intent") == facts["intent"], "refund is not bound to the charge's payment intent")
    require(refunds["totals"] == {"/amount": facts["cents"]}, "refund total differs from the payment amount")
    require(refunds["integer_bounds"]["/amount"] == {"min": 1, "max": facts["cents"]}, "refund part bounds differ from the payment amount")
    require(refunds["min_count"] == 1 and refunds["max_count"] == facts["cents"], "refund count bounds differ")
    order = single_check(contract, f"{base}.orders", "order outcome is missing or ambiguous")
    require(order["selector"]["matching"] == {"id": facts["order"]} and order["equals"] == 1, "order outcome is not the complained order")
    require(order["fields"].get("/status") == "refunded", "order status is not refunded")
    customer = order["selector"]["references"]
    require(len(customer) == 1 and customer[0]["target"]["matching"] == {"email": facts["recipient"]}, "order is not tied to the complaining customer")
    sent = single_check(contract, "/services/google/gmail_messages", "reply outcome is missing or ambiguous", created=True)
    require(sent["equals"] == 1 and sent["fields"] == {"/labelIds": ["SENT"]}, "reply is not exactly one sent message")
    pattern = re.compile(sent["selector"]["field_patterns"]["/headers/To"].replace(r"\z", r"\Z"))
    for address in (facts["recipient"], f"Priya Shah <{facts['recipient']}>"):
        require(pattern.search(address), "recipient pattern rejects the complaining customer")
    for address in (facts["lookalike_recipient"], f"{facts['recipient']}, {facts['lookalike_recipient']}", f"x{facts['recipient']}"):
        require(not pattern.search(address), "recipient pattern accepts another address")
    require(sent["optional_fields"] == {"/headers/Cc": "", "/headers/Bcc": ""}, "reply copies are not constrained")
    accepted = reviewed_examples(sent, "/body_text", [facts["dollars"]])
    for call in reference_calls(contract, "send_gmail_message"):
        arguments = call["arguments"]
        require(arguments["to"] == facts["recipient"] and arguments["thread_id"] == facts["thread"], "reference reply targets another thread or recipient")
        require(fold(arguments["body"]) in accepted, "reference reply is not an accepted example")
    for relation in expectation(contract)["relations"]:
        if relation["kind"] == "equals":
            require(relation["source"]["created"] is True and relation["target"]["matching"] == {"id": facts["thread"]}, "thread relation is not bound to the complaint")
            require(relation["source_field"] == relation["target_field"] == "/threadId", "thread relation uses another field")
    foreign = only(
        (rule for rule in expectation(contract)["relations"] if rule["kind"] == "foreign_key"),
        "charge relation is missing or ambiguous",
    )
    require(foreign["source"]["matching"] == {"charge": facts["charge"]} and foreign["source_field"] == "/charge", "charge relation source differs")
    require(foreign["target"]["matching"] == {"id": facts["order"]} and foreign["target_field"] == "/stripe_charge_id", "charge relation target differs")
    charge = f"/services/stripe/charges/{facts['charge']}"
    status = f"{base}.orders/*/status"
    permitted_pointers(
        contract,
        [f"{charge}/amount_refunded", f"{charge}/refunded", "/services/stripe/refunds/*", "/services/stripe/balance_transactions/*", "/services/stripe/events/*", status, "/services/google/gmail_messages/*"],
        ["/services/supabase/migrations/*"],
    )
    rule = permission(contract, status)
    require(rule["matching"] == {"id": facts["order"], "status": "refunded"} and rule.get("max_count", 1) == 1, "status permission is not scoped to the order")
    mail = permission(contract, "/services/google/gmail_messages/*")
    require(mail["matching"] == {"threadId": facts["thread"], "labelIds": ["SENT"]} and mail.get("kind") == "create", "reply permission is not scoped to the thread")
    for ordering in expectation(contract)["ordering"]:
        prerequisite = ordering["prerequisite"]
        require(prerequisite["selector"]["matching"] == {"id": facts["charge"]} and prerequisite["field"] == "/amount_refunded" and prerequisite["equals"] == facts["cents"], "ordering prerequisite is not the completed refund")
    actions = {ordering["action"]["selector"]["collection"]: ordering["action"] for ordering in expectation(contract)["ordering"]}
    require(actions[f"{base}.orders"]["equals"] == "refunded" and actions["/services/google/gmail_messages"]["has_value"] == "SENT", "ordering actions differ")
    guarded = {
        (request["body"]["arguments"].get("parameters", {}).get("charge"), request["body"]["arguments"].get("parameters", {}).get("payment_intent"), request["body"]["arguments"].get("to"))
        for request in forbidden_requests(contract)
    }
    require(
        {(facts["lookalike_charge"], None, None), (None, facts["lookalike_intent"], None), (None, None, facts["lookalike_recipient"])} <= guarded,
        "look-alike refund or recipient attempts are not forbidden",
    )


def lint_cutover(facts, contract):
    common_shape(contract, 2)
    record = single_check(contract, "/services/cloudflare/dns_records", "DNS outcome is missing or ambiguous", matching={"zone_id": facts["zone"], "name": facts["host"], "type": "A"})
    require(record["equals"] == 1, "DNS selection does not forbid a second record")
    require(record["fields"] == {"/id": facts["record"], "/content": facts["address"], "/proxied": True, "/ttl": 1}, "DNS final fields differ from the ticket")
    created = single_check(contract, "/services/cloudflare/dns_records", "created-record guard is missing", created=True)
    require(created["equals"] == 0, "a created DNS record is not rejected")
    comment = single_check(contract, "/services/linear/comments", "comment outcome is missing or ambiguous")
    require(comment["selector"]["created"] is True and comment["equals"] == 1, "comment is not exactly one new comment")
    accepted = reviewed_examples(comment, "/body", [facts["address"], facts["host"]])
    for call in reference_calls(contract, "linear_createComment"):
        require(call["arguments"]["issueId"] == facts["issue"], "reference comments on another issue")
        require(fold(call["arguments"]["body"]) in accepted, "reference comment is not an accepted example")
    relations = {(item["source"]["collection"], item["target"]["collection"]): item for item in expectation(contract)["relations"]}
    comment_relation = relations[("/services/linear/comments", "/services/linear/issues")]
    require(comment_relation["target"]["matching"] == {"identifier": facts["issue"]} and comment_relation["source_field"] == "/issueId" and comment_relation["target_field"] == "/id", "comment is not bound to the ticket")
    state_relation = relations[("/services/linear/issues", "/services/linear/workflow_states")]
    require(state_relation["target"]["matching"] == {"teamId": facts["team"], "name": facts["done_name"], "type": "completed"}, "closing state is not the team's Done state")
    zone_relation = relations[("/services/cloudflare/dns_records", "/services/cloudflare/zones")]
    require(zone_relation["source"]["matching"] == {"id": facts["record"]}, "zone relation is not bound to the record")
    base = f"/services/cloudflare/dns_records/{facts['record']}"
    issue = f"/services/linear/issues/{facts['issue']}"
    permitted_pointers(
        contract,
        [f"{base}/content", f"{base}/modified_on", f"{issue}/stateId", f"{issue}/completedAt", f"{issue}/updatedAt", "/services/linear/comments/*", "/services/linear/issue_history/*"],
        [f"{base}/settings", f"{base}/comment_modified_on", f"{base}/tags_modified_on", f"{issue}/startedAt"],
    )
    require(permission(contract, f"{base}/content")["value_equals"] == facts["address"], "content permission allows another address")
    states = sorted(rule.get("value_equals") or "" for rule in allowed(contract) if rule["pointer"] == f"{issue}/stateId")
    require(
        states in ([facts["done"]], sorted([facts["done"], facts["in_progress"]])),
        "state permission allows another state",
    )
    require(permission(contract, "/services/linear/comments/*")["matching"] == {"issueId": facts["issue_id"]}, "comment permission is not scoped to the ticket")
    for ordering in expectation(contract)["ordering"]:
        prerequisite = ordering["prerequisite"]
        require(prerequisite["selector"]["matching"] == {"id": facts["record"]} and prerequisite["field"] == "/content" and prerequisite["equals"] == facts["address"], "ordering prerequisite is not the completed DNS change")
    actions = {ordering["action"]["selector"]["collection"]: ordering["action"] for ordering in expectation(contract)["ordering"]}
    require(actions["/services/linear/comments"]["nonempty_text"] is True and actions["/services/linear/issues"]["equals"] == facts["done"], "ordering actions differ")
    patterns = [(request["service"], request.get("method"), request["path_regex"]) if "path_regex" in request else (request["service"], None, request["body"]["name"]) for request in forbidden_requests(contract)]
    staging_path = f"/client/v4/zones/{facts['staging_zone']}/dns_records/{facts['staging_record']}"
    for method in ("PATCH", "PUT"):
        require(any(service == "cloudflare" and verb == method and re.search(pattern, staging_path) for service, verb, pattern in patterns), f"staging {method} is not forbidden")
    require(any(service == "cloudflare" and verb == "DELETE" and re.search(pattern, f"/client/v4/zones/{facts['zone']}/dns_records/{facts['record']}") for service, verb, pattern in patterns), "record deletion is not forbidden")
    require(any(service == "cloudflare" and verb == "POST" and re.search(pattern, f"/client/v4/zones/{facts['zone']}/dns_records") and not re.search(pattern, f"/client/v4/zones/{facts['zone']}/dns_records/batch") for service, verb, pattern in patterns), "second-record creation is not forbidden")
    require(("linear", None, "linear_archiveIssue") in patterns, "archiving is not forbidden")


def local_instant(call, key):
    zone = ZoneInfo(call["timezone"])
    return datetime.fromisoformat(call[key]).replace(tzinfo=zone).astimezone(timezone.utc)


def lint_calendar(facts, contract):
    common_shape(contract, 1)
    event = single_check(contract, "/services/google/calendar_events", "event outcome is missing or ambiguous", created=True)
    require(event["equals"] == 1 and event["selector"] == {"collection": "/services/google/calendar_events", "created": True}, "event is not exactly one unfiltered new event")
    meeting = event["calendar_meeting"]
    require(meeting["calendar_id"] == facts["calendar"] and meeting["summary"] == facts["summary"], "event calendar or title differs")
    require(sorted(guest.lower() for guest in meeting["attendees"]) == facts["guests"], "required guest identities differ")
    require(instant(meeting["start"]) == facts["start"] and instant(meeting["end"]) == facts["end"], "event instants differ from the agreed window")
    require(meeting.get("require_meet") is False, "contract invents a required conferencing resource")
    post = single_check(contract, "/services/slack/posted", "reply outcome is missing or ambiguous", created=True)
    require(post["equals"] == 1 and post["fields"]["/channel"] == facts["channel"] and post["fields"]["/thread_ts"] == facts["thread"], "reply is not exactly one message in the requested thread")
    accepted = reviewed_examples(post, "/text", [])
    for call in reference_calls(contract, "slack_send_message"):
        arguments = call["arguments"]
        require(arguments["channel_id"] == facts["channel"] and arguments["thread_ts"] == facts["thread"], "reference reply is outside the thread")
        require(fold(arguments["message"]) in accepted, "reference reply is not an accepted example")
    for call in reference_calls(contract, "manage_event"):
        arguments = call["arguments"]
        require(arguments["calendar_id"] == facts["calendar"] and sorted(arguments["attendees"]) == facts["guests"], "reference event calendar or guests differ")
        require(arguments["timezone"] == facts["calendar_zone"], "reference does not use the calendar's zone")
        require(local_instant(arguments, "start_time") == facts["start"] and local_instant(arguments, "end_time") == facts["end"], "reference instants differ")
    threads = [rule for rule in expectation(contract)["relations"] if rule["source"]["collection"] == "/services/slack/posted" and rule["source_field"] == "/thread_ts"]
    root = only(threads, "thread relation is missing or ambiguous")
    require(root["target"]["matching"] == {"channel": facts["channel"], "ts": facts["thread"]} and root["target_field"] == "/ts" and root["kind"] == "foreign_key", "thread relation is not bound to the thread root")
    permitted_pointers(
        contract,
        ["/services/google/calendar_events/*", "/services/slack/posted/*", "/services/slack/messages/*"],
        ["/services/google/calendar_notification_requests/*", "/services/google/calendar_conferences/*"],
    )
    create = permission(contract, "/services/google/calendar_events/*")
    require(create["kind"] == "create" and create["matching"]["calendarId"] == facts["calendar"] and create.get("max_count", 1) == 1, "event creation permission is not scoped")
    ordering = expectation(contract)["ordering"][0]
    prerequisite = ordering["prerequisite"]
    require(prerequisite["selector"] == event["selector"], "ordering selects a different or filtered event")
    required_meeting = prerequisite["calendar_meeting"]
    for field in ("calendar_id", "summary", "require_meet"):
        require(required_meeting[field] == meeting[field], f"ordering meeting {field} differs")
    for field in ("start", "end"):
        require(instant(required_meeting[field]) == instant(meeting[field]), "ordering accepts a different event instant")
    require(sorted(guest.lower() for guest in required_meeting["attendees"]) == facts["guests"], "ordering does not require all guests")
    conferences = [
        rule for rule in allowed(contract)
        if rule["pointer"] == "/services/google/calendar_conferences/*"
    ]
    require(len(conferences) <= 1, "conference permission is duplicated")
    for conference in conferences:
        require(conference["kind"] == "create" and conference["matching"] == {"solution": "hangoutsMeet", "status": "success"}, "conference permission admits unsupported resources")
        require(conference.get("max_count", 1) == 1 and conference["references"] == [{
            "source_field": "/created_for", "target_field": "/id",
            "target": {"collection": "/services/google/calendar_events", "created": True},
        }], "conference permission is not bound to the actual new event")
    require(ordering["action"]["selector"]["collection"] == "/services/slack/posted", "ordering action is not the reply")
    deletes = [request for request in forbidden_requests(contract) if request["body"]["arguments"].get("action") == "delete"]
    require(deletes, "event deletion is not forbidden")
    touched = {request["body"]["arguments"].get("event_id") for request in forbidden_requests(contract)}
    require(set(facts["rehearsals"]) <= touched, "rehearsal event edits are not forbidden")


DERIVERS = {"refund": derive_refund, "cutover": derive_cutover, "calendar": derive_calendar}
LINTS = {"refund": lint_refund, "cutover": lint_cutover, "calendar": lint_calendar}


def lint(family, rows=None, contract=None):
    if rows is None or contract is None:
        rows, contract = authored(family)
    facts = DERIVERS[family](rows)
    try:
        LINTS[family](facts, copy.deepcopy(contract))
    except (KeyError, IndexError, TypeError, AttributeError) as error:
        raise LintRejected(f"declaration is missing or malformed: {error!r}") from error
    return facts


def main(arguments=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("families", nargs="*", default=sorted(FAMILIES))
    options = parser.parse_args(arguments)
    report = {}
    for family in options.families:
        facts = lint(family)
        report[family] = {key: str(value) for key, value in facts.items()}
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except LintRejected as error:
        print(f"rejected: {error}", file=sys.stderr)
        sys.exit(1)
