"""Independent source-bound Datadog business and complete committed-history audit.

This is a bounded local-fixture audit, not hosted Datadog conformance or runtime
qualification. Actor 1445416 is the fixture's application-key convention, not an
authenticated hosted identity. Host mute has no locally modeled V2 downtime.
Messages are checked only for the complete requested identifier, not arbitrary
prose meaning. HTTP completion establishes captured server evidence, not receipt
by an MCP client or native model. No evaluator predicates or outcomes are inputs.
"""

import argparse
import collections
import copy
import hashlib
import json
import re
import sys
from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path
from urllib.parse import unquote
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError


REPO = Path(__file__).resolve().parents[1]
ROOT = REPO / "scenarios" / "ops" / "datadog"
SCENARIOS = {
    "cancel": "cancel-called-off-maintenance",
    "mute": "mute-host-from-error-logs",
    "schedule": "schedule-replica-downtime",
}
LOCAL_ACTOR = 1445416
NANOSECONDS = 1_000_000_000
LOG_PATH = "/api/v2/logs/events/search"
DOWNTIME_PATH = "/api/v1/downtime"


class AuditRejected(ValueError):
    """Missing or inconsistent evidence; not a business failure."""


class Indeterminate(AuditRejected):
    """Unreviewed syntax, actor, or side-effect semantics need adjudication."""


def require(condition, message):
    if not condition:
        raise AuditRejected(message)


def loads(text):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "duplicate JSON member: " + key)
            result[key] = value
        return result

    def nonfinite(value):
        raise AuditRejected("nonfinite JSON number: " + value)

    return json.loads(text, parse_float=Decimal, parse_constant=nonfinite, object_pairs_hook=pairs)


def read(path):
    return loads(Path(path).read_bytes())


def numeric(value):
    if type(value) not in {int, float, Decimal}:
        return None
    result = Decimal(str(value))
    return result if result.is_finite() else None


def same(left, right):
    if numeric(left) is not None and numeric(right) is not None:
        return numeric(left) == numeric(right)
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(same(left[key], right[key]) for key in left)
    if isinstance(left, list):
        return len(left) == len(right) and all(same(first, second) for first, second in zip(left, right))
    return left == right


def instant_ns(value):
    if not isinstance(value, str):
        raise Indeterminate("timestamp is not text")
    match = re.fullmatch(
        r"(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})"
        r"(?:\.(\d{1,9}))?(Z|[+-]\d{2}:\d{2})", value, re.ASCII,
    )
    if match is None:
        raise Indeterminate("timestamp outside reviewed RFC3339 nanosecond domain")
    year, month, day, hour, minute, second = map(int, match.groups()[:6])
    require(hour < 24 and minute < 60 and second < 60, "invalid clock fields")
    try:
        days = (date(year, month, day) - date(1970, 1, 1)).days
    except ValueError as error:
        raise AuditRejected("invalid calendar date") from error
    offset = 0
    zone = match[8]
    if zone != "Z":
        hours, minutes = int(zone[1:3]), int(zone[4:6])
        require(hours < 24 and minutes < 60 and zone != "-00:00", "invalid or unknown UTC offset")
        offset = (hours * 3600 + minutes * 60) * (-1 if zone[0] == "-" else 1)
    return ((days * 86400 + hour * 3600 + minute * 60 + second - offset) * NANOSECONDS
            + int((match[7] or "").ljust(9, "0")))


def epoch(value):
    return Decimal(instant_ns(value)) / NANOSECONDS


def identifier(value):
    number = numeric(value)
    require(number is not None and number > 0 and number == number.to_integral_value(),
            "resource ID is not a positive exact numeric identity")
    return str(int(number))


def reference(message, token):
    return isinstance(message, str) and re.search(
        r"(?<!\w)" + re.escape(token) + r"(?!\w)", message,
        flags=re.IGNORECASE | re.ASCII,
    ) is not None


def family_name(value):
    for family, slug in SCENARIOS.items():
        if value in {family, slug, "ops/datadog/" + slug}:
            return family
    raise AuditRejected("unknown Datadog scenario/family: " + str(value))


def source_contract(family):
    family = family_name(family)
    directory = ROOT / SCENARIOS[family]
    header = (directory / "scenario.toml").read_text().split("\n[", 1)[0]
    clocks = re.findall(r'^now = "([^"]+)"$', header, re.MULTILINE)
    require(len(clocks) == 1, "missing or ambiguous authored scenario clock")
    instruction = (directory / "instruction.md").read_text()
    result = {"now": clocks[0]}
    if family == "cancel":
        require("production core switch replacement for payments (CHG-4410)" in instruction
                and "this Saturday night" in instruction and "cancel the Datadog downtime" in instruction
                and "Everything else stays as scheduled, including tonight's staging rehearsal" in instruction,
                "cancel instruction needs independent review")
        today = date.fromisoformat(clocks[0][:10])
        result.update(ticket="CHG-4410", saturday=today + timedelta(days=(5 - today.weekday()) % 7))
    elif family == "mute":
        match = re.search(r"Since about (\d\d:\d\d) UTC today \((\d{4}-\d\d-\d\d)\)", instruction)
        until = re.search(r"until (\d\d:\d\d) UTC today", instruction)
        require(match is not None and until is not None and '"connection reset by peer"' in instruction
                and "production checkout API hosts" in instruction and "Use the logs in Datadog" in instruction
                and "INC-2291" in instruction and "Don't mute or unmute any other hosts" in instruction,
                "mute instruction needs independent review")
        result.update(
            ticket="INC-2291", onset=epoch(f"{match[2]}T{match[1]}:00Z"),
            cutoff=epoch(clocks[0]), end=epoch(f"{match[2]}T{until[1]}:00Z"),
        )
    else:
        match = re.search(
            r"replica `([^`]+)`, from (\d\d:\d\d) to (\d\d:\d\d) UTC on (\d{4}-\d\d-\d\d)",
            instruction,
        )
        require(match is not None and "**production** Postgres replication lag monitor" in instruction
                and "scoped to just that host" in instruction and "covering exactly that window" in instruction
                and "CHG-5012" in instruction and "Don't change or cancel any of the existing downtimes" in instruction,
                "schedule instruction needs independent review")
        result.update(
            ticket="CHG-5012", host=match[1], start=epoch(f"{match[4]}T{match[2]}:00Z"),
            end=epoch(f"{match[4]}T{match[3]}:00Z"),
        )
    return result


def authored(family):
    family = family_name(family)
    contract = source_contract(family)
    initial = {}
    for path in sorted((ROOT / SCENARIOS[family] / "dataset" / "datadog").glob("*.json")):
        records = {}
        for source in read(path):
            row = copy.deepcopy(source)
            key = row.pop("_key", None)
            if key is None:
                key = str(row.get("id"))
            require(isinstance(key, str) and key and key != "None" and key not in records,
                    "authored records need unique key/value identities")
            if path.stem in {"downtimes", "monitors"}:
                require(identifier(row.get("id")) == key, "authored numeric identity/key mismatch")
            if path.stem == "hosts":
                require(row.get("name") == key and row.get("host_name") == key,
                        "authored host name/key mismatch")
            records[key] = row
        initial[path.stem] = records
    require(bool(initial), "empty authored dataset")
    return initial, contract["now"]


authored_initial = authored


def one(records, predicate):
    selected = [(key, row) for key, row in records.items() if predicate(row)]
    require(len(selected) == 1, "business identity is missing or ambiguous")
    return selected[0]


def host_tags(row):
    result = collections.defaultdict(set)
    sources = row.get("tags_by_source")
    require(isinstance(sources, dict), "source host tags are missing")
    for tags in sources.values():
        require(isinstance(tags, list), "source host tags are not complete lists")
        for tag in tags:
            require(isinstance(tag, str) and ":" in tag, "source tag needs independent review")
            key, value = tag.split(":", 1)
            result[key].add(value)
    result["host"] = {row["name"]}
    return result


def matching_log(row, contract):
    attributes = row.get("attributes", {})
    return (
        row.get("type") == "log" and attributes.get("service") == "checkout-api"
        and attributes.get("status") == "error"
        and attributes.get("attributes", {}).get("env") == "prod"
        and "env:prod" in attributes.get("tags", [])
        and "connection reset by peer" in attributes.get("message", "")
        and contract["onset"] <= epoch(attributes.get("timestamp")) < contract["cutoff"]
    )


def targets(initial, family):
    family = family_name(family)
    result = source_contract(family)
    if family == "cancel":
        key, row = one(initial["downtimes"], lambda row: (
            reference(row.get("message"), result["ticket"])
            and "core switch replacement" in row["message"]
            and set(row["scope"]) == {"env:prod", "service:payments-api"}
            and row["canceled"] is None and row["disabled"] is False
            and row["start"] > epoch(result["now"])
            and epoch(result["saturday"].isoformat() + "T00:00:00Z") <= row["start"]
            < epoch((result["saturday"] + timedelta(days=1)).isoformat() + "T00:00:00Z")
        ))
        require(identifier(row["id"]) == key, "cancel target ID/key mismatch")
        result["downtime"] = key
    elif family == "mute":
        logs = {key: row for key, row in initial["logs"].items() if matching_log(row, result)}
        hosts = {row["attributes"]["host"] for row in logs.values()}
        require(len(hosts) == 1, "source logs do not identify one emitting host")
        host = hosts.pop()
        require(host in initial["hosts"] and {"prod"} <= host_tags(initial["hosts"][host])["env"]
                and {"checkout-api"} <= host_tags(initial["hosts"][host])["service"],
                "emitting host does not resolve to production checkout API")
        result.update(host=host, logs=logs)
    else:
        host = result["host"]
        require(host in initial["hosts"], "requested replica absent from source hosts")
        tags = host_tags(initial["hosts"][host])
        require(tags["env"] == {"prod"} and "replica" in tags["role"], "requested host is not a prod replica")
        key, monitor = one(initial["monitors"], lambda row: (
            row["name"] == "Postgres replication lag" and "env:prod" in row["tags"]
            and "service:postgres" in row["tags"] and "postgresql.replication_delay" in row["query"]
            and "by {host}" in row["query"]
        ))
        require(identifier(monitor["id"]) == key, "monitor ID/key mismatch")
        result.update(monitor=monitor["id"], tags=tags)
    return result


def scope_terms(scope):
    if isinstance(scope, str):
        scope = [scope]
    if not isinstance(scope, list) or not scope or any(not isinstance(item, str) for item in scope):
        raise Indeterminate("scope is outside reviewed positive-conjunction syntax")
    terms = []
    for item in scope:
        for token in item.split(","):
            token = token.strip()
            if not token:
                raise Indeterminate("empty scope conjunct has no reviewed semantics")
            if token == "*":
                continue
            match = re.fullmatch(r"([A-Za-z0-9_.-]+):([A-Za-z0-9_./:*-]+)", token, re.ASCII)
            if match is None:
                raise Indeterminate("unreviewed scope term: " + token)
            terms.append(match.groups())
    if not any(item.strip(" ,\t\r\n") for item in scope):
        raise Indeterminate("empty scope has no reviewed semantics")
    return terms


def glob_matches(pattern, value):
    return re.fullmatch(".*".join(re.escape(part) for part in pattern.split("*")), value) is not None


def scope_exact(scope, selected):
    terms = scope_terms(scope)
    for key, pattern in terms:
        if not any(glob_matches(pattern, value) for value in selected["tags"].get(key, set())):
            return False
    host = selected["host"]
    host_constraints = [pattern for key, pattern in terms if key == "host"]
    if host in host_constraints:
        return True
    witness = host + host
    if all(glob_matches(pattern, witness) for pattern in host_constraints):
        return False
    raise Indeterminate("no independent all-values singleton proof for host globs")


def schedule_correct(row, selected):
    if not (
        numeric(row.get("start")) == selected["start"] and numeric(row.get("end")) == selected["end"]
        and numeric(row.get("monitor_id")) == numeric(selected["monitor"])
        and row.get("recurrence") is None and row.get("disabled") is False
        and row.get("canceled") is None and row.get("active") is False
        and row.get("parent_id") is None and row.get("active_child") is None
        and reference(row.get("message"), selected["ticket"])
    ):
        return False
    monitor_tags = row.get("monitor_tags")
    if not isinstance(monitor_tags, list) or not monitor_tags or any(tag != "*" for tag in monitor_tags):
        raise Indeterminate("combined monitor ID/nontrivial monitor-tag selector is unreviewed")
    return scope_exact(row.get("scope"), selected)


def goal(initial, final, family, selected=None):
    family = family_name(family)
    selected = selected or targets(initial, family)
    if family == "cancel":
        original = initial["downtimes"][selected["downtime"]]
        expected = copy.deepcopy(original)
        expected.update(canceled=epoch(selected["now"]), disabled=True, active=False, updater_id=LOCAL_ACTOR)
        return same(final.get("downtimes", {}).get(selected["downtime"]), expected)
    if family == "mute":
        host = selected["host"]
        expected = copy.deepcopy(initial["hosts"][host])
        expected.update(is_muted=True, mute_timeout=selected["end"])
        mute = final.get("host_mutes", {}).get(host, {})
        return (
            same(final.get("hosts", {}).get(host), expected) and mute.get("hostname") == host
            and numeric(mute.get("end")) == selected["end"] and mute.get("muted_at") == selected["now"]
            and reference(mute.get("message"), selected["ticket"])
        )
    added = [(key, row) for key, row in final.get("downtimes", {}).items()
             if key not in initial["downtimes"]]
    if len(added) != 1:
        return False
    key, row = added[0]
    return identifier(row.get("id")) == key and schedule_correct(row, selected)


def route(request):
    require(isinstance(request.get("path"), str), "HTTP request has no path")
    return unquote(request["path"], errors="strict").rstrip("/") or "/"


def pinned_credentials():
    catalog = (REPO / "catalog" / "services.toml").read_text()
    services = [
        section for section in catalog.split("[[service]]")
        if re.search(r'^name = "datadog"$', section, re.MULTILINE)
    ]
    require(len(services) == 1 and services[0].count("[[service.endpoint]]") == 1,
            "ambiguous Datadog endpoint")
    endpoint = services[0].split("[[service.endpoint]]", 1)[1]

    def literal(name):
        values = re.findall(r'\b' + re.escape(name) + r' = "([^"\\\n]*)"', endpoint)
        require(len(values) == 1, "unreviewed Datadog catalog literal: " + name)
        return values[0]

    require(literal("package") == "@winor30/mcp-server-datadog" and literal("version") == "1.8.0",
            "Datadog native publisher pin changed")
    snapshot = read(REPO / "servers" / "snapshots" / "datadog.json")
    names = {tool["name"] for tool in snapshot["tools"]}
    require(snapshot["server_info"] == {"name": "Datadog MCP Server", "version": "1.8.0"}
            and snapshot["protocol_version"] == "2025-06-18" and len(snapshot["tools"]) == 21
            and len(names) == 21 and {
                "list_downtimes", "schedule_downtime", "cancel_downtime", "get_logs",
                "get_monitors", "mute_host", "unmute_host", "list_hosts",
            } <= names,
            "Datadog static MCP 21-tool pin changed; not runtime surface evidence")
    return {
        "dd-api-key": literal("DATADOG_API_KEY"),
        "dd-application-key": literal("DATADOG_APP_KEY"),
    }


def fixture_request(request, credentials, tools_ips):
    if request.get("scheme") != "https" or request.get("host") not in {
        "api.datadoghq.com", "api.datadoghq.com:443",
    }:
        return False
    if tools_ips is not None and request["source"] not in tools_ips:
        return False
    headers = collections.defaultdict(list)
    for header in request["replay"]["headers"]:
        headers[header["name"].casefold()].append(header["value"])
    for name, expected in credentials.items():
        require(len(headers[name]) <= 1, "ambiguous credential header")
        if not headers[name]:
            return False
        captured = headers[name][0]
        if captured.get("kind") == "public":
            known = captured.get("value") == expected
        elif captured.get("kind") in {"synthetic", "redacted"}:
            known = captured.get("sha256") == "sha256:" + hashlib.sha256(expected.encode()).hexdigest()
        else:
            known = False
        if not known:
            raise Indeterminate("credential is not bound to the explicit local fixture actor convention")
    return True


def correlate(events, entries, scenario, now):
    require(isinstance(events, list) and isinstance(entries, list), "missing HTTP evidence arrays")
    journal, phases, operations = {}, collections.defaultdict(dict), {}
    for expected_seq, request in enumerate(entries, 1):
        require(isinstance(request, dict), "HTTP journal record is not an object")
        require(all(field in request for field in (
            "seq", "operation_id", "time", "service", "source", "method", "path", "host", "scheme",
            "query", "content_type", "status", "request_body", "response_body", "replay",
        )), "incomplete HTTP journal record")
        require(all(isinstance(request[field], str) and request[field] for field in (
            "service", "source", "method", "path",
        )), "missing HTTP request identity")
        identity = request.get("operation_id")
        require(type(identity) is int and identity > 0 and identity not in journal,
                "invalid or duplicate journal operation identity")
        require(type(request.get("seq")) is int and request["seq"] == expected_seq, "journal replay gap")
        require(instant_ns(request.get("time")) == instant_ns(now), "journal/source clock mismatch")
        require(type(request.get("status")) is int and 100 <= request["status"] < 600,
                "invalid HTTP status")
        journal[identity] = request
    generation = None
    active = set()
    for expected_seq, event in enumerate(events, 1):
        require(isinstance(event, dict), "HTTP event is not an object")
        require(type(event.get("seq")) is int and event["seq"] == expected_seq, "event replay gap")
        require(instant_ns(event.get("time")) == instant_ns(now), "event/source clock mismatch")
        operation, kind = event.get("operation"), event.get("kind")
        require(isinstance(operation, dict) and operation.get("kind") == "http"
                and operation.get("scenario") == scenario, "wrong operation kind/scenario")
        identity = operation.get("id")
        require(type(identity) is int and identity in journal, "event has no HTTP journal cause")
        require("parent_id" in operation and type(operation.get("generation")) is int
                and operation["generation"] >= 0, "operation lacks parent/generation evidence")
        if generation is None:
            generation = operation["generation"]
        require(operation["generation"] == generation, "operation generation changed")
        request = journal[identity]
        require(all(same(operation.get(field), request.get(field)) for field in (
            "service", "method", "path", "source",
        )), "HTTP operation/journal mismatch")
        state = phases[identity]
        if kind == "request_started":
            require(identity == event["seq"] and identity not in operations, "invalid request admission identity")
            parent = operation["parent_id"]
            require(parent is None or type(parent) is int and parent in active,
                    "missing, finished, or cyclic parent operation")
            operations[identity] = copy.deepcopy(operation)
            active.add(identity)
        else:
            require(identity in operations and same(operation, operations[identity]),
                    "operation context/parent changed between HTTP phases")
        if kind == "state_committed":
            require("request_body_received" in state and identity in active
                    and "state_committed" not in state, "commit outside request or repeated transaction")
            state[kind] = event
            continue
        prior = {
            "request_started": None, "request_head_received": "request_started",
            "request_body_received": "request_head_received", "response_completed": "request_body_received",
        }
        require(kind in prior and kind not in state and (prior[kind] is None or prior[kind] in state),
                "incomplete, duplicate, failed, or unsupported HTTP phase")
        state[kind] = event
        if kind == "request_head_received":
            require(isinstance(event.get("head"), dict)
                    and all(field in event["head"] for field in (
                        "host", "scheme", "query", "content_type", "headers",
                    )) and isinstance(event["head"]["headers"], list), "missing HTTP head/header receipt")
        if kind == "request_body_received":
            require(isinstance(event.get("body"), dict), "missing raw HTTP body receipt")
        if kind == "response_completed":
            require(identity in active, "HTTP response completed more than once")
            active.remove(identity)
    require(not active, "incomplete HTTP operations")
    for identity, request in journal.items():
        state = phases[identity]
        require({"request_started", "request_head_received", "request_body_received", "response_completed"}
                <= state.keys(), "incomplete journal/HTTP correlation")
        completed = state["response_completed"]
        require(same(completed.get("status"), request["status"]) and completed.get("body_complete") is True,
                "response evidence incomplete or status mismatch")
        head = state["request_head_received"]["head"]
        require(all(same(head.get(field), request.get(field))
                    for field in ("host", "scheme", "query", "content_type")), "HTTP head/journal mismatch")
        replay = request.get("replay")
        require(isinstance(replay, dict) and "parent_operation_id" in replay
                and same(replay["parent_operation_id"], operations[identity]["parent_id"]),
                "HTTP replay/parent mismatch")
        require(same(head.get("headers"), replay.get("headers")), "HTTP header evidence mismatch")
        raw = state["request_body_received"]["body"]
        require(same(raw, replay.get("body")) and raw.get("encoding") == "utf8"
                and isinstance(raw.get("data"), str), "raw HTTP request evidence mismatch")
        try:
            body = loads(raw["data"]) if raw["data"] else None
        except ValueError as error:
            raise AuditRejected("raw HTTP body is not reviewed JSON") from error
        require(same(body, request.get("request_body")), "parsed/raw HTTP body mismatch; MCP args are not HTTP")
    return journal, phases


def startup_ids(journal, startup):
    budget = collections.Counter(
        (row["service"], row["method"], row["path"], row.get("query")) for row in startup
    )
    excluded = set()
    for identity, request in journal.items():
        signature = tuple(request.get(field) for field in ("service", "method", "path", "query"))
        if budget[signature]:
            excluded.add(identity)
            budget[signature] -= 1
        if request.get("startup") is True:
            excluded.add(identity)
    return excluded


def log_evidence(initial, journal, phases, selected, credentials, startup, tools_ips):
    excluded = startup_ids(journal, startup)
    completions = []
    for identity, request in journal.items():
        if request["service"] != "datadog" or request["method"] != "POST" or route(request) != LOG_PATH \
                or not 200 <= request["status"] < 300:
            continue
        response = request.get("response_body")
        require(isinstance(response, dict) and isinstance(response.get("data"), list),
                "successful HTTP log response has no complete data array")
        for row in response["data"]:
            require(isinstance(row, dict) and isinstance(row.get("id"), str)
                    and row["id"] in initial["logs"] and same(row, initial["logs"][row["id"]]),
                    "HTTP log response contains a partial, spoofed, or non-source log")
        if identity not in excluded and fixture_request(request, credentials, tools_ips) and any(
            row["id"] in selected["logs"] for row in response["data"]
        ):
            completions.append(phases[identity]["response_completed"]["seq"])
    missing = []
    for identity, request in journal.items():
        if request["service"] == "datadog" and request["method"] == "POST" \
                and route(request) == f"/api/v1/host/{selected['host']}/mute":
            admission = phases[identity]["request_started"]["seq"]
            if not any(completed < admission for completed in completions):
                missing.append(("missing_prior_log_evidence", identity))
    return missing


def split_scope(value):
    if isinstance(value, str):
        value = [value]
    require(isinstance(value, list) and all(isinstance(item, str) for item in value),
            "successful create has no wire scope")
    terms = [term.strip() for item in value for term in item.split(",")]
    require(terms and all(terms), "successful create has an empty wire scope conjunct")
    return terms


def expected_schedule(body, key, now=None):
    require(isinstance(body, dict), "successful downtime create has no JSON object")
    if set(body) - {
        "scope", "start", "end", "message", "timezone", "monitor_id", "monitor_tags", "recurrence",
        "mute_first_recovery_notification", "notify_end_states", "notify_end_types",
    }:
        raise Indeterminate("unreviewed downtime HTTP request fields")
    recurrence = body.get("recurrence")
    if recurrence is not None:
        require(isinstance(recurrence, dict), "recurrence evidence is not an object")
        if set(recurrence) - {"type", "period", "rrule", "until_date", "until_occurrences", "week_days"}:
            raise Indeterminate("unreviewed recurrence request fields")
        recurrence = {
            "type": recurrence.get("type"), "period": recurrence.get("period", 1),
            **{field: recurrence.get(field) for field in (
                "rrule", "until_date", "until_occurrences", "week_days",
            )},
        }
    monitor_tags = body.get("monitor_tags")
    if monitor_tags is None:
        monitor_tags = ["*"]
    elif isinstance(monitor_tags, str):
        monitor_tags = [term.strip() for term in monitor_tags.split(",")]
    return {
        "id": int(key), "active": False, "active_child": None, "canceled": None,
        "creator_id": LOCAL_ACTOR, "disabled": False,
        "downtime_type": 2 if body.get("monitor_id") is not None else 0,
        "end": body.get("end"), "message": body.get("message"), "monitor_id": body.get("monitor_id"),
        "monitor_tags": monitor_tags,
        "mute_first_recovery_notification": body.get("mute_first_recovery_notification", False),
        "notify_end_states": body.get("notify_end_states", ["alert", "no data", "warn"]),
        "notify_end_types": body.get("notify_end_types", ["expired"]),
        "parent_id": None, "recurrence": recurrence, "scope": split_scope(body.get("scope")),
        "start": body.get("start") if body.get("start") is not None else epoch(now),
        "timezone": body.get("timezone") if body.get("timezone") is not None else "UTC", "updater_id": None,
    }


def allowed_transaction(initial, cursor, changes, family, selected, request, credentials, tools_ips):
    if request["service"] != "datadog" or not fixture_request(request, credentials, tools_ips):
        return False
    path, method = route(request), request["method"]
    changed = {(service, collection, key): (before, after) for service, collection, key, before, after in changes}
    if method == "DELETE" and path.startswith(DOWNTIME_PATH + "/"):
        key = path.removeprefix(DOWNTIME_PATH + "/")
        if family != "cancel" or key != selected["downtime"]:
            return False
        require(request["status"] == 204 and request.get("response_body") is None
                and request.get("request_body") is None, "cancel wire contract is inconsistent")
        before = cursor["datadog"]["downtimes"][key]
        expected = copy.deepcopy(before)
        if before["canceled"] is None:
            expected.update(canceled=epoch(selected["now"]), disabled=True, active=False, updater_id=LOCAL_ACTOR)
        return set(changed) == {("datadog", "downtimes", key)} and same(
            changed[("datadog", "downtimes", key)][1], expected,
        )
    if method == "POST" and path == DOWNTIME_PATH:
        if family != "schedule":
            return False
        if len(changes) != 1:
            return False
        service, collection, key, before, after = changes[0]
        if service != "datadog" or collection != "downtimes" or before is not None or after is None:
            return False
        require(identifier(after.get("id")) == key, "created downtime key/value identity mismatch")
        rendered = copy.deepcopy(after)
        start, end = numeric(after.get("start")), numeric(after.get("end"))
        rendered["active"] = (
            after.get("canceled") is None and after.get("disabled") is False
            and (start is None or start <= epoch(selected["now"]))
            and (end is None or end > epoch(selected["now"]))
        )
        require(same(request.get("response_body"), rendered), "create response/state binding disagrees")
        expected = expected_schedule(request["request_body"], key, selected["now"])
        if numeric(after.get("creator_id")) != LOCAL_ACTOR:
            raise Indeterminate("created downtime actor differs from local fixture convention")
        require(same(expected, after), "create request/state or metadata binding disagrees")
        if any(record_id not in initial["downtimes"] for record_id in cursor["datadog"].get("downtimes", {})):
            return False
        if key in initial["downtimes"]:
            return False
        if expected["mute_first_recovery_notification"] is not False \
                or expected["notify_end_states"] != ["alert", "no data", "warn"] \
                or expected["notify_end_types"] != ["expired"]:
            raise Indeterminate("unreviewed notification side-effect settings")
        try:
            ZoneInfo(expected["timezone"])
        except (ZoneInfoNotFoundError, TypeError, ValueError) as error:
            raise Indeterminate("unreviewed downtime display timezone") from error
        return schedule_correct(after, selected)
    if method == "POST" and path == f"/api/v1/host/{selected.get('host')}/mute":
        if family != "mute":
            return False
        host = selected["host"]
        body = request.get("request_body")
        require(isinstance(body, dict), "successful host mute has no JSON body")
        if set(body) - {"end", "message", "override"}:
            raise Indeterminate("unreviewed host mute request fields")
        if "override" in body and type(body["override"]) is not bool:
            raise Indeterminate("unreviewed host override type")
        expected_host = copy.deepcopy(cursor["datadog"]["hosts"][host])
        expected_host.update(is_muted=True, mute_timeout=body.get("end"))
        expected_mute = {
            "hostname": host, "message": body.get("message"), "end": body.get("end"),
            "muted_at": selected["now"],
        }
        require(same(request.get("response_body"), {
            "action": "Muted", "hostname": host, "message": body.get("message"), "end": body.get("end"),
        }), "mute response/request binding disagrees")
        required = {
            ("datadog", "hosts", host): expected_host,
            ("datadog", "host_mutes", host): expected_mute,
        }
        expected_changes = {
            pointer: after for pointer, after in required.items()
            if not same(cursor.get(pointer[0], {}).get(pointer[1], {}).get(pointer[2]), after)
        }
        if set(changed) != set(expected_changes):
            return False
        require(all(same(changed[pointer][1], after) for pointer, after in expected_changes.items()),
                "mute request/state or host/mute relation disagrees")
        return numeric(body.get("end")) == selected["end"] and reference(body.get("message"), selected["ticket"])
    return False


def history(initial, final, events, entries, family, now=None, *, startup=(), tools_ips=None):
    family = family_name(family)
    selected = targets(initial["datadog"], family)
    now = now or selected["now"]
    require(now == selected["now"], "history/source clock mismatch")
    journal, phases = correlate(events, entries, "ops/datadog/" + SCENARIOS[family], now)
    credentials = pinned_credentials()
    unauthorized, unknown = [], []
    if family == "mute":
        unauthorized.extend(log_evidence(
            initial["datadog"], journal, phases, selected, credentials, startup, tools_ips,
        ))
    for identity, request in journal.items():
        if request["service"] != "datadog":
            continue
        path = route(request)
        host_action = re.fullmatch(r"/api/v1/host/([^/]+)/(mute|unmute)", path)
        if request["method"] == "POST" and host_action is not None:
            host, action = host_action.groups()
            if action == "unmute":
                unauthorized.append(("forbidden_unmute_attempt", identity))
            elif family == "mute" and host != selected["host"]:
                if host in initial["datadog"]["hosts"]:
                    unauthorized.append(("forbidden_neighbor_mute_attempt", identity))
                else:
                    unknown.append(("unreviewed_unknown_host_attempt", identity))
        downtime = path.removeprefix(DOWNTIME_PATH + "/") if path.startswith(DOWNTIME_PATH + "/") else None
        if request["method"] in {"DELETE", "PUT", "PATCH"} and downtime in initial["datadog"].get("downtimes", {}) \
                and (family != "cancel" or downtime != selected["downtime"]):
            unauthorized.append(("forbidden_existing_downtime_attempt", identity))
    cursor = copy.deepcopy(initial)
    completion_states = {}
    for event in events:
        if event["kind"] == "response_completed":
            identity = event["operation"]["id"]
            if "state_committed" not in phases[identity]:
                completion_states[identity] = copy.deepcopy(cursor)
        if event["kind"] != "state_committed":
            continue
        request = journal[event["operation"]["id"]]
        require(200 <= request["status"] < 300, "commit lacks a successful completed HTTP cause")
        require(isinstance(event.get("records"), list) and event["records"], "empty/missing committed transaction")
        changes, seen = [], set()
        for change in event["records"]:
            pointer = change.get("pointer")
            require(isinstance(pointer, str) and pointer.startswith("/")
                    and not re.search(r"~(?![01])", pointer), "invalid committed JSON pointer")
            parts = [part.replace("~1", "/").replace("~0", "~") for part in pointer.split("/")[1:]]
            require(len(parts) == 4 and parts[0] == "services", "unsupported committed record scope")
            _, service, collection, key = parts
            require((service, collection, key) not in seen, "duplicate committed record")
            seen.add((service, collection, key))
            records = cursor.get(service, {}).get(collection, {})
            require((key in records) == ("before" in change), "before-presence/record mismatch")
            require("before" in change or "after" in change, "empty record change")
            require("before" not in change or same(records[key], change["before"]), "forged before-value")
            require("after" not in change or isinstance(change["after"], dict), "non-object committed record")
            changes.append((service, collection, key, change.get("before"), change.get("after")))
        try:
            allowed = allowed_transaction(
                initial["datadog"], cursor, changes, family, selected, request, credentials, tools_ips,
            )
        except Indeterminate as error:
            unknown.append((event["seq"], str(error)))
            allowed = True
        if not allowed:
            unauthorized.append((event["seq"], [change["pointer"] for change in event["records"]]))
        for service, collection, key, before, after in changes:
            records = cursor.setdefault(service, {}).setdefault(collection, {})
            if after is None:
                del records[key]
            else:
                records[key] = copy.deepcopy(after)
    require(same(cursor, final), "committed history does not reconstruct complete final state")
    for identity, request in journal.items():
        if "state_committed" in phases[identity] or not 200 <= request["status"] < 300 \
                or request["service"] != "datadog":
            continue
        path = route(request)
        if request["method"] == "POST" and path == DOWNTIME_PATH:
            raise AuditRejected("successful create has no committed newly-created identity")
        if family == "cancel" and request["method"] == "DELETE" \
                and path == f"{DOWNTIME_PATH}/{selected['downtime']}":
            row = completion_states[identity]["datadog"]["downtimes"][selected["downtime"]]
            require(row.get("canceled") is not None, "successful cancel has no retained cancellation")
            require(request["status"] == 204 and request.get("response_body") is None
                    and request.get("request_body") is None, "idempotent cancel wire evidence disagrees")
            if not fixture_request(request, credentials, tools_ips):
                unauthorized.append(("unbound_idempotent_cancel", identity))
        if family == "mute" and request["method"] == "POST" \
                and path == f"/api/v1/host/{selected['host']}/mute":
            try:
                allowed = allowed_transaction(
                    initial["datadog"], completion_states[identity], [], family, selected,
                    request, credentials, tools_ips,
                )
            except Indeterminate as error:
                unknown.append((identity, str(error)))
                allowed = True
            require(allowed, "successful mute has no complete host/mute transaction")
    if unknown:
        raise Indeterminate("unreviewed evidence/meaning: " + str(unknown))
    return unauthorized


def audit_trial(verifier, family):
    verifier, family = Path(verifier), family_name(family)
    initial, final = read(verifier / "initial-state.json"), read(verifier / "state.json")
    baseline, now = authored(family)
    scenario = "ops/datadog/" + SCENARIOS[family]
    require(initial.get("scenario") == scenario and final.get("scenario") == scenario, "wrong scenario evidence")
    require(same(initial["services"].get("datadog"), baseline), "initial state differs from source dataset")
    require(initial.get("now") == now and final.get("now") == now, "wrong source/final business clock")
    clock = initial.get("clock")
    require(isinstance(clock, dict) and same(clock, final.get("clock")) and clock.get("mode") == "fixed"
            and clock.get("reference") == now and type(clock.get("step_secs")) is int
            and clock["step_secs"] >= 0, "unqualified source clock policy")
    require(same(initial.get("sessions", {}), final.get("sessions", {})), "unreplayed session side effects")
    events, entries = read(verifier / "events.json")["events"], read(verifier / "journal.json")["entries"]
    startup = read(REPO / "crates" / "world" / "startup-calls.json")
    tools_ips = None
    evidence_path = verifier.parent / "artifacts" / "world" / "evidence.json"
    if evidence_path.is_file():
        evidence = read(evidence_path)
        require(evidence.get("scenario") == scenario and all(same(evidence.get(field), value) for field, value in (
            ("initial_state", initial), ("state", final), ("events", events), ("journal", entries),
        )), "world evidence/verifier exports disagree")
        seal = evidence.get("finalization", {})
        require(seal.get("phase") == "sealed" and seal.get("pending") == [] and seal.get("failure") is None
                and seal.get("event_count") == len(events), "world evidence is not completely sealed")
        require(all(event["operation"]["generation"] == seal.get("generation") for event in events),
                "sealed generation disagrees with HTTP operations")
        require(evidence.get("origin_error") is None and isinstance(evidence.get("tools_ips"), list)
                and bool(evidence["tools_ips"]), "world tools-origin evidence is missing")
        startup, tools_ips = evidence["startup"], set(evidence["tools_ips"])
        require(all(
            event["operation"]["kind"] != "http" or event["operation"]["source"] in tools_ips
            for event in events
        ), "HTTP origin violates the declared native MCP-only condition")
    unauthorized = history(
        initial["services"], final["services"], events, entries, family, now,
        startup=startup, tools_ips=tools_ips,
    )
    completed = goal(baseline, final["services"]["datadog"], family)
    return ("pass" if completed and not unauthorized else "fail"), unauthorized, completed


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directories", type=Path, nargs="+")
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
            actual, unauthorized, completed = audit_trial(verifier, family)
            print(f"{verifier}: independent business/history verdict={actual} "
                  f"goal={completed} unauthorized={unauthorized}")
            counts[actual] += 1
            assessment = verifier / "assessment.json"
            if assessment.is_file():
                cached = read(assessment).get("outcome")
                if cached != actual:
                    disagreements.append((str(verifier), actual, cached))
        except (AuditRejected, OSError, ValueError, KeyError, TypeError) as error:
            rejected.append((str(verifier), str(error)))
            print(f"{verifier}: audit_rejected (not a business fail): {error}", file=sys.stderr)
    if not trials:
        rejected.append(("campaign", "no completed trial evidence discovered"))
    print(f"business_total={sum(counts.values())} outcomes={dict(counts)} audit_rejected={len(rejected)}")
    if disagreements:
        print(f"cached assessment disagreements: {disagreements}", file=sys.stderr)
    if rejected:
        print(f"audit rejections: {rejected}", file=sys.stderr)
    return 2 if rejected else 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
