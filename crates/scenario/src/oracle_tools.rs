use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::OnceLock,
};

use anyhow::{Context, bail};
use catalog::Catalog;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::SolutionCall;

struct ToolSchema {
    schema: Value,
    validator: OnceLock<Result<jsonschema::Validator, String>>,
}

struct Snapshot {
    path: String,
    sha256: String,
    tools: BTreeMap<String, ToolSchema>,
}

pub struct OracleTools {
    snapshots: BTreeMap<String, Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentValidation {
    StaticValidated,
    PartiallyDeferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallValidation {
    pub endpoint: String,
    pub tool: String,
    pub snapshot_path: String,
    pub snapshot_sha256: String,
    pub arguments: ArgumentValidation,
    pub deferred_paths: Vec<String>,
    pub result_contract: crate::oracle_results::ResultContract,
    pub allowed_api_errors: Vec<crate::oracle_results::AllowedApiError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotIdentity {
    pub endpoint: String,
    pub path: String,
    pub sha256: String,
    pub tool_count: usize,
}

impl OracleTools {
    pub fn identities(&self) -> Vec<SnapshotIdentity> {
        self.snapshots
            .iter()
            .map(|(endpoint, snapshot)| SnapshotIdentity {
                endpoint: endpoint.clone(),
                path: snapshot.path.clone(),
                sha256: snapshot.sha256.clone(),
                tool_count: snapshot.tools.len(),
            })
            .collect()
    }
    pub fn load(root: &Path, catalog: &Catalog) -> anyhow::Result<Self> {
        let mut snapshots = BTreeMap::new();
        for endpoint in catalog
            .services
            .iter()
            .flat_map(|service| &service.endpoints)
        {
            let path = endpoint
                .snapshot
                .clone()
                .unwrap_or_else(|| format!("servers/snapshots/{}.json", endpoint.id));
            let bytes = fs::read(root.join(&path))
                .with_context(|| format!("reading tool snapshot for {}", endpoint.id))?;
            let document: Value = serde_json::from_slice(&bytes)
                .with_context(|| format!("invalid tool snapshot for {}", endpoint.id))?;
            let definitions = document["tools"]
                .as_array()
                .with_context(|| format!("{} snapshot has no tools array", endpoint.id))?;
            let mut tools = BTreeMap::new();
            for definition in definitions {
                let name = definition["name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .context("snapshot tools require nonempty names")?;
                let schema = definition
                    .get("inputSchema")
                    .filter(|schema| schema.is_object())
                    .with_context(|| {
                        format!("{}.{name} has no input schema object", endpoint.id)
                    })?;
                if tools
                    .insert(
                        name.into(),
                        ToolSchema {
                            schema: schema.clone(),
                            validator: OnceLock::new(),
                        },
                    )
                    .is_some()
                {
                    bail!("{} snapshot repeats tool {name}", endpoint.id);
                }
            }
            let sha256 = format!(
                "sha256:{}",
                Sha256::digest(&bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            );
            if snapshots
                .insert(
                    endpoint.id.clone(),
                    Snapshot {
                        path,
                        sha256,
                        tools,
                    },
                )
                .is_some()
            {
                bail!("duplicate endpoint {}", endpoint.id);
            }
        }
        Ok(Self { snapshots })
    }

    pub fn validate(&self, call: &SolutionCall) -> anyhow::Result<CallValidation> {
        crate::oracle_results::validate(call)?;
        let snapshot = self
            .snapshots
            .get(&call.endpoint)
            .with_context(|| format!("unknown reference endpoint {}", call.endpoint))?;
        let tool = snapshot
            .tools
            .get(&call.tool)
            .with_context(|| format!("unknown reference tool {}.{}", call.endpoint, call.tool))?;
        let validator = tool
            .validator
            .get_or_init(|| {
                jsonschema::options()
                    .offline()
                    .should_validate_formats(false)
                    .build(&tool.schema)
                    .map_err(|_| {
                        format!(
                            "{}.{} has an invalid or non-offline input schema",
                            call.endpoint, call.tool
                        )
                    })
            })
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let mut deferred = BTreeSet::new();
        if call.template {
            match &call.arguments {
                Value::Object(arguments) => {
                    for (key, value) in arguments {
                        if contains_binding(value) {
                            deferred
                                .insert(format!("/{}", key.replace('~', "~0").replace('/', "~1")));
                        }
                    }
                }
                Value::String(text) if whole_binding(text) => {
                    deferred.insert(String::new());
                }
                _ => {}
            }
        }
        if !call.arguments.is_object() && !deferred.contains("") {
            bail!(
                "{}.{} arguments must be an object",
                call.endpoint,
                call.tool
            );
        }
        if !deferred.is_empty() && value_dependent_root(&tool.schema) {
            deferred.clear();
            deferred.insert(String::new());
        }
        if !deferred.contains("") {
            for error in validator.iter_errors(&call.arguments) {
                let path = error.instance_path().to_string();
                let dynamic = deferred.iter().any(|root| {
                    path == *root
                        || path
                            .strip_prefix(root)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                });
                if !dynamic {
                    bail!(
                        "{}.{} arguments violate the locked schema at {} (schema {})",
                        call.endpoint,
                        call.tool,
                        if path.is_empty() { "/" } else { &path },
                        error.schema_path()
                    );
                }
            }
        }
        Ok(CallValidation {
            endpoint: call.endpoint.clone(),
            tool: call.tool.clone(),
            snapshot_path: snapshot.path.clone(),
            snapshot_sha256: snapshot.sha256.clone(),
            arguments: if deferred.is_empty() {
                ArgumentValidation::StaticValidated
            } else {
                ArgumentValidation::PartiallyDeferred
            },
            deferred_paths: deferred.into_iter().collect(),
            result_contract: crate::oracle_results::ResultContract::for_tool(
                &call.endpoint,
                &call.tool,
            ),
            allowed_api_errors: call.allow_api_errors.clone(),
        })
    }
}

fn contains_binding(value: &Value) -> bool {
    match value {
        Value::String(text) => text.contains("${"),
        Value::Object(values) => values.values().any(contains_binding),
        Value::Array(values) => values.iter().any(contains_binding),
        _ => false,
    }
}

fn whole_binding(text: &str) -> bool {
    text.strip_prefix("${")
        .and_then(|text| text.strip_suffix('}'))
        .is_some_and(|name| {
            let mut characters = name.chars();
            characters
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
                && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
}

fn value_dependent_root(schema: &Value) -> bool {
    [
        "$ref",
        "$dynamicRef",
        "allOf",
        "anyOf",
        "oneOf",
        "not",
        "if",
        "then",
        "else",
        "dependencies",
        "dependentSchemas",
        "enum",
        "const",
        "unevaluatedProperties",
    ]
    .iter()
    .any(|keyword| schema.get(*keyword).is_some())
}
