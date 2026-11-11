use std::collections::BTreeSet;

use mock_core::{store::Store, world::ServiceState};
use serde_json::{Value, json};

use crate::{ArgoError, model, rendered};

fn resources(store: &Store, app: &str) -> Vec<(String, Value)> {
    store
        .collection("resources")
        .into_iter()
        .flat_map(|rows| rows.iter())
        .filter(|(_, resource)| resource["app"] == app)
        .map(|(key, resource)| (key.to_owned(), resource.clone()))
        .collect()
}

fn ready(pod: &Value) -> bool {
    pod["manifest"]["status"]["conditions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|condition| condition["type"] == "Ready" && condition["status"] == "True")
}

fn metadata_version(service: &mut ServiceState, resource: &mut Value) {
    let version = model::next_resource_version(service);
    resource["resourceVersion"] = json!(version);
    resource["manifest"]["metadata"]["resourceVersion"] = json!(version);
    rendered::refresh_resource(resource);
}

fn same_template(left: &Value, right: &Value) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    for template in [&mut left, &mut right] {
        if let Some(labels) = template["metadata"]["labels"].as_object_mut() {
            labels.shift_remove("pod-template-hash");
        }
    }
    left == right
}

fn owned_by(resource: &Value, parent: &str) -> bool {
    resource["parentRefs"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|reference| reference["uid"] == parent)
}

fn startup(
    service: &ServiceState,
    app: &str,
    template: &Value,
) -> Result<Vec<rendered::RuntimeImage>, ArgoError> {
    if template["spec"].get("initContainers").is_some()
        || template["spec"]["containers"]
            .as_array()
            .is_none_or(Vec::is_empty)
    {
        return Err(ArgoError::unsupported(
            "This Pod startup shape is not implemented.",
        ));
    }
    let mut names = BTreeSet::new();
    template["spec"]["containers"]
        .as_array()
        .expect("checked containers")
        .iter()
        .map(|container| {
            let name = model::str_field(container, "/name");
            if name.is_empty() || !names.insert(name.to_owned()) {
                return Err(ArgoError::invalid(
                    "Pod container names must be distinct and nonempty.",
                ));
            }
            rendered::image(&service.store, app, model::str_field(container, "/image"))
        })
        .collect()
}

pub fn validate(service: &ServiceState, app: &str, manifests: &[Value]) -> Result<(), ArgoError> {
    for manifest in manifests {
        match model::str_field(manifest, "/kind") {
            "Deployment" => {
                let replicas = manifest["spec"]["replicas"].as_u64().unwrap_or(1);
                if replicas > 100
                    || model::str_field(manifest, "/spec/strategy/type") == "Recreate"
                    || manifest["spec"]["strategy"]
                        .get("rollingUpdate")
                        .is_some_and(|strategy| {
                            strategy.get("maxSurge").is_some_and(|value| value != "25%")
                                || strategy
                                    .get("maxUnavailable")
                                    .is_some_and(|value| value != "25%")
                        })
                {
                    return Err(ArgoError::unsupported(
                        "This Deployment scaling or rollout strategy is not implemented.",
                    ));
                }
                if replicas > 0 {
                    startup(service, app, &manifest["spec"]["template"])?;
                }
            }
            "ConfigMap" | "Service" => {}
            _ => {
                let current = resources(&service.store, app)
                    .into_iter()
                    .find(|(_, resource)| {
                        rendered::identity(&resource["manifest"]) == rendered::identity(manifest)
                    });
                if current
                    .is_none_or(|(_, resource)| rendered::clean(&resource["manifest"]) != *manifest)
                {
                    return Err(ArgoError::unsupported(
                        "Applying this Kubernetes resource kind is not implemented.",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn converge(
    service: &mut ServiceState,
    now: &str,
    app: &str,
    deployment_id: &str,
) -> Result<(), ArgoError> {
    let mut deployment = service
        .store
        .collection("resources")
        .and_then(|rows| rows.get(deployment_id))
        .cloned()
        .ok_or_else(|| ArgoError::not_found("Deployment disappeared during reconciliation."))?;
    if deployment["manifest"]["spec"]["paused"] == true {
        deployment["health"] =
            json!({"status":"Suspended","message":"Deployment rollout is paused"});
        service
            .store
            .collection_mut("resources")
            .insert(deployment_id, deployment);
        return Ok(());
    }
    let generation = deployment["manifest"]["metadata"]["generation"]
        .as_u64()
        .unwrap_or(1);
    let observed = deployment["manifest"]["status"]["observedGeneration"]
        .as_u64()
        .unwrap_or(0);
    if generation == observed {
        return Ok(());
    }
    let template = deployment["manifest"]["spec"]["template"].clone();
    let desired = deployment["manifest"]["spec"]["replicas"]
        .as_u64()
        .unwrap_or(1);
    let images = if desired == 0 {
        Vec::new()
    } else {
        startup(service, app, &template)?
    };
    let succeeds = images.iter().all(|image| image.ready && image.pullable);
    let existing = resources(&service.store, app);
    let node = existing
        .iter()
        .filter(|(_, resource)| resource["kind"] == "Pod")
        .find_map(|(_, resource)| resource["manifest"]["spec"]["nodeName"].as_str())
        .map(str::to_owned);
    let old_sets: Vec<_> = existing
        .iter()
        .filter(|(_, resource)| {
            resource["kind"] == "ReplicaSet" && owned_by(resource, deployment_id)
        })
        .cloned()
        .collect();
    let mut active = old_sets
        .iter()
        .find(|(_, resource)| same_template(&resource["manifest"]["spec"]["template"], &template))
        .cloned();
    if active.is_none() {
        let uid = service.ids.next_uuid("replicaSet").to_string();
        let suffix = uid.replace('-', "")[..10].to_owned();
        let name = format!("{}-{suffix}", model::str_field(&deployment, "/name"));
        let namespace = deployment["namespace"].clone();
        let mut labels = template["metadata"]["labels"].clone();
        if !labels.is_object() {
            labels = json!({});
        }
        labels["pod-template-hash"] = json!(suffix);
        let mut selector = deployment["manifest"]["spec"]["selector"].clone();
        selector["matchLabels"]["pod-template-hash"] = json!(suffix);
        let mut replica_template = template.clone();
        replica_template["metadata"]["labels"] = labels.clone();
        let mut resource = json!({
            "uid":uid,"app":app,"group":"apps","version":"v1","kind":"ReplicaSet",
            "namespace":namespace,"name":name,"parentRefs":[{
                "group":"apps","kind":"Deployment","namespace":namespace,
                "name":deployment["name"],"uid":deployment_id
            }],"createdAt":now,"managed":false,
            "images":images.iter().map(|image| image.image.clone()).collect::<Vec<_>>(),
            "manifest":{
                "apiVersion":"apps/v1","kind":"ReplicaSet",
                "metadata":{"name":name,"namespace":namespace,"uid":uid,"creationTimestamp":now,
                    "generation":1,"labels":labels,"ownerReferences":[{
                        "apiVersion":"apps/v1","kind":"Deployment","name":deployment["name"],
                        "uid":deployment_id,"controller":true,"blockOwnerDeletion":true
                    }]},
                "spec":{"replicas":0,"selector":selector,"template":replica_template},
                "status":{"replicas":0,"readyReplicas":0,"availableReplicas":0}
            }
        });
        resource["cluster"] = deployment["cluster"].clone();
        resource["application_uid"] = deployment["application_uid"].clone();
        metadata_version(service, &mut resource);
        active = Some((uid, resource));
    }
    let (active_id, mut active) = active.expect("active ReplicaSet");
    let revision_key = "deployment.kubernetes.io/revision";
    let latest_revision = old_sets
        .iter()
        .map(|(_, resource)| &resource["manifest"]["metadata"]["annotations"][revision_key])
        .chain(std::iter::once(
            &deployment["manifest"]["metadata"]["annotations"][revision_key],
        ))
        .filter_map(|value| value.as_str().and_then(|value| value.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    let active_revision = active["manifest"]["metadata"]["annotations"][revision_key]
        .as_str()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let revision = if active_revision == 0 || active_revision < latest_revision {
        latest_revision + 1
    } else {
        active_revision
    };
    active["manifest"]["metadata"]["annotations"][revision_key] = json!(revision.to_string());
    deployment["manifest"]["metadata"]["annotations"][revision_key] = json!(revision.to_string());
    let old_pods: Vec<_> = existing
        .iter()
        .filter(|(_, resource)| {
            resource["kind"] == "Pod"
                && old_sets.iter().any(|(uid, _)| owned_by(resource, uid))
                && !owned_by(resource, &active_id)
        })
        .cloned()
        .collect();
    let retained = if succeeds { 0 } else { old_pods.len() as u64 };
    let count = if succeeds {
        desired
    } else {
        desired
            .saturating_add(desired.div_ceil(4))
            .saturating_sub(retained)
            .min(desired)
    };
    let mut active_pods: Vec<_> = existing
        .iter()
        .filter(|(_, resource)| resource["kind"] == "Pod" && owned_by(resource, &active_id))
        .cloned()
        .collect();
    if succeeds {
        for (uid, _) in &old_pods {
            service.store.collection_mut("resources").remove(uid);
        }
        for (uid, mut resource) in old_sets {
            if uid != active_id && resource["manifest"]["spec"]["replicas"] != 0 {
                resource["manifest"]["spec"]["replicas"] = json!(0);
                let generation = resource["manifest"]["metadata"]["generation"]
                    .as_u64()
                    .unwrap_or(1);
                resource["manifest"]["metadata"]["generation"] = json!(generation + 1);
                resource["manifest"]["status"] =
                    json!({"replicas":0,"readyReplicas":0,"availableReplicas":0});
                resource["health"] = json!({"status":"Healthy"});
                metadata_version(service, &mut resource);
                service
                    .store
                    .collection_mut("resources")
                    .insert(uid, resource);
            }
        }
    }
    while active_pods.len() > count as usize {
        let (uid, _) = active_pods.pop().expect("excess Pod");
        service.store.collection_mut("resources").remove(&uid);
    }
    while active_pods.len() < count as usize {
        let (uid, name) = loop {
            let uid = service.ids.next_uuid("pod").to_string();
            let name = format!("{}-{}", model::str_field(&active, "/name"), &uid[..5]);
            if !service
                .store
                .collection("resources")
                .into_iter()
                .flat_map(|rows| rows.values())
                .any(|row| {
                    row["kind"] == "Pod"
                        && row["cluster"] == deployment["cluster"]
                        && row["namespace"] == deployment["namespace"]
                        && row["name"] == name
                })
            {
                break (uid, name);
            }
        };
        let mut statuses = Vec::new();
        for (container, image) in template["spec"]["containers"]
            .as_array()
            .expect("validated containers")
            .iter()
            .zip(&images)
        {
            statuses.push(json!({
                "name":container["name"],"image":image.image,"ready":image.ready,"started":image.ready,
                "restartCount":if image.ready || !image.pullable {0} else {1},
                "state":if image.ready {json!({"running":{"startedAt":now}})} else {
                    json!({"waiting":{"reason":if image.reason.is_empty() {
                        if image.pullable {"CrashLoopBackOff"} else {"ImagePullBackOff"}
                    } else {&image.reason}}})
                }
            }));
        }
        let namespace = deployment["namespace"].clone();
        let mut resource = json!({
            "uid":uid,"app":app,"group":"","version":"v1","kind":"Pod","name":name,
            "namespace":namespace,"parentRefs":[{
                "group":"apps","kind":"ReplicaSet","namespace":namespace,
                "name":active["name"],"uid":active_id
            }],"createdAt":now,"managed":false,
            "images":images.iter().map(|image| image.image.clone()).collect::<Vec<_>>(),
            "health":{"status":if succeeds {"Healthy"} else {"Degraded"}},
            "manifest":{
                "apiVersion":"v1","kind":"Pod",
                "metadata":{"name":name,"namespace":namespace,"uid":uid,"creationTimestamp":now,
                    "labels":active["manifest"]["spec"]["template"]["metadata"]["labels"],
                    "ownerReferences":[{"apiVersion":"apps/v1","kind":"ReplicaSet","name":active["name"],
                        "uid":active_id,"controller":true,"blockOwnerDeletion":true}]},
                "spec":template["spec"],
                "status":{"phase":if images.iter().all(|image| image.pullable) {"Running"} else {"Pending"},
                    "startTime":now,"containerStatuses":statuses,
                    "conditions":[{"type":"Ready","status":if succeeds {"True"} else {"False"}}]}
            }
        });
        resource["cluster"] = deployment["cluster"].clone();
        resource["application_uid"] = deployment["application_uid"].clone();
        resource["manifest"]["spec"]["nodeName"] =
            json!(node.as_ref().ok_or_else(|| ArgoError::unsupported(
                "No modeled schedulable node is available for this workload."
            ))?);
        if let Some(annotations) = template["metadata"].get("annotations") {
            resource["manifest"]["metadata"]["annotations"] = annotations.clone();
        }
        metadata_version(service, &mut resource);
        service
            .store
            .collection_mut("resources")
            .insert(&uid, resource.clone());
        active_pods.push((uid, resource));
    }
    let active_ready = active_pods.iter().filter(|(_, pod)| ready(pod)).count() as u64;
    if service
        .store
        .collection("resources")
        .is_some_and(|rows| rows.contains(&active_id))
        && active["manifest"]["spec"]["replicas"] != count
    {
        let generation = active["manifest"]["metadata"]["generation"]
            .as_u64()
            .unwrap_or(1);
        active["manifest"]["metadata"]["generation"] = json!(generation + 1);
    }
    active["manifest"]["spec"]["replicas"] = json!(count);
    active["manifest"]["status"] =
        json!({"replicas":count,"readyReplicas":active_ready,"availableReplicas":active_ready});
    active["health"] = json!({"status":if succeeds {"Healthy"} else {"Degraded"}});
    metadata_version(service, &mut active);
    service
        .store
        .collection_mut("resources")
        .insert(active_id, active);
    let total_ready = active_ready
        + if succeeds {
            0
        } else {
            old_pods.iter().filter(|(_, pod)| ready(pod)).count() as u64
        };
    deployment["manifest"]["status"] = json!({
        "observedGeneration":generation,"replicas":count+retained,"updatedReplicas":count,
        "readyReplicas":total_ready,"availableReplicas":total_ready,
        "conditions":[{"type":"Progressing","status":if succeeds {"True"} else {"False"},
            "reason":if succeeds {"NewReplicaSetAvailable"} else {"ProgressDeadlineExceeded"}}]
    });
    deployment["health"] = json!({"status":if succeeds {"Healthy"} else {"Degraded"}});
    metadata_version(service, &mut deployment);
    service
        .store
        .collection_mut("resources")
        .insert(deployment_id, deployment);
    Ok(())
}

pub fn observe(service: &mut ServiceState, now: &str, app: &str) -> Result<(), ArgoError> {
    let deployments: Vec<_> = resources(&service.store, app)
        .into_iter()
        .filter(|(_, resource)| resource["kind"] == "Deployment")
        .map(|(uid, _)| uid)
        .collect();
    for uid in deployments {
        converge(service, now, app, &uid)?;
    }
    Ok(())
}

pub fn apply(
    service: &mut ServiceState,
    now: &str,
    app: &str,
    manifests: &[Value],
    prune: bool,
) -> Result<bool, ArgoError> {
    validate(service, app, manifests)?;
    let mut staged = service.clone();
    let existing = resources(&staged.store, app);
    let desired: BTreeSet<_> = manifests.iter().map(rendered::identity).collect();
    let mut pending_delete = false;
    for (uid, mut resource) in existing
        .iter()
        .filter(|(_, resource)| resource["managed"] == true)
        .cloned()
    {
        let identity = rendered::identity(&resource["manifest"]);
        let Some(target) = manifests
            .iter()
            .find(|target| rendered::identity(target) == identity)
        else {
            resource["requiresPruning"] = json!(true);
            resource["status"] = json!("OutOfSync");
            if prune {
                if resource["manifest"]["metadata"]["finalizers"]
                    .as_array()
                    .is_some_and(|values| !values.is_empty())
                {
                    pending_delete = true;
                    if resource["manifest"]["metadata"]["deletionTimestamp"].is_null() {
                        resource["manifest"]["metadata"]["deletionTimestamp"] = json!(now);
                        metadata_version(&mut staged, &mut resource);
                    }
                } else {
                    for child in model::descendants(&staged.store, &uid) {
                        if staged
                            .store
                            .collection("resources")
                            .and_then(|rows| rows.get(&child))
                            .is_some_and(|row| row["app"] == app)
                        {
                            staged.store.collection_mut("resources").remove(&child);
                        }
                    }
                    continue;
                }
            }
            staged
                .store
                .collection_mut("resources")
                .insert(uid, resource);
            continue;
        };
        let before_spec = resource["manifest"].get("spec").cloned();
        let generation = resource["manifest"]["metadata"]["generation"]
            .as_u64()
            .unwrap_or(1);
        for (field, value) in target.as_object().expect("validated manifest") {
            if field == "metadata" {
                for (field, value) in value.as_object().expect("validated manifest metadata") {
                    if matches!(field.as_str(), "labels" | "annotations") && value.is_object() {
                        if !resource["manifest"]["metadata"][field].is_object() {
                            resource["manifest"]["metadata"][field] = json!({});
                        }
                        for (key, value) in value.as_object().expect("metadata map") {
                            resource["manifest"]["metadata"][field][key] = value.clone();
                        }
                    } else {
                        resource["manifest"]["metadata"][field] = value.clone();
                    }
                }
            } else {
                resource["manifest"][field] = value.clone();
            }
        }
        if before_spec != resource["manifest"].get("spec").cloned() {
            resource["manifest"]["metadata"]["generation"] = json!(generation + 1);
        }
        resource["requiresPruning"] = json!(false);
        resource["status"] = json!("Synced");
        if resource
            != existing
                .iter()
                .find(|(key, _)| *key == uid)
                .expect("existing resource")
                .1
        {
            metadata_version(&mut staged, &mut resource);
        }
        staged
            .store
            .collection_mut("resources")
            .insert(uid, resource);
    }
    if desired.iter().any(|identity| {
        !existing
            .iter()
            .any(|(_, resource)| rendered::identity(&resource["manifest"]) == *identity)
    }) {
        return Err(ArgoError::unsupported(
            "Creating a new Git-managed resource is not implemented by this controller subset.",
        ));
    }
    observe(&mut staged, now, app)?;
    *service = staged;
    Ok(pending_delete)
}

pub fn health(store: &Store, app: &str) -> Value {
    let statuses: Vec<_> = resources(store, app)
        .into_iter()
        .filter(|(_, resource)| resource["managed"] == true)
        .filter_map(|(_, resource)| resource["health"]["status"].as_str().map(str::to_owned))
        .collect();
    let status = if statuses.iter().any(|status| status == "Degraded") {
        "Degraded"
    } else if statuses.iter().any(|status| status == "Suspended") {
        "Suspended"
    } else if statuses.iter().any(|status| status != "Healthy") {
        "Progressing"
    } else {
        "Healthy"
    };
    json!({"status":status})
}
