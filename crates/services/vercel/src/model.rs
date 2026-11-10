//! Store layout, lookups and the Vercel response views shared by the REST routes and tools.
//!
//! Collections (dataset files under `dataset/vercel/`):
//! - `user`: one document keyed `viewer` (the token's owner).
//! - `teams`: keyed by team id (`team_...`).
//! - `projects`: keyed by project id (`prj_...`). Environment variables live in the
//!   project's `env` array, as in Vercel's project document.
//! - `deployments`: keyed by deployment id (`dpl_...`), stored in the v13 shape.
//! - `domains`: project domains keyed by domain name.
//! - `custom_environments`: keyed `<projectId>/<slug>`.
//! - `deployment_files`: keyed by deployment id, `{ "files": [<file tree>] }`.
//! - `env_by_target` (derived, for grading): keyed by project id,
//!   `{ "production": {KEY: value}, "preview": {...}, "development": {...} }`, with
//!   branch-scoped variables under `"preview:<branch>"`. Rebuilt after every env change.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use mock_core::world::{ServiceState, WorldState};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::NAME;

pub const USER: &str = "user";
pub const TEAMS: &str = "teams";
pub const PROJECTS: &str = "projects";
pub const DEPLOYMENTS: &str = "deployments";
pub const DOMAINS: &str = "domains";
pub const CUSTOM_ENVS: &str = "custom_environments";
pub const FILES: &str = "deployment_files";
pub const ENV_BY_TARGET: &str = "env_by_target";
pub const ENV_VARS: &str = "environment_variables";
pub const ENV_BINDINGS: &str = "environment_bindings";
pub const ALIASES: &str = "aliases";
pub const SOURCES: &str = "deployment_sources";

pub type Query = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub status: StatusCode,
    pub body: Value,
}

pub type ApiResult = Result<(StatusCode, Value), ApiError>;

/// Vercel's error envelope: `{"error": {"code": ..., "message": ...}}`.
pub fn api_error(status: StatusCode, code: &str, message: impl Into<String>) -> ApiError {
    ApiError {
        status,
        body: json!({ "error": { "code": code, "message": message.into() } }),
    }
}

pub fn bad_request(message: impl Into<String>) -> ApiError {
    api_error(StatusCode::BAD_REQUEST, "bad_request", message)
}

pub fn forbidden() -> ApiError {
    api_error(StatusCode::FORBIDDEN, "forbidden", "Not authorized")
}

pub fn project_not_found() -> ApiError {
    api_error(StatusCode::NOT_FOUND, "not_found", "Project not found")
}

pub fn deployment_not_found() -> ApiError {
    api_error(
        StatusCode::NOT_FOUND,
        "not_found",
        "The deployment could not be found.",
    )
}

/// Scenario time in epoch milliseconds, Vercel's timestamp unit.
pub fn now_ms(world: &WorldState) -> i64 {
    world.clock.now_unix() * 1000
}

pub fn svc(world: &mut WorldState) -> &mut ServiceState {
    world.service_mut(NAME)
}

pub fn docs(s: &ServiceState, collection: &str) -> Vec<(String, Value)> {
    s.store
        .collection(collection)
        .map(|c| c.iter().map(|(k, v)| (k.to_owned(), v.clone())).collect())
        .unwrap_or_default()
}

pub fn doc(s: &ServiceState, collection: &str, key: &str) -> Option<Value> {
    s.store
        .collection(collection)
        .and_then(|c| c.get(key))
        .cloned()
}

pub fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

pub fn int_of(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

pub fn q<'a>(query: &'a Query, key: &str) -> Option<&'a str> {
    query.get(key).map(String::as_str).filter(|s| !s.is_empty())
}

pub fn viewer(s: &ServiceState) -> Value {
    doc(s, USER, "viewer").unwrap_or_else(|| {
        json!({ "uid": "usr_toolbench", "email": "dev@example.com", "username": "toolbench", "name": "Tool Bench" })
    })
}

/// `{uid, email, username, githubLogin}` as Vercel embeds the creator in deployments.
pub fn creator(s: &ServiceState) -> Value {
    let v = viewer(s);
    json!({
        "uid": v["uid"],
        "email": v["email"],
        "username": v["username"],
        "githubLogin": v.get("githubLogin").cloned().unwrap_or(v["username"].clone()),
    })
}

/// The team a request is scoped to (`teamId` or `slug`), `None` for the token's default scope
/// (every team the token can see). Unknown teams are `403 forbidden`, as on Vercel.
pub fn scope(s: &ServiceState, query: &Query) -> Result<Option<String>, ApiError> {
    if let Some(id) = q(query, "teamId") {
        return find_team(s, id)
            .filter(|t| str_of(t, "id") == Some(id))
            .map(|_| Some(id.to_owned()))
            .ok_or_else(forbidden);
    }
    if let Some(slug) = q(query, "slug") {
        return find_team(s, slug)
            .and_then(|t| str_of(&t, "id").map(str::to_owned))
            .map(Some)
            .ok_or_else(forbidden);
    }
    Ok(None)
}

pub fn find_team(s: &ServiceState, id_or_slug: &str) -> Option<Value> {
    docs(s, TEAMS)
        .into_iter()
        .map(|(_, t)| t)
        .find(|t| str_of(t, "id") == Some(id_or_slug) || str_of(t, "slug") == Some(id_or_slug))
}

pub fn team_slug(s: &ServiceState, team_id: &str) -> String {
    find_team(s, team_id)
        .and_then(|t| str_of(&t, "slug").map(str::to_owned))
        .unwrap_or_else(|| viewer(s)["username"].as_str().unwrap_or("me").to_owned())
}

fn in_scope(project: &Value, scope: &Option<String>) -> bool {
    scope
        .as_deref()
        .is_none_or(|t| str_of(project, "accountId") == Some(t))
}

/// Project key by id or name within the scope (ids win over names).
pub fn find_project(s: &ServiceState, id_or_name: &str, scope: &Option<String>) -> Option<String> {
    let projects = docs(s, PROJECTS);
    projects
        .iter()
        .find(|(_, p)| str_of(p, "id") == Some(id_or_name) && in_scope(p, scope))
        .or_else(|| {
            projects
                .iter()
                .find(|(_, p)| str_of(p, "name") == Some(id_or_name) && in_scope(p, scope))
        })
        .map(|(k, _)| k.clone())
}

pub fn project_key(s: &ServiceState, id_or_name: &str, query: &Query) -> Result<String, ApiError> {
    let scope = scope(s, query)?;
    find_project(s, id_or_name, &scope).ok_or_else(project_not_found)
}

pub fn projects_in_scope(s: &ServiceState, scope: &Option<String>) -> Vec<Value> {
    docs(s, PROJECTS)
        .into_iter()
        .map(|(_, p)| p)
        .filter(|p| in_scope(p, scope))
        .collect()
}

// ───────────────────────────── deployments ─────────────────────────────

/// Deployments of a project, newest first.
pub fn project_deployments(s: &ServiceState, project_id: &str) -> Vec<Value> {
    let mut out: Vec<Value> = docs(s, DEPLOYMENTS)
        .into_iter()
        .map(|(_, d)| d)
        .filter(|d| str_of(d, "projectId") == Some(project_id))
        .collect();
    out.sort_by_key(|d| std::cmp::Reverse(int_of(d, "createdAt")));
    out
}

pub fn current_production(s: &ServiceState, project_id: &str) -> Option<Value> {
    let domains = production_domains(s, project_id);
    let first = domains.first()?;
    let binding = doc(s, ALIASES, first)?;
    let deployment_id = str_of(&binding, "deploymentId")?;
    if !domains.iter().all(|domain| {
        doc(s, ALIASES, domain).is_some_and(|binding| {
            str_of(&binding, "deploymentId") == Some(deployment_id)
                && str_of(&binding, "projectId") == Some(project_id)
        })
    }) {
        return None;
    }
    doc(s, DEPLOYMENTS, deployment_id).filter(|deployment| {
        str_of(deployment, "projectId") == Some(project_id)
            && str_of(deployment, "target") == Some("production")
            && str_of(deployment, "readyState") == Some("READY")
    })
}

pub fn production_domains(service: &ServiceState, project_id: &str) -> Vec<String> {
    docs(service, DOMAINS)
        .into_iter()
        .map(|(_, domain)| domain)
        .filter(|domain| {
            str_of(domain, "projectId") == Some(project_id)
                && domain["redirect"].is_null()
                && domain["gitBranch"].is_null()
                && domain["customEnvironmentId"].is_null()
        })
        .filter_map(|domain| str_of(&domain, "name").map(str::to_owned))
        .collect()
}

pub fn find_deployment(s: &ServiceState, id_or_url: &str) -> Option<Value> {
    let needle = id_or_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    docs(s, DEPLOYMENTS)
        .into_iter()
        .map(|(_, d)| d)
        .find(|d| str_of(d, "id") == Some(needle) || str_of(d, "url") == Some(needle))
        .or_else(|| {
            let binding = doc(s, ALIASES, needle)?;
            doc(s, DEPLOYMENTS, str_of(&binding, "deploymentId")?)
        })
}

fn alias_assigned(d: &Value) -> Value {
    let has_alias = d["alias"].as_array().is_some_and(|a| !a.is_empty());
    if has_alias && str_of(d, "readyState") == Some("READY") {
        d.get("ready").cloned().unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}

fn owner_id(s: &ServiceState, d: &Value) -> Value {
    str_of(d, "projectId")
        .and_then(|pid| doc(s, PROJECTS, pid))
        .and_then(|p| p.get("accountId").cloned())
        .unwrap_or(Value::Null)
}

/// `GET /v13/deployments/:idOrUrl` shape.
pub fn deployment_v13(s: &ServiceState, d: &Value) -> Value {
    let mut out = d.as_object().cloned().unwrap_or_default();
    out.insert("status".into(), d["readyState"].clone());
    out.insert("aliasAssigned".into(), alias_assigned(d));
    out.insert("aliasError".into(), Value::Null);
    out.insert("ownerId".into(), owner_id(s, d));
    out.insert("version".into(), json!(2));
    out.insert("public".into(), json!(false));
    Value::Object(out)
}

/// One element of `GET /v6/deployments`.
pub fn deployment_v6(d: &Value, current_production_id: Option<&str>) -> Value {
    let id = str_of(d, "id").unwrap_or_default();
    let is_prod_ready =
        str_of(d, "target") == Some("production") && str_of(d, "readyState") == Some("READY");
    let mut out = Map::new();
    out.insert("uid".into(), json!(id));
    out.insert("name".into(), d["name"].clone());
    out.insert("url".into(), d["url"].clone());
    out.insert("created".into(), d["createdAt"].clone());
    out.insert(
        "source".into(),
        d.get("source").cloned().unwrap_or(json!("git")),
    );
    out.insert("state".into(), d["readyState"].clone());
    out.insert("readyState".into(), d["readyState"].clone());
    if let Some(sub) = d.get("readySubstate") {
        out.insert("readySubstate".into(), sub.clone());
    } else if current_production_id == Some(id) {
        out.insert("readySubstate".into(), json!("PROMOTED"));
    }
    out.insert(
        "type".into(),
        d.get("type").cloned().unwrap_or(json!("LAMBDAS")),
    );
    out.insert("creator".into(), d["creator"].clone());
    out.insert("inspectorUrl".into(), d["inspectorUrl"].clone());
    out.insert("meta".into(), d.get("meta").cloned().unwrap_or(json!({})));
    out.insert("target".into(), d["target"].clone());
    out.insert("aliasError".into(), Value::Null);
    out.insert("aliasAssigned".into(), alias_assigned(d));
    out.insert(
        "isRollbackCandidate".into(),
        json!(is_prod_ready && current_production_id != Some(id)),
    );
    out.insert("createdAt".into(), d["createdAt"].clone());
    out.insert("buildingAt".into(), d["buildingAt"].clone());
    out.insert(
        "ready".into(),
        d.get("ready").cloned().unwrap_or(Value::Null),
    );
    out.insert("projectId".into(), d["projectId"].clone());
    Value::Object(out)
}

/// The deployment summary Vercel embeds in projects (`latestDeployments`, `targets`).
pub fn deployment_summary(d: &Value) -> Value {
    json!({
        "id": d["id"],
        "name": d["name"],
        "url": d["url"],
        "target": d["target"],
        "readyState": d["readyState"],
        "type": d.get("type").cloned().unwrap_or(json!("LAMBDAS")),
        "createdAt": d["createdAt"],
        "buildingAt": d["buildingAt"],
        "readyAt": d.get("ready").cloned().unwrap_or(Value::Null),
        "alias": d.get("alias").cloned().unwrap_or(json!([])),
        "aliasAssigned": alias_assigned(d),
        "aliasError": null,
        "creator": d["creator"],
        "meta": d.get("meta").cloned().unwrap_or(json!({})),
    })
}

// ───────────────────────────── projects ─────────────────────────────

/// Project document as Vercel returns it: stored fields plus `latestDeployments` and
/// `targets.production`; `env` (with values masked) only when `with_env`.
pub fn project_view(s: &ServiceState, p: &Value, with_env: bool) -> Value {
    let mut out = p.as_object().cloned().unwrap_or_default();
    let id = str_of(p, "id").unwrap_or_default();
    if with_env {
        let envs: Vec<Value> = p["env"]
            .as_array()
            .map(|a| a.iter().map(|e| env_view(e, false)).collect())
            .unwrap_or_default();
        out.insert("env".into(), Value::Array(envs));
    } else {
        out.shift_remove("env");
    }
    let latest: Vec<Value> = project_deployments(s, id)
        .iter()
        .take(3)
        .map(deployment_summary)
        .collect();
    out.insert("latestDeployments".into(), Value::Array(latest));
    let mut targets = Map::new();
    if let Some(prod) = current_production(s, id) {
        targets.insert("production".into(), deployment_summary(&prod));
    }
    out.insert("targets".into(), Value::Object(targets));
    Value::Object(out)
}

pub fn refresh_deployment_views(service: &mut ServiceState, project_id: &str) {
    let latest: Vec<Value> = project_deployments(service, project_id)
        .iter()
        .take(3)
        .map(deployment_summary)
        .collect();
    let production =
        current_production(service, project_id).map(|value| deployment_summary(&value));
    if let Some(project) = service.store.collection_mut(PROJECTS).get_mut(project_id) {
        project["latestDeployments"] = Value::Array(latest);
        project["targets"] = match production {
            Some(production) => json!({"production":production}),
            None => json!({}),
        };
    }
}

// ───────────────────────────── environment variables ─────────────────────────────

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Deterministic stand-in for Vercel's ciphertext of an encrypted value.
fn ciphertext(id: &str, value: &str) -> String {
    let mut h = Sha256::new();
    h.update(id.as_bytes());
    h.update([0]);
    h.update(value.as_bytes());
    let first = h.finalize();
    let mut h2 = Sha256::new();
    h2.update(first);
    let mut bytes = first.to_vec();
    bytes.extend_from_slice(&h2.finalize()[..16]);
    base64(&bytes)
}

/// An env var as the API shows it: `encrypted` values as ciphertext (unless `decrypt`),
/// `sensitive` values never.
pub fn env_view(e: &Value, decrypt: bool) -> Value {
    let mut out = e.as_object().cloned().unwrap_or_default();
    out.shift_remove("projectId");
    out.shift_remove("scope");
    match str_of(e, "type") {
        Some("encrypted") | Some("secret") => {
            if !decrypt {
                let id = str_of(e, "id").unwrap_or_default();
                let value = str_of(e, "value").unwrap_or_default();
                out.insert("value".into(), json!(ciphertext(id, value)));
            }
            out.insert("decrypted".into(), json!(decrypt));
        }
        Some("sensitive") => {
            out.shift_remove("value");
        }
        _ => {}
    }
    Value::Object(out)
}

fn environment_scopes(variable: &Value) -> Vec<String> {
    let mut scopes: Vec<String> = variable["target"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(
            |target| match str_of(variable, "gitBranch").filter(|branch| !branch.is_empty()) {
                Some(branch) if target == "preview" => format!("preview:{branch}"),
                _ => target.to_owned(),
            },
        )
        .chain(
            variable["customEnvironmentIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|environment| format!("custom:{environment}")),
        )
        .collect();
    scopes.sort();
    scopes.dedup();
    scopes
}

pub fn environment_scope(variable: &Value) -> String {
    environment_scopes(variable).join(",")
}

pub fn project_environment(service: &ServiceState, project_id: &str) -> Vec<(String, Value)> {
    docs(service, ENV_VARS)
        .into_iter()
        .filter(|(_, variable)| str_of(variable, "projectId") == Some(project_id))
        .collect()
}

pub fn rebuild_env_index(s: &mut ServiceState) {
    let projects = docs(s, PROJECTS);
    let previous = docs(s, ENV_BINDINGS);
    s.store.take_collection(ENV_BY_TARGET);
    s.store.take_collection(ENV_BINDINGS);
    for (key, _) in projects {
        let mut by_target: Map<String, Value> = ["production", "preview", "development"]
            .iter()
            .map(|t| ((*t).to_owned(), json!({})))
            .collect();
        let variables = project_environment(s, &key);
        let mut bindings: BTreeMap<String, Value> = previous
            .iter()
            .filter(|(_, binding)| str_of(binding, "projectId") == Some(key.as_str()))
            .map(|(identity, binding)| {
                let mut absent = binding.clone();
                absent["records"] = json!(0);
                absent["value"] = Value::Null;
                absent["type"] = Value::Null;
                (identity.clone(), absent)
            })
            .collect();
        for (_, variable) in &variables {
            let Some(name) = str_of(variable, "key") else {
                continue;
            };
            let value = variable.get("value").cloned().unwrap_or(Value::Null);
            for scope in environment_scopes(variable) {
                let identity = format!("{key}/{scope}/{name}");
                let binding = bindings.entry(identity).or_insert_with(|| {
                    json!({"projectId":key,"scope":scope,"key":name,
                        "value":value,"type":variable["type"],"records":0})
                });
                let count = binding["records"].as_u64().expect("binding count") + 1;
                binding["records"] = json!(count);
                if count == 1 {
                    binding["value"] = value.clone();
                    binding["type"] = variable["type"].clone();
                } else {
                    binding["value"] = Value::Null;
                    binding["type"] = Value::Null;
                }
                by_target
                    .entry(scope)
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .expect("object")
                    .insert(name.to_owned(), binding["value"].clone());
            }
        }
        let projection: Vec<Value> = variables
            .into_iter()
            .map(|(_, mut variable)| {
                let fields = variable.as_object_mut().expect("environment record");
                fields.shift_remove("projectId");
                fields.shift_remove("scope");
                variable
            })
            .collect();
        s.store
            .collection_mut(PROJECTS)
            .get_mut(&key)
            .expect("project")["env"] = Value::Array(projection);
        s.store
            .collection_mut(ENV_BY_TARGET)
            .insert(&key, Value::Object(by_target));
        for (identity, binding) in bindings {
            s.store
                .collection_mut(ENV_BINDINGS)
                .insert(identity, binding);
        }
    }
}

pub fn initialize(service: &mut ServiceState, _now: i64) {
    let declared_aliases: std::collections::BTreeSet<String> = docs(service, ALIASES)
        .into_iter()
        .map(|(alias, _)| alias)
        .collect();
    for (project_id, project) in docs(service, PROJECTS) {
        for variable in project["env"].as_array().into_iter().flatten() {
            let id = str_of(variable, "id").expect("fixture environment record needs an ID");
            let mut variable = variable.clone();
            variable["projectId"] = json!(project_id);
            variable["scope"] = json!(environment_scope(&variable));
            let identity = format!("{project_id}/{id}");
            if let Some(existing) = doc(service, ENV_VARS, &identity) {
                assert_eq!(
                    existing, variable,
                    "conflicting authoritative environment fixtures"
                );
            } else {
                service
                    .store
                    .collection_mut(ENV_VARS)
                    .insert(identity, variable);
            }
        }
    }
    for (deployment_id, deployment) in docs(service, DEPLOYMENTS) {
        if doc(service, SOURCES, &deployment_id).is_none() {
            service.store.collection_mut(SOURCES).insert(
                &deployment_id,
                json!({"deploymentId":deployment_id,"projectId":deployment["projectId"],
                    "gitSource":deployment["gitSource"]}),
            );
        }
        for alias in deployment["alias"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(binding) = doc(service, ALIASES, alias) {
                assert!(
                    declared_aliases.contains(alias)
                        || str_of(&binding, "deploymentId") == Some(deployment_id.as_str()),
                    "ambiguous fixture aliases require explicit current bindings"
                );
            } else {
                service.store.collection_mut(ALIASES).insert(
                    alias,
                    json!({"alias":alias,"projectId":deployment["projectId"],"deploymentId":deployment_id}),
                );
            }
        }
    }
    rebuild_env_index(service);
    for (project_id, _) in docs(service, PROJECTS) {
        refresh_deployment_views(service, &project_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_like_rfc4648() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }

    #[test]
    fn env_view_masks_by_type() {
        let enc = json!({"id": "a", "key": "K", "value": "secret", "type": "encrypted"});
        let v = env_view(&enc, false);
        assert_ne!(v["value"], "secret");
        assert_eq!(v["decrypted"], false);
        assert_eq!(env_view(&enc, true)["value"], "secret");
        let sens = json!({"id": "b", "key": "K", "value": "x", "type": "sensitive"});
        assert!(env_view(&sens, false).get("value").is_none());
        let plain = json!({"id": "c", "key": "K", "value": "x", "type": "plain"});
        assert_eq!(env_view(&plain, false)["value"], "x");
    }
}
