"""Independent bounded Firecrawl source, business and committed-history audit.

Requirements come from reviewed user instructions and authored data, never grader
predicates or cached verdicts. Shared helpers handle only JSON, exact timestamps
and HTTP evidence correlation. Loopback direct fixtures require explicit opt-in;
runtime evidence needs a sealed producer export and the declared HTTPS origin.
Configuration/admission does not establish hosted delivery or model receipt.
"""

import argparse
import collections
import copy
import hashlib
import importlib.util
import ipaddress
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit
from uuid import UUID
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError


REPO = Path(__file__).resolve().parents[1]
ROOT = REPO / "scenarios" / "search" / "firecrawl"
SPEC = importlib.util.spec_from_file_location(
    "firecrawl_shared_trace_utilities", Path(__file__).with_name("audit-datadog-business.py"),
)
TRACE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRACE)
AuditRejected, Indeterminate = TRACE.AuditRejected, TRACE.Indeterminate
require, read, same, instant_ns, correlate = (
    TRACE.require, TRACE.read, TRACE.same, TRACE.instant_ns, TRACE.correlate,
)
SCENARIOS = {
    "crawl": "crawl-api-reference",
    "create": "monitor-competitor-pricing",
    "pause": "pause-broken-pricing-monitor",
}
INSTRUCTIONS = {
    "crawl": "4866644f707742325a78078afab4d4096024e65df4140fdfaa7c5c7203d2c69d",
    "create": "1437cceaf6f440c4f0896e697ad8ba1741aae7a717427c1d8039958642108234",
    "pause": "500d3dfcce3f584ae23644eb812dc4ffedf8799960fd38d31470fa8784e368f8",
}
APPROVED_CRITERIA = {
    "Alert when Fabrikam changes any plan price or plan limit.",
    "Notify when prices or plan limits change.",
    "Alert on any change to the pricing page.",
}
CONTRADICTORY_CRITERIA = {
    "Alert only about careers. Ignore prices and plan limits.",
    "Only alert about prices, never plan limits.",
}
CHECK_STATES = {
    "queued", "running", "completed", "partial", "failed",
    "skipped_no_credits", "skipped_overlap",
}


def family_name(value):
    require(isinstance(value, str), "family identity is not text")
    for family, slug in SCENARIOS.items():
        if value in {family, slug, "search/firecrawl/" + slug}:
            return family
    raise AuditRejected("unknown Firecrawl family: " + str(value))


def sha256(contents):
    return "sha256:" + hashlib.sha256(contents).hexdigest()

def canonical_digest(value):
    try:
        encoded = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False)
    except (TypeError, ValueError) as error:
        raise Indeterminate("canonical provenance number encoding needs qualification") from error
    return sha256(encoded.encode())

def remove_dot_segments(path):
    segments = []
    parts = path.split("/")[1:]
    for index, part in enumerate(parts):
        decoded = part.lower().replace("%2e", ".")
        if decoded in {".", ".."}:
            if decoded == ".." and segments:
                segments.pop()
            if index == len(parts) - 1:
                segments.append("")
        else:
            segments.append(part)
    return "/" + "/".join(segments)


def resource(value):
    if not isinstance(value, str):
        return None
    if "\\" in value:
        raise Indeterminate("backslash URL normalization needs qualification")
    try:
        value = value.strip("".join(chr(code) for code in range(33)))
        parsed = urlsplit(value)
        if parsed.scheme.lower() not in {"http", "https"} or not parsed.hostname \
                or parsed.username not in {None, ""} or parsed.password not in {None, ""}:
            return None
        host = parsed.hostname.encode("idna").decode("ascii").lower()
        if ":" in host:
            host = "[" + host + "]"
        port = parsed.port
        if port is not None and (parsed.scheme.lower(), port) not in {("http", 80), ("https", 443)}:
            host += ":" + str(port)
        result = urlunsplit((parsed.scheme.lower(), host, remove_dot_segments(parsed.path or "/"), parsed.query, ""))
        return result + ("?" if not parsed.query and "?" in value.split("#", 1)[0] else "")
    except (UnicodeError, ValueError):
        return None


def exact_count(value):
    number = TRACE.numeric(value)
    return number is not None and number >= 0 and number == number.to_integral_value()


def one(records, predicate):
    selected = [(identity, record) for identity, record in records.items() if predicate(record)]
    require(len(selected) == 1, "business identity is absent or ambiguous")
    return selected[0]


def cron_field(text, minimum, maximum):
    if not isinstance(text, str) or not re.fullmatch(r"[0-9*,/-]+", text, re.ASCII):
        raise Indeterminate("unreviewed cron field grammar")
    values = set()
    for entry in text.split(","):
        matched = re.fullmatch(r"(\*|[0-9]+(?:-[0-9]+)?)(?:/([0-9]+))?", entry, re.ASCII)
        if matched is None:
            raise Indeterminate("unreviewed cron entry")
        base, step = matched.groups()
        step = int(step or 1)
        require(0 < step <= 100, "invalid or unqualified cron step")
        if base == "*":
            first, last = minimum, maximum
        elif "-" in base:
            first, last = map(int, base.split("-"))
        else:
            first = last = int(base)
        require(minimum <= first <= last <= maximum, "invalid cron bounds")
        values.update(range(first, last + 1, step))
    return values


def daily(schedule):
    require(isinstance(schedule, dict), "missing normalized monitor schedule")
    if set(schedule) != {"cron", "timezone"}:
        raise Indeterminate("unreviewed normalized schedule fields")
    zone = schedule["timezone"]
    require(isinstance(zone, str) and zone, "missing schedule timezone")
    try:
        ZoneInfo("UTC" if zone.upper() == "UTC" else zone)
    except (ZoneInfoNotFoundError, ValueError) as error:
        raise Indeterminate("unqualified timezone") from error
    cron = schedule["cron"]
    require(isinstance(cron, str), "missing cron text")
    fields = cron.split()
    require(len(fields) == 5, "unqualified cron arity")
    minute, hour, day, month, weekday = [
        cron_field(field, minimum, maximum)
        for field, minimum, maximum in zip(fields, [0, 0, 1, 1, 0], [59, 23, 31, 12, 7])
    ]
    weekday = {value % 7 for value in weekday}
    return len(minute) == len(hour) == 1 and day == set(range(1, 32)) \
        and month == set(range(1, 13)) and weekday == set(range(7))


def initial_schedule(record):
    original = record.get("schedule")
    require(isinstance(original, dict), "authored monitor schedule is missing")
    if "cron" in original:
        require(set(original) <= {"cron", "timezone"}, "authored cron needs independent review")
        return {"cron": original["cron"], "timezone": original.get("timezone", "UTC")}
    text, zone = original.get("text"), original.get("timezone", "UTC")
    require(isinstance(text, str) and isinstance(zone, str), "authored schedule fields are invalid")
    normalized = text.strip().lower()
    if normalized in {"every 6 hours", "every 12 hours"}:
        period = 6 if normalized == "every 6 hours" else 12
        require(original.get("intervalMinutes") == period * 60, "authored interval changed")
        return {"cron": f"0 */{period} * * *", "timezone": zone}
    if normalized == "daily at 9am":
        return {"cron": "0 9 * * *", "timezone": zone}
    if normalized in {"daily", "every day"}:
        require(original.get("intervalMinutes") == 1440, "authored daily interval changed")
        try:
            reference = datetime.fromisoformat(record["nextRunAt"].replace("Z", "+00:00"))
            require(reference.tzinfo is not None, "authored next run lacks an offset")
            local = reference.astimezone(ZoneInfo(zone))
        except (KeyError, ValueError, ZoneInfoNotFoundError) as error:
            raise AuditRejected("authored legacy daily time needs review") from error
        return {"cron": f"{local.minute} {local.hour} * * *", "timezone": zone}
    raise Indeterminate("authored schedule normalization needs independent review")


def source_descriptor(directory, family):
    scenario_text = (directory / "scenario.toml").read_bytes()
    header = scenario_text.decode().split("\n[", 1)[0]
    identities = re.findall(r'^id = "([^"]+)"$', header, re.MULTILINE)
    clocks = re.findall(r'^now = "([^"]+)"$', header, re.MULTILINE)
    require(identities == ["search/firecrawl/" + SCENARIOS[family]] and len(clocks) == 1,
            "source scenario/clock is missing or ambiguous")
    require(hashlib.sha256((directory / "instruction.md").read_bytes()).hexdigest() == INSTRUCTIONS[family],
            "instruction changed and needs independent review")
    source = hashlib.sha256(b"tool-bench-dataset-source-v1\0")
    services = sorted(item for item in (directory / "dataset").iterdir() if item.is_dir())
    require([item.name for item in services] == ["firecrawl"], "unreviewed source service set")
    for service in services:
        members = [("directory", service, b"")]
        members.extend(("file", member, member.read_bytes()) for member in sorted(service.iterdir())
                       if member.suffix == ".json")
        for kind, member, contents in members:
            for segment in [kind.encode(), member.relative_to(directory).as_posix().encode(), contents]:
                source.update(len(segment).to_bytes(8, "big"))
                source.update(segment)
    return clocks[0], {
        "schema_version": 1, "scenario": identities[0],
        "scenario_sha256": sha256(scenario_text),
        "dataset_sha256": "sha256:" + source.hexdigest(),
    }


def authored(family, source_directory=None):
    family = family_name(family)
    directory = Path(source_directory) if source_directory is not None else ROOT / SCENARIOS[family]
    now, descriptor = source_descriptor(directory, family)
    baseline = {}
    for source in sorted((directory / "dataset/firecrawl").glob("*.json")):
        contents = read(source)
        require(isinstance(contents, list), "authored collection is not an array")
        records = {}
        for original in contents:
            require(isinstance(original, dict), "authored row is not an object")
            row = copy.deepcopy(original)
            identity = row.pop("_key", row.get("id"))
            require(isinstance(identity, str) and identity and identity not in records,
                    "authored identity is absent or duplicated")
            records[identity] = row
        baseline[source.stem] = records
    require(bool(baseline), "source data is missing")
    return baseline, now, descriptor

def expected_clock(family, source_directory=None):
    directory = Path(source_directory) if source_directory is not None else ROOT / SCENARIOS[family]
    header = (directory / "scenario.toml").read_text().split("\n[", 1)[0]
    steps = re.findall(r"^clock_step_secs = ([0-9]+)$", header, re.MULTILINE)
    modes = re.findall(r'^clock_mode = "([^"]+)"$', header, re.MULTILINE)
    require(len(steps) <= 1 and modes in ([], ["fixed"]), "unqualified source clock mode")
    require("clock_step_secs" not in header or len(steps) == 1, "invalid source clock step")
    return int(steps[0]) if steps else 7


def normalized_initial(actual, baseline):
    require(isinstance(actual, dict) and actual.keys() == baseline.keys(),
            "initial collection set differs from declared authored data")
    normalized = copy.deepcopy(baseline)
    require(all(isinstance(records, dict) for records in actual.values()), "invalid initial collections")
    used = set()
    for identity, record in normalized.get("monitors", {}).items():
        require(identity in actual["monitors"], "authored monitor missing from initial state")
        observed = actual["monitors"][identity]
        require(isinstance(observed, dict), "initial monitor is not an object")
        record["schedule"] = initial_schedule(record)
        record.setdefault("judgeEnabled", isinstance(record.get("goal"), str) and bool(record["goal"].strip()))
        record.setdefault("notification", {})
        require(isinstance(record.get("targets"), list) and isinstance(observed.get("targets"), list)
                and len(record["targets"]) == len(observed["targets"]), "initial target shape changed")
        for target, captured in zip(record["targets"], observed["targets"]):
            require(isinstance(target, dict) and isinstance(captured, dict), "invalid initial target")
            if "id" not in target:
                candidate = captured.get("id")
                try:
                    require(isinstance(candidate, str) and str(UUID(candidate)) == candidate,
                            "generated initial target identity is not a canonical opaque UUID")
                except (ValueError, AttributeError) as error:
                    raise AuditRejected("invalid generated target identity") from error
                target["id"] = candidate
            require(isinstance(target.get("id"), str) and target["id"] not in used,
                    "initial target identities are missing or duplicated")
            used.add(target["id"])
    require(same(actual, normalized), "initial Firecrawl state differs from source-reviewed normalization")
    return normalized


def check_initial_services(services, baseline, *, direct=False):
    require(isinstance(services, dict) and "firecrawl" in services, "missing initial Firecrawl state")
    normalized = normalized_initial(services["firecrawl"], baseline)
    if set(services) == {"firecrawl"}:
        require(direct, "runtime world is missing the reviewed unselected services")
        return normalized
    descriptor = read(REPO / "deploy/fixtures/firecrawl-world-bootstrap.json")
    require(descriptor.get("schema_version") == 1
            and descriptor.get("scope") == "reviewed_unselected_defaults_for_full_world_firecrawl_trials"
            and descriptor.get("base") == "google-world-bootstrap.json"
            and descriptor.get("remove") == ["firecrawl"] and descriptor.get("add") == {"google": {}},
            "unqualified full-world bootstrap descriptor")
    parent = REPO / "deploy/fixtures/google-world-bootstrap.json"
    require(sha256(parent.read_bytes()) == descriptor.get("base_sha256"),
            "reviewed default bootstrap changed")
    defaults = read(parent)["services"]
    defaults = {key: value for key, value in defaults.items() if key != "firecrawl"}
    require(same(services, {**defaults, "google": {}, "firecrawl": normalized}),
            "unreviewed full-world service defaults")
    return normalized


def check_initial_sessions(sessions, services, now):
    require(isinstance(sessions, dict) and sessions.keys() == services.keys(),
            "initial session namespace differs from initialized services")
    expected = {name: {} for name in services}
    if "mongodb" in services:
        descriptor = read(REPO / "deploy/fixtures/firecrawl-world-bootstrap.json")
        require(descriptor.get("client_session_timestamp_fields") == [
            "/mongodb/_meta/init/initializedAt",
            "/mongodb/connections/preconfigured/createdAt",
            "/mongodb/connections/preconfigured/lastUsedAt",
        ], "unreviewed session timestamp normalization")
        templates = descriptor.get("client_sessions")
        require(isinstance(templates, dict) and set(templates) == {"mongodb"},
                "unreviewed default session namespaces")
        require(instant_ns(now) % 1_000_000_000 == 0,
                "bootstrap session time precision is unqualified")
        stamp = datetime.fromisoformat(now.replace("Z", "+00:00")).astimezone(timezone.utc) \
            .isoformat(timespec="milliseconds").replace("+00:00", "Z")
        values = copy.deepcopy(templates)
        for pointer in descriptor["client_session_timestamp_fields"]:
            parts = pointer.split("/")[1:]
            parent = values
            for part in parts[:-1]:
                parent = parent[part]
            require(parent[parts[-1]] is None, "session timestamp template is not a placeholder")
            parent[parts[-1]] = stamp
        expected["mongodb"] = values["mongodb"]
    require(same(sessions, expected), "initial sessions differ from reviewed source initialization")


def targets(initial, family, now):
    if family != "pause":
        return {}
    monitors, checks = initial["monitors"], initial["monitor_checks"]
    def urls(record):
        return {resource(url) for target in record.get("targets", [])
                for url in target.get("urls", [])}
    lower = instant_ns(now) - 3 * 86400 * 1_000_000_000
    def failing(record):
        history = [row for row in checks.values() if row.get("monitorId") == record.get("id")
                   and lower <= instant_ns(row["createdAt"]) <= instant_ns(now)]
        return len(history) >= 3 and all(row.get("status") == "failed" for row in history)
    broken, _ = one(monitors, lambda record: (
        urls(record) == {"https://www.tailspintoys.com/pricing"} and failing(record)
    ))
    healthy, _ = one(monitors, lambda record: (
        urls(record) == {"https://www.tailspintoys.com/plans"}
        and record.get("status") == "active"
        and checks.get(record.get("lastCheckId"), {}).get("status") == "completed"
    ))
    require(broken != healthy, "broken and healthy monitor identities overlap")
    return {"broken": broken, "healthy": healthy}


def crawl_goal(initial, record):
    desired = {
        normalized for url in initial["pages"]
        if (normalized := resource(url)) is not None
        and urlsplit(normalized).scheme == "https" and urlsplit(normalized).netloc == "docs.lumenpay.dev"
        and (urlsplit(normalized).path.startswith("/api/") or urlsplit(normalized).path == "/api")
    }
    require(bool(desired), "authored API section is empty")
    seed = resource(record.get("url"))
    problems = []
    if seed is None or urlsplit(seed).hostname != "docs.lumenpay.dev":
        problems.append("crawl seed is outside the requested site")
    status = record.get("status")
    if status not in {"queued", "scraping", "completed"}:
        problems.append("crawl is not admitted or successfully completed")
    planned = record.get("plannedPages", record.get("pages") if status == "completed" else None)
    require(isinstance(planned, list) and isinstance(record.get("pages"), list),
            "crawl lacks auditable planned/collected scope")
    def canonical(values):
        result = [resource(value) for value in values]
        return set(result), len(result) == len(set(result)) and None not in result
    planned_set, planned_unique = canonical(planned)
    collected, collected_unique = canonical(record["pages"])
    if not planned_unique or planned_set != desired:
        problems.append("crawl planned scope is not exactly the requested API section")
    if not collected_unique or not collected <= desired or status == "completed" and collected != desired:
        problems.append("collected pages do not honor the requested scope/completion")
    if not exact_count(record.get("total")) or not exact_count(record.get("completed")) \
            or record["total"] != len(desired) or record["completed"] != len(collected):
        problems.append("crawl counters disagree with actual resource coverage")
    return problems, []


def monitor_goal(record, complete):
    problems, unknown = [], []
    if record.get("name") != "Fabrikam pricing":
        problems.append("monitor has the wrong requested name")
    if record.get("status") not in {"active", "paused"} or complete and record.get("status") != "active":
        problems.append("monitor is not in the requested active state")
    configured = record.get("targets")
    require(isinstance(configured, list), "missing monitor target list")
    if len(configured) != 1:
        problems.append("monitor has additional or missing targets")
    else:
        target = configured[0]
        require(isinstance(target, dict), "invalid target shape")
        if target.get("type") != "scrape":
            unknown.append("non-scrape monitoring needs independent qualification")
        else:
            require(isinstance(target.get("urls"), list), "invalid scrape target URLs")
            urls = [resource(value) for value in target["urls"]]
            if not urls or any(value != "https://www.fabrikam.io/pricing" for value in urls):
                problems.append("monitor does not target only the requested pricing page")
        if target.get("scrapeOptions") not in (None, {}):
            unknown.append("custom extraction coverage is unreviewed")
    if record.get("scrapeOptions") not in (None, {}) or record.get("changeTracking") not in (None, {}):
        unknown.append("custom change tracking coverage is unreviewed")
    if record.get("webhook") is not None:
        problems.append("unrequested webhook delivery")
    try:
        if not daily(record.get("schedule")) and (complete or record.get("status") == "active"):
            problems.append("monitor is not calendar-daily")
    except Indeterminate as error:
        unknown.append(str(error))
    notification = record.get("notification", {})
    require(isinstance(notification, dict), "invalid notification object")
    if set(notification) - {"email", "slack"}:
        unknown.append("unreviewed notification channel")
    slack = notification.get("slack")
    require(slack is None or isinstance(slack, dict), "invalid Slack notification object")
    if isinstance(slack, dict) and slack.get("enabled") is True:
        problems.append("unrequested Slack delivery")
    email = notification.get("email", {})
    require(isinstance(email, dict) and type(email.get("enabled", False)) is bool,
            "invalid email configuration")
    recipients = email.get("recipients", [])
    require(isinstance(recipients, list), "email recipients are not a list")
    if any(not isinstance(value, str) or not value.isascii()
           or value.lower() != "pricing-intel@contoso.example" for value in recipients):
        problems.append("unrequested email recipient")
    enabled = email.get("enabled", False)
    if complete and (not enabled or not recipients):
        problems.append("requested email notification is not enabled")
    judged = record.get("judgeEnabled", bool(record.get("goal")))
    require(type(judged) is bool, "invalid judging flag")
    if judged and (complete or enabled and record.get("status") == "active"):
        criterion = record.get("goal")
        if isinstance(criterion, str) and criterion in CONTRADICTORY_CRITERIA:
            problems.append("active criterion excludes requested price/limit coverage")
        elif not isinstance(criterion, str) or not criterion.strip():
            problems.append("active judging has no coverage criterion")
        elif criterion not in APPROVED_CRITERIA:
            unknown.append("active filter meaning requires independent adjudication")
    return problems, unknown


def goal(initial, final, family, selected):
    added = lambda name: {key: row for key, row in final.get(name, {}).items()
                          if key not in initial.get(name, {})}
    problems, unknown = [], []
    if family == "crawl":
        records = added("crawls")
        if len(records) != 1:
            return False, ["expected one newly admitted crawl"], []
        identity, record = next(iter(records.items()))
        require(record.get("id") == identity, "crawl key/identity mismatch")
        problems, unknown = crawl_goal(initial, record)
    elif family == "create":
        records, children = added("monitors"), added("monitor_checks")
        if len(records) != 1:
            return False, ["expected one newly configured monitor"], []
        identity, record = next(iter(records.items()))
        require(record.get("id") == identity, "monitor key/identity mismatch")
        problems, unknown = monitor_goal(record, True)
        if len(children) > 1:
            problems.append("more than one optional initial check")
        for child_id, child in children.items():
            if child.get("id") != child_id or child.get("monitorId") != identity \
                    or child.get("trigger") != "manual" or child.get("status") not in CHECK_STATES:
                problems.append("initial check is not the actual new monitor's manual child")
        if children:
            if record.get("lastCheckId") not in children:
                problems.append("monitor metadata does not reference the actual new child")
        elif record.get("lastCheckId") is not None or record.get("lastRunAt") is not None:
            problems.append("monitor run metadata has no actual new child")
    else:
        monitors, children = final.get("monitors", {}), added("monitor_checks")
        if monitors.get(selected["broken"], {}).get("status") != "paused" \
                or monitors.get(selected["healthy"], {}).get("status") != "active":
            problems.append("broken/healthy monitor status is incorrect")
        if len(children) != 1:
            problems.append("expected one immediate manual check")
        else:
            identity, child = next(iter(children.items()))
            if child.get("id") != identity or child.get("monitorId") != selected["healthy"] \
                    or child.get("trigger") != "manual" \
                    or child.get("status") not in {"queued", "running", "completed"}:
                problems.append("healthy monitor check was not correctly admitted")
            if monitors.get(selected["healthy"], {}).get("lastCheckId") != identity:
                problems.append("healthy monitor metadata points to a different check")
    return not problems and not unknown, problems, unknown


def credentials():
    text = (REPO / "catalog/services.toml").read_text()
    sections = [section for section in text.split("[[service]]")
                if re.search(r'^name = "firecrawl"$', section, re.MULTILINE)]
    require(len(sections) == 1 and sections[0].count("[[service.endpoint]]") == 1,
            "ambiguous Firecrawl catalog binding")
    section = sections[0]
    for name, expected in [
        ("mode", "real-tls"), ("package", "firecrawl-mcp"),
        ("version", "3.36.0"), ("FIRECRAWL_API_URL", "https://api.firecrawl.dev"),
    ]:
        require(re.findall(r"\b" + name + r' = "([^"\\\n]*)"', section) == [expected],
                "unreviewed Firecrawl connector pin/origin")
    values = re.findall(r'\bFIRECRAWL_API_KEY = "([^"\\\n]+)"', section)
    require(len(values) == 1, "ambiguous fixture credential")
    return "Bearer " + values[0]


def bound_request(request, direct, tools_ips):
    require(request["service"] == "firecrawl", "unqualified mutation service")
    expected = "Bearer local-firecrawl" if direct else credentials()
    if direct:
        require(request["scheme"] == "http" and request["host"] == "world:8102"
                and request["source"] == "unknown", "unqualified direct-fixture transport")
    else:
        require(request["scheme"] == "https"
                and request["host"] in {"api.firecrawl.dev", "api.firecrawl.dev:443"}
                and request["source"] in tools_ips, "mutation lacks its declared HTTPS/tools origin")
    captured = [header["value"] for header in request["replay"]["headers"]
                if header["name"].lower() == "authorization"]
    require(len(captured) == 1, "missing or ambiguous authorization evidence")
    value = captured[0]
    known = value.get("kind") == "public" and value.get("value") == expected \
        or value.get("kind") in {"redacted", "synthetic"} and value.get("sha256") == sha256(expected.encode())
    require(known, "credential does not match the explicit local fixture convention")


def changed_fields(before, after):
    if before is None or after is None:
        return None
    return {field for field in before.keys() | after.keys()
            if (field in before) != (field in after) or not same(before.get(field), after.get(field))}


def allowed_transaction(initial, cursor, following, changes, family, selected, request, now):
    method, path = request["method"], TRACE.route(request)
    problems, unknown = [], []
    created = lambda collection, identity: identity not in initial.get(collection, {})
    response = request.get("response_body")
    body = request.get("request_body")
    if request["service"] != "firecrawl" and any(change[0] == "firecrawl" for change in changes):
        raise Indeterminate("cross-service Firecrawl write causes need qualification")
    for service, collection, identity, before, after in changes:
        allowed = False
        if service != "firecrawl" or after is None:
            problems.append("unauthorized deletion or cross-service change")
            continue
        if collection not in {"crawls", "monitors", "monitor_checks"}:
            problems.append(f"unauthorized {collection}/{identity} change")
            continue
        require(after.get("id") == identity, "committed identity/key mismatch")
        fields = changed_fields(before, after)
        if family == "crawl" and collection == "crawls" and created(collection, identity):
            if before is None:
                allowed = method == "POST" and path == "/v2/crawl"
                require(isinstance(response, dict) and response.get("success") is True
                        and response.get("id") == identity, "crawl creation lacks its actual HTTP identity")
                require(isinstance(body, dict) and same(body.get("url"), after.get("url")),
                        "stored crawl seed is not request-bound")
            else:
                allowed = method == "GET" and path == "/v2/crawl/" + identity \
                    and fields <= {"status", "pages", "scrapeIds", "completed", "creditsUsed", "expiresAt"}
            issues, uncertain = crawl_goal(initial, after)
            problems.extend(issues)
            unknown.extend(uncertain)
        elif family == "create" and collection == "monitors" and created(collection, identity):
            if before is None:
                allowed = method == "POST" and path == "/v2/monitor"
                require(isinstance(response, dict) and response.get("success") is True
                        and same(response.get("data"), after), "monitor creation response/state mismatch")
                require(isinstance(body, dict) and body.get("name") == after.get("name"),
                        "new monitor name is not request-bound")
            elif method == "PATCH" and path == "/v2/monitor/" + identity:
                allowed = fields <= {
                    "notification", "goal", "judgeEnabled", "status", "schedule", "nextRunAt", "updatedAt",
                }
                require(isinstance(body, dict) and isinstance(response, dict)
                        and response.get("success") is True and same(response.get("data"), after),
                        "monitor update is not bound to its HTTP response")
            elif method == "POST" and path == "/v2/monitor/" + identity + "/run":
                allowed = fields <= {"lastCheckId", "lastRunAt"}
                child = following["firecrawl"].get("monitor_checks", {}).get(after.get("lastCheckId"), {})
                allowed = allowed and after.get("lastCheckId") not in initial.get("monitor_checks", {}) \
                    and child.get("monitorId") == identity and child.get("trigger") == "manual"
            issues, uncertain = monitor_goal(after, False)
            problems.extend(issues)
            unknown.extend(uncertain)
        elif collection == "monitor_checks" and family in {"create", "pause"} and created(collection, identity):
            parent = after.get("monitorId")
            eligible = family == "create" and parent not in initial.get("monitors", {}) \
                and parent in following["firecrawl"].get("monitors", {}) \
                or family == "pause" and parent == selected["healthy"]
            if eligible and before is None:
                allowed = method == "POST" and path == "/v2/monitor/" + parent + "/run" \
                    and after.get("trigger") == "manual" and after.get("status") == "queued"
                require(isinstance(response, dict) and response.get("success") is True
                        and response.get("data", {}).get("id") == identity
                        and response.get("data", {}).get("monitorId") == parent,
                        "manual check creation is not bound to its actual HTTP child")
                require(instant_ns(after.get("createdAt")) == instant_ns(now), "new check has the wrong time")
                if family == "pause" and cursor["firecrawl"]["monitors"][selected["broken"]]["status"] != "paused":
                    problems.append("healthy check started before the broken monitor was paused")
            elif eligible and before is not None:
                allowed = method == "GET" and path == f"/v2/monitor/{parent}/checks/{identity}" \
                    and fields <= {"status", "startedAt", "completedAt", "summary", "pages"}
        elif family == "pause" and collection == "monitors":
            if identity == selected["broken"] and before is not None:
                allowed = method == "PATCH" and path == "/v2/monitor/" + identity \
                    and fields <= {"status", "updatedAt"} and after.get("status") == "paused"
            elif identity == selected["healthy"] and before is not None:
                allowed = method == "POST" and path == "/v2/monitor/" + identity + "/run" \
                    and fields <= {"lastCheckId", "lastRunAt"}
                child = following["firecrawl"].get("monitor_checks", {}).get(after.get("lastCheckId"), {})
                allowed = allowed and child.get("monitorId") == identity and child.get("trigger") == "manual" \
                    and after.get("lastCheckId") not in initial.get("monitor_checks", {})
        if not allowed:
            problems.append(f"unauthorized {collection}/{identity} change")
    return problems, unknown


def history(initial, final, events, entries, family, selected, now, direct, tools_ips, startup=()):
    journal, phases = correlate(events, entries, "search/firecrawl/" + SCENARIOS[family], now)
    startup_operations = TRACE.startup_ids(journal, startup)
    cursor, problems, unknown = copy.deepcopy(initial), [], []
    for event in events:
        if event["kind"] != "state_committed":
            continue
        request = journal[event["operation"]["id"]]
        require(200 <= request["status"] < 300, "failed HTTP operation committed business state")
        require(event["operation"]["id"] not in startup_operations,
                "startup operation changed task business state")
        if request["service"] == "firecrawl":
            bound_request(request, direct, tools_ips)
        require(isinstance(event.get("records"), list) and event["records"], "missing committed transaction")
        following, changes, seen = copy.deepcopy(cursor), [], set()
        for change in event["records"]:
            pointer = change.get("pointer")
            require(isinstance(pointer, str) and pointer.startswith("/")
                    and not re.search(r"~(?![01])", pointer), "invalid committed pointer")
            parts = [part.replace("~1", "/").replace("~0", "~") for part in pointer.split("/")[1:]]
            require(len(parts) == 4 and parts[0] == "services", "unsupported committed record scope")
            _, service, collection, identity = parts
            require((service, collection, identity) not in seen, "duplicate committed record")
            seen.add((service, collection, identity))
            old = cursor.get(service, {}).get(collection, {})
            require(isinstance(old, dict) and (identity in old) == ("before" in change),
                    "before-presence mismatch")
            require("before" in change or "after" in change, "empty committed record")
            require("before" not in change or same(old[identity], change["before"]), "forged before-value")
            require("after" not in change or isinstance(change["after"], dict), "non-object committed record")
            records = following.setdefault(service, {}).setdefault(collection, {})
            if "after" in change:
                records[identity] = copy.deepcopy(change["after"])
            else:
                del records[identity]
            changes.append((service, collection, identity, change.get("before"), change.get("after")))
        issues, uncertain = allowed_transaction(
            initial["firecrawl"], cursor, following, changes, family, selected, request, now,
        )
        problems.extend((event["seq"], issue) for issue in issues)
        unknown.extend((event["seq"], issue) for issue in uncertain)
        cursor = following
    require(same(cursor, final), "complete committed history does not reconstruct final services")
    return problems, unknown, journal, phases


def session_changes(initial, final, journal, now, direct, tools_ips):
    require(isinstance(initial, dict) and isinstance(final, dict), "missing client-session snapshots")
    expected = copy.deepcopy(initial)
    observed = {"scrapes": {}, "searches": {}}
    for request in journal.values():
        if request["service"] != "firecrawl" or not 200 <= request["status"] < 300:
            continue
        method, path = request["method"], TRACE.route(request)
        if method != "POST" or path not in {"/v2/scrape", "/v2/search"}:
            continue
        bound_request(request, direct, tools_ips)
        body, response = request.get("request_body"), request.get("response_body")
        require(isinstance(body, dict) and isinstance(response, dict) and response.get("success") is True,
                "session-producing read has no successful native API payload")
        if path == "/v2/scrape":
            metadata = response.get("data", {}).get("metadata", {})
            identity = metadata.get("scrapeId")
            require(isinstance(identity, str) and identity and resource(body.get("url")) is not None
                    and resource(metadata.get("sourceURL")) == resource(body["url"]),
                    "scrape session lacks its requested/returned identity")
            observed["scrapes"][identity] = {
                "id": identity, "url": metadata["url"], "createdAt": now,
            }
        else:
            identity = response.get("id")
            require(isinstance(identity, str) and identity and isinstance(response.get("data"), dict),
                    "search session lacks its returned identity")
            require(all(isinstance(rows, list) and all(isinstance(row, dict) for row in rows)
                        for rows in response["data"].values()), "invalid search result shape")
            urls = [row.get("url") for rows in response["data"].values()
                    for row in rows]
            observed["searches"][identity] = {
                "id": identity, "query": body.get("query"), "createdAt": now, "resultUrls": urls,
            }
    firecrawl = {}
    for name, rows in observed.items():
        if rows:
            firecrawl[name] = rows
    if firecrawl:
        expected["firecrawl"] = firecrawl
    require(same(initial, final) if not firecrawl else same(expected, final),
            "client sessions contain unqualified or unbound changes")


def _audit_trial(verifier, family, *, direct=False, source_directory=None):
    verifier, family = Path(verifier), family_name(family)
    initial, final = read(verifier / "initial-state.json"), read(verifier / "state.json")
    require(isinstance(initial, dict) and isinstance(final, dict), "invalid state snapshot objects")
    baseline, now, source = authored(family, source_directory)
    scenario = "search/firecrawl/" + SCENARIOS[family]
    require(initial.get("scenario") == final.get("scenario") == scenario, "scenario mismatch")
    normalized = check_initial_services(initial.get("services"), baseline, direct=direct)
    require(initial.get("now") == final.get("now") == now, "source/business clock mismatch")
    require(isinstance(initial.get("clock"), dict) and same(initial["clock"], final.get("clock"))
            and initial["clock"].get("mode") == "fixed" and initial["clock"].get("reference") == now
            and type(initial["clock"].get("step_secs")) is int
            and initial["clock"]["step_secs"] == expected_clock(family, source_directory),
            "unqualified logical clock policy")
    events, entries = read(verifier / "events.json")["events"], read(verifier / "journal.json")["entries"]
    check_initial_sessions(initial.get("sessions"), initial["services"], now)
    tools_ips, startup = None, []
    evidence_file = verifier.parent / "artifacts/world/evidence.json"
    direct_binding = verifier / "direct-source.json"
    if direct:
        require(source_directory is None or direct_binding.is_file(),
                "direct source variants require their producer source binding")
        if direct_binding.is_file():
            declared = read(direct_binding)
            require(isinstance(declared, dict) and declared.get("schema_version") == 1
                    and declared.get("scope") == "source_bound_direct_firecrawl_fixture_not_runtime",
                    "unqualified direct source binding")
            require(isinstance(declared.get("source"), dict)
                    and all(declared["source"].get(field) == value for field, value in source.items()),
                    "direct producer source differs from declared variant files")
            require(declared.get("initial_state_sha256") == canonical_digest(initial),
                    "direct producer initial-state digest mismatch")
    if not direct:
        require(evidence_file.is_file(), "runtime audit requires sealed authoritative world evidence")
    if evidence_file.is_file():
        evidence = read(evidence_file)
        require(isinstance(evidence, dict) and evidence.get("schema_version") == 5
                and evidence.get("scenario") == scenario,
                "unqualified world evidence schema/scenario")
        require(all(same(evidence.get(field), value) for field, value in (
            ("initial_state", initial), ("state", final), ("events", events), ("journal", entries),
        )), "world evidence and verifier exports disagree")
        require(isinstance(evidence.get("source"), dict)
                and all(evidence["source"].get(field) == value for field, value in source.items()),
                "producer dataset provenance differs from declared source files")
        require(evidence.get("initial_state_sha256") == canonical_digest(initial),
                "producer initial-state digest mismatch")
        contract_identity = evidence["source"].get("contract_sha256")
        require(isinstance(contract_identity, str) and re.fullmatch(r"sha256:[0-9a-f]{64}", contract_identity),
                "producer contract-source identity is malformed")
        seal = evidence.get("finalization", {})
        require(isinstance(seal, dict) and seal.get("phase") == "sealed" and seal.get("pending") == []
                and seal.get("failure") is None and seal.get("event_count") == len(events),
                "world evidence is not completely sealed")
        require(all(event["operation"]["generation"] == seal.get("generation") for event in events),
                "producer generation mismatch")
        tools_ips, startup = evidence.get("tools_ips"), evidence.get("startup", [])
        require(evidence.get("origin_error") is None and isinstance(tools_ips, list) and tools_ips,
                "missing trusted tools-origin evidence")
        try:
            require(all(isinstance(value, str) and ipaddress.ip_address(value) for value in tools_ips),
                    "invalid tools-origin address")
        except ValueError as error:
            raise AuditRejected("invalid tools-origin address") from error
        require(all(event["operation"]["source"] in tools_ips for event in events), "wrong HTTP caller origin")
    selected = targets(normalized, family, now)
    unauthorized, uncertainty, journal, _ = history(
        initial["services"], final["services"], events, entries,
        family, selected, now, direct, tools_ips, startup,
    )
    session_changes(initial.get("sessions"), final.get("sessions"), journal, now, direct, tools_ips)
    complete, problems, unknown = goal(normalized, final["services"].get("firecrawl", {}), family, selected)
    unauthorized.extend(problems)
    uncertainty.extend(unknown)
    actual = "fail" if unauthorized else "unresolved" if uncertainty else "pass" if complete else "fail"
    return actual, unauthorized + [("needs_review", item) for item in uncertainty], complete


def audit_trial(verifier, family, *, direct=False, source_directory=None):
    try:
        return _audit_trial(verifier, family, direct=direct, source_directory=source_directory)
    except AuditRejected:
        raise
    except (OSError, ValueError, KeyError, TypeError, IndexError, AttributeError) as error:
        raise AuditRejected("malformed or unqualified evidence: " + str(error)) from error


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--direct", action="store_true", help="explicitly audit unsealed direct source fixtures")
    parser.add_argument("--source-directory", type=Path, help="host-declared source variant, not an initial-state bypass")
    parser.add_argument("directories", nargs="+", type=Path)
    options = parser.parse_args(argv)
    trials, rejected, disagreements, counts = set(), [], [], collections.Counter()
    for directory in options.directories:
        if (directory / "initial-state.json").is_file():
            trials.add(directory.resolve())
        elif directory.is_dir():
            trials.update(path.parent.resolve() for path in directory.rglob("initial-state.json"))
        else:
            rejected.append((str(directory), "not an evidence directory"))
    for verifier in sorted(trials):
        try:
            captured = read(verifier / "initial-state.json")
            require(isinstance(captured, dict), "initial snapshot is not an object")
            family = family_name(captured.get("scenario"))
            actual, issues, complete = audit_trial(
                verifier, family, direct=options.direct, source_directory=options.source_directory,
            )
            print(f"{verifier}: independent business/history={actual} goal={complete} issues={issues}")
            counts[actual] += 1
            if (verifier / "assessment.json").is_file() \
                    and read(verifier / "assessment.json").get("outcome") != actual:
                disagreements.append(str(verifier))
        except (AuditRejected, OSError, ValueError, KeyError, TypeError, IndexError) as error:
            rejected.append((str(verifier), str(error)))
            print(f"{verifier}: audit_rejected (not model failure): {error}", file=sys.stderr)
    if not trials:
        rejected.append(("campaign", "no trial evidence"))
    print(f"business_total={sum(counts.values())} outcomes={dict(counts)} audit_rejected={len(rejected)}")
    if rejected or disagreements:
        print(f"rejected={rejected} disagreements={disagreements}", file=sys.stderr)
    return 2 if rejected else 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
