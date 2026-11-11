use std::collections::BTreeMap;

use mock_core::world::ServiceState;
use serde_json::{Value, json};

use crate::{
    common::{opt_str, str_field},
    representation::{self, ResourceAddress},
};

pub enum ExecutionError {
    Unsupported(String),
    Conflict(String),
}

impl From<&str> for ExecutionError {
    fn from(message: &str) -> Self {
        Self::Unsupported(message.into())
    }
}

impl From<String> for ExecutionError {
    fn from(message: String) -> Self {
        Self::Unsupported(message)
    }
}

pub fn instances(state: &Value) -> Vec<Value> {
    if let Some(instances) = state["resource-instances"].as_array() {
        return instances.clone();
    }
    state["resources"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|resource| {
            let count = resource["count"].as_u64().unwrap_or(1);
            let module = str_field(resource, "module");
            let prefix = if module.is_empty() || module == "root" {
                String::new()
            } else {
                format!("{module}.")
            };
            let base = format!(
                "{prefix}{}.{}",
                str_field(resource, "type"),
                str_field(resource, "name")
            );
            (0..count).map(move |index| {
                let address = if count > 1 {
                    format!("{base}[{index}]")
                } else {
                    base.clone()
                };
                json!({
                    "address":address,"module":if module.is_empty() {"root"} else {module},
                    "type":resource["type"],"name":resource["name"],"provider":resource["provider"],
                    "index":if count > 1 {json!(index)} else {Value::Null},
                    "attributes":resource.get("attributes").cloned().unwrap_or(json!({}))
                })
            })
        })
        .collect()
}

fn summaries(instances: &[Value]) -> Vec<Value> {
    let mut grouped = BTreeMap::<(String, String, String, String), u64>::new();
    for instance in instances {
        let identity = (
            str_field(instance, "module").into(),
            str_field(instance, "type").into(),
            str_field(instance, "name").into(),
            str_field(instance, "provider").into(),
        );
        *grouped.entry(identity).or_default() += 1;
    }
    grouped
        .into_iter()
        .map(|((module, kind, name, provider), count)| {
            json!({"module":module,"type":kind,"name":name,"provider":provider,"count":count})
        })
        .collect()
}

fn seed_before(state: &mut Value, changes: &[Value]) {
    let rows = state["resource-instances"]
        .as_array_mut()
        .expect("materialized resource instances");
    for change in changes {
        if let Some(before) = change["before"].as_object()
            && let Some(instance) = rows
                .iter_mut()
                .find(|row| row["address"] == change["address"])
        {
            let attributes = instance["attributes"]
                .as_object_mut()
                .expect("resource attributes are an object");
            for (field, value) in before {
                if let Some(existing) = attributes.get(field) {
                    assert_eq!(
                        existing, value,
                        "contradictory initial resource observations"
                    );
                } else {
                    attributes.insert(field.clone(), value.clone());
                }
            }
        }
    }
}

pub fn initialize(service: &mut ServiceState, _now: i64) {
    let workspaces = service
        .store
        .collection("workspaces")
        .cloned()
        .unwrap_or_default();
    for (identity, workspace) in workspaces.iter() {
        let mut workspace = workspace.clone();
        if let Some(changes) = workspace
            .pointer_mut("/simulated-plan/resource-changes")
            .and_then(Value::as_array_mut)
        {
            changes
                .iter_mut()
                .for_each(representation::normalize_change);
        }
        service
            .store
            .collection_mut("workspaces")
            .insert(identity.to_owned(), workspace);
    }
    let plans = service
        .store
        .collection("plans")
        .cloned()
        .unwrap_or_default();
    for (identity, plan) in plans.iter() {
        let mut plan = plan.clone();
        if let Some(changes) = plan["resource-changes-detail"].as_array_mut() {
            changes
                .iter_mut()
                .for_each(representation::normalize_change);
        }
        record_plan_changes(service, &plan);
        service
            .store
            .collection_mut("plans")
            .insert(identity.to_owned(), plan);
    }
    let workspaces = service
        .store
        .collection("workspaces")
        .cloned()
        .unwrap_or_default();
    let runs = service
        .store
        .collection("runs")
        .cloned()
        .unwrap_or_default();
    let plans = service
        .store
        .collection("plans")
        .cloned()
        .unwrap_or_default();
    let states: Vec<_> = service
        .store
        .collection("state_versions")
        .into_iter()
        .flat_map(|rows| rows.iter())
        .map(|(identity, state)| (identity.to_owned(), state.clone()))
        .collect();
    for (identity, mut state) in states {
        state["resource-instances"] = json!(instances(&state));
        state["resources"] = json!(summaries(
            state["resource-instances"]
                .as_array()
                .expect("materialized instances")
        ));
        if state.get("lineage").is_none() {
            state["lineage"] = json!(crate::common::hex_digest(str_field(&state, "workspace")));
        }
        if let Some(workspace) = workspaces.get(str_field(&state, "workspace"))
            && workspace["current-state-version"] == identity
        {
            if let Some(changes) = workspace
                .pointer("/simulated-plan/resource-changes")
                .and_then(Value::as_array)
            {
                seed_before(&mut state, changes);
            }
            if let Some(run) = opt_str(workspace, "current-run").and_then(|id| runs.get(id))
                && crate::resources::CONFIRMABLE.contains(&str_field(run, "status"))
                && let Some(plan) = opt_str(run, "plan").and_then(|id| plans.get(id))
                && let Some(changes) = plan["resource-changes-detail"].as_array()
            {
                seed_before(&mut state, changes);
            }
        }
        service
            .store
            .collection_mut("state_versions")
            .insert(identity, state);
    }
    for (identity, mut workspace) in workspaces
        .iter()
        .map(|(id, row)| (id.to_owned(), row.clone()))
    {
        if let Some(state) = opt_str(&workspace, "current-state-version")
            .and_then(|id| {
                service
                    .store
                    .collection("state_versions")
                    .and_then(|rows| rows.get(id))
            })
            .cloned()
        {
            workspace["resource-count"] = json!(instances(&state).len());
            record_current_resources(service, &identity, &state);
            if let Some(run) = opt_str(&workspace, "current-run").and_then(|id| runs.get(id))
                && crate::resources::CONFIRMABLE.contains(&str_field(run, "status"))
                && let Some(plan) = opt_str(run, "plan")
                    .and_then(|id| service.store.collection_mut("plans").get_mut(id))
            {
                plan["base-state-version"] = state["id"].clone();
                plan["base-serial"] = state["serial"].clone();
                plan["base-lineage"] = state["lineage"].clone();
            }
            service
                .store
                .collection_mut("workspaces")
                .insert(identity, workspace);
        }
    }
}

pub fn record_plan_changes(service: &mut ServiceState, plan: &Value) {
    for change in plan["resource-changes-detail"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let identity = format!("{}/{}", str_field(plan, "id"), str_field(change, "address"));
        let mut row = change.clone();
        row["id"] = json!(identity);
        row["plan"] = plan["id"].clone();
        service
            .store
            .collection_mut("plan_changes")
            .insert(identity, row);
    }
}

pub fn record_current_resources(service: &mut ServiceState, workspace: &str, state: &Value) {
    let previous: Vec<_> = service
        .store
        .collection("managed_resources")
        .into_iter()
        .flat_map(|rows| rows.iter())
        .filter(|(_, row)| row["workspace"] == workspace)
        .map(|(identity, _)| identity.to_owned())
        .collect();
    let rows: BTreeMap<_, _> = instances(state)
        .into_iter()
        .map(|mut row| {
            row["workspace"] = json!(workspace);
            (format!("{workspace}/{}", str_field(&row, "address")), row)
        })
        .collect();
    for identity in previous {
        if !rows.contains_key(&identity) {
            service
                .store
                .collection_mut("managed_resources")
                .remove(&identity);
        }
    }
    for (identity, row) in rows {
        service
            .store
            .collection_mut("managed_resources")
            .insert(identity, row);
    }
}

fn has_unknown(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Array(values) => values.iter().any(has_unknown),
        Value::Object(fields) => fields.values().any(has_unknown),
        _ => false,
    }
}

pub fn plan(
    service: &ServiceState,
    workspace: &Value,
    variables: &[Value],
) -> Result<Vec<Value>, ExecutionError> {
    let mut changes = workspace
        .pointer("/simulated-plan/resource-changes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let previous = opt_str(workspace, "current-state-version")
        .and_then(|identity| {
            service
                .store
                .collection("state_versions")
                .and_then(|rows| rows.get(identity))
        })
        .cloned()
        .unwrap_or_else(|| json!({"resources":[]}));
    let previous: BTreeMap<_, _> = instances(&previous)
        .into_iter()
        .map(|row| (str_field(&row, "address").to_owned(), row))
        .collect();
    for binding in service
        .store
        .collection("planning_bindings")
        .into_iter()
        .flat_map(|rows| rows.values())
        .filter(|binding| binding["workspace"] == workspace["id"])
    {
        let matches: Vec<_> = variables
            .iter()
            .filter(|variable| {
                variable["key"] == binding["variable"]
                    && variable["category"] == binding["category"]
            })
            .collect();
        let [variable] = matches.as_slice() else {
            return Err(ExecutionError::Conflict(
                "A planning input is missing or ambiguous.".into(),
            ));
        };
        let text = variable["value"].as_str().ok_or_else(|| {
            ExecutionError::Conflict("Terraform API variable values must be strings.".into())
        })?;
        let value = match str_field(binding, "value-type") {
            "integer" => {
                let value = mock_core::exact_integer(&json!(text)).ok_or_else(|| {
                    ExecutionError::Conflict(
                        "The configured planning input is not an integer.".into(),
                    )
                })?;
                json!(value)
            }
            "string" => json!(text),
            "boolean" => {
                let value = text.parse::<bool>().map_err(|_| {
                    ExecutionError::Conflict(
                        "The configured planning input is not a boolean.".into(),
                    )
                })?;
                json!(value)
            }
            _ => return Err("This configuration's planning value type is not implemented.".into()),
        };
        let address = str_field(binding, "resource");
        let pointer = str_field(binding, "attribute");
        let change = changes
            .iter_mut()
            .find(|change| change["address"] == address)
            .ok_or("A declared configuration resource template is unavailable.")?;
        let after = change["after"]
            .pointer_mut(pointer)
            .ok_or("A declared configuration attribute template is unavailable.")?;
        *after = value;
        if let Some(instance) = previous.get(address)
            && let Some(before) = instance["attributes"].pointer(pointer)
        {
            let destination = change["before"]
                .pointer_mut(pointer)
                .ok_or("A declared configuration before-value is unavailable.")?;
            *destination = before.clone();
        }
    }
    changes.retain(|change| change["before"] != change["after"]);
    Ok(changes)
}

fn agrees_with_plan(planned: &Value, resolved: &Value, unknown: &Value) -> bool {
    if unknown == true {
        return true;
    }
    match planned {
        Value::Object(fields) => fields.iter().all(|(field, value)| {
            resolved
                .get(field)
                .is_some_and(|actual| agrees_with_plan(value, actual, &unknown[field]))
        }),
        Value::Array(values) => resolved.as_array().is_some_and(|actual| {
            values.len() == actual.len()
                && values
                    .iter()
                    .zip(actual)
                    .enumerate()
                    .all(|(index, (expected, actual))| {
                        agrees_with_plan(expected, actual, &unknown[index])
                    })
        }),
        _ => planned == resolved,
    }
}

pub fn apply(
    service: &ServiceState,
    previous: &Value,
    plan: &Value,
) -> Result<Value, ExecutionError> {
    if str_field(plan, "id").is_empty() || !plan["resource-changes-detail"].is_array() {
        return Err("The execution's immutable plan is unavailable.".into());
    }
    if plan.get("json-output").is_some() {
        return Err("An independent display-plan override is not implemented.".into());
    }
    for (planned, actual) in [
        ("base-state-version", "id"),
        ("base-serial", "serial"),
        ("base-lineage", "lineage"),
    ] {
        if plan.get(planned).is_none() {
            return Err("The saved plan's base state identity is unavailable.".into());
        }
        if plan[planned] != previous[actual] {
            return Err(ExecutionError::Conflict(
                "The saved plan's base state has changed.".into(),
            ));
        }
    }
    let previous_instances = instances(previous);
    for instance in &previous_instances {
        ResourceAddress::from_instance(instance)?;
    }
    let mut rows: BTreeMap<String, Value> = previous_instances
        .iter()
        .cloned()
        .map(|row| (str_field(&row, "address").to_owned(), row))
        .collect();
    if rows.len() != previous_instances.len() {
        return Err("Duplicate or deposed resource identities are not implemented.".into());
    }
    let outcomes = service
        .store
        .collection("execution_outcomes")
        .and_then(|records| records.get(str_field(plan, "id")));
    for change in plan["resource-changes-detail"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let address = str_field(change, "address");
        let identity = ResourceAddress::parse(address)?;
        let actions: Vec<_> = change["actions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if actions == ["no-op"] {
            continue;
        }
        if !matches!(actions.as_slice(), ["create"] | ["update"] | ["delete"]) {
            return Err("This Terraform resource action is not implemented.".into());
        }
        if actions.contains(&"delete") {
            let existing = rows.get(address).ok_or_else(|| {
                ExecutionError::Conflict(
                    "The deleted resource instance is no longer present.".into(),
                )
            })?;
            if let Some(before) = change["before"].as_object()
                && before
                    .iter()
                    .any(|(field, value)| existing["attributes"].get(field) != Some(value))
            {
                return Err(ExecutionError::Conflict(
                    "The deleted resource's before-values no longer match.".into(),
                ));
            }
            rows.remove(address);
        }
        if actions == ["delete"] {
            continue;
        }
        let supplied = outcomes.and_then(|outcomes| outcomes["resources"].get(address));
        if (has_unknown(&change["after-unknown"]) || actions == ["create"])
            && supplied.and_then(|row| row.get("attributes")).is_none()
        {
            return Err(
                "Resolved Terraform execution values are unavailable for this plan.".into(),
            );
        }
        let after = supplied
            .and_then(|row| row.get("attributes"))
            .unwrap_or(&change["after"]);
        if !after.is_object() {
            return Err(
                "Resolved Terraform execution values are unavailable for this plan.".into(),
            );
        }
        if !agrees_with_plan(&change["after"], after, &change["after-unknown"]) {
            return Err("The declared execution values contradict known plan values.".into());
        }
        if actions.contains(&"create") {
            let provider = supplied
                .and_then(|row| row["provider"].as_str())
                .or_else(|| change["provider"].as_str())
                .ok_or("The created resource's provider binding is unavailable.")?;
            if rows.contains_key(address) {
                return Err(ExecutionError::Conflict(
                    "The created resource address is already present.".into(),
                ));
            }
            let instance = json!({
                "address":address,"module":identity.module,"type":identity.kind,"name":identity.name,"provider":provider,
                "index":identity.index,"attributes":after
            });
            ResourceAddress::from_instance(&instance)?;
            rows.insert(address.into(), instance);
        } else {
            let instance = rows.get_mut(address).ok_or_else(|| {
                ExecutionError::Conflict(
                    "The updated resource instance is no longer present.".into(),
                )
            })?;
            let attributes = instance["attributes"]
                .as_object_mut()
                .ok_or("The existing resource attributes are unavailable.")?;
            if let Some(before) = change["before"].as_object()
                && before
                    .iter()
                    .any(|(field, value)| attributes.get(field) != Some(value))
            {
                return Err(ExecutionError::Conflict(
                    "The plan's before-values do not match the recorded resource state.".into(),
                ));
            }
            for (field, value) in after.as_object().expect("checked after object") {
                attributes.insert(field.clone(), value.clone());
            }
        }
    }
    let rows: Vec<_> = rows.into_values().collect();
    let mut state = previous.clone();
    state["resources"] = json!(summaries(&rows));
    state["resource-instances"] = json!(rows);
    Ok(state)
}

pub fn state_resources(state: &Value) -> Result<Vec<Value>, String> {
    let mut grouped = BTreeMap::<(String, String, String, String), Vec<Value>>::new();
    for instance in instances(state) {
        ResourceAddress::from_instance(&instance)?;
        let identity = (
            str_field(&instance, "module").into(),
            str_field(&instance, "type").into(),
            str_field(&instance, "name").into(),
            str_field(&instance, "provider").into(),
        );
        let mut value = json!({"schema_version":0,"attributes":instance["attributes"]});
        if !instance["index"].is_null() {
            value["index_key"] = instance["index"].clone();
        }
        grouped.entry(identity).or_default().push(value);
    }
    Ok(grouped
        .into_iter()
        .map(|((module, kind, name, provider), instances)| {
            let mut row = json!({
                "mode":"managed","type":kind,"name":name,
                "provider":format!("provider[\"{provider}\"]"),"instances":instances
            });
            if module != "root" {
                row["module"] = json!(module);
            }
            row
        })
        .collect())
}
