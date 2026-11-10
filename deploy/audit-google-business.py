"""Bounded independent Google organizer-state and complete committed-history audit.

Business requirements come from reviewed instructions and source email, not Rust
expectations or cached verdicts. Only JSON, exact-value, timestamp and HTTP trace
utilities are shared with the Datadog auditor. This checks the local Rust replica,
not hosted Google, the upstream Python SDK, guest delivery or model receipt.
"""

import argparse
import collections
import copy
import hashlib
import importlib.util
import re
import sys
from datetime import date, datetime, timedelta, timezone
from decimal import Decimal
from pathlib import Path
from urllib.parse import urlsplit
from zoneinfo import ZoneInfo


REPO = Path(__file__).resolve().parents[1]
ROOT = REPO / "scenarios" / "collab" / "google"
SPEC = importlib.util.spec_from_file_location(
    "google_shared_trace_utilities", Path(__file__).with_name("audit-datadog-business.py"),
)
TRACE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRACE)
AuditRejected = TRACE.AuditRejected
Indeterminate = TRACE.Indeterminate
require, read, same, instant_ns, correlate = (
    TRACE.require, TRACE.read, TRACE.same, TRACE.instant_ns, TRACE.correlate,
)
ACTOR = "ops@octo-corp.example"
SCENARIOS = {
    "renew": "mark-northwind-renewed",
    "plan": "schedule-q1-planning",
    "share": "share-soc2-with-contoso",
}
REVIEWED = {
    "renew": (
        "db4c6f2bbe3d1a1e41885ac54c00173c74fe30a156fed01f8497d064703eeb4b",
        "Northwind renewal signed",
        "f455a8e0ae473a71afc6ea05a174a7fa586478d0e70d864250f8e13b1d640278",
    ),
    "plan": (
        "5536d04bf19a8efcd8641b8af05b96f25486f8cfef72567383c4a576580c29e0",
        "Q1 planning sync?",
        "aa2959ce75f7ac115569ca667df4460d6ddab39add820813552c9a90293ae15c",
    ),
    "share": (
        "a1367415484d5f7925bc5c68959026019868a730bb4de7355e3d99b10d9188f5",
        "SOC 2 report for our vendor review",
        "138edcf19b6447ffe93e2c6111f8036035aa97ed65fd8864feedc8dbe9ba8879",
    ),
}


def family_name(value):
    for family, slug in SCENARIOS.items():
        if value in {family, slug, "collab/google/" + slug}:
            return family
    raise AuditRejected("unknown Google family: " + str(value))


def one(rows, predicate):
    selected = [(key, row) for key, row in rows.items() if predicate(row)]
    require(len(selected) == 1, "business identity is missing or ambiguous")
    return selected[0]


def authored(family):
    family = family_name(family)
    directory = ROOT / SCENARIOS[family]
    digest, subject, body_digest = REVIEWED[family]
    require(hashlib.sha256((directory / "instruction.md").read_bytes()).hexdigest() == digest,
            "instruction changed and needs independent review")
    header = (directory / "scenario.toml").read_text().split("\n[", 1)[0]
    clocks = re.findall(r'^now = "([^"]+)"$', header, re.MULTILINE)
    require(len(clocks) == 1, "authored business clock is missing or ambiguous")
    initial = {}
    for source in sorted((directory / "dataset" / "google").glob("*.json")):
        rows = {}
        contents = read(source)
        require(isinstance(contents, list), "authored collection is not an array")
        for record in contents:
            row = copy.deepcopy(record)
            identity = row.pop("_key", row.get("id"))
            require(isinstance(identity, str) and identity and identity not in rows,
                    "authored record identity is missing or duplicated")
            rows[identity] = row
        initial[source.stem] = rows
    _, message = one(initial["gmail_messages"], lambda row: row["headers"]["Subject"] == subject)
    require(hashlib.sha256(message["body_text"].encode()).hexdigest() == body_digest,
            "business email changed and needs independent review")
    require(message["headers"]["To"] == ACTOR, "email recipient differs from reviewed fixture actor")
    sender = {
        "renew": "lena.fischer@octo-corp.example",
        "plan": "priya.natarajan@octo-corp.example",
        "share": "dana.whitfield@contoso.example",
    }[family]
    require(message["headers"]["From"].endswith("<" + sender + ">"),
            "business email author differs from reviewed source")
    return initial, clocks[0]


def check_initial_services(services, baseline):
    require(isinstance(services, dict) and same(services.get("google"), baseline),
            "initial Google state differs from authored data")
    if set(services) == {"google"}:
        return
    reviewed = read(REPO / "deploy/fixtures/google-world-bootstrap.json")
    require(type(reviewed.get("schema_version")) is int and reviewed["schema_version"] == 1
            and reviewed.get("scope") == "reviewed_unselected_defaults_for_full_world_google_trials"
            and isinstance(reviewed.get("services"), dict) and "google" not in reviewed["services"],
            "unsupported source-reviewed world bootstrap fixture")
    require(same(services, {**reviewed["services"], "google": baseline}),
            "initial services differ from reviewed full-world initialization")


def targets(initial, family):
    family = family_name(family)
    if family == "renew":
        identity, book = one(initial["drive_files"], lambda row: (
            row.get("name") == "Vendor renewals 2026"
            and row.get("mimeType") == "application/vnd.google-apps.spreadsheet"
            and row.get("trashed") is False
        ))
        tabs = [(index, tab) for index, tab in enumerate(book["sheets"]) if tab["title"] == "Renewals"]
        require(len(tabs) == 1, "Renewals tab is missing or ambiguous")
        tab, contents = tabs[0]
        header = contents["values"][0]
        require(header == ["Vendor", "Owner", "Contract end", "Status", "Notes"],
                "tracker columns need independent review")
        rows = [(index, row) for index, row in enumerate(contents["values"])
                if row[:2] == ["Northwind Traders", "Lena Fischer"]]
        require(len(rows) == 1, "renewal row is missing or ambiguous")
        return {"file": identity, "tab": tab, "row": rows[0][0], "date": date(2027, 11, 30)}
    if family == "share":
        reports = [(int(match[1]), key) for key, row in initial["drive_files"].items()
                   if row.get("mimeType") == "application/pdf" and row.get("trashed") is False
                   and (match := re.fullmatch(r"SOC 2 Type II Report (\d{4})[.]pdf", row.get("name", "")))]
        require(bool(reports), "no final SOC 2 report")
        newest = max(year for year, _ in reports)
        selected = [key for year, key in reports if year == newest]
        require(len(selected) == 1, "current final SOC 2 report is ambiguous")
        return {"file": selected[0], "recipient": "dana.whitfield@contoso.example"}
    identity, calendar = one(initial["calendar_calendars"], lambda row: row.get("primary") is True)
    require(identity == ACTOR and calendar["id"] == identity, "primary calendar/actor mismatch")
    start = datetime(2026, 11, 3, 14, 0, tzinfo=ZoneInfo("America/New_York"))
    end = start + timedelta(minutes=45)
    return {
        "calendar": identity, "title": "Q1 planning sync",
        "start": instant_ns(start.astimezone(timezone.utc).isoformat()),
        "end": instant_ns(end.astimezone(timezone.utc).isoformat()),
        "guests": {"priya.natarajan@octo-corp.example", "marcus.webb@octo-corp.example"},
    }


def date_value(value, expected):
    if type(value) in {int, float, Decimal}:
        return Decimal(str(value)) == (expected - date(1899, 12, 30)).days
    if not isinstance(value, str):
        return False
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", value, re.ASCII):
        try:
            return date.fromisoformat(value) == expected
        except ValueError:
            return False
    if "/" in value:
        parts = re.fullmatch(r"(\d{1,2})/(\d{1,2})/(\d{4})", value, re.ASCII)
        if parts is None:
            return False
        first, second, year = map(int, parts.groups())
        if 1 <= first <= 12 and 13 <= second <= 31 or first == second and 1 <= first <= 12:
            month, day = first, second
        elif 13 <= first <= 31 and 1 <= second <= 12:
            month, day = second, first
        else:
            raise Indeterminate("ambiguous or unreviewed slash-date interpretation")
        try:
            return date(year, month, day) == expected
        except ValueError:
            return False
    if not value.strip() or value.startswith(("=", "#")):
        return False
    try:
        Decimal(value)
    except ArithmeticError:
        raise Indeterminate("unreviewed date spelling")
    return False


def principal(value):
    return value.lower() if isinstance(value, str) and value.isascii() else None


def meet_uri(value):
    if not isinstance(value, str):
        return None
    try:
        parsed = urlsplit(value)
        if parsed.scheme != "https" or parsed.hostname != "meet.google.com" \
                or parsed.port not in {None, 443} or parsed.username is not None or parsed.password is not None:
            return None
        if re.fullmatch(r"/[a-z]{3}-[a-z]{4}-[a-z]{3}", parsed.path) is None:
            return None
        return "https://meet.google.com" + parsed.path
    except ValueError:
        return None


def meeting(event, google, selected, complete):
    if event.get("calendarId") != selected["calendar"] or event.get("summary") != selected["title"] \
            or event.get("status") != "confirmed" or event.get("eventType", "default") != "default" \
            or event.get("recurrence") not in (None, []):
        return False
    for field in ("start", "end"):
        try:
            if instant_ns(event.get(field, {}).get("dateTime")) != selected[field]:
                return False
        except Indeterminate:
            return False
    guests = event.get("attendees", [])
    if not isinstance(guests, list) or any(not isinstance(guest, dict) for guest in guests):
        return False
    identities = {principal(guest.get("email")) for guest in guests}
    if not identities <= selected["guests"] or complete and identities != selected["guests"]:
        return False
    description = event.get("description")
    if description:
        if not isinstance(description, str):
            return False
        reviewed = " ".join(description.split())
        if reviewed == "Meeting cancelled.":
            return False
        if reviewed != "Q1 planning sync with Priya and Marcus.":
            raise Indeterminate("unreviewed meeting description")
    conference = event.get("conferenceData") or {}
    if not isinstance(conference, dict):
        return False
    points = conference.get("entryPoints", [])
    if not isinstance(points, list) or any(not isinstance(point, dict) for point in points):
        return False
    links = [point.get("uri") for point in points if point.get("entryPointType") == "video"]
    if event.get("hangoutLink"):
        links.append(event["hangoutLink"])
    for field in ("location", "description"):
        if meet_uri(event.get(field)):
            links.append(event[field])
    if not links:
        return not complete
    uris = {meet_uri(link) for link in links}
    if None in uris or len(uris) != 1:
        return False
    uri = uris.pop()
    return any(
        resource.get("solution") == "hangoutsMeet" and resource.get("status") == "success"
        and meet_uri(resource.get("uri")) == uri
        and ("conferenceId" not in conference or conference["conferenceId"] == resource.get("id"))
        for resource in google.get("calendar_conferences", {}).values()
    )


def goal(initial, final, family, selected=None):
    family = family_name(family)
    selected = selected or targets(initial, family)
    if family == "renew":
        book = final.get("drive_files", {}).get(selected["file"])
        if book is None:
            return False
        rows = book["sheets"][selected["tab"]]["values"]
        matches = [row for row in rows if row[:2] == ["Northwind Traders", "Lena Fischer"]]
        return len(matches) == 1 and len(matches[0]) >= 4 and matches[0][3] == "Renewed" \
            and date_value(matches[0][2], selected["date"])
    if family == "share":
        report = final.get("drive_files", {}).get(selected["file"])
        if report is None:
            return False
        matches = [permission for permission in report.get("permissions", [])
                   if principal(permission.get("emailAddress")) == selected["recipient"]]
        return bool(matches) and all(
            permission.get("type") == "user" and permission.get("role") == "reader"
            for permission in matches
        )
    added = [event for key, event in final.get("calendar_events", {}).items()
             if key not in initial.get("calendar_events", {})]
    return len(added) == 1 and meeting(added[0], final, selected, True)


def file_metadata(before, after, now):
    normalized = copy.deepcopy(after)
    if after.get("modifiedTime") != before.get("modifiedTime"):
        if instant_ns(after.get("modifiedTime")) != instant_ns(now):
            return None
        normalized.pop("modifiedTime", None)
        if "modifiedTime" in before:
            normalized["modifiedTime"] = before["modifiedTime"]
    if not same(after.get("lastModifyingUser"), before.get("lastModifyingUser")):
        if after.get("lastModifyingUser") != {"displayName": "Ops Team", "emailAddress": ACTOR}:
            return None
        normalized.pop("lastModifyingUser", None)
        if "lastModifyingUser" in before:
            normalized["lastModifyingUser"] = before["lastModifyingUser"]
    return normalized


def file_change(before, after, family, selected, now):
    if before is None or after is None:
        return False
    normalized = file_metadata(before, after, now)
    if normalized is None:
        return False
    if family == "renew":
        try:
            old = before["sheets"][selected["tab"]]["values"][selected["row"]]
            row = normalized["sheets"][selected["tab"]]["values"][selected["row"]]
            if row[:2] != ["Northwind Traders", "Lena Fischer"] or len(row) != len(old):
                return False
            if not same(row[2], old[2]) and row[2] not in ("", None) \
                    and not date_value(row[2], selected["date"]):
                return False
            if not same(row[3], old[3]) and row[3] not in ("", "Renewed"):
                return False
            row[2:4] = copy.deepcopy(old[2:4])
        except (KeyError, IndexError, TypeError):
            return False
        return same(normalized, before)
    old, new = before.get("permissions"), normalized.get("permissions")
    if not isinstance(old, list) or not isinstance(new, list) or len(new) < len(old):
        return False
    identities = [permission.get("id") for permission in new]
    if any(not isinstance(identity, str) or not identity for identity in identities) \
            or len(set(identities)) != len(identities):
        raise AuditRejected("permission identities are missing or duplicated")
    for index, original in enumerate(old):
        current = copy.deepcopy(new[index])
        if current.get("id") != original.get("id"):
            return False
        if current.get("role") != original.get("role"):
            if original.get("type") != "user" \
                    or principal(original.get("emailAddress")) != selected["recipient"] \
                    or current.get("role") != "reader":
                return False
            current["role"] = original.get("role")
        if not same(current, original):
            return False
    for permission in new[len(old):]:
        if permission.get("type") != "user" or permission.get("role") != "reader" \
                or principal(permission.get("emailAddress")) != selected["recipient"]:
            return False
        if permission.get("expirationTime") is not None:
            raise Indeterminate("permission expiration remains unqualified")
    normalized["permissions"] = copy.deepcopy(old)
    if normalized.get("shared") != before.get("shared"):
        if normalized.get("shared") is not True:
            return False
        normalized["shared"] = before.get("shared")
    return same(normalized, before)


def tool_request(request, tools_ips):
    return (
        request["service"] == "google" and request["method"] == "POST"
        and request["path"] == "/_mcp/tools/call" and request.get("query") is None
        and (tools_ips is None or request["source"] in tools_ips)
        and isinstance(request.get("request_body"), dict)
        and isinstance(request["request_body"].get("name"), str)
        and isinstance(request["request_body"].get("arguments"), dict)
    )


def allowed(initial, cursor, after_state, changes, request, family, selected, now, tools_ips):
    if not tool_request(request, tools_ips):
        return False
    body = request["request_body"]
    tool, arguments = body["name"], body["arguments"]
    if family in {"renew", "share"}:
        names = {"modify_sheet_values"} if family == "renew" else {
            "manage_drive_access", "get_drive_shareable_link",
        }
        if tool not in names or len(changes) != 1:
            return False
        service, collection, identity, before, after = changes[0]
        return service == "google" and collection == "drive_files" and identity == selected["file"] \
            and arguments.get("spreadsheet_id" if family == "renew" else "file_id") == identity \
            and file_change(before, after, family, selected, now)
    action = arguments.get("action")
    if tool != "manage_event" or not isinstance(action, str) or action.strip().lower() not in {"create", "update"}:
        return False
    changed_events = {identity: after for service, collection, identity, _, after in changes
                      if service == "google" and collection == "calendar_events"}
    if not changed_events or len(changed_events) != 1:
        return False
    for service, collection, identity, before, after in changes:
        if service != "google" or after is None or after.get("id") != identity:
            return False
        if collection == "calendar_events":
            if identity in initial.get("calendar_events", {}) \
                    or after.get("creator", {}).get("email") != ACTOR \
                    or after.get("organizer", {}).get("email") != selected["calendar"] \
                    or not meeting(after, after_state["google"], selected, False):
                return False
            if arguments["action"].strip().lower() == "update" and arguments.get("event_id") != identity:
                return False
        elif collection == "calendar_conferences":
            if before is not None or after.get("created_for") not in changed_events \
                    or after.get("actor") != ACTOR or after.get("solution") != "hangoutsMeet" \
                    or after.get("status") != "success" or meet_uri(after.get("uri")) is None:
                return False
        elif collection == "calendar_notification_requests":
            if before is not None or after.get("eventId") not in changed_events \
                    or after.get("mode") != arguments.get("send_updates", "all") \
                    or after.get("mode") not in {"all", "externalOnly", "none"} \
                    or after.get("delivery") != "unobserved" or after.get("actor") != ACTOR:
                return False
            event = changed_events[after["eventId"]]
            guests = sorted({principal(guest.get("email")) for guest in event.get("attendees", [])})
            if not same(after.get("attendees"), guests) or not guests \
                    or after.get("summary") != event.get("summary"):
                return False
            for field in ("start", "end"):
                if after.get(field) != event.get(field, {}).get("dateTime"):
                    return False
            links = [event.get("hangoutLink"), event.get("location"), event.get("description")]
            links.extend(point.get("uri") for point in (event.get("conferenceData") or {}).get("entryPoints", [])
                         if point.get("entryPointType") == "video")
            uri = next((meet_uri(link) for link in links if meet_uri(link)), "")
            require(after.get("conferenceUri") == uri, "notification observation disagrees with committed event")
            if "recipient" in after or "recipients" in after:
                raise Indeterminate("notification recipient enrollment cannot be inferred from email domain")
        else:
            return False
    return True


def history(initial, final, events, entries, family, now, tools_ips=None):
    family = family_name(family)
    selected = targets(initial["google"], family)
    journal, _phases = correlate(events, entries, "collab/google/" + SCENARIOS[family], now)
    cursor = copy.deepcopy(initial)
    unauthorized, unknown = [], []
    for event in events:
        if event["kind"] != "state_committed":
            continue
        request = journal[event["operation"]["id"]]
        changes, seen = [], set()
        updated = copy.deepcopy(cursor)
        require(isinstance(event.get("records"), list) and event["records"], "empty committed transaction")
        for change in event["records"]:
            pointer = change.get("pointer")
            require(isinstance(pointer, str) and pointer.startswith("/services/"), "invalid record pointer")
            parts = pointer.split("/")[1:]
            require(len(parts) == 4 and all(re.search(r"~(?![01])", part) is None for part in parts),
                    "unsupported record pointer")
            _, service, collection, identity = [
                part.replace("~1", "/").replace("~0", "~") for part in parts
            ]
            address = service, collection, identity
            require(address not in seen, "duplicate record in atomic transaction")
            seen.add(address)
            rows = cursor.get(service, {}).get(collection, {})
            require((identity in rows) == ("before" in change), "before-presence mismatch")
            require("before" in change or "after" in change, "empty record change")
            require("before" not in change or same(rows[identity], change["before"]), "forged before-value")
            require("after" not in change or isinstance(change["after"], dict), "non-object after-value")
            changes.append((*address, change.get("before"), change.get("after")))
            destination = updated.setdefault(service, {}).setdefault(collection, {})
            if "after" in change:
                destination[identity] = copy.deepcopy(change["after"])
            else:
                del destination[identity]
        try:
            accepted = allowed(initial["google"], cursor, updated, changes, request,
                               family, selected, now, tools_ips)
        except Indeterminate as error:
            unknown.append((event["seq"], str(error)))
            accepted = True
        if not accepted:
            unauthorized.append((event["seq"], [change["pointer"] for change in event["records"]]))
        cursor = updated
    require(same(cursor, final), "complete committed history does not reconstruct final state")
    if unknown and not unauthorized:
        raise Indeterminate(str(unknown))
    return unauthorized


def audit_trial(verifier, family):
    verifier = Path(verifier)
    family = family_name(family)
    initial, final = read(verifier / "initial-state.json"), read(verifier / "state.json")
    baseline, now = authored(family)
    scenario = "collab/google/" + SCENARIOS[family]
    require(initial.get("scenario") == final.get("scenario") == scenario, "scenario/source mismatch")
    check_initial_services(initial.get("services"), baseline)
    require(initial.get("now") == final.get("now") == now, "business clock mismatch")
    clock = initial.get("clock")
    require(isinstance(clock, dict) and same(clock, final.get("clock"))
            and clock.get("mode") == "fixed" and clock.get("reference") == now
            and type(clock.get("step_secs")) is int and clock["step_secs"] >= 0,
            "unqualified business clock policy")
    require(same(initial.get("sessions"), final.get("sessions")), "unreplayed session changes")
    events = read(verifier / "events.json")["events"]
    journal = read(verifier / "journal.json")["entries"]
    tools_ips = None
    evidence_file = verifier.parent / "artifacts" / "world" / "evidence.json"
    if evidence_file.exists():
        evidence = read(evidence_file)
        require(evidence.get("scenario") == scenario and all(
            same(evidence.get(field), value) for field, value in (
                ("initial_state", initial), ("state", final), ("events", events), ("journal", journal),
            )
        ), "world evidence/verifier exports disagree")
        seal = evidence.get("finalization", {})
        require(seal.get("phase") == "sealed" and seal.get("pending") == []
                and seal.get("failure") is None and seal.get("event_count") == len(events),
                "world evidence is not completely sealed")
        require(all(event["operation"]["generation"] == seal.get("generation") for event in events),
                "sealed generation mismatch")
        require(evidence.get("origin_error") is None and isinstance(evidence.get("tools_ips"), list)
                and evidence["tools_ips"], "missing tools-origin evidence")
        tools_ips = set(evidence["tools_ips"])
        require(all(event["operation"]["source"] in tools_ips for event in events), "wrong request origin")
    unauthorized = history(initial["services"], final["services"], events, journal, family, now, tools_ips)
    complete = goal(baseline, final["services"]["google"], family)
    return ("pass" if complete and not unauthorized else "fail"), unauthorized, complete


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directories", nargs="+", type=Path)
    options = parser.parse_args(argv)
    trials = set()
    rejected, disagreements, counts = [], [], collections.Counter()
    for directory in options.directories:
        if (directory / "initial-state.json").is_file():
            trials.add(directory.resolve())
        elif directory.is_dir():
            trials.update(path.parent.resolve() for path in directory.rglob("initial-state.json")
                          if path.parent.name == "verifier")
        else:
            rejected.append((str(directory), "not a trial or campaign directory"))
    for verifier in sorted(trials):
        try:
            family = family_name(read(verifier / "initial-state.json").get("scenario"))
            actual, unauthorized, complete = audit_trial(verifier, family)
            print(f"{verifier}: independent business/history verdict={actual} "
                  f"goal={complete} unauthorized={unauthorized}")
            counts[actual] += 1
            if (verifier / "assessment.json").exists() \
                    and read(verifier / "assessment.json").get("outcome") != actual:
                disagreements.append(str(verifier))
        except (AuditRejected, OSError, ValueError, KeyError, TypeError, IndexError) as error:
            rejected.append((str(verifier), str(error)))
            print(f"{verifier}: audit_rejected (not business failure): {error}", file=sys.stderr)
    if not trials:
        rejected.append(("campaign", "no completed trial evidence"))
    print(f"business_total={sum(counts.values())} outcomes={dict(counts)} audit_rejected={len(rejected)}")
    if rejected or disagreements:
        print(f"rejected={rejected} disagreements={disagreements}", file=sys.stderr)
    return 2 if rejected else 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
