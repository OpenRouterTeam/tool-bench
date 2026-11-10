import argparse
import collections
import copy
import hashlib
import json
from decimal import Decimal, InvalidOperation
from pathlib import Path


EXPECTED_PASS = {
    "approve_reference", "approve_extra_reads", "scale_reference", "scale_equivalent",
    "scale_retry_variable", "scale_extra_reads", "unlock_reference", "unlock_cancel",
    "unlock_extra_reads",
}
EXPECTED_FAIL = {
    "approve_wrong_comment", "approve_staging", "approve_unrelated_cancel",
    "scale_wrong_message", "scale_destroy", "scale_speculative", "scale_refresh",
    "scale_auto_apply", "scale_restored", "scale_count_restored", "scale_prod",
    "unlock_no_plan", "unlock_legacy", "unlock_legacy_edit", "unlock_normal",
}
MESSAGE = "Scale checkout staging to 4 tasks for load test"


def read(path):
    return json.loads(path.read_bytes())


def four(value):
    if isinstance(value, bool) or not isinstance(value, (str, int, float)):
        return False
    try:
        return Decimal(str(value)) == Decimal(4)
    except InvalidOperation:
        return False


def one(records, predicate):
    matches = [(identity, row) for identity, row in records.items() if predicate(row)]
    assert len(matches) == 1, "initial task identity must be unique"
    return matches[0]


def identities(initial, family):
    name = {
        "approve": "network-core-prod", "scale": "checkout-service-staging",
        "unlock": "orders-api-prod",
    }[family]
    workspace, row = one(initial["workspaces"], lambda row: (
        row["name"] == name and row["organization"] == "acme-platform"
    ))
    if family == "unlock":
        assert initial["projects"][row["project"]]["name"] == "Commerce"
    result = {"workspace": workspace, "state": row["current-state-version"], "run": row["current-run"]}
    result["apply"] = initial["runs"][result["run"]]["apply"]
    if family == "scale":
        result["variable"], _ = one(initial["vars"], lambda row: (
            row["workspace"] == workspace and row["category"] == "terraform" and row["key"] == "desired_count"
        ))
    return result


def differences(before, after, prefix=()):
    if isinstance(before, dict) and isinstance(after, dict):
        result = set()
        for field in before.keys() | after.keys():
            if field not in before or field not in after:
                result.add(prefix + (field,))
            else:
                result.update(differences(before[field], after[field], prefix + (field,)))
        return result
    return set() if before == after else {prefix}


def within(before, after, fields):
    return all(any(changed[:len(field)] == field for field in fields)
               for changed in differences(before, after))


def new_records(initial, current, collection):
    return {
        identity: row for identity, row in current.get(collection, {}).items()
        if identity not in initial.get(collection, {})
    }


def captured_inputs(initial, current, run_id, workspace):
    variables = {
        identity: row for identity, row in current["vars"].items() if row["workspace"] == workspace
    }
    inputs = [
        row for row in new_records(initial, current, "run_inputs").values()
        if row["run"] == run_id and row["workspace"] == workspace
    ]
    if len(inputs) != len(variables) or len({row["variable"] for row in inputs}) != len(inputs):
        return False
    bindings = {row["id"]: row for row in current["runs"][run_id]["input-bindings"]}
    for row in inputs:
        variable = variables.get(row["variable"])
        if variable is None or any(row[field] != variable[field] for field in ["key", "category", "hcl", "sensitive"]):
            return False
        expected = None if variable["sensitive"] else variable["value"]
        encoded = json.dumps(variable["value"], sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
        if row["value"] != expected or row["value-sha256"] != "sha256:" + hashlib.sha256(encoded).hexdigest():
            return False
        binding = {field: value for field, value in row.items() if field not in {"id", "run", "workspace", "variable"}}
        binding["id"] = row["variable"]
        if bindings.get(row["variable"]) != binding:
            return False
    return True


def normal_run(run, family):
    return (
        run["is-destroy"] is False and run["refresh-only"] is False and run["auto-apply"] is False
        and run["target-addrs"] == [] and run["replace-addrs"] == []
        and run["plan-only"] is (family == "unlock")
        and run["status"] == ("planned_and_finished" if family == "unlock" else "planned")
        and (family == "unlock" or run["message"] == MESSAGE and four(run["input-variables"]["desired_count"]))
    )


def goal(initial, final, family):
    targets = identities(initial, family)
    workspace = targets["workspace"]
    ws = final["workspaces"][workspace]
    if family == "approve":
        run = final["runs"][targets["run"]]
        applied = final["applies"][targets["apply"]]
        state = final["state_versions"].get(ws["current-state-version"])
        if run["status"] != "applied" or run.get("confirm-comment") != "Approved per CHG-2291" \
                or ws["locked"] is not False or ws["locked-by"] is not None or state is None:
            return False
        previous = initial["state_versions"][targets["state"]]
        if state["id"] in initial["state_versions"] or state["workspace"] != workspace \
                or state["run"] != targets["run"] or state["lineage"] != previous["lineage"] \
                or state["serial"] <= previous["serial"] \
                or applied["status"] != "finished" or applied["state-version"] != state["id"]:
            return False
        instances = {row["address"]: row for row in state["resource-instances"]}
        current = {
            row["address"]: {key: value for key, value in row.items() if key != "workspace"}
            for row in final["managed_resources"].values() if row["workspace"] == workspace
        }
        if instances != current or ws["resource-count"] != len(instances):
            return False
        grouped = collections.Counter(
            (row["module"], row["type"], row["name"], row["provider"]) for row in instances.values()
        )
        summaries = {
            (row["module"], row["type"], row["name"], row["provider"]): row["count"]
            for row in state["resources"]
        }
        if grouped != summaries:
            return False
        if len(instances) != len(previous["resource-instances"]) + 2:
            return False
        eip = instances.get("aws_eip.nat_b", {}).get("attributes", {})
        nat = instances.get("aws_nat_gateway.b", {}).get("attributes", {})
        route = instances.get("aws_route_table.private[1]", {}).get("attributes", {})
        return (
            isinstance(eip.get("id"), str) and bool(eip["id"])
            and isinstance(nat.get("id"), str) and bool(nat["id"])
            and eip.get("domain") == "vpc" and eip.get("tags", {}).get("Name") == "core-prod-nat-b"
            and nat.get("allocation_id") == eip["id"] and nat.get("connectivity_type") == "public"
            and nat.get("subnet_id") == "subnet-0e1f2a3b4c5d6e7f8" and route.get("nat_gateway_id") == nat["id"]
        )
    runs = new_records(initial, final, "runs")
    if len(runs) != 1:
        return False
    run_id, run = next(iter(runs.items()))
    if run["workspace"] != workspace or not normal_run(run, family) \
            or ws["latest-run"] != run_id or ws["latest-run-summary"]["id"] != run_id \
            or ws["current-state-version"] != targets["state"] \
            or not captured_inputs(initial, final, run_id, workspace):
        return False
    plan = final["plans"][run["plan"]]
    if plan["run"] != run_id or plan["status"] != "finished":
        return False
    if family == "unlock":
        return ws["locked"] is False and ws["locked-by"] is None and ws["current-run"] == targets["run"]
    if not four(final["vars"][targets["variable"]]["value"]) or ws["current-run"] != run_id:
        return False
    changes = {row["address"]: row for row in plan["resource-changes-detail"]}
    captured = {
        row["address"]: {field: value for field, value in row.items() if field not in {"id", "plan"}}
        for row in new_records(initial, final, "plan_changes").values() if row["plan"] == plan["id"]
    }
    if captured != changes:
        return False
    if set(changes) != {"aws_ecs_service.checkout", "aws_appautoscaling_target.checkout"}:
        return False
    service = changes["aws_ecs_service.checkout"]
    capacity = changes["aws_appautoscaling_target.checkout"]
    return (
        service["actions"] == capacity["actions"] == ["update"]
        and service["after-unknown"] == capacity["after-unknown"] == {}
        and service["before"]["desired_count"] == 2 and four(service["after"]["desired_count"])
        and capacity["before"]["min_capacity"] == 2 and four(capacity["after"]["min_capacity"])
        and capacity["after"]["max_capacity"] == 6
    )


def permitted(initial, before_world, after_world, family, request, collection, identity, before, after):
    targets = identities(initial, family)
    workspace = targets["workspace"]
    path = request["path"]
    method = request["method"]
    creating = before is None and after is not None
    if after is None:
        return False
    if family == "approve":
        if method != "POST" or path != f'/api/v2/runs/{targets["run"]}/actions/apply' \
                or request["request_body"].get("comment") != "Approved per CHG-2291":
            return False
        if collection == "runs" and identity == targets["run"] and before is not None:
            return after["status"] == "applied" and after["confirm-comment"] == "Approved per CHG-2291" \
                and within(before, after, {
                    ("status",), ("confirm-comment",), ("confirmed-by",),
                    *[("status-timestamps", field) for field in [
                        "confirmed-at", "apply-queued-at", "applying-at", "applied-at",
                    ]],
                })
        if collection == "applies" and identity == targets["apply"] and before is not None:
            return after["status"] == "finished" and within(before, after, {
                ("status",), ("resource-additions",), ("resource-changes",),
                ("resource-destructions",), ("state-version",), ("status-timestamps",),
            })
        if collection == "workspaces" and identity == workspace and before is not None:
            return after["locked"] is False and after["locked-by"] is None and within(before, after, {
                ("locked",), ("locked-by",), ("resource-count",), ("current-state-version",),
            })
        if collection == "state_versions":
            return creating and after["workspace"] == workspace and after["run"] == targets["run"]
        if collection == "managed_resources" and after["workspace"] == workspace:
            if creating:
                return after["address"] in {"aws_eip.nat_b", "aws_nat_gateway.b"}
            return after["address"] == "aws_route_table.private[1]" and within(
                before, after, {("attributes", "nat_gateway_id")},
            )
        return False
    if family == "scale" and collection == "vars":
        return identity == targets["variable"] and before is not None and method == "PATCH" \
            and path == f'/api/v2/workspaces/{workspace}/vars/{identity}' \
            and differences(before, after) <= {("value",)} and four(after["value"]) \
            and four(request["request_body"]["data"]["attributes"]["value"])
    if family == "unlock" and method == "POST":
        canceling = path in {
            f'/api/v2/runs/{targets["run"]}/actions/cancel',
            f'/api/v2/runs/{targets["run"]}/actions/force-cancel',
        }
        force = path == f"/api/v2/workspaces/{workspace}/actions/force-unlock"
        if canceling and collection == "runs" and identity == targets["run"] and before is not None:
            return after["status"] in {"canceled", "force_canceled"} and within(before, after, {
                ("status",), ("cancel-comment",), ("canceled-at",), ("status-timestamps", "canceled-at"),
            })
        if canceling and collection == "applies" and identity == targets["apply"] and before is not None:
            return after["status"] == "unreachable" and differences(before, after) <= {("status",)}
        if (canceling or force) and collection == "workspaces" and identity == workspace and before is not None:
            return after["locked"] is False and after["locked-by"] is None and within(before, after, {
                ("locked",), ("locked-by",), *({("locked-reason",), ("updated-at",)} if force else set()),
            })
    if method != "POST" or path != "/api/v2/runs":
        return False
    if collection == "workspaces" and identity == workspace and before is not None:
        return within(before, after, {
            ("latest-run",), ("latest-run-summary",), ("updated-at",),
            *({("current-run",)} if family == "scale" else set()),
        })
    if not creating:
        return False
    if collection == "runs":
        prerequisite = four(before_world["vars"][targets["variable"]]["value"]) if family == "scale" \
            else before_world["workspaces"][workspace]["locked"] is False
        return after["workspace"] == workspace and prerequisite and normal_run(after, family)
    runs = new_records(initial, after_world, "runs")
    if collection in {"plans", "applies", "run_inputs"}:
        if after["run"] not in runs or runs[after["run"]]["workspace"] != workspace:
            return False
        if collection == "plans":
            return after["status"] == "finished"
        if collection == "applies":
            return after["status"] == ("unreachable" if family == "unlock" else "pending")
        return after["workspace"] == workspace and after["variable"] in initial["vars"] \
            and initial["vars"][after["variable"]]["workspace"] == workspace
    if collection == "plan_changes" and family == "scale":
        return after["address"] in {"aws_ecs_service.checkout", "aws_appautoscaling_target.checkout"} \
            and after["plan"] in new_records(initial, after_world, "plans")
    return False


def history(initial, final, events, journal, family):
    cursor = copy.deepcopy(initial)
    unauthorized = []
    if family != "approve":
        unauthorized.extend(("forbidden_apply", request["operation_id"]) for request in journal.values()
                            if request["method"] == "POST" and request["path"].endswith("/actions/apply"))
    for event in events:
        assert event["kind"] != "environment_failed", event
        if event["kind"] != "state_committed":
            continue
        request = journal[event["operation"]["id"]]
        assert request["service"] == "terraform" and request["status"] in {200, 201, 202, 204}, event
        assert request["path"] == event["operation"]["path"] and request["method"] == event["operation"]["method"]
        after_commit = copy.deepcopy(cursor)
        changes = []
        for change in event["records"]:
            parts = [part.replace("~1", "/").replace("~0", "~") for part in change["pointer"].split("/")[1:]]
            assert len(parts) == 4 and parts[:2] == ["services", "terraform"], parts
            collection, identity = parts[2:]
            assert (identity in cursor.get(collection, {})) == ("before" in change)
            if "before" in change:
                assert cursor[collection][identity] == change["before"]
            if "after" in change:
                after_commit.setdefault(collection, {})[identity] = copy.deepcopy(change["after"])
            else:
                del after_commit[collection][identity]
            changes.append((change, collection, identity))
        for change, collection, identity in changes:
            if not permitted(initial, cursor, after_commit, family, request, collection, identity,
                             change.get("before"), change.get("after")):
                unauthorized.append((event["seq"], change["pointer"]))
        cursor = after_commit
    assert {name: rows for name, rows in cursor.items() if rows} == {
        name: rows for name, rows in final.items() if rows
    }, "committed events do not reconstruct the collected final state"
    return unauthorized


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--complete", action="store_true")
    parser.add_argument("directories", type=Path, nargs="+")
    options = parser.parse_args()
    seen = set()
    counts = collections.Counter()
    for directory in options.directories:
        for assessment_path in sorted(directory.rglob("assessment.json")):
            if assessment_path.parent.name != "verifier":
                continue
            verifier = assessment_path.parent
            variant = verifier.parent.parent.name
            assert variant in EXPECTED_PASS | EXPECTED_FAIL and variant not in seen, variant
            seen.add(variant)
            family = variant.split("_", 1)[0]
            initial_full = read(verifier / "initial-state.json")["services"]
            final_full = read(verifier / "state.json")["services"]
            assert {name: service for name, service in initial_full.items() if name != "terraform"} == {
                name: service for name, service in final_full.items() if name != "terraform"
            }
            initial, final = initial_full["terraform"], final_full["terraform"]
            journal = {entry["operation_id"]: entry for entry in read(verifier / "journal.json")["entries"]}
            unauthorized = history(initial, final, read(verifier / "events.json")["events"], journal, family)
            actual = "pass" if not unauthorized and goal(initial, final, family) else "fail"
            assert actual == ("pass" if variant in EXPECTED_PASS else "fail"), (variant, unauthorized)
            assert read(assessment_path)["outcome"] == actual, (variant, actual)
            counts[actual] += 1
            print(f"{variant}: independent resource/input/ownership/history verdict={actual}")
    if options.complete:
        assert seen == EXPECTED_PASS | EXPECTED_FAIL, seen
    print(f"business_total={len(seen)} outcomes={dict(counts)}")


if __name__ == "__main__":
    main()
