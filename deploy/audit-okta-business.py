import argparse
import collections
import copy
import json
from pathlib import Path
from urllib.parse import unquote


CLIENT_ID = "0oa8wq4n2Yc6Ds7Rt0x7"
EXPECTED_PASS = {
    "transfer_reference", "transfer_reverse", "transfer_login", "transfer_full", "transfer_extra_reads",
    "transfer_attributes",
    "offboard_reference", "offboard_reverse", "offboard_login",
    "policy_reference", "policy_split", "policy_retry",
}
EXPECTED_FAIL = {
    "transfer_restored", "transfer_other_user", "transfer_extra_group", "transfer_other_member",
    "transfer_profile_only", "offboard_keep_group", "offboard_wrong", "offboard_delete",
    "offboard_profile", "policy_scope", "policy_priority", "policy_new_policy", "policy_employee",
    "policy_disabled",
}


def read(path):
    return json.loads(path.read_bytes())


def one(records, predicate):
    matches = [(identity, record) for identity, record in records.items() if predicate(record)]
    assert len(matches) == 1, "initial task identity is not unique"
    return matches[0]


def identities(initial, family):
    login = (
        "alex.kim@northwind-consulting.example" if family == "offboard"
        else "maria.gonzalez@octo-corp.example"
    )
    user_id, user = one(initial["users"], lambda record: record["profile"]["login"] == login)
    assert user_id == user["id"]
    groups = {}
    for name in ["Platform Engineering", "Support Agents", "Contractors - Salesforce"]:
        group_id, group = one(initial["groups"], lambda record: record["profile"]["name"] == name)
        assert group_id == group["id"] and group["type"] == "OKTA_GROUP"
        groups[name] = group_id
    policy_id, policy = one(initial["policies"], lambda record: (
        record["type"] == "PASSWORD" and record["name"] == "Contractors Password Policy"
    ))
    rules = {
        identity: record for identity, record in initial["policy_rules"].items()
        if identity.startswith(policy_id + "/") and record["type"] == "PASSWORD"
        and record["system"] is False
    }
    assert len(rules) == 1
    rule_key, rule = next(iter(rules.items()))
    return {
        "user_id": user_id, "login": login, "groups": groups,
        "policy_id": policy_id, "rule_key": rule_key, "rule_id": rule["id"],
    }


def resolve_user(initial, value):
    if value in initial["users"]:
        return value
    matching = [
        identity for identity, user in initial["users"].items()
        if user["profile"]["login"].casefold() == value.casefold()
    ]
    if not matching:
        matching = [
            identity for identity, user in initial["users"].items()
            if user["profile"]["login"].split("@", 1)[0].casefold() == value.casefold()
        ]
    return matching[0] if len(matching) == 1 else None


def request_identity(initial, request):
    path = [unquote(segment) for segment in request["path"].split("/")[1:]]
    if path[:3] == ["api", "v1", "users"] and len(path) in {4, 6}:
        user_id = resolve_user(initial, path[3])
        if len(path) == 4:
            return ("user", user_id)
        if path[4:] == ["lifecycle", "deactivate"]:
            return ("deactivate", user_id)
    if path[:3] == ["api", "v1", "groups"] and len(path) == 6 and path[4] == "users":
        return ("membership", path[3], resolve_user(initial, path[5]))
    if path[:3] == ["api", "v1", "policies"] and len(path) == 6 and path[4] == "rules":
        return ("rule", path[3], path[5])
    return ("other",)


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


def password_rule_for(service, user_id):
    user = service["users"].get(user_id)
    if user is None:
        return None
    provider = user["credentials"]["provider"]["type"]
    groups = {
        membership["groupId"] for membership in service["group_members"].values()
        if membership["userId"] == user_id
    }
    policies = []
    for policy_id, policy in service["policies"].items():
        if policy["type"] != "PASSWORD" or policy["status"] != "ACTIVE":
            continue
        conditions = policy.get("conditions", {})
        people = conditions.get("people", {})
        if conditions.get("authProvider", {}).get("provider") != provider:
            continue
        if not groups.intersection(people.get("groups", {}).get("include", [])):
            continue
        if user_id in people.get("users", {}).get("exclude", []):
            continue
        policies.append((policy_id, policy))
    policies.sort(key=lambda item: item[1]["priority"])
    if len({policy["priority"] for _, policy in policies}) != len(policies):
        return None
    for policy_id, _ in policies:
        applicable = []
        for identity, rule in service["policy_rules"].items():
            if not identity.startswith(policy_id + "/") or rule["status"] != "ACTIVE" or rule["type"] != "PASSWORD":
                continue
            conditions = rule.get("conditions", {})
            if conditions.get("network", {}).get("connection") != "ANYWHERE":
                continue
            if user_id in conditions.get("people", {}).get("users", {}).get("exclude", []):
                continue
            applicable.append((identity, rule))
        applicable.sort(key=lambda item: item[1]["priority"])
        if applicable:
            if len(applicable) > 1 and applicable[0][1]["priority"] == applicable[1][1]["priority"]:
                return None
            return applicable[0]
    return None


def permitted_log(initial, record, targets, family, request):
    if record["actor"]["id"] != CLIENT_ID or record["outcome"]["result"] != "SUCCESS":
        return False
    operation = request_identity(initial, request)
    subject = targets["user_id"]
    actual_targets = [target["id"] for target in record["target"]]
    if family == "transfer" and operation == ("user", subject):
        return request["method"] in {"POST", "PUT"} \
            and record["eventType"] == "user.account.update_profile" and actual_targets == [subject]
    if family == "offboard" and operation == ("deactivate", subject):
        return request["method"] == "POST" \
            and record["eventType"] == "user.lifecycle.deactivate" and actual_targets == [subject]
    if family == "policy":
        return (
            operation == ("rule", targets["policy_id"], targets["rule_id"])
            and request["method"] == "PUT" and record["eventType"] == "policy.rule.update"
            and actual_targets == [targets["rule_id"], targets["policy_id"]]
        )
    if operation[0] != "membership" or operation[2] != subject:
        return False
    group = operation[1]
    if family == "transfer":
        allowed = {
            targets["groups"]["Platform Engineering"]: ("PUT", "group.user_membership.add"),
            targets["groups"]["Support Agents"]: ("DELETE", "group.user_membership.remove"),
        }
    else:
        allowed = {
            targets["groups"]["Contractors - Salesforce"]: ("DELETE", "group.user_membership.remove"),
        }
    return group in allowed and allowed[group] == (request["method"], record["eventType"]) \
        and actual_targets == [subject, group]


def permitted_change(initial, collection, identifier, before, after, targets, family, request):
    operation = request_identity(initial, request)
    subject = targets["user_id"]
    if collection == "logs":
        return before is None and after is not None \
            and permitted_log(initial, after, targets, family, request)
    if collection == "users":
        if before is None or after is None or identifier != subject:
            return False
        changed = differences(before, after)
        if family == "transfer":
            if operation != ("user", subject) or request["method"] not in {"POST", "PUT"}:
                return False
            if not changed <= {("profile", "title"), ("profile", "department"), ("lastUpdated",)}:
                return False
            expected = {"title": "Site Reliability Engineer", "department": "Platform Engineering"}
            for field, value in expected.items():
                if ("profile", field) in changed and (
                    after["profile"][field] != value
                    or request["request_body"]["profile"].get(field) != value
                ):
                    return False
            return True
        if family != "offboard" or operation != ("deactivate", subject) or request["method"] != "POST":
            return False
        return changed <= {
            ("status",), ("activated",), ("statusChanged",), ("lastUpdated",), ("transitioningToStatus",),
        } and after["status"] == "DEPROVISIONED" and after["activated"] is None \
            and "transitioningToStatus" not in after
    if collection == "policy_rules":
        if family != "policy" or before is None or after is None or identifier != targets["rule_key"]:
            return False
        if operation != ("rule", targets["policy_id"], targets["rule_id"]) or request["method"] != "PUT":
            return False
        changed = differences(before, after)
        if not changed <= {
            ("actions", "selfServicePasswordReset", "access"),
            ("actions", "selfServiceUnlock", "access"), ("lastUpdated",),
        }:
            return False
        return all(
            after["actions"][action]["access"] == "DENY"
            and request["request_body"]["actions"][action]["access"] == "DENY"
            for action in ["selfServicePasswordReset", "selfServiceUnlock"]
            if ("actions", action, "access") in changed
        )
    if collection not in {"group_members", "groups"} or family not in {"transfer", "offboard"}:
        return False
    if operation[0] != "membership" or operation[2] != subject:
        return False
    group = operation[1]
    if family == "transfer":
        allowed = {
            targets["groups"]["Platform Engineering"]: "PUT",
            targets["groups"]["Support Agents"]: "DELETE",
        }
    else:
        allowed = {targets["groups"]["Contractors - Salesforce"]: "DELETE"}
    if group not in allowed or request["method"] != allowed[group]:
        return False
    if collection == "groups":
        return identifier == group and before is not None and after is not None \
            and differences(before, after) <= {("lastMembershipUpdated",)}
    if identifier != f"{group}/{subject}":
        return False
    if request["method"] == "PUT":
        return before is None and after == {"groupId": group, "userId": subject}
    return before == {"groupId": group, "userId": subject} and after is None


def goal(initial, final, family):
    targets = identities(initial, family)
    subject = targets["user_id"]
    user = final["users"].get(subject)
    if user is None:
        return False
    if family == "transfer":
        return (
            user["status"] == "ACTIVE"
            and user["profile"]["title"] == "Site Reliability Engineer"
            and user["profile"]["department"] == "Platform Engineering"
            and f'{targets["groups"]["Platform Engineering"]}/{subject}' in final["group_members"]
            and f'{targets["groups"]["Support Agents"]}/{subject}' not in final["group_members"]
        )
    if family == "offboard":
        return user["status"] == "DEPROVISIONED" and "transitioningToStatus" not in user \
            and f'{targets["groups"]["Contractors - Salesforce"]}/{subject}' not in final["group_members"]
    policy = final["policies"].get(targets["policy_id"])
    rule = final["policy_rules"].get(targets["rule_key"])
    contractor_id, _ = one(initial["users"], lambda record: (
        record["profile"]["login"] == "alex.kim@northwind-consulting.example"
    ))
    selected = password_rule_for(final, contractor_id)
    return policy is not None and rule is not None and selected is not None \
        and selected[0] == targets["rule_key"] and policy["status"] == rule["status"] == "ACTIVE" \
        and rule["actions"]["passwordChange"]["access"] == "ALLOW" \
        and rule["actions"]["selfServicePasswordReset"]["access"] == "DENY" \
        and rule["actions"]["selfServiceUnlock"]["access"] == "DENY"


def history(initial, final, events, journal, family):
    targets = identities(initial, family)
    cursor = copy.deepcopy(initial)
    unauthorized = []
    for request in journal.values():
        if request["method"] == "DELETE" and request["path"].startswith("/api/v1/users/"):
            unauthorized.append(("forbidden_user_delete", request["operation_id"]))
    for event in events:
        assert event["kind"] != "environment_failed", event
        if event["kind"] != "state_committed":
            continue
        request = journal[event["operation"]["id"]]
        assert request["service"] == "okta" and request["status"] in {200, 201, 204}, event
        assert request["path"] == event["operation"]["path"]
        assert request["method"] == event["operation"]["method"]
        for change in event["records"]:
            parts = [part.replace("~1", "/").replace("~0", "~")
                     for part in change["pointer"].split("/")[1:]]
            assert len(parts) == 4 and parts[:2] == ["services", "okta"], parts
            collection, identifier = parts[2:]
            assert (identifier in cursor.get(collection, {})) == ("before" in change)
            before, after = change.get("before"), change.get("after")
            if "before" in change:
                assert cursor[collection][identifier] == before, change["pointer"]
            if not permitted_change(initial, collection, identifier, before, after, targets, family, request):
                unauthorized.append((event["seq"], change["pointer"]))
            if "after" in change:
                cursor.setdefault(collection, {})[identifier] = copy.deepcopy(after)
            else:
                del cursor[collection][identifier]
    assert {name: rows for name, rows in cursor.items() if rows} == {
        name: rows for name, rows in final.items() if rows
    }, "committed events do not reconstruct the collected final records"
    return unauthorized


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--complete", action="store_true")
    parser.add_argument("directories", type=Path, nargs="+")
    options = parser.parse_args()
    counts = collections.Counter()
    seen = set()
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
            assert {name: service for name, service in initial_full.items() if name != "okta"} == {
                name: service for name, service in final_full.items() if name != "okta"
            }, variant
            initial, final = initial_full["okta"], final_full["okta"]
            journal = {entry["operation_id"]: entry for entry in read(verifier / "journal.json")["entries"]}
            unauthorized = history(initial, final, read(verifier / "events.json")["events"], journal, family)
            actual = "pass" if not unauthorized and goal(initial, final, family) else "fail"
            assert actual == ("pass" if variant in EXPECTED_PASS else "fail"), (variant, unauthorized)
            assert read(assessment_path)["outcome"] == actual, (variant, actual)
            counts[actual] += 1
            print(f"{variant}: separately implemented identity/preservation/history verdict={actual}")
    if options.complete:
        assert seen == EXPECTED_PASS | EXPECTED_FAIL, seen
    print(f"business_total={len(seen)} outcomes={dict(counts)}")


if __name__ == "__main__":
    main()
