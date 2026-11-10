"""Independent source-bound PagerDuty outcomes and complete committed-history audit."""

import argparse
import collections
import copy
import hashlib
import json
import re
import sys
from datetime import date
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[1] / "scenarios" / "ops" / "pagerduty"
SCENARIOS = {
    "cover": "cover-shift-override",
    "handoff": "reassign-replication-lag",
    "resolve": "resolve-prod-checkout",
}
EXPECTED_PASS = {
    "cover_reference", "cover_extra_reads", "cover_offsets", "cover_split", "cover_overlap",
    "cover_repeated", "cover_fractional_split", "cover_native_truncation",
    "cover_native_offset_truncation", "cover_fractional_reads",
    "handoff_reference", "handoff_note_first", "handoff_extra_reads", "handoff_repeated_note",
    "resolve_reference", "resolve_first", "resolve_acknowledge", "resolve_paraphrase",
    "resolve_repeated_note",
}
EXPECTED_FAIL = {
    "cover_wrong_user", "cover_wrong_schedule", "cover_gap", "cover_early", "cover_late",
    "cover_other_schedule", "cover_micro_gap", "cover_micro_late_start", "cover_micro_late_end",
    "cover_native_no_carry", "handoff_wrong_primary", "handoff_ack_restore", "handoff_urgency",
    "handoff_urgency_restore", "handoff_weak_note", "handoff_other_note",
    "resolve_wrong_note", "resolve_extra_incident", "resolve_ack_only", "resolve_other_note",
}
NANOSECONDS = 1_000_000_000
START = "2026-10-31T09:00:00Z"
END = "2026-10-31T17:00:00Z"
HANDOFF = "Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize."
RESOLUTION = {
    "Resolved by rolling back release 2026.10.29.3.",
    "Rolling back release 2026.10.29.3 resolved this incident.",
    "Production checkout recovered after release 2026.10.29.3 was rolled back; the incident is resolved.",
}


class AuditRejected(ValueError):
    """Evidence is incomplete, inconsistent or outside this audit's admitted domain."""


class Indeterminate(AuditRejected):
    """An unreviewed meaning requires adjudication, not a guessed business failure."""


def require(condition, message):
    if not condition:
        raise AuditRejected(message)


def read(path):
    return json.loads(path.read_bytes())


def same(left, right):
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(same(left[key], right[key]) for key in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(same(first, second) for first, second in zip(left, right))
    return left == right


def instant(value):
    if not isinstance(value, str):
        raise Indeterminate("timestamp is not text")
    match = re.fullmatch(
        r"(\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})"
        r"(?:\.(\d{1,9}))?(Z|z|[+-]\d{2}:\d{2})?", value, re.ASCII,
    )
    if match is None:
        raise Indeterminate("timestamp outside the reviewed calendar domain")
    year, month, day, hour, minute, second = map(int, match.groups()[:6])
    require(hour < 24 and minute < 60 and second < 60, "invalid or unsupported clock fields")
    try:
        days = (date(year, month, day) - date(1970, 1, 1)).days
    except ValueError as error:
        raise Indeterminate("timestamp calendar is invalid") from error
    zone = match[8]
    offset = 0
    if zone not in {None, "Z", "z"}:
        hours, minutes = int(zone[1:3]), int(zone[4:6])
        require(hours < 24 and minutes < 60, "invalid timestamp offset")
        offset = (hours * 3600 + minutes * 60) * (-1 if zone[0] == "-" else 1)
    return ((days * 86400 + hour * 3600 + minute * 60 + second - offset) * NANOSECONDS
            + int((match[7] or "").ljust(9, "0")))


def one(records, predicate):
    selected = [(identity, row) for identity, row in records.items() if predicate(row)]
    require(len(selected) == 1, "business identity is missing or ambiguous")
    return selected[0]


def authored(family):
    directory = ROOT / SCENARIOS[family]
    header = (directory / "scenario.toml").read_text().split("\n[", 1)[0]
    clocks = re.findall(r'^now = "([^"]+)"$', header, re.MULTILINE)
    require(len(clocks) == 1, "authored clock is missing")
    instruction = (directory / "instruction.md").read_text()
    if family == "cover":
        require("Saturday, October 31" in instruction and "09:00 to 17:00 UTC" in instruction
                and "Jordan Lee from the Platform team" in instruction,
                "cover instruction needs independent review")
    elif family == "handoff":
        require(HANDOFF in instruction and "don't acknowledge or resolve anything" in instruction,
                "handoff instruction needs independent review")
    else:
        require("resolve that incident" in instruction and "release 2026.10.29.3" in instruction,
                "resolve instruction needs independent review")
    initial = {}
    for path in sorted((directory / "dataset" / "pagerduty").glob("*.json")):
        records = {}
        for source in read(path):
            row = copy.deepcopy(source)
            identity = row.pop("_key", row.get("id"))
            require(isinstance(identity, str) and identity and identity not in records,
                    "authored records need unique identities")
            records[identity] = row
        initial[path.stem] = records
    require(bool(initial), "authored dataset is empty")
    return initial, clocks[0]


def schedule_user(initial, schedule_id, at):
    schedule = initial["schedules"][schedule_id]
    winner = None
    for layer in schedule["schedule_layers"]:
        if layer.get("restrictions"):
            raise Indeterminate("restricted rotations require independent qualification")
        start = instant(layer["start"])
        end = instant(layer["end"]) if layer.get("end") else None
        if at < start or end is not None and at >= end:
            continue
        users = [row["user"]["id"] for row in layer["users"]]
        period = layer["rotation_turn_length_seconds"]
        require(type(period) is int and period > 0 and users, "invalid rotation")
        anchor = instant(layer.get("rotation_virtual_start", layer["start"]))
        winner = users[((at - anchor) // (period * NANOSECONDS)) % len(users)]
    overlapping = [
        row["user"]["id"] for key, row in initial["overrides"].items()
        if key.startswith(schedule_id + "/") and instant(row["start"]) <= at < instant(row["end"])
    ]
    if overlapping:
        require(len(set(overlapping)) == 1, "conflicting baseline overrides need priority qualification")
        winner = overlapping[0]
    require(winner in initial["users"], "current duty is missing from the directory")
    return winner


def targets(initial, family, now):
    account = initial["account"]["account"]["user_id"]
    actor = initial["users"][account]
    require(actor["id"] == account, "account actor identity mismatch")
    result = {"actor": account, "actor_record": actor}
    if family == "cover":
        team, _ = one(initial["teams"], lambda row: row["name"] == "Platform")
        schedule, _ = one(initial["schedules"], lambda row: (
            row["name"] == "Platform Primary" and any(edge["id"] == team for edge in row["teams"])
        ))
        user, _ = one(initial["users"], lambda row: (
            row["name"] == "Jordan Lee" and any(edge["id"] == team for edge in row["teams"])
        ))
        require(schedule_user(initial, schedule, instant(now)) == account,
                "requester is not the authored primary")
        require(now.startswith("2026-10-30T"), "cover relative date needs new qualification")
        return dict(result, schedule=schedule, user=user)
    service_name = "orders-db" if family == "handoff" else "checkout-api"
    service, _ = one(initial["services"], lambda row: row["name"] == service_name)
    incident, row = one(initial["incidents"], lambda row: (
        row["service"]["id"] == service and row["status"] == "triggered"
        and ("replica-2" in row["title"] if family == "handoff"
             else row["title"] == "Checkout error rate above 5%")
    ))
    result.update(incident=incident, incident_record=row, service=service)
    if family == "handoff":
        team, _ = one(initial["teams"], lambda row: row["name"] == "Database Reliability")
        _, policy = one(initial["escalation_policies"], lambda row: (
            any(edge["id"] == team for edge in row["teams"]) and "staging" not in row["name"].casefold()
        ))
        users = set()
        for target in policy["escalation_rules"][0]["targets"]:
            users.add(target["id"] if target["type"] == "user_reference"
                      else schedule_user(initial, target["id"], instant(now)))
        require(len(users) == 1, "primary handoff is ambiguous")
        result["user"] = users.pop()
    return result


def note_meaning(family, content):
    if not isinstance(content, str):
        return False
    normalized = " ".join(content.split())
    if family == "handoff":
        if normalized == HANDOFF:
            return True
        if normalized in {
            "", "replica-2", "Do not hand replica-2 off to DBRE.",
            "Handing off to DBRE: replica-2 disk is at 100%, volume resize completed.",
        }:
            return False
    else:
        if normalized in RESOLUTION:
            return True
        if normalized in {
            "", "2026.10.29.3",
            "Release 2026.10.29.3 was not rolled back; this incident is still broken.",
            "Resolved by rolling forward release 2026.10.29.3.",
            "I rolled back release 2026.10.29.3 and resolved this incident.",
        }:
            return False
    raise Indeterminate("unreviewed complete note meaning")


def interval(row):
    return instant(row["start"]), instant(row["end"])


def within_window(row):
    start, end = interval(row)
    return instant(START) <= start < end <= instant(END)


def goal(initial, final, family, selected):
    if family == "cover":
        created = [(key, row) for key, row in final["overrides"].items()
                   if key not in initial["overrides"]]
        if not created or any(
            not key.startswith(selected["schedule"] + "/") or row["user"]["id"] != selected["user"]
            or not within_window(row) for key, row in created
        ):
            return False
        covered = instant(START)
        for start, end in sorted(interval(row) for _, row in created):
            if start > covered:
                return False
            covered = max(covered, end)
        return covered == instant(END)
    incident = final["incidents"][selected["incident"]]
    if family == "handoff":
        assignments = incident["assignments"]
        if incident["status"] != "triggered" or len(assignments) != 1 \
                or assignments[0]["assignee"]["id"] != selected["user"]:
            return False
    else:
        if incident["status"] != "resolved" or incident["assignments"] != [] \
                or incident.get("pending_actions") != []:
            return False
        alerts = [identity for identity, row in initial["alerts"].items()
                  if row["incident"]["id"] == selected["incident"]]
        require(alerts, "target production alerts are missing")
        if any(final["alerts"][identity]["status"] != "resolved" for identity in alerts):
            return False
    created = [(identity, row) for identity, row in final["notes"].items()
               if identity not in initial["notes"]]
    if not created or any(not identity.startswith(selected["incident"] + "/")
                          or row["user"]["id"] != selected["actor"] for identity, row in created):
        return False
    results, unknown = [], []
    for _, row in created:
        try:
            results.append(note_meaning(family, row["content"]))
        except Indeterminate as error:
            unknown.append(error)
    if False in results:
        return False
    if unknown:
        raise unknown[0]
    return True


def ref(identity, kind):
    return {"id": identity, "type": kind + "_reference"}


def route(request):
    return unquote(request["path"]).rstrip("/") or "/"


def actor_request(request, selected, require_from=True):
    if request["scheme"] != "https" or request["host"].split(":")[0] != "api.pagerduty.com":
        return False
    headers = {header["name"].casefold(): header["value"] for header in request["replay"]["headers"]}
    authorization = headers.get("authorization", {})
    if authorization.get("kind") not in {"synthetic", "redacted", "public"}:
        return False
    if not authorization.get("value", authorization.get("sha256")):
        return False
    if not require_from:
        return True
    sender = headers.get("from", {})
    email = selected["actor_record"]["email"]
    if sender.get("kind") == "public":
        return sender.get("value", "").casefold() == email.casefold()
    return sender.get("sha256") == "sha256:" + hashlib.sha256(email.encode()).hexdigest()


def action(request, selected):
    if request["method"] != "PUT":
        return None
    target = selected["incident"]
    number = str(selected["incident_record"]["incident_number"])
    aliases = {target, number}
    path, body = route(request), request["request_body"]
    require(isinstance(body, dict), "mutating request has no JSON object body")
    if path == "/incidents":
        candidates = [row for row in body.get("incidents", []) if row.get("id") in aliases]
        return candidates[0] if candidates else None
    if path in {"/incidents/" + alias for alias in aliases}:
        return body.get("incident", body)
    return None


def note_request(request, selected):
    aliases = {selected["incident"], str(selected["incident_record"]["incident_number"])}
    if request["method"] != "POST" or route(request) not in {
        f"/incidents/{alias}/notes" for alias in aliases
    }:
        return None
    body = request["request_body"]
    require(isinstance(body, dict), "note request has no JSON object body")
    return body.get("note", body).get("content")


def permitted(initial, collection, identity, before, after, family, selected, request, stamp):
    if after is None or type(request["status"]) is not int or not 200 <= request["status"] < 300:
        return False
    if family == "cover":
        if collection != "overrides" or before is not None or identity in initial["overrides"] \
                or not identity.startswith(selected["schedule"] + "/") \
                or route(request) != f"/schedules/{selected['schedule']}/overrides" \
                or request["method"] != "POST" or not actor_request(request, selected, False) \
                or after["user"] != ref(selected["user"], "user") or not within_window(after):
            return False
        response = request.get("response_body")
        results = response if isinstance(response, list) else [response] if isinstance(response, dict) else []
        responses = [row for row in results if row.get("override", {}).get("id") == after.get("id")]
        require(len(responses) == 1, "committed override has no unique response identity")
        require(responses[0].get("status", 201) == 201
                and responses[0].get("errors") in (None, []),
                "committed override contradicts the reported suboperation result")
        body = request["request_body"]
        inputs = body.get("overrides", [body["override"]] if "override" in body else [])
        matches = []
        for row in inputs:
            if not isinstance(row, dict):
                continue
            user = row.get("user") or {}
            if not isinstance(user, dict):
                continue
            try:
                matched = user.get("id", row.get("user_id")) == after["user"]["id"] \
                    and interval(row) == interval(after) \
                    and same(row.get("time_zone"), after.get("time_zone"))
            except (AuditRejected, KeyError, TypeError):
                matched = False
            if matched:
                matches.append(row)
        if not matches or not isinstance(after.get("id"), str) or not after["id"]:
            return False
        returned = responses[0]["override"]
        require(interval(returned) == interval(after) and returned["user"]["id"] == after["user"]["id"],
                "created override response/state disagreement")
        if set(after) - {"id", "start", "end", "user", "time_zone"}:
            raise Indeterminate("unreviewed override metadata")
        return True
    if not actor_request(request, selected):
        return False
    update = action(request, selected)
    content = note_request(request, selected)
    target = selected["incident"]
    if collection == "incidents" and identity == target and before is not None:
        expected = copy.deepcopy(before)
        if content is not None:
            expected["updated_at"] = stamp
            return same(after, expected)
        if update is None:
            return False
        if family == "handoff":
            assignments = update.get("assignments", [])
            if len(assignments) == 1 and assignments[0]["assignee"]["id"] == selected["user"] \
                    and before["status"] == "triggered":
                expected["assignments"] = [{"at": stamp, "assignee": ref(selected["user"], "user")}]
                expected["assigned_via"] = "direct_assignment"
        elif update.get("status") == "acknowledged" and before["status"] != "resolved":
            expected.update(status="acknowledged", last_status_change_at=stamp,
                            last_status_change_by=ref(selected["actor"], "user"))
            acknowledgements = expected.setdefault("acknowledgements", [])
            if not any(row["acknowledger"]["id"] == selected["actor"] for row in acknowledgements):
                acknowledgements.append({"at": stamp, "acknowledger": ref(selected["actor"], "user")})
        elif update.get("status") == "resolved":
            expected.update(status="resolved", resolved_at=stamp, last_status_change_at=stamp,
                            last_status_change_by=ref(selected["actor"], "user"),
                            assignments=[], pending_actions=[])
        expected["updated_at"] = stamp
        return same(after, expected)
    if collection == "alerts" and family == "resolve" and before is not None \
            and before["incident"]["id"] == target and update is not None \
            and update.get("status") == "resolved":
        expected = copy.deepcopy(before)
        expected.update(status="resolved", resolved_at=stamp)
        return same(after, expected)
    if before is not None:
        return False
    if collection == "notes":
        if content is None or identity in initial["notes"] or not identity.startswith(target + "/"):
            return False
        expected = {
            "id": after.get("id"), "content": content, "created_at": stamp,
            "user": ref(selected["actor"], "user"),
            "channel": {"summary": "The PagerDuty website or APIs"},
        }
        if not isinstance(after.get("id"), str) or not after["id"] or not same(after, expected):
            return False
        response = request.get("response_body", {}).get("note", {})
        require(response.get("id") == after["id"] and response.get("content") == content,
                "note response/state identity or content disagreement")
        return note_meaning(family, content)
    if collection != "log_entries" or identity in initial["log_entries"] \
            or after.get("id") != identity or not identity:
        return False
    expected = {
        "id": identity, "created_at": stamp, "agent": ref(selected["actor"], "user"),
        "channel": {"type": "api"}, "service": selected["incident_record"]["service"],
        "incident": ref(target, "incident"), "teams": selected["incident_record"]["teams"], "contexts": [],
    }
    actor_name = selected["actor_record"]["name"]
    if content is not None:
        expected.update(type="annotate_log_entry", summary=f"Note added by {actor_name}",
                        channel={"type": "note", "summary": content})
        correct = note_meaning(family, content)
    elif family == "handoff" and update is not None and len(update.get("assignments", [])) == 1 \
            and update["assignments"][0]["assignee"]["id"] == selected["user"]:
        expected.update(type="assign_log_entry",
                        summary=f"Reassigned to {initial['users'][selected['user']]['name']} by {actor_name}",
                        assignees=[ref(selected["user"], "user")])
        correct = True
    elif family == "resolve" and update is not None and update.get("status") in {"acknowledged", "resolved"}:
        acknowledged = update["status"] == "acknowledged"
        expected.update(type="acknowledge_log_entry" if acknowledged else "resolve_log_entry",
                        summary=f"{'Acknowledged' if acknowledged else 'Resolved'} by {actor_name}")
        if acknowledged:
            expected["acknowledgement_timeout"] = 1800
        if "resolution" in update:
            expected["note"] = update["resolution"]
        correct = True
    else:
        return False
    return correct and same(after, expected)


def correlate(events, entries, scenario):
    journal, phases, previous = {}, collections.defaultdict(dict), 0
    for request in entries:
        identity = request["operation_id"]
        require(type(identity) is int and identity > 0 and identity not in journal,
                "invalid or duplicate journal identity")
        require(type(request["seq"]) is int and request["seq"] > previous, "journal is unordered")
        previous = request["seq"]
        journal[identity] = request
    previous = 0
    for event in events:
        require(type(event["seq"]) is int and event["seq"] > previous, "events are unordered")
        previous = event["seq"]
        operation, kind = event["operation"], event["kind"]
        require(operation["kind"] == "http" and operation["scenario"] == scenario,
                "wrong operation kind/scenario")
        require(operation["id"] in journal, "event has no HTTP journal cause")
        request = journal[operation["id"]]
        require(all(same(operation.get(field), request.get(field))
                    for field in ["service", "method", "path", "source"]),
                "HTTP event/journal correlation disagrees")
        state = phases[operation["id"]]
        if kind == "state_committed":
            require("request_body_received" in state and "response_completed" not in state,
                    "commit is outside its HTTP request")
            continue
        prior = {
            "request_started": None, "request_head_received": "request_started",
            "request_body_received": "request_head_received", "response_completed": "request_body_received",
        }
        require(kind in prior and kind not in state and (prior[kind] is None or prior[kind] in state),
                "incomplete, duplicate, failed or unsupported HTTP phase")
        state[kind] = event
    for identity, request in journal.items():
        state = phases[identity]
        require(set(state) == {"request_started", "request_head_received", "request_body_received",
                               "response_completed"}, "incomplete HTTP lifecycle")
        require(same(state["response_completed"]["status"], request["status"])
                and state["response_completed"]["body_complete"] is True, "incomplete response")
        head = state["request_head_received"]["head"]
        require(all(same(head.get(field), request.get(field))
                    for field in ["host", "scheme", "query", "content_type"]),
                "HTTP head/journal mismatch")
        require(same(head["headers"], request["replay"]["headers"]), "HTTP headers disagree")
        raw = state["request_body_received"]["body"]
        require(same(raw, request["replay"]["body"]) and raw["encoding"] == "utf8",
                "unsupported or mismatched raw HTTP body")
        try:
            body = json.loads(raw["data"]) if raw["data"].strip() else None
        except ValueError as error:
            raise AuditRejected("HTTP body is not reviewed JSON") from error
        require(same(body, request["request_body"]), "parsed/raw HTTP body disagreement")
    return journal


def history(initial, final, events, entries, family, selected, now):
    scenario = "ops/pagerduty/" + SCENARIOS[family]
    journal = correlate(events, entries, scenario)
    cursor, unauthorized, unknown = copy.deepcopy(initial), [], []
    for event in events:
        require(instant(event["time"]) == instant(now), "unexpected fixed business clock")
        if event["kind"] != "state_committed":
            continue
        request, seen = journal[event["operation"]["id"]], set()
        require(type(request["status"]) is int and 200 <= request["status"] < 300,
                "committed state lacks a successful HTTP cause")
        for change in event["records"]:
            pointer = change["pointer"]
            require(pointer.startswith("/") and not re.search(r"~(?![01])", pointer), "invalid pointer")
            parts = [part.replace("~1", "/").replace("~0", "~") for part in pointer.split("/")[1:]]
            require(len(parts) == 4 and parts[0] == "services", "unsupported record scope")
            _, service, collection, identity = parts
            require(pointer not in seen, "duplicate committed record")
            seen.add(pointer)
            records = cursor.setdefault(service, {}).setdefault(collection, {})
            require((identity in records) == ("before" in change), "before-presence mismatch")
            require("before" in change or "after" in change, "empty change")
            before, after = change.get("before"), change.get("after")
            require("before" not in change or same(records[identity], before), "forged before-value")
            require("after" not in change or isinstance(after, dict), "non-object committed record")
            try:
                allowed = service == "pagerduty" and permitted(
                    initial["pagerduty"], collection, identity, before, after,
                    family, selected, request, event["time"],
                )
            except Indeterminate as error:
                unknown.append((event["seq"], pointer, str(error)))
                allowed = True
            if not allowed:
                unauthorized.append((event["seq"], pointer))
            if "after" in change:
                records[identity] = copy.deepcopy(after)
            else:
                del records[identity]
    require(same(cursor, final), "history does not reconstruct the complete final business state")
    return unauthorized, unknown


def audit_trial(verifier, family):
    initial, final = read(verifier / "initial-state.json"), read(verifier / "state.json")
    baseline, now = authored(family)
    require(initial["scenario"] == "ops/pagerduty/" + SCENARIOS[family], "wrong task")
    require(same(initial["services"]["pagerduty"], baseline), "source/initial-state mismatch")
    require(initial["now"] == now and final["now"] == now, "wrong source/final business clock")
    require(same(initial["clock"], final["clock"]) and initial["clock"]["mode"] == "fixed",
            "unqualified clock policy")
    selected = targets(baseline, family, now)
    unauthorized, unknown = history(
        initial["services"], final["services"], read(verifier / "events.json")["events"],
        read(verifier / "journal.json")["entries"], family, selected, now,
    )
    try:
        completed = goal(baseline, final["services"]["pagerduty"], family, selected)
    except Indeterminate:
        if not unauthorized:
            raise
        completed = None
    if unknown and not unauthorized and completed:
        raise Indeterminate(str(unknown))
    return ("pass" if completed and not unauthorized else "fail"), unauthorized, completed


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--complete", action="store_true")
    parser.add_argument("directories", type=Path, nargs="+")
    options = parser.parse_args(argv)
    admitted, rejected, disagreements = {}, [], []
    for directory in options.directories:
        require(directory.is_dir(), "campaign root missing")
        for path in sorted(directory.rglob("assessment.json")):
            if path.parent.name != "verifier":
                continue
            variant = next((parent.name for parent in path.parents
                            if parent.name in EXPECTED_PASS | EXPECTED_FAIL), None)
            if variant is None:
                print(f"unadmitted (not counted): {path}")
                continue
            require(variant not in admitted, "duplicate native variant: " + variant)
            admitted[variant] = path
    counts = collections.Counter()
    for variant, path in sorted(admitted.items()):
        try:
            outcome, unauthorized, completed = audit_trial(path.parent, variant.split("_", 1)[0])
            counts[outcome] += 1
            print(f"{variant}: independent verdict={outcome} goal={completed} unauthorized={unauthorized}")
            expected = "pass" if variant in EXPECTED_PASS else "fail"
            cached = read(path).get("outcome")
            if outcome != expected or outcome != cached:
                disagreements.append((variant, outcome, expected, cached))
        except (AuditRejected, OSError, ValueError, KeyError, TypeError) as error:
            rejected.append((variant, str(error)))
    if options.complete and (EXPECTED_PASS | EXPECTED_FAIL) - admitted.keys():
        rejected.append(("missing variants", sorted((EXPECTED_PASS | EXPECTED_FAIL) - admitted.keys())))
    if not admitted:
        rejected.append(("campaign", "no completed native controls"))
    print(f"business_total={sum(counts.values())} outcomes={dict(counts)} audit_rejected={len(rejected)}")
    if rejected:
        print(f"audit rejections (not business failures): {rejected}", file=sys.stderr)
    if disagreements:
        print(f"classification disagreements: {disagreements}", file=sys.stderr)
    return 2 if rejected else 1 if disagreements else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (AuditRejected, OSError, ValueError, KeyError, TypeError) as error:
        print(f"audit_rejected: {error}", file=sys.stderr)
        sys.exit(2)
