//! `/api/v1/applications`: list, get, create, update, delete, sync.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
};
use mock_core::{http::json, world::SharedWorld};
use serde_json::{Map, Value, json};

use crate::{
    NAME, Params,
    error::{ApiResult, ArgoError},
    model::{self, DEFAULT_APP_NAMESPACE, SyncRequest, app_key, render_app, str_field},
};

fn parse_body(body: &Bytes) -> Result<Value, ArgoError> {
    if body.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_slice(body).map_err(|_| ArgoError::bad_json())
}

/// `selector` in Kubernetes label-selector syntax (`k=v`, `k==v`, `k!=v`, `k`, `!k`).
fn selector_matches(selector: &str, labels: &Value) -> bool {
    selector
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .all(|term| {
            let label = |k: &str| labels.get(k.trim()).and_then(Value::as_str);
            if let Some((k, v)) = term.split_once("!=") {
                label(k) != Some(v.trim())
            } else if let Some((k, v)) = term.split_once("==").or_else(|| term.split_once('=')) {
                label(k) == Some(v.trim())
            } else if let Some(k) = term.strip_prefix('!') {
                label(k).is_none()
            } else {
                label(term).is_some()
            }
        })
}

pub async fn list(State(world): State<SharedWorld>, params: Params) -> ApiResult {
    let world = world.lock().expect("world lock");
    let store = model::store(&world);
    let projects = params.all("projects");
    let projects = if projects.is_empty() {
        params.all("project")
    } else {
        projects
    };
    let mut keys: Vec<(String, String, String)> = store
        .collection("applications")
        .map(|c| {
            c.iter()
                .filter(|(_, app)| {
                    let name = str_field(app, "/metadata/name");
                    params.non_empty("search").is_none_or(|s| name.contains(s))
                        && params.non_empty("name").is_none_or(|n| name == n)
                        && params
                            .non_empty("appNamespace")
                            .is_none_or(|ns| str_field(app, "/metadata/namespace") == ns)
                        && params
                            .non_empty("repo")
                            .is_none_or(|r| str_field(app, "/spec/source/repoURL") == r)
                        && (projects.is_empty()
                            || projects.contains(&str_field(app, "/spec/project")))
                        && params.non_empty("selector").is_none_or(|s| {
                            selector_matches(
                                s,
                                app.pointer("/metadata/labels").unwrap_or(&Value::Null),
                            )
                        })
                })
                .map(|(k, app)| {
                    (
                        str_field(app, "/metadata/name").to_owned(),
                        str_field(app, "/metadata/namespace").to_owned(),
                        k.to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    keys.sort();
    let items: Vec<Value> = keys
        .iter()
        .filter_map(|(_, _, k)| render_app(store, k))
        .collect();
    Ok(json(
        StatusCode::OK,
        json!({ "metadata": {}, "items": items }),
    ))
}

pub async fn get(
    State(world): State<SharedWorld>,
    Path(name): Path<String>,
    params: Params,
) -> ApiResult {
    let mut world = world.lock().expect("world lock");
    let key = app_key(params.get("appNamespace"), &name);
    model::observe(&mut world, &key)?;
    let store = model::store(&world);
    let app = render_app(store, &key).ok_or_else(|| ArgoError::app_not_found(&name))?;
    Ok(json(StatusCode::OK, app))
}

/// The metadata fields a client may set; the server owns the rest.
fn client_metadata(input: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    for field in ["labels", "annotations", "finalizers"] {
        if let Some(v) = input.pointer(&format!("/metadata/{field}"))
            && !v.is_null()
        {
            out.insert(field.to_owned(), v.clone());
        }
    }
    out
}

pub async fn create(State(world): State<SharedWorld>, params: Params, body: Bytes) -> ApiResult {
    let input = parse_body(&body)?;
    let name = str_field(&input, "/metadata/name").to_owned();
    if name.is_empty() {
        return Err(ArgoError::invalid(
            "Application.argoproj.io \"\" is invalid: metadata.name: Required value: name or generateName is required",
        ));
    }
    let ns = match str_field(&input, "/metadata/namespace") {
        "" => DEFAULT_APP_NAMESPACE.to_owned(),
        ns => ns.to_owned(),
    };
    let key = format!("{ns}/{name}");
    let spec = input.get("spec").cloned().unwrap_or_else(|| json!({}));
    let upsert = params.flag("upsert").unwrap_or(false);

    let mut guard = world.lock().expect("world lock");
    let world = &mut *guard;
    let now = world.clock.now_rfc3339();
    let svc = world.service_mut(NAME);
    let validation = model::validate_spec(&svc.store, &name, &ns, &spec);
    model::checked(world, validation)?;
    let svc = world.service_mut(NAME);

    if let Some(existing) = model::get_app(&svc.store, &key).cloned() {
        if existing["spec"] == spec {
            return Ok(json(
                StatusCode::OK,
                render_app(&svc.store, &key).unwrap_or(existing),
            ));
        }
        if !upsert {
            return Err(ArgoError::invalid(
                "existing application spec is different, use upsert flag to force update",
            ));
        }
        let result = apply_update(svc, &now, &key, &input, spec);
        return Ok(json(StatusCode::OK, model::checked(world, result)?));
    }

    let uid = svc.ids.next_uuid("application").to_string();
    let rv = model::next_resource_version(svc);
    let mut metadata = json!({
        "name": name,
        "namespace": ns,
        "uid": uid,
        "resourceVersion": rv,
        "generation": 1,
        "creationTimestamp": now,
    });
    for (k, v) in client_metadata(&input) {
        metadata[k] = v;
    }
    let mut app = json!({
        "metadata": metadata,
        "spec": spec,
        "status": {
            "health": { "status": "Missing" },
            "reconciledAt": now,
            "sourceType": "Directory",
            "controllerNamespace": DEFAULT_APP_NAMESPACE,
        },
    });
    app["status"]["sync"] = json!({ "status": "OutOfSync" });
    svc.store
        .collection_mut("applications")
        .insert(key.clone(), app.clone());
    model::app_event(
        svc,
        &now,
        &app,
        "argocd-server",
        "ResourceCreated",
        "admin created application",
    );
    let result = model::reconcile(svc, &now, &key);
    model::checked(world, result)?;
    let svc = world.service_mut(NAME);
    let out = render_app(&svc.store, &key).expect("just created");
    Ok(json(StatusCode::OK, out))
}

fn apply_update(
    svc: &mut mock_core::world::ServiceState,
    now: &str,
    key: &str,
    input: &Value,
    mut spec: Value,
) -> Result<Value, ArgoError> {
    let mut app = model::require_app(&svc.store, key)?.clone();
    let candidate = json!({"spec":spec});
    if crate::rendered::cluster(&svc.store, &candidate)
        == crate::rendered::cluster(&svc.store, &app)
        && candidate["spec"]["destination"]["namespace"] == app["spec"]["destination"]["namespace"]
    {
        spec["destination"] = app["spec"]["destination"].clone();
    }
    if let Some(policy) = spec
        .pointer_mut("/syncPolicy/automated")
        .and_then(Value::as_object_mut)
        && policy.get("enabled").is_some_and(Value::is_null)
    {
        policy.shift_remove("enabled");
    }
    let rv = model::next_resource_version(svc);
    let changed_spec = app["spec"] != spec;
    app["spec"] = spec;
    if let Some(meta) = app["metadata"].as_object_mut() {
        // Update replaces labels, annotations and finalizers with the request's.
        for field in ["labels", "annotations", "finalizers"] {
            meta.remove(field);
        }
        for (k, v) in client_metadata(input) {
            meta.insert(k, v);
        }
        let generation =
            meta.get("generation").and_then(Value::as_i64).unwrap_or(1) + i64::from(changed_spec);
        meta.insert("generation".to_owned(), json!(generation));
        meta.insert("resourceVersion".to_owned(), json!(rv));
    }
    svc.store
        .collection_mut("applications")
        .insert(key.to_owned(), app.clone());
    model::app_event(
        svc,
        now,
        &app,
        "argocd-server",
        "ResourceUpdated",
        "admin updated application spec",
    );
    model::reconcile(svc, now, key)?;
    Ok(render_app(&svc.store, key).expect("app exists"))
}

pub async fn update(
    State(world): State<SharedWorld>,
    Path(name): Path<String>,
    body: Bytes,
) -> ApiResult {
    let input = parse_body(&body)?;
    let ns = match str_field(&input, "/metadata/namespace") {
        "" => DEFAULT_APP_NAMESPACE.to_owned(),
        ns => ns.to_owned(),
    };
    let key = format!("{ns}/{name}");
    let spec = input.get("spec").cloned().unwrap_or_else(|| json!({}));
    let mut guard = world.lock().expect("world lock");
    let world = &mut *guard;
    let now = world.clock.now_rfc3339();
    let svc = world.service_mut(NAME);
    model::require_app(&svc.store, &key)?;
    let validation = model::validate_spec(&svc.store, &name, &ns, &spec);
    model::checked(world, validation)?;
    let svc = world.service_mut(NAME);
    let result = apply_update(svc, &now, &key, &input, spec);
    Ok(json(StatusCode::OK, model::checked(world, result)?))
}

pub async fn update_spec(
    State(world): State<SharedWorld>,
    Path(name): Path<String>,
    params: Params,
    body: Bytes,
) -> ApiResult {
    let spec = parse_body(&body)?;
    let key = app_key(params.get("appNamespace"), &name);
    let mut guard = world.lock().expect("world lock");
    let world = &mut *guard;
    let now = world.clock.now_rfc3339();
    let svc = world.service_mut(NAME);
    let application = model::require_app(&svc.store, &key)?.clone();
    let namespace = str_field(&application, "/metadata/namespace");
    let validation = model::validate_spec(&svc.store, &name, namespace, &spec);
    model::checked(world, validation)?;
    let svc = world.service_mut(NAME);
    let result = apply_update(svc, &now, &key, &application, spec);
    let updated = model::checked(world, result)?;
    Ok(json(StatusCode::OK, updated["spec"].clone()))
}

pub async fn delete(
    State(world): State<SharedWorld>,
    Path(name): Path<String>,
    params: Params,
) -> ApiResult {
    let key = app_key(params.get("appNamespace"), &name);
    let cascade = params.flag("cascade").unwrap_or(true);
    let propagation = params.non_empty("propagationPolicy");
    if let Some(policy) = propagation
        && !["foreground", "background", "orphan"].contains(&policy.to_ascii_lowercase().as_str())
    {
        return Err(ArgoError::invalid(format!(
            "invalid propagation policy: {policy}"
        )));
    }
    let cascade = cascade && !propagation.is_some_and(|p| p.eq_ignore_ascii_case("orphan"));
    let mut guard = world.lock().expect("world lock");
    let world = &mut *guard;
    let now = world.clock.now_rfc3339();
    let svc = world.service_mut(NAME);
    let app = model::require_app(&svc.store, &key)?.clone();
    if cascade {
        let uids: Vec<String> = model::app_resources(&svc.store, &key)
            .iter()
            .map(|r| str_field(r, "/uid").to_owned())
            .collect();
        let resources = svc.store.collection_mut("resources");
        for uid in uids {
            resources.remove(&uid);
        }
    } else {
        // Orphaned: the objects stay in the cluster but no longer belong to the app.
        let resources = svc.store.collection_mut("resources");
        let uids: Vec<String> = resources
            .iter()
            .filter(|(_, r)| r["app"] == key.as_str())
            .map(|(k, _)| k.to_owned())
            .collect();
        for uid in uids {
            if let Some(r) = resources.get_mut(&uid) {
                r["app"] = Value::Null;
            }
        }
    }
    svc.store.collection_mut("applications").remove(&key);
    model::app_event(
        svc,
        &now,
        &app,
        "argocd-server",
        "ResourceDeleted",
        "admin deleted application",
    );
    Ok(json(StatusCode::OK, json!({})))
}

pub async fn sync(
    State(world): State<SharedWorld>,
    Path(name): Path<String>,
    body: Bytes,
) -> ApiResult {
    let input = parse_body(&body)?;
    let key = app_key(input["appNamespace"].as_str(), &name);
    if ["prune", "dryRun"].iter().any(|field| {
        input
            .get(*field)
            .is_some_and(|value| !value.is_boolean() && !value.is_null())
    }) {
        return Err(ArgoError::invalid("Sync flags must be boolean."));
    }
    let sync_options = match input.get("syncOptions") {
        Some(Value::Object(options))
            if options.get("items").is_some_and(|items| {
                items
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
            }) =>
        {
            options.get("items").cloned()
        }
        None | Some(Value::Null) => None,
        _ => {
            return Err(ArgoError::invalid(
                "syncOptions must be a message with a string-array items field.",
            ));
        }
    };
    let req = SyncRequest {
        revision: input["revision"]
            .as_str()
            .filter(|r| !r.is_empty())
            .map(str::to_owned),
        prune: input["prune"].as_bool().unwrap_or(false),
        dry_run: input["dryRun"].as_bool().unwrap_or(false),
        sync_options,
        automated: false,
    };
    let mut guard = world.lock().expect("world lock");
    let world = &mut *guard;
    let now = world.clock.now_rfc3339();
    let svc = world.service_mut(NAME);
    let result = model::sync(svc, &now, &key, req);
    model::checked(world, result)?;
    let svc = world.service_mut(NAME);
    let out = render_app(&svc.store, &key).expect("app exists");
    Ok(json(StatusCode::OK, out))
}
