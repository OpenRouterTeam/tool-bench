import argparse
import collections
import copy
import json
from pathlib import Path


STOREFRONT = "prj_3dw2VoUa166qmaLV3P6dm828isAX"
CHECKOUT = "prj_9NauQLxYvOeIyWZWq6mqvavv9xuH"
PREDECESSOR = "dpl_8o704GjcwumkLAaxPwjoxb4V6QDF"
PRODUCTION_ALIASES = {"acme-shop.com", "storefront-acme-shop.vercel.app"}
EXPECTED_PASS = {
    "flag_reference", "flag_split_encrypted", "flag_reverse",
    "rotate_reference", "rollback_reference", "rollback_retry",
}
EXPECTED_FAIL = {
    "flag_branch", "flag_production", "flag_docs", "flag_existing_remove",
    "rotate_plain", "rotate_mask", "rotate_test_changed", "rotate_other_project",
    "rollback_bad", "rollback_failed", "rollback_unrouted", "rollback_preview",
    "rollback_spoof", "rollback_regions", "rollback_other_project", "rollback_environment",
}


def read(path):
    return json.loads(path.read_bytes())


def scopes(variable):
    result = set()
    for target in variable["target"]:
        branch = variable.get("gitBranch")
        result.add(f"preview:{branch}" if target == "preview" and branch else target)
    result.update(f"custom:{identity}" for identity in variable.get("customEnvironmentIds", []))
    return result


def changed_fields(before, after):
    if not isinstance(before, dict) or not isinstance(after, dict):
        return None
    return {field for field in before.keys() | after.keys() if before.get(field) != after.get(field)}


def configuration(service):
    bindings = collections.defaultdict(list)
    for variable in service.get("environment_variables", {}).values():
        for scope in scopes(variable):
            bindings[(variable["projectId"], scope, variable["key"])].append(variable)
    return bindings


def check_views(service, known):
    bindings = configuration(service)
    known.update("/".join(identity) for identity in bindings)
    expected_targets = {}
    for project_id, project in service["projects"].items():
        variables = [
            {field: value for field, value in variable.items() if field not in {"projectId", "scope"}}
            for variable in service.get("environment_variables", {}).values()
            if variable["projectId"] == project_id
        ]
        actual_variables = {variable["id"]: variable for variable in project["env"]}
        expected_variables = {variable["id"]: variable for variable in variables}
        assert len(actual_variables) == len(project["env"]), ("duplicate projected identity", project_id)
        assert len(expected_variables) == len(variables), ("duplicate canonical identity", project_id)
        assert actual_variables == expected_variables, ("project environment projection", project_id)
        expected_targets[project_id] = {"production": {}, "preview": {}, "development": {}}
    expected_bindings = {}
    for identity, variables in bindings.items():
        project_id, scope, key = identity
        value = variables[0]["value"] if len(variables) == 1 else None
        kind = variables[0]["type"] if len(variables) == 1 else None
        expected_targets[project_id].setdefault(scope, {})[key] = value
        expected_bindings["/".join(identity)] = {
            "projectId": project_id, "scope": scope, "key": key,
            "value": value, "type": kind, "records": len(variables),
        }
    actual = service.get("environment_bindings", {})
    assert set(actual) == known, "effective binding identities"
    for identity, binding in actual.items():
        if identity not in expected_bindings:
            assert binding["records"] == 0 and binding["value"] is None and binding["type"] is None
        else:
            assert binding == expected_bindings[identity], ("effective binding", identity)
    assert service["env_by_target"] == expected_targets, "target projection"


def canonical_creation(family, record):
    if not isinstance(record, dict):
        return False
    if family == "flag":
        return record["projectId"] == STOREFRONT and (
            record["key"] == "NEXT_PUBLIC_ENABLE_GIFT_CARDS" and record["value"] == "true"
            and bool(scopes(record)) and scopes(record) <= {"preview", "development"}
            or record["key"] == "PAYMENTS_API_URL"
            and record["value"] == "https://sandbox.pay.acme-shop.com" and scopes(record) == {"preview"}
        )
    return (
        family == "rotate" and record["projectId"] == CHECKOUT
        and record["key"] == "STRIPE_SECRET_KEY" and scopes(record) == {"production"}
        and record["type"] == "sensitive" and record["value"] == "sk_live_51NvCheckoutRot8Hq2"
    )


def check_history(initial, final, events, journal, family):
    cursor = copy.deepcopy(initial)
    known = set(initial.get("environment_bindings", {}))
    unauthorized = []
    for event in events:
        assert event["kind"] != "environment_failed", event
        if event["kind"] != "state_committed":
            continue
        operation = event["operation"]
        request = journal[operation["id"]]
        assert request["method"] == "POST" and request["path"] == "/_mcp/tools/call"
        assert request["status"] == 200
        tool = request["request_body"]["name"]
        arguments = request["request_body"]["arguments"]
        pending = []
        for change in event["records"]:
            tokens = [token.replace("~1", "/").replace("~0", "~")
                      for token in change["pointer"].split("/")[1:]]
            assert len(tokens) == 4 and tokens[:2] == ["services", "vercel"], tokens
            collection, identity = tokens[2:]
            assert (identity in cursor.get(collection, {})) == ("before" in change)
            if "before" in change:
                assert cursor[collection][identity] == change["before"], change["pointer"]
            if "after" in change:
                cursor.setdefault(collection, {})[identity] = copy.deepcopy(change["after"])
            else:
                del cursor[collection][identity]
            pending.append((collection, identity, change))
        check_views(cursor, known)
        for collection, identity, change in pending:
            before, after = change.get("before"), change.get("after")
            fields = changed_fields(before, after)
            permitted = False
            if collection == "environment_variables":
                permitted = before is None and canonical_creation(family, after)
                if before is None:
                    assert tool == "vercel-create-environment-variables"
                    assert any(
                        all(after[field] == submitted[field] for field in ["key", "value", "type"])
                        and scopes(after) == scopes(submitted)
                        for submitted in arguments["environmentVariables"]
                    ), "canonical environment record does not match the actual write"
                if family == "rotate" and after is None:
                    assert tool == "vercel-remove-environment-variable" and arguments["id"] == before["id"]
                    permitted = (
                        before["projectId"] == CHECKOUT and before["key"] == "STRIPE_SECRET_KEY"
                        and scopes(before) == {"production"} and before["type"] == "sensitive"
                    )
            elif collection in {"env_by_target", "environment_bindings"}:
                permitted = family in {"flag", "rotate"}
            elif collection == "projects":
                target = CHECKOUT if family == "rotate" else STOREFRONT
                permitted = identity == target and fields is not None and fields <= (
                    {"latestDeployments", "targets", "updatedAt"} if family == "rollback"
                    else {"env", "updatedAt"}
                )
            elif family == "rollback" and collection == "deployment_sources":
                assert tool == "vercel-create-deployment"
                assert after["originalDeploymentId"] == arguments["deploymentId"]
                expected_overrides = (
                    {"regions": arguments["regions"]} if "regions" in arguments else {}
                )
                assert after["overrides"] == expected_overrides
                preserved = (
                    "regions" not in arguments
                    or arguments["regions"] == initial["deployments"].get(
                        arguments["deploymentId"], {}
                    ).get("regions")
                )
                assert after["sourcePreserved"] == preserved
                permitted = (
                    before is None and after["projectId"] == STOREFRONT
                    and after.get("originalDeploymentId") == PREDECESSOR
                    and after.get("gitSource") == initial["deployments"][PREDECESSOR]["gitSource"]
                    and after.get("sourcePreserved") is True
                    and (after["overrides"] == {} or after["overrides"] == {
                        "regions": initial["deployments"][PREDECESSOR]["regions"]
                    })
                )
            elif family == "rollback" and collection == "deployments":
                provenance = cursor.get("deployment_sources", {}).get(identity, {})
                permitted = (
                    before is None and after["projectId"] == STOREFRONT
                    and after["source"] == "redeploy" and after["target"] == "production"
                    and provenance.get("originalDeploymentId") == PREDECESSOR
                    and provenance.get("sourcePreserved") is True
                )
            elif family == "rollback" and collection == "aliases":
                provenance = cursor.get("deployment_sources", {}).get((after or {}).get("deploymentId"), {})
                deployment = cursor.get("deployments", {}).get((after or {}).get("deploymentId"), {})
                permitted = (
                    identity in PRODUCTION_ALIASES and fields == {"deploymentId"}
                    and provenance.get("originalDeploymentId") == PREDECESSOR
                    and provenance.get("sourcePreserved") is True
                    and deployment.get("readyState") == "READY"
                    and deployment.get("target") == "production"
                    and deployment.get("projectId") == STOREFRONT
                )
            if not permitted:
                unauthorized.append((event["seq"], change["pointer"]))
    assert {key: value for key, value in cursor.items() if value} == {
        key: value for key, value in final.items() if value
    }, "committed events do not reconstruct the final resource records"
    return unauthorized


def goal(initial, final, family):
    bindings = configuration(final)
    if family in {"flag", "rotate"}:
        desired = (
            [(STOREFRONT, "preview", "NEXT_PUBLIC_ENABLE_GIFT_CARDS", "true", None),
             (STOREFRONT, "development", "NEXT_PUBLIC_ENABLE_GIFT_CARDS", "true", None),
             (STOREFRONT, "preview", "PAYMENTS_API_URL", "https://sandbox.pay.acme-shop.com", None)]
            if family == "flag" else
            [(CHECKOUT, "production", "STRIPE_SECRET_KEY", "sk_live_51NvCheckoutRot8Hq2", "sensitive")]
        )
        for project_id, scope, key, value, kind in desired:
            variables = bindings.get((project_id, scope, key), [])
            if len(variables) != 1 or variables[0]["value"] != value:
                return False
            if kind is not None and variables[0]["type"] != kind:
                return False
        return True
    created = {
        identity: deployment for identity, deployment in final["deployments"].items()
        if identity not in initial["deployments"]
    }
    if not created:
        return False
    wanted_source = initial["deployments"][PREDECESSOR]["gitSource"]
    if any(
        deployment["projectId"] != STOREFRONT or deployment["target"] != "production"
        or deployment["readyState"] != "READY" or deployment["source"] != "redeploy"
        or deployment["gitSource"] != wanted_source
        or deployment["meta"].get("githubCommitSha") != wanted_source["sha"]
        for deployment in created.values()
    ):
        return False
    identifiers = {
        final["aliases"][alias]["deploymentId"] for alias in PRODUCTION_ALIASES
    }
    return (
        len(identifiers) == 1 and identifiers <= created.keys()
        and final["projects"][STOREFRONT]["targets"]["production"]["id"] in identifiers
    )


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
            initial_full = read(verifier / "initial-state.json")["services"]
            final_full = read(verifier / "state.json")["services"]
            assert {key: value for key, value in initial_full.items() if key != "vercel"} == {
                key: value for key, value in final_full.items() if key != "vercel"
            }, variant
            initial, final = initial_full["vercel"], final_full["vercel"]
            family = variant.split("_", 1)[0]
            journal = {entry["operation_id"]: entry
                       for entry in read(verifier / "journal.json")["entries"]}
            unauthorized = check_history(
                initial, final, read(verifier / "events.json")["events"], journal, family,
            )
            expected = "pass" if not unauthorized and goal(initial, final, family) else "fail"
            assert expected == ("pass" if variant in EXPECTED_PASS else "fail"), (variant, unauthorized)
            assert read(assessment_path)["outcome"] == expected, (variant, expected)
            counts[expected] += 1
            print(f"{variant}: separately implemented business/history verdict={expected}")
    if options.complete:
        assert seen == EXPECTED_PASS | EXPECTED_FAIL, seen
    print(f"business_total={len(seen)} outcomes={dict(counts)}")


if __name__ == "__main__":
    main()
