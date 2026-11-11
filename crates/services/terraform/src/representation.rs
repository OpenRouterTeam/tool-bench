use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::{
    common::{Db, opt_str, str_field},
    execution,
};

pub struct ResourceAddress {
    pub module: String,
    pub kind: String,
    pub name: String,
    pub index: Option<Value>,
}

fn identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn indexed_name(value: &str) -> Result<(String, Option<Value>), String> {
    let (name, index) = if let Some((name, indexed)) = value.split_once('[') {
        let index = indexed
            .strip_suffix(']')
            .ok_or("The resource instance address is incomplete.")?;
        if index.is_empty() || !index.bytes().all(|character| character.is_ascii_digit()) {
            return Err("Only numeric indexed resource addresses are implemented.".into());
        }
        let index = index
            .parse::<u64>()
            .map_err(|_| "The resource instance index is out of range.")?;
        if index.to_string() != indexed[..indexed.len() - 1] {
            return Err("The resource instance index is not canonical.".into());
        }
        (name, Some(json!(index)))
    } else {
        (value, None)
    };
    if !identifier(name) {
        return Err("This resource address is not implemented.".into());
    }
    Ok((name.into(), index))
}

impl ResourceAddress {
    pub fn from_instance(instance: &Value) -> Result<Self, String> {
        let identity = Self::parse(str_field(instance, "address"))?;
        let provider = str_field(instance, "provider");
        let provider_parts: Vec<_> = provider.split('/').collect();
        if instance.get("mode").is_some_and(|mode| mode != "managed")
            || instance["type"] != identity.kind
            || instance["name"] != identity.name
            || instance["module"] != identity.module
            || instance["index"] != identity.index.clone().unwrap_or(Value::Null)
            || provider_parts.len() != 3
            || provider_parts.iter().any(|part| {
                part.is_empty()
                    || !part.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
                    })
            })
        {
            return Err("This resource identity or provider binding is not implemented.".into());
        }
        Ok(identity)
    }

    pub fn parse(address: &str) -> Result<Self, String> {
        let parts: Vec<_> = address.split('.').collect();
        if parts.len() < 2 || parts.len() % 2 != 0 {
            return Err("The resource address is incomplete.".into());
        }
        for pair in parts[..parts.len() - 2].as_chunks::<2>().0 {
            if pair[0] != "module" {
                return Err("Only managed resource addresses are implemented.".into());
            }
            indexed_name(pair[1])?;
        }
        let kind = parts[parts.len() - 2];
        if !identifier(kind) {
            return Err("The resource type is invalid.".into());
        }
        let (name, index) = indexed_name(parts[parts.len() - 1])?;
        Ok(Self {
            module: if parts.len() == 2 {
                "root".into()
            } else {
                parts[..parts.len() - 2].join(".")
            },
            kind: kind.into(),
            name,
            index,
        })
    }

    fn fields(&self, address: &str, provider: &str) -> Value {
        let mut row = json!({
            "address":address,"mode":"managed","type":self.kind,"name":self.name,
            "provider_name":provider
        });
        if let Some(index) = &self.index {
            row["index"] = index.clone();
        }
        if self.module != "root" {
            row["module_address"] = json!(self.module);
        }
        row
    }
}

fn authored_unknowns(value: &Value) -> (Value, Option<Value>) {
    match value {
        Value::String(text) if text == "(known after apply)" => (Value::Null, Some(json!(true))),
        Value::Object(fields) => {
            let mut known = serde_json::Map::new();
            let mut unknown = serde_json::Map::new();
            for (field, value) in fields {
                let (value, mask) = authored_unknowns(value);
                known.insert(field.clone(), value);
                if let Some(mask) = mask {
                    unknown.insert(field.clone(), mask);
                }
            }
            (json!(known), (!unknown.is_empty()).then(|| json!(unknown)))
        }
        Value::Array(values) => {
            let split: Vec<_> = values.iter().map(authored_unknowns).collect();
            let known: Vec<_> = split.iter().map(|(value, _)| value.clone()).collect();
            let unknown = split.iter().any(|(_, mask)| mask.is_some()).then(|| {
                json!(
                    split
                        .into_iter()
                        .map(|(_, mask)| mask.unwrap_or(json!(false)))
                        .collect::<Vec<_>>()
                )
            });
            (json!(known), unknown)
        }
        _ => (value.clone(), None),
    }
}

pub fn normalize_change(change: &mut Value) {
    if change.get("after-unknown").is_none() {
        let (known, unknown) = authored_unknowns(&change["after"]);
        change["after"] = known;
        change["after-unknown"] = unknown.unwrap_or(json!({}));
    }
}

fn module_values(resources: Vec<Value>, outputs: Value) -> Value {
    let mut modules = BTreeMap::<String, Vec<Value>>::new();
    for resource in resources {
        let module = resource["module_address"]
            .as_str()
            .unwrap_or("root")
            .to_owned();
        modules.entry(module.clone()).or_default().push(resource);
        if module != "root" {
            let parts: Vec<_> = module.split('.').collect();
            for length in (2..parts.len()).step_by(2) {
                modules.entry(parts[..length].join(".")).or_default();
            }
        }
    }
    fn render(address: &str, modules: &BTreeMap<String, Vec<Value>>) -> Value {
        let mut module = json!({});
        if address != "root" {
            module["address"] = json!(address);
        }
        if let Some(resources) = modules.get(address).filter(|rows| !rows.is_empty()) {
            module["resources"] = json!(resources);
        }
        let children: Vec<_> = modules
            .keys()
            .filter(|candidate| {
                let parent = candidate.rsplitn(3, '.').nth(2).unwrap_or("root");
                candidate.as_str() != address && parent == address
            })
            .map(|child| render(child, modules))
            .collect();
        if !children.is_empty() {
            module["child_modules"] = json!(children);
        }
        module
    }
    json!({"root_module":render("root", &modules),"outputs":outputs})
}

pub fn outputs(state: &Value) -> Value {
    let outputs: serde_json::Map<_, _> = state["outputs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|output| {
            (
                str_field(output, "name").to_owned(),
                json!({
                    "value":output["value"],"type":output["type"],"sensitive":output["sensitive"]
                }),
            )
        })
        .collect();
    json!(outputs)
}

pub fn state_json(state: &Value) -> Result<Value, String> {
    let resources = execution::instances(state)
        .into_iter()
        .map(|row| {
            let address = str_field(&row, "address");
            let mut resource =
                ResourceAddress::from_instance(&row)?.fields(address, str_field(&row, "provider"));
            resource["schema_version"] = json!(0);
            resource["values"] = row["attributes"].clone();
            resource["sensitive_values"] = json!({});
            Ok(resource)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "format_version":"1.0","terraform_version":state["terraform-version"],
        "values":module_values(resources, outputs(state))
    }))
}

pub fn plan_json(db: &Db, plan: &Value) -> Result<Value, String> {
    if plan.get("json-output").is_some() {
        return Err("An independent display-plan override is not implemented.".into());
    }
    let run = opt_str(plan, "run").and_then(|identity| db.get("runs", identity));
    let workspace = run
        .and_then(|run| opt_str(run, "workspace"))
        .and_then(|identity| db.get("workspaces", identity));
    let base =
        opt_str(plan, "base-state-version").and_then(|identity| db.get("state_versions", identity));
    let provider_state = base.or_else(|| {
        workspace
            .and_then(|workspace| opt_str(workspace, "current-state-version"))
            .and_then(|identity| db.get("state_versions", identity))
    });
    let initial = base.map(execution::instances).unwrap_or_default();
    let mut planned: BTreeMap<_, _> = initial
        .into_iter()
        .map(|resource| (str_field(&resource, "address").to_owned(), resource))
        .collect();
    let mut changes = Vec::new();
    for change in plan["resource-changes-detail"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let address = str_field(change, "address");
        let identity = ResourceAddress::parse(address)?;
        let outcome = db
            .get("execution_outcomes", str_field(plan, "id"))
            .and_then(|outcome| outcome["resources"].get(address));
        let providers: BTreeSet<_> = provider_state
            .map(execution::instances)
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row["type"] == identity.kind)
            .filter_map(|row| row["provider"].as_str().map(str::to_owned))
            .collect();
        let provider = opt_str(change, "provider")
            .or_else(|| outcome.and_then(|row| opt_str(row, "provider")))
            .map(str::to_owned)
            .or_else(|| {
                (providers.len() == 1).then(|| providers.first().expect("one provider").clone())
            })
            .ok_or("The plan resource's provider binding is unavailable or ambiguous.")?;
        let mut resource = identity.fields(address, &provider);
        resource["change"] = json!({
            "actions":change["actions"],"before":change["before"],"after":change["after"],
            "after_unknown":change.get("after-unknown").cloned().unwrap_or(json!({})),
            "before_sensitive":{},"after_sensitive":{}
        });
        changes.push(resource);
        if change["after"].is_null() {
            planned.remove(address);
        } else {
            let attributes = planned
                .get(address)
                .map(|row| row["attributes"].clone())
                .unwrap_or(json!({}));
            let mut attributes = attributes
                .as_object()
                .cloned()
                .ok_or("The plan resource attributes are unavailable.")?;
            for (field, value) in change["after"]
                .as_object()
                .ok_or("The planned object representation is unavailable.")?
            {
                attributes.insert(field.clone(), value.clone());
            }
            planned.insert(
                address.into(),
                json!({
                    "address":address,"provider":provider,"attributes":attributes
                }),
            );
        }
    }
    let resources = planned
        .into_values()
        .map(|row| {
            let address = str_field(&row, "address");
            let mut resource =
                ResourceAddress::parse(address)?.fields(address, str_field(&row, "provider"));
            resource["schema_version"] = json!(0);
            resource["values"] = row["attributes"].clone();
            resource["sensitive_values"] = json!({});
            Ok(resource)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let errored = str_field(plan, "status") == "errored";
    Ok(json!({
        "format_version":"1.2",
        "terraform_version":run.and_then(|run| opt_str(run, "terraform-version")).unwrap_or("1.9.5"),
        "planned_values":module_values(resources, base.map(outputs).unwrap_or(json!({}))),
        "resource_changes":changes,
        "configuration":{"root_module":{}},
        "timestamp":plan.pointer("/status-timestamps/finished-at").cloned().unwrap_or(Value::Null),
        "applyable":plan.get("has-changes").cloned().unwrap_or(json!(false)),
        "complete":!errored,"errored":errored
    }))
}
