//! The Argo CD world model over the store.
//!
//! Collections (all grader-addressable):
//! - `applications`: `<app namespace>/<name>` -> v1alpha1 Application. `status.resources` and
//!   `status.summary` are derived from `resources` when served.
//! - `projects`: `<name>` -> AppProject.
//! - `clusters`: `<name>` -> Cluster.
//! - `resources`: `<uid>` -> a live Kubernetes object in an application's tree: the
//!   ResourceNode fields (`group`, `version`, `kind`, `namespace`, `name`, `uid`, `parentRefs`,
//!   `info`, `networkingInfo`, `images`, `health`, `createdAt`, `resourceVersion`) plus
//!   `app` (application key), `managed` (top-level, in git), `status` (Synced/OutOfSync),
//!   `requiresPruning`, `manifest` (live object) and optional `targetManifest`.
//! - `events`: `<uid>` -> core/v1 Event (applications and their resources).
//! - `logs`: `<pod>/<container>` -> `{podName, namespace, container, lines: [{timeStamp, content}]}`.
//! - `revisions`: `<sha>` -> `{repoURL, sha, refs, message, author, date, health?}`, the git
//!   commits the repo server can resolve. `health` is the application health once synced.

use std::sync::LazyLock;

use mock_core::{
    store::Store,
    world::{ServiceState, WorldState},
};
use serde_json::{Map, Value, json};

use crate::{NAME, error::ArgoError, rendered, workload};

pub const DEFAULT_APP_NAMESPACE: &str = "argocd";
const DEFAULT_HISTORY_LIMIT: usize = 10;

static EMPTY: LazyLock<Store> = LazyLock::new(Store::default);

pub fn store(world: &WorldState) -> &Store {
    world.service(NAME).map_or(&EMPTY, |s| &s.store)
}

pub fn app_key(namespace: Option<&str>, name: &str) -> String {
    let ns = namespace
        .filter(|n| !n.is_empty())
        .unwrap_or(DEFAULT_APP_NAMESPACE);
    format!("{ns}/{name}")
}

pub fn get_app<'a>(store: &'a Store, key: &str) -> Option<&'a Value> {
    store.collection("applications").and_then(|c| c.get(key))
}

pub fn require_app<'a>(store: &'a Store, key: &str) -> Result<&'a Value, ArgoError> {
    get_app(store, key).ok_or_else(|| ArgoError::app_not_found(key_name(key)))
}

fn key_name(key: &str) -> &str {
    key.split_once('/').map_or(key, |(_, n)| n)
}

pub fn str_field<'a>(v: &'a Value, pointer: &str) -> &'a str {
    v.pointer(pointer).and_then(Value::as_str).unwrap_or("")
}

/// Live objects of one application, in dataset order.
pub fn app_resources<'a>(store: &'a Store, key: &str) -> Vec<&'a Value> {
    store
        .collection("resources")
        .map(|c| c.values().filter(|r| r["app"] == key).collect())
        .unwrap_or_default()
}

/// A resource of the application matching group/kind/namespace/name (version is not compared).
pub fn find_resource<'a>(
    store: &'a Store,
    key: &str,
    group: &str,
    kind: &str,
    namespace: &str,
    name: &str,
) -> Option<&'a Value> {
    app_resources(store, key).into_iter().find(|r| {
        str_field(r, "/group") == group
            && str_field(r, "/kind") == kind
            && str_field(r, "/namespace") == namespace
            && str_field(r, "/name") == name
    })
}

/// `<uid>` plus the uids of every resource below it in the tree.
pub fn descendants(store: &Store, root_uid: &str) -> Vec<String> {
    let all: Vec<&Value> = store
        .collection("resources")
        .map(|c| c.values().collect())
        .unwrap_or_default();
    let mut out = vec![root_uid.to_owned()];
    let mut i = 0;
    while i < out.len() {
        let parent = out[i].clone();
        for r in &all {
            let is_child = r["parentRefs"]
                .as_array()
                .is_some_and(|refs| refs.iter().any(|p| p["uid"] == parent.as_str()));
            let uid = str_field(r, "/uid");
            if is_child && !out.iter().any(|u| u == uid) {
                out.push(uid.to_owned());
            }
        }
        i += 1;
    }
    out
}

/// A ResourceNode as served in the resource tree.
pub fn tree_node(r: &Value) -> Value {
    let mut node = Map::new();
    for field in [
        "version",
        "kind",
        "namespace",
        "name",
        "uid",
        "group",
        "parentRefs",
        "info",
        "networkingInfo",
        "resourceVersion",
        "images",
        "health",
        "createdAt",
    ] {
        match r.get(field) {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) if s.is_empty() => {}
            Some(v) => {
                node.insert(field.to_owned(), v.clone());
            }
        }
    }
    Value::Object(node)
}

/// A ResourceStatus entry of `status.resources`.
fn resource_status(r: &Value) -> Value {
    let mut out = Map::new();
    for field in ["group", "version", "kind", "namespace", "name", "status"] {
        if let Some(Value::String(s)) = r.get(field)
            && !s.is_empty()
        {
            out.insert(field.to_owned(), json!(s));
        }
    }
    if let Some(health) = r.get("health").filter(|h| !h.is_null()) {
        out.insert("health".to_owned(), health.clone());
    }
    if r["requiresPruning"] == true {
        out.insert("requiresPruning".to_owned(), json!(true));
    }
    Value::Object(out)
}

/// The Application as the API serves it, with derived `status.resources` and `status.summary`.
pub fn render_app(store: &Store, key: &str) -> Option<Value> {
    let mut app = get_app(store, key)?.clone();
    let resources = app_resources(store, key);
    let managed: Vec<Value> = resources
        .iter()
        .filter(|r| r["managed"] == true)
        .map(|r| resource_status(r))
        .collect();
    let mut images: Vec<String> = Vec::new();
    let mut urls: Vec<String> = Vec::new();
    for r in &resources {
        for image in r["images"].as_array().into_iter().flatten() {
            if let Some(s) = image.as_str()
                && !images.iter().any(|i| i == s)
            {
                images.push(s.to_owned());
            }
        }
        for url in r
            .pointer("/networkingInfo/externalURLs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(s) = url.as_str()
                && !urls.iter().any(|u| u == s)
            {
                urls.push(s.to_owned());
            }
        }
    }
    let mut summary = Map::new();
    if !urls.is_empty() {
        summary.insert("externalURLs".to_owned(), json!(urls));
    }
    if !images.is_empty() {
        summary.insert("images".to_owned(), json!(images));
    }
    let status = app
        .as_object_mut()?
        .entry("status")
        .or_insert_with(|| json!({}));
    if let Some(status) = status.as_object_mut() {
        if !managed.is_empty() {
            status.insert("resources".to_owned(), Value::Array(managed));
        }
        status.insert("summary".to_owned(), Value::Object(summary));
    }
    Some(app)
}

fn normalize_repo(url: &str) -> &str {
    let url = url.trim_end_matches('/');
    url.strip_suffix(".git").unwrap_or(url)
}

/// The commit a revision (branch, tag, `HEAD`, full or short SHA) resolves to in `repo_url`.
pub fn resolve_revision<'a>(store: &'a Store, repo_url: &str, revision: &str) -> Option<&'a Value> {
    let commits: Vec<&Value> = store
        .collection("revisions")
        .map(|c| {
            c.values()
                .filter(|r| normalize_repo(str_field(r, "/repoURL")) == normalize_repo(repo_url))
                .collect()
        })
        .unwrap_or_default();
    let has_ref = |c: &&&Value, name: &str| {
        c["refs"]
            .as_array()
            .is_some_and(|refs| refs.iter().any(|r| r == name))
    };
    let rev = revision.trim();
    if rev.is_empty() || rev == "HEAD" {
        for name in ["HEAD", "main", "master"] {
            if let Some(c) = commits.iter().find(|c| has_ref(c, name)) {
                return Some(c);
            }
        }
        return None;
    }
    if let Some(c) = commits.iter().find(|c| has_ref(c, rev)) {
        return Some(c);
    }
    let rev = rev.to_ascii_lowercase();
    if rev.len() >= 7 && rev.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return commits
            .iter()
            .find(|c| str_field(c, "/sha").starts_with(&rev))
            .copied();
    }
    None
}

pub fn repo_known(store: &Store, repo_url: &str) -> bool {
    store.collection("revisions").is_some_and(|c| {
        c.values()
            .any(|r| normalize_repo(str_field(r, "/repoURL")) == normalize_repo(repo_url))
    })
}

/// `*`-only glob, as Argo CD uses for project source repos and destinations.
pub fn glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some((b'*', rest)) => (0..=t.len()).any(|i| go(rest, &t[i..])),
            Some((c, rest)) => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

pub fn cluster_by<'a>(store: &'a Store, field: &str, value: &str) -> Option<&'a Value> {
    store
        .collection("clusters")
        .and_then(|c| c.values().find(|cl| cl[field] == value))
}

/// Validates an application spec the way the API server does before create/update.
pub fn validate_spec(
    store: &Store,
    name: &str,
    app_ns: &str,
    spec: &Value,
) -> Result<(), ArgoError> {
    if !spec.is_object() || !spec["source"].is_object() || !spec["destination"].is_object() {
        return Err(ArgoError::invalid(
            "Application spec, source, and destination must be objects.",
        ));
    }
    if spec["destination"]
        .as_object()
        .expect("destination object")
        .keys()
        .any(|field| !matches!(field.as_str(), "server" | "name" | "namespace"))
    {
        return Err(ArgoError::invalid("Unknown Application destination field."));
    }
    if ["sources", "ignoreDifferences"]
        .iter()
        .any(|field| spec.get(*field).is_some())
        || ["helm", "kustomize", "plugin", "directory", "chart", "ref"]
            .iter()
            .any(|field| spec["source"].get(*field).is_some())
    {
        return Err(ArgoError::unsupported(
            "This Application source or comparison configuration is not implemented.",
        ));
    }
    if let Some(policy) = spec
        .pointer("/syncPolicy/automated")
        .filter(|value| !value.is_null())
        && (!policy.is_object()
            || ["enabled", "prune", "selfHeal", "allowEmpty"]
                .iter()
                .any(|field| {
                    policy
                        .get(*field)
                        .is_some_and(|value| !value.is_null() && !value.is_boolean())
                }))
    {
        return Err(ArgoError::invalid(
            "Automated sync policy fields must be boolean or null.",
        ));
    }
    let invalid = |msg: String| {
        ArgoError::invalid(format!(
            "application spec for {name} is invalid: InvalidSpecError: {msg}"
        ))
    };
    let project_name = str_field(spec, "/project");
    let Some(project) = store
        .collection("projects")
        .and_then(|c| c.get(project_name))
    else {
        return Err(invalid(format!(
            "application references project {project_name} which does not exist"
        )));
    };
    if app_ns != DEFAULT_APP_NAMESPACE {
        let allowed = project
            .pointer("/spec/sourceNamespaces")
            .and_then(Value::as_array)
            .is_some_and(|ns| {
                ns.iter()
                    .any(|n| n.as_str().is_some_and(|n| glob(n, app_ns)))
            });
        if !allowed {
            return Err(ArgoError::permission_denied(format!(
                "app is not allowed in project \"{project_name}\", or the project does not exist"
            )));
        }
    }

    let dest_server = str_field(spec, "/destination/server");
    let dest_name = str_field(spec, "/destination/name");
    let dest_ns = str_field(spec, "/destination/namespace");
    if !dest_server.is_empty() && !dest_name.is_empty() {
        return Err(invalid(format!(
            "application destination can't have both name and server defined: {dest_name} {dest_server}"
        )));
    }
    let cluster = if !dest_name.is_empty() {
        cluster_by(store, "name", dest_name).ok_or_else(|| {
            invalid(format!(
                "Unable to find destination server: there are no clusters with this name: {dest_name}"
            ))
        })?
    } else if !dest_server.is_empty() {
        cluster_by(store, "server", dest_server).ok_or_else(|| {
            invalid(format!(
                "Unable to find destination server: there are no clusters with this URL: {dest_server}"
            ))
        })?
    } else {
        return Err(invalid(
            "Destination server missing from app spec".to_owned(),
        ));
    };
    let server = str_field(cluster, "/server");
    let cluster_name = str_field(cluster, "/name");
    if let Some(existing) = get_app(store, &format!("{app_ns}/{name}"))
        && rendered::cluster(store, existing).as_deref() != Some(server)
    {
        return Err(ArgoError::unsupported(
            "Moving an existing Application between clusters is not implemented.",
        ));
    }

    let repo = str_field(spec, "/source/repoURL");
    let repo_ok = project
        .pointer("/spec/sourceRepos")
        .and_then(Value::as_array)
        .is_some_and(|repos| {
            repos
                .iter()
                .any(|r| r.as_str().is_some_and(|r| glob(r, repo)))
        });
    if !repo_ok {
        return Err(invalid(format!(
            "application repo {repo} is not permitted in project '{project_name}'"
        )));
    }
    let dest_ok = project
        .pointer("/spec/destinations")
        .and_then(Value::as_array)
        .is_some_and(|dests| {
            dests.iter().any(|d| {
                let ns_ok = glob(str_field(d, "/namespace"), dest_ns);
                let server_ok = d["server"].as_str().is_some_and(|p| glob(p, server))
                    || d["name"].as_str().is_some_and(|p| glob(p, cluster_name));
                ns_ok && server_ok
            })
        });
    if !dest_ok {
        return Err(invalid(format!(
            "application destination server '{server}' and namespace '{dest_ns}' do not match any of the allowed destinations in project '{project_name}'"
        )));
    }

    if !repo_known(store, repo) {
        return Err(invalid(
            "repository not accessible: repository not found".to_owned(),
        ));
    }
    let target = str_field(spec, "/source/targetRevision");
    if resolve_revision(store, repo, target).is_none() {
        return Err(invalid(format!(
            "Unable to generate manifests in {}: rpc error: code = Unknown desc = unable to resolve '{}' to a commit SHA",
            str_field(spec, "/source/path"),
            if target.is_empty() { "HEAD" } else { target }
        )));
    }
    Ok(())
}

pub fn next_resource_version(svc: &mut ServiceState) -> String {
    svc.ids.next_int("resourceVersion", 2_000_000).to_string()
}

/// Records an event on the application (as the API server or controller would).
pub fn app_event(
    svc: &mut ServiceState,
    now: &str,
    app: &Value,
    component: &str,
    reason: &str,
    message: &str,
) {
    let uid = svc.ids.next_uuid("event").to_string();
    let suffix: String = uid.chars().filter(|c| *c != '-').take(16).collect();
    let rv = next_resource_version(svc);
    let name = str_field(app, "/metadata/name");
    let ns = str_field(app, "/metadata/namespace");
    let event = json!({
        "metadata": {
            "name": format!("{name}.{suffix}"),
            "namespace": ns,
            "uid": uid,
            "resourceVersion": rv,
            "creationTimestamp": now,
        },
        "involvedObject": {
            "kind": "Application",
            "namespace": ns,
            "name": name,
            "uid": app.pointer("/metadata/uid").cloned().unwrap_or(Value::Null),
            "apiVersion": "argoproj.io/v1alpha1",
            "resourceVersion": app.pointer("/metadata/resourceVersion").cloned().unwrap_or(Value::Null),
        },
        "reason": reason,
        "message": message,
        "source": { "component": component },
        "firstTimestamp": now,
        "lastTimestamp": now,
        "count": 1,
        "type": "Normal",
        "eventTime": null,
        "reportingComponent": "",
        "reportingInstance": "",
    });
    svc.store.collection_mut("events").insert(uid, event);
}

#[derive(Debug, Clone, Default)]
pub struct SyncRequest {
    pub revision: Option<String>,
    pub prune: bool,
    pub dry_run: bool,
    pub sync_options: Option<Value>,
    /// Started by the controller's automated sync rather than a user.
    pub automated: bool,
}

pub fn automation_enabled(app: &Value) -> bool {
    app.pointer("/spec/syncPolicy/automated")
        .is_some_and(|policy| {
            policy.is_object() && policy.get("enabled") != Some(&Value::Bool(false))
        })
}

pub fn checked<T>(world: &mut WorldState, result: Result<T, ArgoError>) -> Result<T, ArgoError> {
    if let Err(error) = &result
        && error.capability_gap.is_some()
    {
        let operation =
            mock_core::events::current().expect("capability failures have operation context");
        world.fail_operation(
            &operation,
            "unsupported_argocd_operation",
            error.capability_gap.as_deref().expect("capability reason"),
        );
    }
    result
}

fn kubectl_name(r: &Value) -> String {
    let kind = str_field(r, "/kind").to_ascii_lowercase();
    let group = str_field(r, "/group");
    let name = str_field(r, "/name");
    if group.is_empty() {
        format!("{kind}/{name}")
    } else {
        format!("{kind}.{group}/{name}")
    }
}

fn deployed_revision(app: &Value) -> Option<&str> {
    app.pointer("/status/history")
        .and_then(Value::as_array)
        .and_then(|h| h.last())
        .and_then(|h| h["revision"].as_str())
}

/// Sync status of `key` from its target revision and managed resources.
fn sync_status(store: &Store, key: &str, app: &Value, deployed: Option<&str>) -> (String, Value) {
    let repo = str_field(app, "/spec/source/repoURL");
    let target = str_field(app, "/spec/source/targetRevision");
    let target_sha = resolve_revision(store, repo, target).map(|c| str_field(c, "/sha").to_owned());
    let drifted = app_resources(store, key)
        .iter()
        .any(|r| r["managed"] == true && r["status"] == "OutOfSync");
    let status = if !drifted && target_sha.is_some() && target_sha.as_deref() == deployed {
        "Synced"
    } else {
        "OutOfSync"
    };
    (
        status.to_owned(),
        target_sha.map_or(Value::Null, Value::String),
    )
}

fn set_sync(app: &mut Value, status: &str, revision: Value) {
    let source = app.pointer("/spec/source").cloned().unwrap_or(Value::Null);
    let destination = app
        .pointer("/spec/destination")
        .cloned()
        .unwrap_or(Value::Null);
    let mut sync = json!({
        "status": status,
        "comparedTo": { "source": source, "destination": destination },
    });
    if !revision.is_null() {
        sync["revision"] = revision;
    }
    app["status"]["sync"] = sync;
}

/// After a spec change: recompare against the new target and auto-sync if enabled.
pub fn reconcile(svc: &mut ServiceState, now: &str, key: &str) -> Result<(), ArgoError> {
    let Some(mut app) = get_app(&svc.store, key).cloned() else {
        return Ok(());
    };
    let before = str_field(&app, "/status/sync/status").to_owned();
    let (status, revision) = sync_status(&svc.store, key, &app, deployed_revision(&app));
    set_sync(&mut app, &status, revision);
    app["status"]["reconciledAt"] = json!(now);
    if before != status && !before.is_empty() {
        app_event(
            svc,
            now,
            &app,
            "argocd-application-controller",
            "ResourceUpdated",
            &format!("Updated sync status: {before} -> {status}"),
        );
    }
    let automated = automation_enabled(&app);
    let prune = app
        .pointer("/spec/syncPolicy/automated/prune")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    svc.store
        .collection_mut("applications")
        .insert(key.to_owned(), app);
    if automated
        && status == "OutOfSync"
        && get_app(&svc.store, key).is_some_and(|app| !app["operation"].is_object())
    {
        sync(
            svc,
            now,
            key,
            SyncRequest {
                prune,
                automated: true,
                ..SyncRequest::default()
            },
        )?;
    }
    Ok(())
}

/// Runs a sync operation to completion (the mock's controller finishes it immediately).
pub fn sync(
    svc: &mut ServiceState,
    now: &str,
    key: &str,
    req: SyncRequest,
) -> Result<(), ArgoError> {
    let mut app = require_app(&svc.store, key)?.clone();
    let options = req
        .sync_options
        .as_ref()
        .or_else(|| app.pointer("/spec/syncPolicy/syncOptions"));
    if options
        .into_iter()
        .flat_map(|options| options.as_array().into_iter().flatten())
        .any(|option| {
            !matches!(
                option.as_str(),
                Some(
                    "CreateNamespace=true"
                        | "CreateNamespace=false"
                        | "ApplyOutOfSyncOnly=true"
                        | "ApplyOutOfSyncOnly=false"
                        | "Validate=true"
                        | "Validate=false"
                )
            )
        })
    {
        return Err(ArgoError::unsupported(
            "This operation sync option is not implemented.",
        ));
    }
    let phase = str_field(&app, "/status/operationState/phase");
    if app["operation"].is_object() || phase == "Running" || phase == "Terminating" {
        return Err(ArgoError::failed_precondition(
            "another operation is already in progress",
        ));
    }
    let target = str_field(&app, "/spec/source/targetRevision").to_owned();
    let automated_policy = automation_enabled(&app);
    if let Some(rev) = req.revision.as_deref()
        && !req.automated
        && automated_policy
        && !req.dry_run
        && rev != if target.is_empty() { "HEAD" } else { &target }
    {
        return Err(ArgoError::failed_precondition(format!(
            "Cannot sync to {rev}: auto-sync currently set to {target}"
        )));
    }
    let repo = str_field(&app, "/spec/source/repoURL").to_owned();
    let wanted = req.revision.clone().unwrap_or_else(|| target.clone());
    let Some(commit) = resolve_revision(&svc.store, &repo, &wanted).cloned() else {
        let shown = if wanted.is_empty() { "HEAD" } else { &wanted };
        return Err(ArgoError::failed_precondition(format!(
            "rpc error: code = Unknown desc = unable to resolve '{shown}' to a commit SHA"
        )));
    };
    let sha = str_field(&commit, "/sha").to_owned();
    let source = app.pointer("/spec/source").cloned().unwrap_or(Value::Null);
    let rendered = rendered::lookup(&svc.store, &source, &sha)?;
    workload::validate(svc, key, &rendered.manifests)?;
    if req.automated
        && req.prune
        && rendered.manifests.is_empty()
        && app.pointer("/spec/syncPolicy/automated/allowEmpty") != Some(&Value::Bool(true))
    {
        return Ok(());
    }
    let initiated_by = if req.automated {
        json!({ "automated": true })
    } else {
        json!({ "username": "admin" })
    };
    let mut operation_sync = json!({
        "revision":sha,"source":source,"prune":req.prune,"dryRun":req.dry_run
    });
    if let Some(options) = req.sync_options {
        operation_sync["syncOptions"] = options;
    }
    app["operation"] = json!({"sync":operation_sync,"initiatedBy":initiated_by,"retry":{}});
    app["status"]
        .as_object_mut()
        .expect("application status")
        .shift_remove("operationState");
    app["metadata"]["resourceVersion"] = json!(next_resource_version(svc));
    app_event(
        svc,
        now,
        &app,
        "argocd-server",
        "OperationStarted",
        &format!("initiated sync to {sha}"),
    );
    svc.store.collection_mut("applications").insert(key, app);
    Ok(())
}

pub fn observe(world: &mut WorldState, key: &str) -> Result<(), ArgoError> {
    let now = world.clock.now_rfc3339();
    let service = world.service_mut(NAME);
    let app = require_app(&service.store, key)?.clone();
    let rollout_pending = app_resources(&service.store, key).iter().any(|resource| {
        resource["kind"] == "Deployment"
            && resource["manifest"]["metadata"]["generation"].as_u64()
                != resource["manifest"]["status"]["observedGeneration"].as_u64()
    });
    if !app["operation"].is_object() && !rollout_pending {
        return Ok(());
    }
    let mut staged = service.clone();
    let result = if app["operation"].is_object() {
        if app["status"]["operationState"]["phase"] != "Running" {
            let mut app = app.clone();
            app["status"]["operationState"] = json!({
                "operation":app["operation"],"phase":"Running","startedAt":now,
                "syncResult":{"revision":app["operation"]["sync"]["revision"],"source":app["operation"]["sync"]["source"]}
            });
            app["metadata"]["resourceVersion"] = json!(next_resource_version(&mut staged));
            staged.store.collection_mut("applications").insert(key, app);
            Ok(())
        } else {
            finish_sync(&mut staged, &now, key)
        }
    } else {
        workload::observe(&mut staged, &now, key).map(|()| {
            let mut app = require_app(&staged.store, key)
                .expect("observed app")
                .clone();
            app["status"]["health"] = workload::health(&staged.store, key);
            app["metadata"]["resourceVersion"] = json!(next_resource_version(&mut staged));
            staged.store.collection_mut("applications").insert(key, app);
        })
    };
    if let Err(error) = result {
        if let Some(reason) = &error.capability_gap {
            let operation = mock_core::events::current()
                .expect("controller observation has an operation context");
            world.fail_operation(&operation, "unsupported_argocd_operation", reason);
        }
        return Err(error);
    }
    *world.service_mut(NAME) = staged;
    Ok(())
}

fn finish_sync(svc: &mut ServiceState, now: &str, key: &str) -> Result<(), ArgoError> {
    let mut app = require_app(&svc.store, key)?.clone();
    let operation = app["operation"].clone();
    let sha = str_field(&operation, "/sync/revision").to_owned();
    let source = operation["sync"]["source"].clone();
    let req = SyncRequest {
        revision: Some(sha.clone()),
        prune: operation["sync"]["prune"] == true,
        dry_run: operation["sync"]["dryRun"] == true,
        sync_options: operation["sync"].get("syncOptions").cloned(),
        automated: operation["initiatedBy"]["automated"] == true,
    };
    let rendered = rendered::lookup(&svc.store, &source, &sha)?;
    let initiated_by = operation["initiatedBy"].clone();

    let managed: Vec<Value> = app_resources(&svc.store, key)
        .into_iter()
        .filter(|r| r["managed"] == true)
        .cloned()
        .collect();
    let dry = if req.dry_run { " (dry run)" } else { "" };
    let mut results = Vec::new();
    for r in &managed {
        let requires_pruning = !rendered
            .manifests
            .iter()
            .any(|target| rendered::identity(target) == rendered::identity(&r["manifest"]));
        let (status, message) = if requires_pruning {
            if req.prune {
                ("Pruned", format!("pruned{dry}"))
            } else {
                ("PruneSkipped", "ignored (requires pruning)".to_owned())
            }
        } else if r["status"] == "Synced" {
            ("Synced", format!("{} unchanged{dry}", kubectl_name(r)))
        } else {
            ("Synced", format!("{} configured{dry}", kubectl_name(r)))
        };
        let mut result = json!({
            "group": str_field(r, "/group"),
            "version": str_field(r, "/version"),
            "kind": str_field(r, "/kind"),
            "namespace": str_field(r, "/namespace"),
            "name": str_field(r, "/name"),
            "status": status,
            "message": message,
            "hookPhase": "Running",
            "syncPhase": "Sync",
        });
        if req.dry_run {
            result["hookPhase"] = json!("Succeeded");
        }
        results.push(result);
    }

    let mut operation_sync = json!({ "revision": sha });
    if req.prune {
        operation_sync["prune"] = json!(true);
    }
    if req.dry_run {
        operation_sync["dryRun"] = json!(true);
    }
    if let Some(options) = &req.sync_options {
        operation_sync["syncOptions"] = options.clone();
    }
    let mut operation = json!({ "sync": operation_sync, "initiatedBy": initiated_by, "retry": {} });
    if req.automated {
        operation["sync"]["prune"] = json!(req.prune);
    }

    let before_sync = str_field(&app, "/status/sync/status").to_owned();
    let before_health = str_field(&app, "/status/health/status").to_owned();
    let mut pending_delete = false;
    if !req.dry_run {
        pending_delete = workload::apply(svc, now, key, &rendered.manifests, req.prune)?;
        app["status"]["health"] = workload::health(&svc.store, key);
    }
    let succeeded =
        req.dry_run || !pending_delete && app["status"]["health"]["status"] == "Healthy";
    if !req.dry_run && succeeded {
        let limit = app
            .pointer("/spec/revisionHistoryLimit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_HISTORY_LIMIT, |l| l as usize);
        if !app["status"]["history"].is_array() {
            app["status"]["history"] = json!([]);
        }
        if let Some(history) = app["status"]["history"].as_array() {
            let id = history
                .iter()
                .filter_map(|h| h["id"].as_i64())
                .max()
                .unwrap_or(-1)
                + 1;
            let entry = json!({
                "revision": sha,
                "deployedAt": now,
                "id": id,
                "source": source,
                "deployStartedAt": now,
                "initiatedBy": initiated_by,
            });
            let mut all: Vec<Value> = svc
                .store
                .collection(rendered::HISTORY)
                .into_iter()
                .flat_map(|rows| rows.values())
                .filter(|row| row["app"] == key)
                .cloned()
                .collect();
            let sequence = all
                .iter()
                .filter_map(|row| row["sequence"].as_u64())
                .max()
                .map_or(0, |value| value + 1);
            let uid = svc.ids.next_uuid("syncHistory").to_string();
            let row = json!({"app":key,"sequence":sequence,"history":entry});
            svc.store
                .collection_mut(rendered::HISTORY)
                .insert(format!("{key}/{uid}"), row.clone());
            all.push(row);
            all.sort_by_key(|row| row["sequence"].as_u64().expect("history sequence"));
            app["status"]["history"] = Value::Array(
                all.iter()
                    .skip(all.len().saturating_sub(limit))
                    .map(|row| row["history"].clone())
                    .collect(),
            );
        }
    }
    if !req.dry_run {
        let deployed = if succeeded {
            Some(sha.as_str())
        } else {
            deployed_revision(&app)
        };
        let (status, revision) = sync_status(&svc.store, key, &app, deployed);
        set_sync(&mut app, &status, revision);
    }

    app["status"]["operationState"] = json!({
        "operation": operation,
        "phase": if pending_delete {"Running"} else if succeeded {"Succeeded"} else {"Failed"},
        "message": if pending_delete {"Waiting for resource finalizers"} else if succeeded {"successfully synced (all tasks run)"} else {"Workloads did not reach readiness"},
        "syncResult": { "resources": results, "revision": sha, "source": source },
        "startedAt": now,
    });
    if !pending_delete {
        app["status"]["operationState"]["finishedAt"] = json!(now);
    }
    app["status"]["reconciledAt"] = json!(now);
    if !pending_delete && let Some(obj) = app.as_object_mut() {
        obj.remove("operation");
    }

    app_event(
        svc,
        now,
        &app,
        "argocd-application-controller",
        if pending_delete {
            "OperationProgressing"
        } else if succeeded {
            "OperationCompleted"
        } else {
            "OperationFailed"
        },
        &format!(
            "Sync operation to {sha}: {}",
            if pending_delete {
                "waiting for deletion"
            } else if succeeded {
                "succeeded"
            } else {
                "workload readiness failed"
            }
        ),
    );
    let after_sync = str_field(&app, "/status/sync/status").to_owned();
    if after_sync != before_sync {
        app_event(
            svc,
            now,
            &app,
            "argocd-application-controller",
            "ResourceUpdated",
            &format!("Updated sync status: {before_sync} -> {after_sync}"),
        );
    }
    let after_health = str_field(&app, "/status/health/status").to_owned();
    if after_health != before_health {
        app_event(
            svc,
            now,
            &app,
            "argocd-application-controller",
            "ResourceUpdated",
            &format!("Updated health status: {before_health} -> {after_health}"),
        );
    }
    app["metadata"]["resourceVersion"] = json!(next_resource_version(svc));
    svc.store
        .collection_mut("applications")
        .insert(key.to_owned(), app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matches_like_argocd() {
        assert!(glob("*", "anything"));
        assert!(glob(
            "https://github.com/acme/*",
            "https://github.com/acme/x"
        ));
        assert!(!glob("payments-*", "billing"));
        assert!(glob("payments-*", "payments-prod"));
        assert!(glob("", ""));
        assert!(!glob("", "x"));
    }
}
