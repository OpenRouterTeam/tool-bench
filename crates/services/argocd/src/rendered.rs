use std::collections::{BTreeMap, BTreeSet};

use mock_core::{store::Store, world::ServiceState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{ArgoError, model};

pub const SOURCES: &str = "rendered_revisions";
pub const IMAGES: &str = "runtime_images";
pub const HISTORY: &str = "sync_history";
pub const DESIRED: &str = "desired_resources";

pub fn cluster(store: &Store, application: &Value) -> Option<String> {
    let server = model::str_field(application, "/spec/destination/server");
    if !server.is_empty() {
        Some(server.to_owned())
    } else {
        model::cluster_by(
            store,
            "name",
            model::str_field(application, "/spec/destination/name"),
        )
        .and_then(|cluster| cluster["server"].as_str())
        .map(str::to_owned)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RenderedRevision {
    #[serde(rename = "repoURL")]
    pub repo_url: String,
    pub path: String,
    pub revision: String,
    pub manifests: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeImage {
    pub app: String,
    pub image: String,
    pub pullable: bool,
    pub ready: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

pub fn identity(manifest: &Value) -> (String, String, String, String) {
    (
        model::str_field(manifest, "/apiVersion")
            .split_once('/')
            .map_or("", |(group, _)| group)
            .to_owned(),
        model::str_field(manifest, "/kind").to_owned(),
        model::str_field(manifest, "/metadata/namespace").to_owned(),
        model::str_field(manifest, "/metadata/name").to_owned(),
    )
}

pub fn clean(manifest: &Value) -> Value {
    let mut manifest = manifest.clone();
    if let Some(fields) = manifest.as_object_mut() {
        fields.shift_remove("status");
    }
    if let Some(metadata) = manifest["metadata"].as_object_mut() {
        for field in [
            "uid",
            "resourceVersion",
            "creationTimestamp",
            "generation",
            "managedFields",
        ] {
            metadata.shift_remove(field);
        }
        if let Some(annotations) = metadata
            .get_mut("annotations")
            .and_then(Value::as_object_mut)
        {
            annotations.shift_remove("deployment.kubernetes.io/revision");
            annotations.shift_remove("argocd.argoproj.io/tracking-id");
            if annotations.is_empty() {
                metadata.shift_remove("annotations");
            }
        }
    }
    manifest
}

fn revisions(store: &Store) -> Vec<RenderedRevision> {
    store
        .collection(SOURCES)
        .into_iter()
        .flat_map(|rows| rows.values())
        .map(|row| serde_json::from_value(row.clone()).expect("validated rendered revision"))
        .collect()
}

pub fn refresh_resource(resource: &mut Value) {
    let identity = identity(&resource["manifest"]);
    resource["resource_identity"] =
        json!(serde_json::to_string(&identity).expect("resource identity serializes"));
    let mut content = clean(&resource["manifest"]);
    content["metadata"] = json!({"name":resource["manifest"]["metadata"]["name"],
        "namespace":resource["manifest"]["metadata"]["namespace"]});
    resource["content_digest"] = json!(mock_core::provenance::canonical_sha256(&content));
    resource["deleting"] = json!(resource["manifest"]["metadata"]["deletionTimestamp"].is_string());
    let controllers: Vec<_> = resource["manifest"]["metadata"]["ownerReferences"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|owner| owner["controller"] == true)
        .filter_map(|owner| owner["uid"].as_str())
        .map(str::to_owned)
        .collect();
    assert!(
        controllers.len() <= 1,
        "a resource cannot have multiple controlling owners"
    );
    if let [controller] = controllers.as_slice() {
        resource["controller_uid"] = json!(controller);
    } else {
        resource
            .as_object_mut()
            .expect("resource object")
            .shift_remove("controller_uid");
    }
    if resource["kind"] == "Deployment" || resource["kind"] == "ReplicaSet" {
        let mut template = resource["manifest"]["spec"]["template"].clone();
        if let Some(labels) = template["metadata"]["labels"].as_object_mut() {
            labels.shift_remove("pod-template-hash");
        }
        resource["template_digest"] = json!(mock_core::provenance::canonical_sha256(&template));
    }
    if resource["kind"] == "ReplicaSet" {
        resource["active"] = json!(
            resource["manifest"]["spec"]["replicas"]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
    }
}

pub fn lookup(
    store: &Store,
    source: &Value,
    revision: &str,
) -> Result<RenderedRevision, ArgoError> {
    let matches: Vec<_> = revisions(store)
        .into_iter()
        .filter(|rendered| {
            rendered.repo_url == model::str_field(source, "/repoURL")
                && rendered.path == model::str_field(source, "/path")
                && rendered.revision == revision
        })
        .collect();
    match matches.as_slice() {
        [rendered] => Ok(rendered.clone()),
        [] => Err(ArgoError::unsupported(
            "A rendered manifest snapshot is unavailable for this repository, path, and revision.",
        )),
        _ => Err(ArgoError::unsupported(
            "The rendered manifest source is ambiguous.",
        )),
    }
}

pub fn current(store: &Store, application: &Value) -> Result<RenderedRevision, ArgoError> {
    let source = &application["spec"]["source"];
    let commit = model::resolve_revision(
        store,
        model::str_field(source, "/repoURL"),
        model::str_field(source, "/targetRevision"),
    )
    .ok_or_else(|| ArgoError::failed_precondition("The configured revision cannot be resolved."))?;
    lookup(store, source, model::str_field(commit, "/sha"))
}

pub fn image(store: &Store, app: &str, image: &str) -> Result<RuntimeImage, ArgoError> {
    let matches: Vec<RuntimeImage> = store
        .collection(IMAGES)
        .into_iter()
        .flat_map(|rows| rows.values())
        .filter(|row| row["app"] == app && row["image"] == image)
        .map(|row| serde_json::from_value(row.clone()).expect("validated runtime image"))
        .collect();
    match matches.as_slice() {
        [fact] => Ok(fact.clone()),
        _ => Err(ArgoError::unsupported(
            "Runtime startup evidence is unavailable or ambiguous for this workload image.",
        )),
    }
}

fn validate_revision(store: &Store, revision: &RenderedRevision) {
    assert!(
        !revision.repo_url.is_empty() && !revision.path.is_empty() && !revision.revision.is_empty(),
        "rendered revisions require repository, path, and concrete revision"
    );
    assert!(
        model::resolve_revision(store, &revision.repo_url, &revision.revision)
            .is_some_and(|commit| commit["sha"] == revision.revision),
        "rendered revisions require a known concrete repository commit"
    );
    let mut identities = BTreeSet::new();
    for manifest in &revision.manifests {
        let (group, kind, namespace, name) = identity(manifest);
        assert!(
            manifest.is_object()
                && !model::str_field(manifest, "/apiVersion").is_empty()
                && !kind.is_empty()
                && !name.is_empty()
                && !namespace.is_empty()
                && *manifest == clean(manifest)
                && identities.insert((group, kind, namespace, name)),
            "rendered manifests require distinct namespaced identities without server-owned state"
        );
    }
}

pub fn initialize(service: &mut ServiceState, _now: i64) {
    let existing: Vec<RenderedRevision> = service
        .store
        .collection(SOURCES)
        .into_iter()
        .flat_map(|rows| rows.values())
        .map(|row| {
            serde_json::from_value(row.clone()).expect("unsupported rendered-revision fixture")
        })
        .collect();
    let mut identities = BTreeSet::new();
    for revision in &existing {
        validate_revision(&service.store, revision);
        assert!(
            identities.insert((
                revision.repo_url.clone(),
                revision.path.clone(),
                revision.revision.clone()
            )),
            "rendered revision identity must be unique"
        );
    }
    let applications: Vec<(String, Value)> = service
        .store
        .collection("applications")
        .into_iter()
        .flat_map(|rows| rows.iter())
        .map(|(key, row)| (key.to_owned(), row.clone()))
        .collect();
    assert!(
        service
            .store
            .collection(HISTORY)
            .is_none_or(|rows| rows.is_empty()),
        "sync history is materialized from initial Application history"
    );
    for (app, application) in &applications {
        for (sequence, history) in application["status"]["history"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            service.store.collection_mut(HISTORY).insert(
                format!("{app}/initial-{sequence}"),
                json!({"app":app,"sequence":sequence,"history":history}),
            );
        }
    }
    for (app, application) in &applications {
        let source = &application["spec"]["source"];
        let revision = model::str_field(application, "/status/sync/revision");
        if revision.is_empty() || lookup(&service.store, source, revision).is_ok() {
            continue;
        }
        let mut manifests = Vec::new();
        let mut complete = true;
        for resource in model::app_resources(&service.store, app)
            .into_iter()
            .filter(|resource| resource["managed"] == true)
        {
            if resource["requiresPruning"] == true {
                continue;
            }
            if let Some(target) = resource
                .get("targetManifest")
                .filter(|target| !target.is_null())
            {
                manifests.push(clean(target));
            } else if resource["status"] == "Synced" {
                manifests.push(clean(&resource["manifest"]));
            } else {
                complete = false;
            }
        }
        if complete {
            let rendered = RenderedRevision {
                repo_url: model::str_field(source, "/repoURL").to_owned(),
                path: model::str_field(source, "/path").to_owned(),
                revision: revision.to_owned(),
                manifests,
            };
            validate_revision(&service.store, &rendered);
            service.store.collection_mut(SOURCES).insert(
                format!("{app}@{revision}"),
                serde_json::to_value(rendered).expect("rendered revision serializes"),
            );
        }
    }
    for revision in revisions(&service.store) {
        for manifest in revision.manifests {
            let (group, kind, namespace, name) = identity(&manifest);
            let mut row = json!({
                "repoURL":revision.repo_url,"path":revision.path,"revision":revision.revision,
                "group":group,"kind":kind,"namespace":namespace,"name":name,"manifest":manifest
            });
            row["resource_identity"] = json!(
                serde_json::to_string(&identity(&row["manifest"]))
                    .expect("desired identity serializes")
            );
            let mut content = clean(&row["manifest"]);
            content["metadata"] = json!({"name":row["name"],"namespace":row["namespace"]});
            row["content_digest"] = json!(mock_core::provenance::canonical_sha256(&content));
            let key = mock_core::provenance::canonical_sha256(&row);
            assert!(
                service
                    .store
                    .collection_mut(DESIRED)
                    .insert(key, row)
                    .is_none(),
                "desired resource identities must be unique"
            );
        }
    }
    let mut facts: BTreeMap<(String, String), Option<RuntimeImage>> = BTreeMap::new();
    for resource in service
        .store
        .collection("resources")
        .into_iter()
        .flat_map(|rows| rows.values())
    {
        if resource["kind"] != "Pod" {
            continue;
        }
        for container in resource["manifest"]["status"]["containerStatuses"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(ready) = container["ready"].as_bool() else {
                continue;
            };
            let image = model::str_field(container, "/image");
            let app = model::str_field(resource, "/app");
            if image.is_empty() || app.is_empty() {
                continue;
            }
            let reason = model::str_field(container, "/state/waiting/reason").to_owned();
            let fact = RuntimeImage {
                app: app.to_owned(),
                image: image.to_owned(),
                ready,
                pullable: !matches!(reason.as_str(), "ImagePullBackOff" | "ErrImagePull"),
                reason,
            };
            facts
                .entry((app.to_owned(), image.to_owned()))
                .and_modify(|observed| {
                    if observed.as_ref() != Some(&fact) {
                        *observed = None;
                    }
                })
                .or_insert(Some(fact));
        }
    }
    let explicit: Vec<RuntimeImage> = service
        .store
        .collection(IMAGES)
        .into_iter()
        .flat_map(|rows| rows.values())
        .map(|row| serde_json::from_value(row.clone()).expect("unsupported runtime-image fixture"))
        .collect();
    let mut image_ids = BTreeSet::new();
    for fact in explicit {
        assert!(
            !fact.image.is_empty()
                && model::get_app(&service.store, &fact.app).is_some()
                && (!fact.ready || fact.pullable)
                && image_ids.insert((fact.app.clone(), fact.image.clone())),
            "runtime image facts require distinct app/image identities and coherent startup outcomes"
        );
        facts.remove(&(fact.app, fact.image));
    }
    for (_, fact) in facts {
        if let Some(fact) = fact {
            service.store.collection_mut(IMAGES).insert(
                format!("{}/{}", fact.app, fact.image),
                serde_json::to_value(fact).expect("runtime image serializes"),
            );
        }
    }
    let projected: BTreeMap<_, _> = service
        .store
        .collection("resources")
        .into_iter()
        .flat_map(|rows| rows.iter())
        .filter_map(|(key, resource)| {
            let app = resource["app"]
                .as_str()
                .and_then(|key| model::get_app(&service.store, key))?;
            let destination = cluster(&service.store, app)?;
            let desired = current(&service.store, app).ok();
            let target = desired.as_ref().and_then(|revision| {
                revision
                    .manifests
                    .iter()
                    .find(|manifest| identity(manifest) == identity(&resource["manifest"]))
            });
            Some((
                key.to_owned(),
                (
                    destination,
                    target.map(|manifest| clean(&resource["manifest"]) == *manifest),
                    desired.is_some(),
                    app["metadata"]["uid"].clone(),
                ),
            ))
        })
        .collect();
    let resources = service.store.collection_mut("resources");
    let keys: Vec<String> = resources.iter().map(|(key, _)| key.to_owned()).collect();
    for key in keys {
        let resource = resources.get_mut(&key).expect("resource exists");
        if let Some((cluster, target_equal, source_known, application_uid)) = projected.get(&key) {
            assert!(
                resource
                    .get("cluster")
                    .is_none_or(|existing| existing == cluster),
                "resource cluster must agree with its owning Application"
            );
            resource["cluster"] = json!(cluster);
            resource["application_uid"] = application_uid.clone();
            if resource["managed"] == true && *source_known {
                resource["requiresPruning"] = json!(target_equal.is_none());
                resource["status"] = json!(if *target_equal == Some(true) {
                    "Synced"
                } else {
                    "OutOfSync"
                });
            }
        }
        resource
            .as_object_mut()
            .expect("resource object")
            .shift_remove("targetManifest");
        if resource["kind"] == "Deployment" {
            let generation = resource["manifest"]["metadata"]["generation"]
                .as_u64()
                .or_else(|| resource["manifest"]["status"]["observedGeneration"].as_u64())
                .unwrap_or(1)
                .max(1);
            resource["manifest"]["metadata"]["generation"] = json!(generation);
            if resource["manifest"]["status"]["observedGeneration"].is_null() {
                resource["manifest"]["status"]["observedGeneration"] = json!(generation);
            }
        }
        if resource["kind"] == "ReplicaSet"
            && let Some(hash) =
                resource["manifest"]["spec"]["selector"]["matchLabels"]["pod-template-hash"]
                    .as_str()
                    .map(str::to_owned)
        {
            resource["manifest"]["spec"]["template"]["metadata"]["labels"]["pod-template-hash"] =
                json!(hash);
        }
        if resource["kind"] == "ReplicaSet"
            && resource["manifest"]["metadata"]["generation"].is_null()
        {
            resource["manifest"]["metadata"]["generation"] = json!(1);
        }
        refresh_resource(resource);
    }
}
