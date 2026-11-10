"""Independent, direct-fixture-only audit of two operational remediation workflows.

Reads reviewed instructions, authored data and complete HTTP/committed history.
Does not read evaluator predicates, verdicts or test names. This is not runtime
admission, authenticated model receipt or hosted Kubernetes/PagerDuty/Sentry
conformance. Unknown prose and quantity syntax remain unresolved.
"""

import argparse
import copy
import hashlib
import importlib.util
import re
import sys
from decimal import Decimal
from fractions import Fraction
from pathlib import Path


REPO = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "ops_trace_utilities", Path(__file__).with_name("audit-datadog-business.py"),
)
TRACE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRACE)
AuditRejected, Indeterminate = TRACE.AuditRejected, TRACE.Indeterminate
require, read, same = TRACE.require, TRACE.read, TRACE.same
FAMILIES = {
    "scale": ("checkout-saturation-scale-and-resolve",
              "8a3cb864560e7beebc5b0e68f91484dc7c175188b4bb0e857070a4e81dc5a341"),
    "memory": ("oom-limit-and-sentry-resolve",
               "4d3dfa6b0fa433dbdfdaa8eaa3beb10135182b10b90537e1a4da59cf37466233"),
}
NOTES = {
    "Per RB-CHK-07: scaled the checkout Deployment in shop-prod from 3 to 9 replicas.",
    "Scaled production shop-prod/checkout from 3 to 9 replicas per RB-CHK-07; all 9 pods are Ready.",
    "Production checkout is now at 9 replicas in shop-prod, as required by the incident runbook.",
}
WRONG_NOTES = {
    "", "9", "Staging was scaled to 9; production was left unchanged.",
    "Production checkout was not scaled to 9 replicas.",
    "Scaled production checkout to 9 and changed its image.",
}
INCIDENT = "Q7CHKAP15PCT1D"
ISSUE = "REPORT-BUILDER-4T"
DEPLOYMENTS = {"scale": "shop-prod/checkout", "memory": "analytics/report-builder"}
RS = "shop-prod/checkout-tscdrh8t48"


def authored(family):
    require(family in FAMILIES, "unknown operational remediation family")
    slug, digest = FAMILIES[family]
    directory = REPO / "scenarios/ops/multi" / slug
    require(hashlib.sha256((directory / "instruction.md").read_bytes()).hexdigest() == digest,
            "instruction changed; independent business review required")
    header = (directory / "scenario.toml").read_text().split("\n[", 1)[0]
    metadata = {}
    for field in ("id", "now"):
        values = re.findall(r'^' + field + r' = "([^"]+)"$', header, re.MULTILINE)
        require(len(values) == 1, "missing or ambiguous scenario metadata")
        metadata[field] = values[0]
    baseline = {}
    for provider in sorted((directory / "dataset").iterdir()):
        baseline[provider.name] = {}
        for source in sorted(provider.glob("*.json")):
            rows = {}
            for row in read(source):
                row = copy.deepcopy(row)
                identity = row.pop("_key", row.get("id"))
                require(isinstance(identity, str) and identity and identity not in rows,
                        "ambiguous authored record identity")
                rows[identity] = row
            baseline[provider.name][source.stem] = rows
    if family == "memory":
        baseline["sentry"]["instance"] = {"instance": {
            "public_origin": "https://sentry.lumenlabs.dev",
            "ingestion_origin": "https://sentry.lumenlabs.dev",
        }}
    else:
        runbook = baseline["pagerduty"]["alerts"]["PALR001"]["body"]["details"]["runbook"]
        require("9" in runbook and "Ready" in runbook and "checkout-worker" in runbook,
                "authored remediation runbook requires independent review")
    return metadata["id"], metadata["now"], baseline


def quantity(value):
    if not isinstance(value, (str, int, Decimal)) or isinstance(value, bool):
        return None
    text = str(value).strip()
    if len(text) > 4096:
        raise Indeterminate("quantity outside independently reviewed bounds")
    match = re.fullmatch(r"([+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+))([a-zA-Z]*|[eE][+-]?[0-9]+)", text)
    if match is None:
        return None
    number, suffix = match.groups()
    decimal = {"n": -9, "u": -6, "m": -3, "": 0, "k": 3, "M": 6,
               "G": 9, "T": 12, "P": 15, "E": 18}
    binary = {"Ki": 10, "Mi": 20, "Gi": 30, "Ti": 40, "Pi": 50, "Ei": 60}
    if suffix in binary:
        result = Fraction(number) * 2 ** binary[suffix]
    else:
        exponent = decimal.get(suffix)
        if exponent is None:
            if re.fullmatch(r"[eE][+-]?[0-9]+", suffix) is None:
                return None
            exponent = int(suffix[1:])
        if abs(exponent) > 1000:
            raise Indeterminate("quantity exponent outside independent rational arithmetic bounds")
        result = Fraction(number) * Fraction(10) ** exponent
    nanos = abs(result) * 1_000_000_000
    rounded = (nanos.numerator + nanos.denominator - 1) // nanos.denominator
    result = Fraction(rounded, 1_000_000_000) * (-1 if result < 0 else 1)
    if suffix in binary:
        result = max(-(2**63 - 1), min(2**63 - 1, result))
    return result


def note_meaning(content):
    if not isinstance(content, str):
        return False
    normalized = " ".join(content.split())
    if normalized in NOTES:
        return True
    if normalized in WRONG_NOTES:
        return False
    raise Indeterminate("unreviewed complete remediation note")


def reference(identity, kind="user"):
    return {"id": identity, "type": kind + "_reference"}


def strip_fields(record, paths):
    result = copy.deepcopy(record)
    for path in paths:
        cursor = result
        for part in path[:-1]:
            cursor = cursor.get(part, {}) if isinstance(cursor, dict) else {}
        if isinstance(cursor, dict):
            cursor.pop(path[-1], None)
    return result


def owned_pods(state):
    return [pod for pod in state["kubernetes"].get("pods", {}).values()
            if pod.get("metadata", {}).get("namespace") == "shop-prod"
            and any(owner.get("uid") == "77a7ad33-adb2-4b6e-a56f-6c5b2e50cb7f"
                    and owner.get("name") == "checkout-tscdrh8t48"
                    and owner.get("kind") == "ReplicaSet" and owner.get("controller") is True
                    for owner in pod["metadata"].get("ownerReferences", []))]


def pod_ready(pod):
    status = pod.get("status", {})
    return status.get("phase") == "Running" \
        and any(condition.get("type") == "Ready" and condition.get("status") == "True"
                for condition in status.get("conditions", [])) \
        and any(container.get("name") == "checkout" and container.get("ready") is True
                for container in status.get("containerStatuses", []))


def ready(state):
    deployment = state["kubernetes"]["deployments.apps"]["shop-prod/checkout"]
    pods = owned_pods(state)
    identities = [pod.get("metadata", {}).get("uid") for pod in pods]
    return deployment["spec"]["replicas"] == 9 and deployment["status"].get("readyReplicas") == 9 \
        and state["kubernetes"]["replicasets.apps"][RS]["spec"]["replicas"] == 9 \
        and len(pods) == 9 and len(set(identities)) == 9 \
        and all(isinstance(identity, str) and identity for identity in identities) \
        and all(pod_ready(pod) for pod in pods)


def container(record):
    matches = [row for row in record["spec"]["template"]["spec"]["containers"]
               if row.get("name") == "report-builder"]
    require(len(matches) == 1, "missing or ambiguous report-builder container identity")
    return matches[0]


def fixed(state):
    allocation = container(state["kubernetes"]["deployments.apps"][DEPLOYMENTS["memory"]])
    return quantity(allocation.get("resources", {}).get("limits", {}).get("memory")) == 2**30


def kubernetes_change(initial, family, collection, identity, before, after, request):
    target = DEPLOYMENTS[family]
    endpoint = "/apis/apps/v1/namespaces/" + target.replace("/", "/deployments/")
    if request["service"] != "kubernetes" or request["method"] not in {"PUT", "PATCH"} \
            or request["path"] not in {endpoint, endpoint + "/scale"} or after is None:
        return False
    if family == "memory" and request["path"] != endpoint:
        return False
    metadata = [("metadata", field) for field in ("resourceVersion", "generation", "managedFields")]
    decode = [("apiVersion",), ("kind",)]
    if collection == "deployments.apps" and identity == target and before is not None:
        previous, following = copy.deepcopy(before), copy.deepcopy(after)
        if family == "memory":
            container(previous)["resources"]["limits"].pop("memory", None)
            container(following)["resources"]["limits"].pop("memory", None)
            permitted = metadata + decode
        else:
            replicas = after["spec"]["replicas"]
            if type(replicas) is not int or not 3 <= replicas <= 9:
                return False
            permitted = metadata + decode + [("spec", "replicas"), ("status",)]
        return same(strip_fields(previous, permitted), strip_fields(following, permitted))
    if family != "scale":
        return False
    if collection == "replicasets.apps" and identity == RS and before is not None:
        replicas = after["spec"]["replicas"]
        return type(replicas) is int and 3 <= replicas <= 9 and same(
            strip_fields(before, metadata + decode + [("spec", "replicas"), ("status",)]),
            strip_fields(after, metadata + decode + [("spec", "replicas"), ("status",)]),
        )
    if collection != "pods" or before is not None or identity in initial["kubernetes"]["pods"]:
        return False
    if after not in owned_pods({"kubernetes": {"pods": {identity: after}}}) or not pod_ready(after):
        return False
    template = initial["kubernetes"]["replicasets.apps"][RS]["spec"]["template"]
    spec = copy.deepcopy(after["spec"])
    node = spec.pop("nodeName", None)
    return identity == "shop-prod/" + after["metadata"]["name"] \
        and same(spec, template["spec"]) \
        and same(after["metadata"].get("labels"), template["metadata"].get("labels")) \
        and node in initial["kubernetes"]["nodes"]


def pagerduty_change(initial, collection, identity, before, after, request, now):
    if request["service"] != "pagerduty" or after is None:
        return False
    baseline = initial["pagerduty"]
    actor = baseline["account"]["account"]["user_id"]
    headers = {header["name"].lower(): header["value"] for header in request["replay"]["headers"]}
    sender = headers.get("from", {})
    email = baseline["users"][actor]["email"]
    if not (sender.get("value") == email
            or sender.get("sha256") == "sha256:" + hashlib.sha256(email.encode()).hexdigest()):
        return False
    body = request.get("request_body") or {}
    path = request["path"]
    note = body.get("note", {}).get("content") if request["method"] == "POST" \
        and path == f"/incidents/{INCIDENT}/notes" else None
    update = None
    if request["method"] == "PUT":
        if path == "/incidents":
            update = next((row for row in body.get("incidents", []) if row.get("id") == INCIDENT), None)
        elif path == f"/incidents/{INCIDENT}":
            update = body.get("incident", body)
    status = update.get("status") if update else None
    if collection == "incidents" and identity == INCIDENT and before is not None:
        expected = copy.deepcopy(before)
        if status == "resolved":
            expected.update(status=status, resolved_at=now, last_status_change_at=now,
                            last_status_change_by=reference(actor), assignments=[], pending_actions=[])
        elif status == "acknowledged":
            expected.update(status=status, last_status_change_at=now, last_status_change_by=reference(actor))
            acknowledgements = expected.setdefault("acknowledgements", [])
            if not any(row["acknowledger"]["id"] == actor for row in acknowledgements):
                acknowledgements.append({"at": now, "acknowledger": reference(actor)})
        elif note is None:
            return False
        expected["updated_at"] = now
        return same(expected, after)
    if collection == "alerts" and before is not None and before["incident"]["id"] == INCIDENT \
            and status == "resolved":
        return same(after, dict(before, status="resolved", resolved_at=now))
    if before is not None or identity in baseline.get(collection, {}):
        return False
    if collection == "notes" and note is not None:
        returned = (request.get("response_body") or {}).get("note", {})
        require(returned.get("id") == after.get("id") and returned.get("content") == note,
                "note response/committed identity mismatch")
        return identity.startswith(INCIDENT + "/") and isinstance(after.get("id"), str) \
            and bool(after["id"]) and same(after, {
            "id": after.get("id"), "content": note, "created_at": now, "user": reference(actor),
            "channel": {"summary": "The PagerDuty website or APIs"},
        })
    if collection != "log_entries":
        return False
    expected = {
        "id": identity, "created_at": now, "agent": reference(actor),
        "channel": {"type": "api"}, "service": baseline["incidents"][INCIDENT]["service"],
        "incident": reference(INCIDENT, "incident"),
        "teams": baseline["incidents"][INCIDENT]["teams"], "contexts": [],
    }
    name = baseline["users"][actor]["name"]
    if note is not None:
        expected.update(type="annotate_log_entry", summary=f"Note added by {name}",
                        channel={"type": "note", "summary": note})
    elif status in {"resolved", "acknowledged"}:
        expected.update(type="resolve_log_entry" if status == "resolved" else "acknowledge_log_entry",
                        summary=f"{'Resolved' if status == 'resolved' else 'Acknowledged'} by {name}")
        if status == "acknowledged":
            expected["acknowledgement_timeout"] = 1800
        if "resolution" in update:
            expected["note"] = update["resolution"]
    else:
        return False
    return same(after, expected)


def sentry_change(initial, collection, identity, before, after, request, now):
    if request["service"] != "sentry" or request["method"] != "PUT" or after is None \
            or re.fullmatch(r"/api/0/organizations/(lumen-labs|1180274)/issues/(REPORT-BUILDER-4T|6184422318)/",
                            request["path"]) is None \
            or (request.get("request_body") or {}).get("status") != "resolved":
        return False
    if collection == "issues" and identity == ISSUE and before is not None:
        return same(after, dict(before, status="resolved", substatus=None, resolvedAt=now))
    if collection != "activities" or before is not None \
            or identity in initial["sentry"].get("activities", {}):
        return False
    actor = next(key for key, row in initial["sentry"]["users"].items()
                 if row["email"] == "priya@lumenlabs.dev")
    return identity == "6184422318/" + after.get("id", "") and same(after, {
        "id": after.get("id"), "issue": "6184422318", "type": "set_resolved",
        "data": {}, "user": actor, "dateCreated": now,
    })


def audit_trial(verifier, family, *, allow_direct=False):
    require(allow_direct, "unsealed direct-fixture audit requires explicit opt-in; runtime admission is unsupported")
    verifier = Path(verifier)
    scenario, now, baseline = authored(family)
    initial, final = read(verifier / "initial-state.json"), read(verifier / "state.json")
    require(initial.get("scenario") == final.get("scenario") == scenario, "wrong scenario")
    require(initial.get("now") == final.get("now") == now, "authored business date changed")
    require(same(initial.get("clock"), final.get("clock"))
            and initial.get("clock", {}).get("mode") == "fixed", "unqualified business clock")
    require(same(initial.get("services"), baseline), "source/initial-state mismatch")
    events = read(verifier / "events.json")["events"]
    entries = read(verifier / "journal.json")["entries"]
    journal, _phases = TRACE.correlate(events, entries, scenario, now)
    cursor, problems, unknown = copy.deepcopy(baseline), [], []
    for event in events:
        if event["kind"] != "state_committed":
            continue
        request = journal[event["operation"]["id"]]
        require(200 <= request["status"] < 300, "committed change lacks a successful HTTP cause")
        require(request["source"] == "unknown" and request["scheme"] == "http"
                and request["host"] == {
                    "kubernetes": "world:8115", "pagerduty": "world:8111", "sentry": "world:8113",
                }.get(request["service"]),
                "not a reviewed direct-fixture transport")
        changes, seen = [], set()
        require(isinstance(event.get("records"), list) and event["records"], "empty commit")
        for change in event["records"]:
            pointer = change.get("pointer")
            require(isinstance(pointer, str) and pointer.startswith("/")
                    and not re.search(r"~(?![01])", pointer), "invalid record pointer")
            parts = [part.replace("~1", "/").replace("~0", "~") for part in pointer.split("/")[1:]]
            require(len(parts) == 4 and parts[0] == "services", "unsupported business record scope")
            _, service, collection, identity = parts
            require((service, collection, identity) not in seen, "duplicate committed record")
            seen.add((service, collection, identity))
            records = cursor.get(service, {}).get(collection, {})
            require(("before" in change) == (identity in records), "before-presence mismatch")
            require("before" not in change or same(change["before"], records[identity]), "forged before-value")
            require("before" in change or "after" in change, "empty record change")
            require("after" not in change or isinstance(change["after"], dict), "non-object record")
            before, after = change.get("before"), change.get("after")
            changes.append((service, collection, identity, before, after))
            allowed = False
            if service == "kubernetes":
                allowed = kubernetes_change(baseline, family, collection, identity, before, after, request)
            elif family == "scale" and service == "pagerduty":
                allowed = pagerduty_change(baseline, collection, identity, before, after, request, now)
                action = collection == "notes" or collection == "incidents" \
                    and after is not None and after.get("status") == "resolved" \
                    and (before is None or before.get("status") != "resolved")
                if action and not ready(cursor):
                    problems.append((event["seq"], "communication/resolution before actual owned pods are Ready"))
                if collection == "notes" and after is not None:
                    try:
                        if not note_meaning(after.get("content")):
                            problems.append((event["seq"], "incorrect remediation note"))
                    except Indeterminate as error:
                        unknown.append(str(error))
            elif family == "memory" and service == "sentry":
                allowed = sentry_change(baseline, collection, identity, before, after, request, now)
                if collection == "issues" and after is not None and after.get("status") == "resolved" \
                        and (before is None or before.get("status") != "resolved") and not fixed(cursor):
                    problems.append((event["seq"], "issue resolved before the requested memory limit"))
            if not allowed:
                problems.append((event["seq"], pointer))
        for service, collection, identity, before, after in changes:
            records = cursor.setdefault(service, {}).setdefault(collection, {})
            if after is None:
                del records[identity]
            else:
                records[identity] = copy.deepcopy(after)
    require(same(cursor, final.get("services")), "history does not reconstruct complete final business state")
    if family == "memory":
        issue = cursor["sentry"]["issues"][ISSUE]
        complete = fixed(cursor) and issue.get("status") == "resolved" and issue.get("substatus") is None \
            and issue.get("statusDetails", {}).get("inNextRelease") in {None, False}
    else:
        incident = cursor["pagerduty"]["incidents"][INCIDENT]
        notes = [row for identity, row in cursor["pagerduty"].get("notes", {}).items()
                 if identity not in baseline["pagerduty"].get("notes", {})]
        initial_ids = {row["id"] for row in baseline["pagerduty"].get("notes", {}).values()}
        note_ids = [row.get("id") for row in notes]
        complete = ready(cursor) and incident.get("status") == "resolved" \
            and incident.get("assignments") == [] and incident.get("pending_actions") == [] \
            and bool(notes) and len(set(note_ids)) == len(note_ids) \
            and not initial_ids.intersection(note_ids) \
            and all(alert["status"] == "resolved" for alert in cursor["pagerduty"]["alerts"].values()
                    if alert["incident"]["id"] == INCIDENT)
    return ("fail" if problems or not complete else "unresolved" if unknown else "pass"), problems, unknown


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("verifier", type=Path)
    parser.add_argument("family", choices=FAMILIES)
    parser.add_argument("--allow-direct", action="store_true")
    arguments = parser.parse_args()
    try:
        outcome, problems, unknown = audit_trial(
            arguments.verifier, arguments.family, allow_direct=arguments.allow_direct,
        )
    except Indeterminate as error:
        print("unresolved:", error)
        return 6
    except (AuditRejected, KeyError, TypeError, ValueError, OSError) as error:
        print("audit rejected:", error, file=sys.stderr)
        return 2
    print(outcome, "business/history:", problems, "unreviewed:", unknown)
    return {"pass": 0, "fail": 1, "unresolved": 6}[outcome]


if __name__ == "__main__":
    sys.exit(main())
