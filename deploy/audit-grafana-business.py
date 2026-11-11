"""Independent, deliberately bounded Grafana business and committed-history audit."""

import argparse
import collections
import copy
import json
import re
import sys
from datetime import date
from pathlib import Path
from urllib.parse import unquote


EXPECTED_PASS = {
    "ack_reference", "ack_extra_reads", "ack_username_answer", "ack_structured_answer",
    "ann_reference", "ann_numeric_dashboard", "ann_latency_panel", "ann_extra_tags",
    "ann_uid_precedence",
    "sil_reference", "sil_offset_regex", "sil_redundant", "sil_other_creator",
}
EXPECTED_FAIL = {
    "ack_wrong_answer", "ack_wrong_group", "ack_resolve_restore", "ack_datasource_edit",
    "ann_wrong_panel", "ann_region", "ann_wrong_text", "ann_extra_annotation", "ann_restore_old",
    "sil_broad", "sil_narrow", "sil_wrong_ticket", "sil_extra", "sil_existing_edit",
}
SCENARIOS = {
    "ack": "ack-checkout-page-oncall",
    "ann": "annotate-checkout-deploy",
    "sil": "silence-replica-maintenance",
}
SCENARIO_ROOT = Path(__file__).resolve().parents[1] / "scenarios" / "ops" / "grafana"
ANNOTATION_TIME = 1794492300000
ANNOTATION_TEXT = "Deploy checkout v4.19.0"
SILENCE_PATH = "/api/alertmanager/grafana/api/v2/silences"
SILENCE_LABELS = {"alertname": "PostgresReplicationLag", "instance": "pg-replica-2"}


class AuditRejected(ValueError):
    """Incomplete, inconsistent, or unsupported evidence; not a business failure."""


class Indeterminate(AuditRejected):
    """The independent bounded semantics cannot decide this form."""


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


def authored_initial(family):
    directory = SCENARIO_ROOT / SCENARIOS[family]
    header = (directory / "scenario.toml").read_text().split("\n[", 1)[0]
    clocks = re.findall(r'^now = "([^"]+)"$', header, re.MULTILINE)
    require(len(clocks) == 1, "missing or ambiguous authored scenario clock")
    initial = {}
    for path in sorted((directory / "dataset" / "grafana").glob("*.json")):
        records = {}
        for row in read(path):
            identity = row["_key"]
            require(isinstance(identity, str) and identity not in records, "ambiguous authored record")
            records[identity] = {field: value for field, value in row.items() if field != "_key"}
        initial[path.stem] = records
    require(bool(initial), "missing authored Grafana dataset")
    return initial, clocks[0]


def one(records, predicate):
    matches = [(identity, row) for identity, row in records.items() if predicate(row)]
    require(len(matches) == 1, "authored target identity is not unique")
    return matches[0]


def identities(initial, family):
    if family == "ack":
        team, _ = one(initial["oncall_teams"], lambda row: row["name"] == "Checkout")
        schedule, duty = one(initial["oncall_schedules"], lambda row: (
            row["name"] == "Checkout Primary" and row["team_id"] == team
        ))
        group, row = one(initial["oncall_alert_groups"], lambda row: (
            row["team_id"] == team and row["state"] == "new"
            and row["resolved_at"] is None and row["silenced_at"] is None
            and any(label["key"]["name"] == "env" and label["value"]["name"] == "prod"
                    for label in row["labels"])
            and "checkout" in row["title"].casefold()
            and any(alert["labels"].get("alertname") == "HighCheckoutLatency"
                    for alert in row["last_alert"]["payload"]["alerts"])
        ))
        users = duty["on_call_now"]
        require(isinstance(users, list) and bool(users) and len(set(users)) == len(users),
                "invalid or empty authored current duty")
        require(all(user in initial["oncall_users"] for user in users), "duty missing directory entry")
        require(row["id"] == group and duty["id"] == schedule, "authored identity/key mismatch")
        return {"group": group, "team": team, "schedule": schedule, "users": set(users)}
    if family == "ann":
        folder, _ = one(initial["folders"], lambda row: row["title"] == "Production")
        _, row = one(initial["dashboards"], lambda row: (
            row["folderUid"] == folder and row["dashboard"]["title"] == "Checkout Service"
        ))
        dashboard = row["dashboard"]
        panels = {
            panel["id"] for panel in dashboard["panels"]
            if panel["type"] == "timeseries" and "latency" in panel["title"].casefold()
            and any("histogram_quantile" in target.get("expr", "") for target in panel["targets"])
        }
        require(bool(panels), "no authored latency panel")
        return {"dashboard": dashboard, "panels": panels, "user": initial["user"]["current"]}
    require(family == "sil", "unknown business family")
    return {}


def instant_ns(value):
    if not isinstance(value, str):
        raise Indeterminate("timestamp is not a string")
    match = re.fullmatch(
        r"(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})"
        r"(?:\.(\d{1,9}))?(Z|[+-]\d{2}:\d{2})", value, re.ASCII,
    )
    if match is None:
        raise Indeterminate("timestamp outside strict RFC3339 nanosecond subset")
    year, month, day, hour, minute, second = map(int, match.groups()[:6])
    if hour > 23 or minute > 59 or second > 59:
        raise Indeterminate("invalid clock fields or unsupported leap second")
    try:
        days = (date(year, month, day) - date(1970, 1, 1)).days
    except ValueError as error:
        raise Indeterminate("invalid calendar date") from error
    zone = match[8]
    offset = 0
    if zone != "Z":
        offset_hour, offset_minute = int(zone[1:3]), int(zone[4:6])
        if offset_hour > 23 or offset_minute > 59 or zone == "-00:00":
            raise Indeterminate("invalid or unknown UTC offset")
        offset = (offset_hour * 60 + offset_minute) * 60 * (1 if zone[0] == "+" else -1)
    seconds = days * 86400 + hour * 3600 + minute * 60 + second - offset
    return seconds * 1_000_000_000 + int((match[7] or "").ljust(9, "0"))


def alias_index(initial):
    candidates = collections.defaultdict(set)
    for identity, user in initial["oncall_users"].items():
        for field in ("username", "email", "name"):
            if user.get(field):
                candidates[user[field].casefold()].add(identity)
        if user.get("name"):
            candidates[user["name"].split()[-1].casefold()].add(identity)
    return {alias: next(iter(owners)) for alias, owners in candidates.items() if len(owners) == 1}


def alias_person(text, aliases):
    text = text.strip().casefold()
    if text.endswith("."):
        text = text[:-1]
    if len(text) >= 2 and text[0] == text[-1] == "`":
        text = text[1:-1]
    if text in aliases:
        return aliases[text]
    parts = re.split(r"\s*(?:[()<>\[\],/]| — | - )\s*", text)
    parts = [part for part in parts if part]
    if 1 < len(parts) <= 4 and all(part in aliases for part in parts):
        owners = {aliases[part] for part in parts}
        if len(owners) == 1:
            return owners.pop()
    return None


def structured_answer(initial, targets, answer):
    unknown = False
    if isinstance(answer, list):
        users = answer
    elif isinstance(answer, dict):
        users = answer.get("users")
        schedule = initial["oncall_schedules"][targets["schedule"]]
        bindings = {
            "scheduleId": targets["schedule"], "scheduleName": schedule["name"],
            "teamId": targets["team"],
        }
        for field, value in answer.items():
            if field in bindings and not same(value, bindings[field]):
                return False
            if field not in {*bindings, "users", "schedule"}:
                unknown = True
        if "schedule" in answer:
            supplied = answer["schedule"]
            if isinstance(supplied, str):
                if supplied not in {targets["schedule"], schedule["name"]}:
                    return False
            elif isinstance(supplied, dict):
                for field, value in supplied.items():
                    source_field = {"teamId": "team_id", "timezone": "time_zone"}.get(field, field)
                    if source_field in schedule:
                        if not same(value, schedule[source_field]):
                            return False
                    else:
                        unknown = True
            else:
                unknown = True
    else:
        raise Indeterminate("unsupported structured answer root")
    if not isinstance(users, list):
        raise Indeterminate("structured answer needs a users array")
    if len(users) != len(targets["users"]):
        return False
    resolved = []
    for supplied in users:
        if not isinstance(supplied, dict):
            unknown = True
            continue
        matches = []
        identity_fields = set(supplied) & {"id", "username", "email", "name"}
        if not identity_fields:
            unknown = True
            continue
        for identity, row in initial["oncall_users"].items():
            if all(same(supplied[field], row.get(field)) for field in identity_fields):
                matches.append(identity)
        if len(matches) != 1:
            return False
        identity = matches[0]
        resolved.append(identity)
        for field, value in supplied.items():
            if field in initial["oncall_users"][identity]:
                if not same(value, initial["oncall_users"][identity][field]):
                    return False
            else:
                unknown = True
    if len(resolved) != len(set(resolved)) or not set(resolved) <= targets["users"]:
        return False
    if unknown:
        raise Indeterminate("unreviewed answer metadata or user presentation")
    return set(resolved) == targets["users"]


def answer_correct(initial, answer_bytes):
    targets = identities(initial, "ack")
    if not isinstance(answer_bytes, bytes) or len(answer_bytes) > 65536:
        raise Indeterminate("answer must be bounded canonical bytes")
    try:
        text = answer_bytes.decode("utf-8").strip()
    except UnicodeDecodeError as error:
        raise Indeterminate("answer is not UTF-8") from error
    if not text:
        return False
    try:
        parsed = json.loads(text)
    except ValueError:
        parsed = None
    if isinstance(parsed, (list, dict)):
        return structured_answer(initial, targets, parsed)
    if isinstance(parsed, str):
        text = parsed
    aliases = alias_index(initial)
    person = alias_person(text, aliases)
    if person is not None:
        return targets["users"] == {person}
    parts = re.split(r"(?:\s*;\s*|\n+| and |,\s+)", text)
    people = [alias_person(part, aliases) for part in parts]
    if all(person is not None for person in people):
        return len(people) == len(set(people)) and set(people) == targets["users"]
    affirmative = []
    unknown = False
    for sentence in re.split(r"(?<=[.!?])\s+", text):
        negative = re.fullmatch(r"(.+?) is not on[ -]call[.!]?", sentence, re.IGNORECASE)
        positive = re.fullmatch(
            r"(?:The (?:primary )?on-call person is (.+?)|(.+?) is on[ -]call(?: right now)?)[.!]?",
            sentence, re.IGNORECASE,
        )
        if negative:
            subject = alias_person(negative[1], aliases)
            if subject in targets["users"]:
                return False
            unknown = True
        elif positive:
            subject = alias_person(positive[1] or positive[2], aliases)
            if subject is not None and subject not in targets["users"]:
                return False
            if subject is None:
                unknown = True
            else:
                affirmative.append(subject)
        else:
            unknown = True
    if unknown:
        raise Indeterminate("unreviewed prose is not a whole truthful duty answer")
    return len(affirmative) == len(set(affirmative)) and set(affirmative) == targets["users"]


def regex_language(pattern):
    if pattern.startswith("^"):
        pattern = pattern[1:]
    if pattern.endswith("$") and not pattern.endswith(r"\$"):
        pattern = pattern[:-1]
    while pattern.startswith("(?:") and pattern.endswith(")"):
        pattern = pattern[3:-1]
    prefix = pattern.endswith(".*")
    if prefix:
        pattern = pattern[:-2]
    literal = ""
    position = 0
    while position < len(pattern):
        character = pattern[position]
        if character == "\\":
            position += 1
            if position >= len(pattern) or pattern[position] not in r".\+*?()[]{}|^$-":
                raise Indeterminate("unreviewed regex escape")
            character = pattern[position]
        elif character in r".+*?()[]{}|^$" or ord(character) < 32:
            raise Indeterminate("regex outside literal/singleton/prefix subset")
        literal += character
        position += 1
    return ("prefix" if prefix else "literal", literal)


def matcher_terms(matchers):
    if not isinstance(matchers, list) or not matchers:
        raise Indeterminate("invalid or empty matcher array")
    result = []
    for matcher in matchers:
        if not isinstance(matcher, dict) or set(matcher) - {"name", "value", "isRegex", "isEqual"}:
            raise Indeterminate("unreviewed matcher fields")
        name, value = matcher.get("name"), matcher.get("value")
        regex, equal = matcher.get("isRegex", False), matcher.get("isEqual", True)
        if not isinstance(name, str) or not name or not isinstance(value, str) \
                or type(regex) is not bool or type(equal) is not bool:
            raise Indeterminate("invalid matcher types")
        try:
            name.encode("utf-8")
            value.encode("utf-8")
        except UnicodeEncodeError as error:
            raise Indeterminate("matcher is not a valid UTF-8 label string") from error
        result.append((name, value, regex, equal))
    return result


def accepts(language, equal, value):
    kind, literal = language
    matched = value == literal if kind == "literal" else (
        value.startswith(literal) and "\n" not in value[len(literal):]
    )
    return matched if equal else not matched


def predicate_exact(matchers):
    grouped = collections.defaultdict(list)
    unsupported = False
    for name, value, regex, equal in matcher_terms(matchers):
        try:
            language = regex_language(value) if regex else ("literal", value)
        except Indeterminate:
            unsupported = True
            language = None
        grouped[name].append((language, equal))
    for name, constraints in grouped.items():
        if name not in SILENCE_LABELS and any(language is not None for language, _ in constraints):
            return False
    for name, target in SILENCE_LABELS.items():
        constraints = grouped.get(name, [])
        known = [(language, equal) for language, equal in constraints if language is not None]
        if any(not accepts(language, equal, target) for language, equal in known):
            return False
        singleton = any(language == ("literal", target) and equal for language, equal in known)
        if singleton:
            continue
        if len(known) == len(constraints):
            witnesses = {"", target + "-other", "pg-replica-1", "other"}
            for (kind, literal), equal in known:
                witnesses.update({literal, literal + "x", literal + "xx"})
            if any(witness != target and all(accepts(language, equal, witness)
                                           for language, equal in known) for witness in witnesses):
                return False
        unsupported = True
    if unsupported:
        raise Indeterminate("cannot prove complete predicate equality with bounded regex algebra")
    return True


def annotation_correct(initial, row):
    targets = identities(initial, "ann")
    dashboard, user = targets["dashboard"], targets["user"]
    return (
        type(row.get("id")) is int and row["id"] > 0
        and same(row.get("dashboardId"), dashboard["id"]) and row.get("dashboardUID") == dashboard["uid"]
        and type(row.get("panelId")) is int and row["panelId"] in {0, *targets["panels"]}
        and same(row.get("userId"), user["id"])
        and row.get("login") == user["login"] and row.get("email") == user["email"]
        and same(row.get("time"), ANNOTATION_TIME) and same(row.get("timeEnd"), ANNOTATION_TIME)
        and row.get("text") == ANNOTATION_TEXT and isinstance(row.get("tags"), list)
        and all(isinstance(tag, str) for tag in row["tags"]) and {"deploy", "checkout"} <= set(row["tags"])
    )


def silence_correct(row):
    if not isinstance(row.get("id"), str) or not row["id"] \
            or not isinstance(row.get("createdBy"), str) or not row["createdBy"].strip() \
            or not isinstance(row.get("comment"), str) \
            or re.search(r"(?<![\w-])CHG-7781(?![\w-])", row["comment"]) is None:
        return False
    if instant_ns(row.get("startsAt")) != instant_ns("2026-11-13T02:00:00Z") \
            or instant_ns(row.get("endsAt")) != instant_ns("2026-11-13T04:00:00Z"):
        return False
    return predicate_exact(row.get("matchers"))


def goal(initial, final, family, answer_bytes=b""):
    if family == "ack":
        target = identities(initial, family)["group"]
        row = final.get("oncall_alert_groups", {}).get(target, {})
        return row.get("state") == "acknowledged" and row.get("resolved_at") is None \
            and row.get("resolved_by") is None and row.get("silenced_at") is None \
            and row.get("acknowledged_by") is None and bool(row.get("acknowledged_at")) \
            and answer_correct(initial, answer_bytes)
    collection = "annotations" if family == "ann" else "silences"
    added = {identity: row for identity, row in final.get(collection, {}).items()
             if identity not in initial[collection]}
    if len(added) != 1:
        return False
    identity, row = next(iter(added.items()))
    if str(row.get("id")) != identity:
        return False
    return annotation_correct(initial, row) if family == "ann" else silence_correct(row)


def canonical_answer(verifier):
    trial = verifier.parent
    manifest = read(trial / "artifacts" / "manifest.json")
    matches = [entry for entry in manifest if entry.get("source") == "/app/answer.md"]
    require(len(matches) == 1, "canonical answer collection is missing or ambiguous")
    entry = matches[0]
    require(entry.get("type") == "file" and entry.get("destination") == "artifacts/agent/answer.md",
            "unrecognized canonical answer artifact binding")
    if entry.get("status") in {"missing", "empty"}:
        return b""
    require(entry.get("status") == "ok", "answer collection did not complete")
    path = trial / entry["destination"]
    require(path.is_file() and path.resolve().is_relative_to(trial.resolve()),
            "canonical collected answer file is missing or escapes trial")
    return path.read_bytes()


def route(request):
    return unquote(request["path"]).rstrip("/")


def authenticated(request):
    headers = request.get("replay", {}).get("headers", [])
    for header in headers:
        if header.get("name", "").casefold() != "authorization":
            continue
        value = header.get("value", {})
        if value.get("kind") == "public" and value.get("value"):
            return True
        if value.get("kind") in {"synthetic", "redacted"} and value.get("sha256"):
            return True
    return False


def journal_index(entries):
    result = {}
    previous = 0
    for entry in entries:
        identity = entry["operation_id"]
        require(type(identity) is int and identity > 0 and identity not in result,
                "duplicate or invalid journal operation identity")
        require(type(entry["seq"]) is int and entry["seq"] > previous, "unordered journal")
        previous = entry["seq"]
        result[identity] = entry
    return result


def correlate(events, journal):
    phases = collections.defaultdict(dict)
    previous = 0
    for event in events:
        require(type(event["seq"]) is int and event["seq"] > previous, "unordered event history")
        previous = event["seq"]
        kind = event["kind"]
        require(kind in {
            "request_started", "request_head_received", "request_body_received",
            "state_committed", "response_completed",
        }, f"unsupported or failed environment event: {kind}")
        operation = event["operation"]
        require(operation["id"] in journal, "event has no journal cause")
        request = journal[operation["id"]]
        for field in ("service", "method", "path", "source"):
            require(same(operation.get(field), request.get(field)), f"operation/journal {field} mismatch")
        if kind == "state_committed":
            require("request_body_received" in phases[operation["id"]]
                    and "response_completed" not in phases[operation["id"]],
                    "commit outside its HTTP request")
            continue
        require(kind not in phases[operation["id"]], "duplicate HTTP evidence phase")
        prior = {
            "request_head_received": "request_started",
            "request_body_received": "request_head_received",
            "response_completed": "request_body_received",
        }.get(kind)
        require(prior is None or prior in phases[operation["id"]], "out-of-order HTTP evidence")
        phases[operation["id"]][kind] = event
    for identity, request in journal.items():
        evidence = phases[identity]
        require(set(evidence) == {
            "request_started", "request_head_received", "request_body_received", "response_completed",
        }, "incomplete journal/event correlation")
        response = evidence["response_completed"]
        require(same(response["status"], request["status"]) and response.get("body_complete") is True,
                "response evidence incomplete or status mismatch")
        head = evidence["request_head_received"]["head"]
        for field in ("host", "scheme", "query", "content_type"):
            require(same(head.get(field), request.get(field)), f"HTTP head/journal {field} mismatch")
        require(same(head["headers"], request["replay"]["headers"]), "HTTP header evidence mismatch")
        raw = evidence["request_body_received"]["body"]
        require(same(raw, request["replay"]["body"]), "raw HTTP request body mismatch")
        require(raw.get("encoding") == "utf8" and isinstance(raw.get("data"), str),
                "unsupported raw HTTP body encoding")
        try:
            body = json.loads(raw["data"]) if raw["data"].strip() else None
        except ValueError as error:
            raise AuditRejected("non-JSON raw HTTP body requires adjudication") from error
        require(same(body, request["request_body"]), "parsed body/journal mismatch; MCP args are not HTTP")


def permitted_change(initial, cursor, collection, identity, before, after, family, request, event_time):
    if after is None:
        return False
    path = route(request)
    if request["service"] != "grafana" or request["method"] != "POST" \
            or type(request["status"]) is not int or not 200 <= request["status"] < 300 \
            or not authenticated(request):
        return False
    if family == "ack":
        target = identities(initial, family)["group"]
        if collection != "oncall_alert_groups" or identity != target or before is None \
                or path != f"/oncall/api/v1/alert_groups/{target}/acknowledge":
            return False
        expected = copy.deepcopy(before)
        expected.update(state="acknowledged", acknowledged_at=event_time, acknowledged_by=None)
        return before["state"] == "new" and same(after, expected)
    target_collection = "annotations" if family == "ann" else "silences"
    if collection != target_collection or before is not None or identity in initial[target_collection] \
            or any(key not in initial[target_collection] for key in cursor.get(target_collection, {})):
        return False
    body = request["request_body"]
    if not isinstance(body, dict) or str(after.get("id")) != identity:
        return False
    if family == "ann":
        if path != "/api/annotations" or not annotation_correct(initial, after):
            return False
        if set(body) - {"dashboardUID", "dashboardId", "panelId", "time", "timeEnd", "text", "tags", "data"}:
            raise Indeterminate("unreviewed annotation request fields")
        targets = identities(initial, family)
        dashboard = targets["dashboard"]
        if body.get("dashboardUID"):
            if body["dashboardUID"] != dashboard["uid"]:
                return False
        elif not same(body.get("dashboardId"), dashboard["id"]):
            return False
        for field, value in {
            "panelId": body.get("panelId", 0), "time": body.get("time"),
            "timeEnd": body.get("timeEnd", body.get("time")),
            "text": body.get("text"), "tags": body.get("tags"), "data": body.get("data"),
        }.items():
            if not same(after.get(field), value):
                return False
        response = request.get("response_body")
        if not isinstance(response, dict) or not same(response.get("id"), after["id"]):
            return False
        if instant_ns(event_time) % 1_000_000:
            raise Indeterminate("annotation commit clock not an exact millisecond")
        milliseconds = instant_ns(event_time) // 1_000_000
        if not same(after.get("created"), milliseconds) or not same(after.get("updated"), milliseconds):
            return False
        if set(after) - {
            "id", "dashboardId", "dashboardUID", "panelId", "userId", "login", "email",
            "created", "updated", "time", "timeEnd", "text", "tags", "data",
        }:
            raise Indeterminate("unreviewed annotation record fields")
        return True
    if path != SILENCE_PATH or body.get("id") not in {None, ""} or not silence_correct(after):
        return False
    if set(body) - {"id", "matchers", "startsAt", "endsAt", "createdBy", "comment"}:
        raise Indeterminate("unreviewed silence request fields")
    if set(after) - {"id", "matchers", "startsAt", "endsAt", "createdBy", "comment", "updatedAt"}:
        raise Indeterminate("unreviewed silence record fields")
    response = request.get("response_body")
    if not isinstance(response, dict) or response.get("silenceID") != after["id"]:
        return False
    if collections.Counter(matcher_terms(body.get("matchers"))) != collections.Counter(
        matcher_terms(after.get("matchers")),
    ):
        return False
    return all(same(body.get(field), after.get(field)) for field in ("createdBy", "comment")) \
        and all(instant_ns(body.get(field)) == instant_ns(after.get(field))
                for field in ("startsAt", "endsAt")) \
        and instant_ns(after.get("updatedAt")) == instant_ns(event_time)


def history(initial_services, final_services, events, journal, family):
    correlate(events, journal)
    initial = initial_services["grafana"]
    cursor = copy.deepcopy(initial_services)
    unauthorized = []
    for request in journal.values():
        path = route(request)
        if family == "sil" and request["method"] == "DELETE":
            unauthorized.append(("forbidden_delete_attempt", request["operation_id"]))
        if family == "ack" and request["method"] not in {"GET", "HEAD", "OPTIONS"} and (
            re.fullmatch(r"/oncall/api/v1/alert_groups/[^/]+/(?:resolve|unresolve|silence|unsilence)", path)
            or re.search(r"/(?:silence|silences)(?:/|$)", path)
        ):
            unauthorized.append(("forbidden_resolve_or_silence_attempt", request["operation_id"]))
    for event in events:
        if event["kind"] != "state_committed":
            continue
        request = journal[event["operation"]["id"]]
        require(type(request["status"]) is int and 200 <= request["status"] < 300,
                "committed state has no successful HTTP cause")
        seen = set()
        for change in event["records"]:
            pointer = change["pointer"]
            require(pointer.startswith("/") and re.search(r"~(?![01])", pointer) is None,
                    "invalid JSON pointer")
            parts = [part.replace("~1", "/").replace("~0", "~") for part in pointer.split("/")[1:]]
            require(len(parts) == 4 and parts[0] == "services", "not a committed service record")
            _, service, collection, identity = parts
            require(pointer not in seen, "duplicate record in one commit")
            seen.add(pointer)
            records = cursor.get(service, {}).get(collection, {})
            require((identity in records) == ("before" in change), "before-presence/record mismatch")
            require("before" in change or "after" in change, "empty record change")
            if "before" in change:
                require(same(records[identity], change["before"]), "forged before-value: " + pointer)
            before, after = change.get("before"), change.get("after")
            require("after" not in change or isinstance(after, dict), "committed record is not an object")
            allowed = service == "grafana" and permitted_change(
                initial, cursor["grafana"], collection, identity, before, after,
                family, request, event["time"],
            )
            if not allowed:
                unauthorized.append((event["seq"], pointer))
            if "after" in change:
                cursor.setdefault(service, {}).setdefault(collection, {})[identity] = copy.deepcopy(after)
            else:
                del cursor[service][collection][identity]
    require(same(cursor, final_services), "committed history does not reconstruct complete final state")
    return unauthorized


def audit_trial(verifier, family):
    initial_full, final_full = read(verifier / "initial-state.json"), read(verifier / "state.json")
    authored, now = authored_initial(family)
    require(same(initial_full["services"]["grafana"], authored), "initial state differs from authored dataset")
    require(initial_full["scenario"] == "ops/grafana/" + SCENARIOS[family], "wrong scenario evidence")
    require(initial_full["now"] == now, "wrong authored scenario clock")
    journal = journal_index(read(verifier / "journal.json")["entries"])
    unauthorized = history(
        initial_full["services"], final_full["services"],
        read(verifier / "events.json")["events"], journal, family,
    )
    try:
        answer_bytes = canonical_answer(verifier) if family == "ack" else b""
        completed = goal(authored, final_full["services"]["grafana"], family, answer_bytes)
    except Indeterminate:
        if not unauthorized:
            raise
        completed = None
    return ("pass" if not unauthorized and completed else "fail"), unauthorized, completed


def discover(directories):
    admitted, unadmitted = {}, []
    for directory in directories:
        require(directory.is_dir(), "campaign root is not a directory: " + str(directory))
        for assessment in sorted(directory.rglob("assessment.json")):
            if assessment.parent.name != "verifier":
                continue
            variant = next((parent.name for parent in assessment.parents
                            if parent.name in EXPECTED_PASS | EXPECTED_FAIL), None)
            if variant is None:
                unadmitted.append(assessment)
                continue
            require(variant not in admitted, "duplicate variant admission: " + variant)
            admitted[variant] = assessment
    return admitted, unadmitted


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--complete", action="store_true")
    parser.add_argument("directories", type=Path, nargs="+")
    options = parser.parse_args(argv)
    try:
        admitted, unadmitted = discover(options.directories)
    except (AuditRejected, OSError, ValueError) as error:
        print(f"audit_rejected: {error}", file=sys.stderr)
        return 2
    for path in unadmitted:
        print(f"unadmitted (not scored): {path}")
    counts = collections.Counter()
    rejected, disagreements = [], []
    for variant, assessment in sorted(admitted.items()):
        try:
            actual, unauthorized, completed = audit_trial(assessment.parent, variant.split("_", 1)[0])
            print(f"{variant}: independent business/history verdict={actual} "
                  f"goal={completed} unauthorized={unauthorized}")
            counts[actual] += 1
            expected = "pass" if variant in EXPECTED_PASS else "fail"
            cached = read(assessment).get("outcome")
            if actual != expected or actual != cached:
                disagreements.append((variant, actual, expected, cached))
        except (AuditRejected, OSError, ValueError, KeyError, TypeError) as error:
            rejected.append((variant, str(error)))
            print(f"{variant}: audit_rejected (not a business fail): {error}", file=sys.stderr)
    missing = (EXPECTED_PASS | EXPECTED_FAIL) - admitted.keys()
    if options.complete and missing:
        rejected.append(("missing variants", sorted(missing)))
    if not admitted:
        rejected.append(("campaign", "no admitted completed trials"))
    print(f"business_total={sum(counts.values())} outcomes={dict(counts)} audit_rejected={len(rejected)}")
    if disagreements:
        print(f"classification/assessment disagreements: {disagreements}", file=sys.stderr)
    if rejected:
        print(f"audit rejections: {rejected}", file=sys.stderr)
    return 2 if rejected else 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
