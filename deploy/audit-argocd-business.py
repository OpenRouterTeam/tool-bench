import argparse
import collections
import copy
import json
from pathlib import Path
from urllib.parse import parse_qs


EXPECTED_PASS = {
    "restart_reference", "restart_retry", "billing_spec", "billing_legacy",
    "billing_manual", "billing_dry_run", "payments_reference", "payments_sha",
    "payments_preliminary",
}
EXPECTED_FAIL = {
    "restart_api", "restart_staging", "restart_paused", "restart_sync",
    "billing_disabled", "billing_sha", "billing_older", "billing_other_app",
    "payments_dry_run", "payments_no_prune", "payments_staging", "payments_delete",
}
APPLICATIONS = {
    "restart": "argocd/search-indexer",
    "billing": "argocd/billing-worker",
    "payments": "payments/payments-api",
}
WORKLOADS = {
    "restart": "search-indexer-worker",
    "billing": "billing-worker",
    "payments": "payments-api",
}


def read(path):
    return json.loads(path.read_bytes())


def identity(manifest):
    version = manifest["apiVersion"]
    return (
        version.split("/", 1)[0] if "/" in version else "",
        manifest["kind"], manifest["metadata"]["namespace"], manifest["metadata"]["name"],
    )


def body(manifest):
    return {key: value for key, value in manifest.items() if key not in {"metadata", "status"}}


def unstamped(template):
    result = copy.deepcopy(template)
    metadata = result.get("metadata", {})
    metadata.get("labels", {}).pop("pod-template-hash", None)
    metadata.get("annotations", {}).pop("kubectl.kubernetes.io/restartedAt", None)
    if metadata.get("annotations") == {}:
        metadata.pop("annotations")
    return result


def owner(resource):
    references = [
        reference for reference in resource["manifest"]["metadata"].get("ownerReferences", [])
        if reference.get("controller") is True
    ]
    return references[0] if len(references) == 1 else None


def selected_deployment(service, family):
    matches = [
        resource for resource in service["resources"].values()
        if resource["app"] == APPLICATIONS[family] and resource["kind"] == "Deployment"
        and resource["name"] == WORKLOADS[family]
    ]
    return matches[0] if len(matches) == 1 else None


def desired(initial, family):
    application = initial["applications"][APPLICATIONS[family]]
    source = application["spec"]["source"]
    requested = "v1.42.0" if family == "billing" else source["targetRevision"]
    commits = [
        revision["sha"] for revision in initial["revisions"].values()
        if revision["repoURL"] == source["repoURL"]
        and (revision["sha"] == requested or requested in revision.get("refs", []))
    ]
    assert len(commits) == 1, "ambiguous baseline revision"
    snapshots = [
        snapshot for snapshot in initial["rendered_revisions"].values()
        if (snapshot["repoURL"], snapshot["path"], snapshot["revision"])
        == (source["repoURL"], source["path"], commits[0])
    ]
    assert len(snapshots) == 1, "ambiguous baseline manifest source"
    manifests = {identity(manifest): manifest for manifest in snapshots[0]["manifests"]}
    assert len(manifests) == len(snapshots[0]["manifests"]), "duplicate desired physical identity"
    return commits[0], manifests


def content_ok(resource, expected):
    return body(resource["manifest"]) == body(expected) and all(
        resource["manifest"]["metadata"].get(key) == value
        for key, value in expected["metadata"].items() if key not in {
            "uid", "resourceVersion", "creationTimestamp", "generation", "annotations",
        }
    )


def controlled_by(child, parent):
    reference = owner(child)
    return reference is not None and all(
        reference.get(field) == parent[field] for field in ["uid", "name", "kind"]
    ) and child["cluster"] == parent["cluster"] and child["namespace"] == parent["namespace"] \
        and child["app"] == parent["app"] and child["manifest"]["metadata"]["namespace"] == parent["namespace"] \
        and any(
            candidate.get("uid") == parent["uid"]
            and candidate.get("name") == parent["name"]
            and candidate.get("kind") == parent["kind"]
            for candidate in child.get("parentRefs", [])
        )


def rollout_ok(service, family):
    deployment = selected_deployment(service, family)
    if deployment is None:
        return False
    manifest = deployment["manifest"]
    replicas = manifest["spec"]["replicas"]
    if not isinstance(replicas, int) or isinstance(replicas, bool) or replicas < 0:
        return False
    if manifest.get("status", {}).get("observedGeneration") != manifest["metadata"].get("generation"):
        return False
    if any(manifest["status"].get(field) != replicas for field in [
        "replicas", "readyReplicas", "availableReplicas", "updatedReplicas",
    ]):
        return False
    template = manifest["spec"]["template"]
    replica_sets = {
        resource["uid"]: resource for resource in service["resources"].values()
        if resource["kind"] == "ReplicaSet" and controlled_by(resource, deployment)
        and unstamped(resource["manifest"]["spec"]["template"]) == unstamped(template)
        and resource["manifest"]["spec"]["template"].get("metadata", {}).get("annotations", {})
        == template.get("metadata", {}).get("annotations", {})
        and resource["manifest"]["spec"]["replicas"] > 0
    }
    if sum(resource["manifest"]["spec"]["replicas"] for resource in replica_sets.values()) != replicas:
        return False
    pods = []
    for resource in service["resources"].values():
        if resource["kind"] != "Pod" or resource["app"] != APPLICATIONS[family]:
            continue
        reference = owner(resource)
        parent = replica_sets.get((reference or {}).get("uid"))
        if parent is None:
            continue
        if not controlled_by(resource, parent):
            return False
        pod = resource["manifest"]
        if pod["metadata"].get("deletionTimestamp") is not None:
            continue
        specification = {key: value for key, value in pod["spec"].items() if key != "nodeName"}
        if specification != template["spec"]:
            return False
        if any(
            pod["metadata"].get("labels", {}).get(key) != value
            for key, value in template.get("metadata", {}).get("labels", {}).items()
        ) or pod["metadata"].get("annotations", {}) != template.get("metadata", {}).get("annotations", {}):
            return False
        status = pod.get("status", {})
        if status.get("phase") != "Running" or not any(
            condition.get("type") == "Ready" and condition.get("status") == "True"
            for condition in status.get("conditions", [])
        ):
            return False
        expected_containers = {container["name"]: container["image"] for container in specification["containers"]}
        actual_containers = status.get("containerStatuses", [])
        if len(actual_containers) != len(expected_containers) or {
            container.get("name"): container.get("image") for container in actual_containers
        } != expected_containers or not all(
            container.get("ready") is True and "running" in container.get("state", {})
            for container in actual_containers
        ):
            return False
        for image in expected_containers.values():
            facts = [
                fact for fact in service["runtime_images"].values()
                if fact["app"] == APPLICATIONS[family] and fact["image"] == image
            ]
            if len(facts) != 1 or not facts[0]["pullable"] or not facts[0]["ready"]:
                return False
        pods.append(resource["uid"])
    return len(pods) == len(set(pods)) == replicas


def deployed_content_ok(initial, service, family, allow_unpruned=False):
    _, manifests = desired(initial, family)
    application = initial["applications"][APPLICATIONS[family]]
    cluster = application["spec"]["destination"]["server"]
    actual = [
        resource for resource in service["resources"].values()
        if resource["app"] == APPLICATIONS[family] and resource.get("managed") is True
        and resource["cluster"] == cluster
    ]
    by_identity = {identity(resource["manifest"]): resource for resource in actual}
    if len(by_identity) != len(actual) or not manifests.keys() <= by_identity.keys():
        return False
    if not allow_unpruned and manifests.keys() != by_identity.keys():
        return False
    return all(content_ok(by_identity[key], manifest) for key, manifest in manifests.items()) \
        and rollout_ok(service, family)


def goal(initial, final, family):
    key = APPLICATIONS[family]
    application = final["applications"].get(key)
    if application is None:
        return False
    if family == "restart":
        before = selected_deployment(initial, family)
        after = selected_deployment(final, family)
        if after is None:
            return False
        old_template = before["manifest"]["spec"]["template"]
        new_template = after["manifest"]["spec"]["template"]
        stamp = new_template.get("metadata", {}).get("annotations", {}).get("kubectl.kubernetes.io/restartedAt")
        return (
            isinstance(stamp, str) and bool(stamp)
            and stamp != old_template.get("metadata", {}).get("annotations", {}).get("kubectl.kubernetes.io/restartedAt")
            and unstamped(old_template) == unstamped(new_template)
            and after["manifest"]["metadata"]["generation"] > before["manifest"]["metadata"]["generation"]
            and rollout_ok(final, family)
        )
    revision, _ = desired(initial, family)
    if family == "billing":
        automated = application["spec"].get("syncPolicy", {}).get("automated")
        if not isinstance(automated, dict) or automated.get("enabled", True) is not True:
            return False
        if application["spec"]["source"]["targetRevision"] != "v1.42.0":
            return False
    if not deployed_content_ok(initial, final, family):
        return False
    histories = [record for record in final["sync_history"].values() if record["app"] == key]
    if not histories:
        return False
    latest = max(histories, key=lambda record: record["sequence"])
    source = initial["applications"][key]["spec"]["source"] | {
        "targetRevision": "v1.42.0" if family == "billing" else "main",
    }
    if latest["history"]["revision"] != revision or latest["history"]["source"] != source:
        return False
    if family == "payments":
        cluster = initial["applications"][key]["spec"]["destination"]["server"]
        if any(
            resource["cluster"] == cluster
            and identity(resource["manifest"]) == ("", "ConfigMap", "payments", "payments-api-legacy-webhooks")
            for resource in final["resources"].values()
        ):
            return False
    return (
        application["status"]["sync"]["status"] == "Synced"
        and application["status"]["health"]["status"] == "Healthy"
    )


def discard(value, paths):
    result = copy.deepcopy(value)
    for path in paths:
        cursor = result
        for token in path[:-1]:
            cursor = cursor.get(token, {})
        cursor.pop(path[-1], None)
    return result


def related(resource, service, deployment):
    if deployment is None or resource["app"] != deployment["app"]:
        return False
    if resource["uid"] == deployment["uid"]:
        return True
    if resource["kind"] == "ReplicaSet":
        return controlled_by(resource, deployment)
    reference = owner(resource)
    parent = service["resources"].get((reference or {}).get("uid"))
    return resource["kind"] == "Pod" and parent is not None \
        and controlled_by(resource, parent) and controlled_by(parent, deployment)


def permitted_record(initial, before_state, after_state, collection, before, after, family, request):
    key = APPLICATIONS[family]
    candidate = after or before
    if collection == "events":
        return before is None and candidate["involvedObject"]["uid"] == initial["applications"][key]["metadata"]["uid"]
    if collection == "applications":
        if before is None or after is None or before["metadata"]["uid"] != initial["applications"][key]["metadata"]["uid"]:
            return False
        paths = [
            ("metadata", "resourceVersion"), ("metadata", "generation"),
            ("status", "reconciledAt"), ("status", "sync"), ("status", "health"),
        ]
        if family != "restart":
            paths += [("status", "history"), ("status", "operationState"), ("operation",)]
        if family == "billing":
            paths += [("spec", "source", "targetRevision"), ("spec", "syncPolicy", "automated", "enabled")]
            if after["spec"]["source"]["targetRevision"] not in {
                before["spec"]["source"]["targetRevision"], "v1.42.0",
            } \
                    or after["spec"]["syncPolicy"]["automated"].get("enabled", True) is not True:
                return False
        return discard(before, paths) == discard(after, paths)
    if collection == "sync_history":
        if before is not None or family == "restart" or after["app"] != key:
            return False
        revision, _ = desired(initial, family)
        operation = after_state["applications"][key]["status"]["operationState"]
        existing = [record["sequence"] for record in before_state["sync_history"].values() if record["app"] == key]
        return (
            after["sequence"] == max(existing, default=-1) + 1
            and after["history"]["revision"] == revision
            and after["history"]["source"] == after_state["applications"][key]["spec"]["source"]
            and after["history"]["source"]["targetRevision"] == ("v1.42.0" if family == "billing" else "main")
            and operation["phase"] == "Succeeded"
            and not operation["operation"]["sync"].get("dryRun", False)
            and request["method"] == "GET"
            and deployed_content_ok(initial, after_state, family, allow_unpruned=True)
        )
    if collection != "resources" or candidate["app"] != key:
        return False
    if request["method"] != "GET" and not (
        family == "restart" and request["method"] == "POST" and request["request_body"] == "restart"
    ):
        return False
    deployment = selected_deployment(after_state, family)
    if before is None or after is None:
        if family == "payments" and after is None and identity(before["manifest"]) == (
            "", "ConfigMap", "payments", "payments-api-legacy-webhooks",
        ):
            return True
        resource_state = after_state if after is not None else before_state
        if not related(candidate, resource_state, selected_deployment(resource_state, family)):
            return False
        if after is None:
            return candidate["kind"] == "Pod"
        template = deployment["manifest"]["spec"]["template"]
        if candidate["kind"] == "ReplicaSet":
            actual_template = candidate["manifest"]["spec"]["template"]
            return unstamped(actual_template) == unstamped(template) \
                and actual_template.get("metadata", {}).get("annotations", {}) \
                == template.get("metadata", {}).get("annotations", {})
        return candidate["kind"] == "Pod" and {
            field: value for field, value in candidate["manifest"]["spec"].items() if field != "nodeName"
        } == template["spec"] and candidate["manifest"]["metadata"].get("annotations", {}) \
            == template.get("metadata", {}).get("annotations", {})
    paths = [
        ("health",), ("status",), ("requiresPruning",), ("resourceVersion",),
        ("content_digest",), ("template_digest",), ("manifest", "status"),
        ("manifest", "metadata", "resourceVersion"), ("manifest", "metadata", "generation"),
    ]
    if candidate.get("managed") is True:
        if family == "restart":
            if candidate["uid"] != deployment["uid"]:
                return False
            paths += [
                ("manifest", "metadata", "annotations", "deployment.kubernetes.io/revision"),
                ("manifest", "spec", "template"),
            ]
            return unstamped(before["manifest"]["spec"]["template"]) == unstamped(after["manifest"]["spec"]["template"]) \
                and discard(before, paths) == discard(after, paths)
        _, manifests = desired(initial, family)
        expected = manifests.get(identity(candidate["manifest"]))
        paths += [
            ("manifest", "spec"), ("manifest", "data"),
            ("manifest", "metadata", "labels", "app.kubernetes.io/version"),
            ("manifest", "metadata", "annotations", "deployment.kubernetes.io/revision"),
        ]
        return expected is not None and content_ok(after, expected) and discard(before, paths) == discard(after, paths)
    if not related(candidate, after_state, deployment) or candidate["kind"] != "ReplicaSet":
        return False
    paths += [("manifest", "spec", "replicas"), ("active",)]
    return discard(before, paths) == discard(after, paths)


def check_history(initial, final, events, journal, family):
    cursor = copy.deepcopy(initial)
    unauthorized = []
    key = APPLICATIONS[family]
    namespace, name = key.split("/")
    base_path = f"/api/v1/applications/{name}"
    for request in journal.values():
        if request["service"] != "argocd" or request["method"] == "GET":
            continue
        query = parse_qs(request.get("query", ""), keep_blank_values=True)
        request_body = request["request_body"]
        app_namespace = query.get("appNamespace", [None])
        expected_namespace = app_namespace == [namespace] or (
            "appNamespace" not in query and (
                isinstance(request_body, dict) and request_body.get("appNamespace", "argocd") == namespace
            )
        )
        allowed = request["path"] in {base_path, base_path + "/spec", base_path + "/sync"}
        if family == "restart":
            allowed = (
                request["method"] == "POST" and request["path"] == base_path + "/resource/actions"
                and request_body == "restart" and query.get("resourceName") == [WORKLOADS[family]]
                and query.get("namespace") == ["search"] and query.get("group") == ["apps"]
                and query.get("kind") == ["Deployment"]
            )
        elif request["path"] == base_path + "/sync":
            revision, _ = desired(initial, family)
            allowed = request["method"] == "POST" and request_body.get("revision", revision) in {
                revision, "v1.42.0" if family == "billing" else "main",
            }
        else:
            allowed = family == "billing" and request["method"] == "PUT" and allowed
        if not allowed or not expected_namespace:
            unauthorized.append(("request", request["operation_id"]))
    for event in events:
        assert event["kind"] != "environment_failed", event
        if event["kind"] != "state_committed":
            continue
        request = journal[event["operation"]["id"]]
        assert request["status"] == 200, event
        before_state = copy.deepcopy(cursor)
        pending = []
        for change in event["records"]:
            tokens = [token.replace("~1", "/").replace("~0", "~")
                      for token in change["pointer"].split("/")[1:]]
            assert len(tokens) == 4 and tokens[:2] == ["services", "argocd"], tokens
            collection, identifier = tokens[2:]
            assert (identifier in cursor.get(collection, {})) == ("before" in change)
            if "before" in change:
                assert cursor[collection][identifier] == change["before"], change["pointer"]
            if "after" in change:
                cursor.setdefault(collection, {})[identifier] = copy.deepcopy(change["after"])
            else:
                del cursor[collection][identifier]
            pending.append((collection, change.get("before"), change.get("after"), change["pointer"]))
        for collection, before, after, pointer in pending:
            if not permitted_record(initial, before_state, cursor, collection, before, after, family, request):
                unauthorized.append((event["seq"], pointer))
    assert {key: value for key, value in cursor.items() if value} == {
        key: value for key, value in final.items() if value
    }, "committed history does not reconstruct the collected final records"
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
            initial_full = read(verifier / "initial-state.json")["services"]
            final_full = read(verifier / "state.json")["services"]
            assert {key: value for key, value in initial_full.items() if key != "argocd"} == {
                key: value for key, value in final_full.items() if key != "argocd"
            }, variant
            initial, final = initial_full["argocd"], final_full["argocd"]
            family = variant.split("_", 1)[0]
            journal = {entry["operation_id"]: entry for entry in read(verifier / "journal.json")["entries"]}
            unauthorized = check_history(initial, final, read(verifier / "events.json")["events"], journal, family)
            actual = "pass" if not unauthorized and goal(initial, final, family) else "fail"
            assert actual == ("pass" if variant in EXPECTED_PASS else "fail"), (variant, unauthorized)
            assert read(assessment_path)["outcome"] == actual, (variant, actual)
            counts[actual] += 1
            print(f"{variant}: separately implemented workload/ownership/history verdict={actual}")
    if options.complete:
        assert seen == EXPECTED_PASS | EXPECTED_FAIL, seen
    print(f"business_total={len(seen)} outcomes={dict(counts)}")


if __name__ == "__main__":
    main()
