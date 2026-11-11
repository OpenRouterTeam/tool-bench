//! Controlled tool exposure at the bridge: the tools container registers exactly the
//! endpoints and tools a task's registration directive names, in its order.
//!
//! The directive only selects and orders. Every listed definition must still hash to its
//! pinned value, so a definition can be hidden or moved but never edited or invented.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::front::{Backend, Reply};

/// Tools-container variable carrying the compact JSON [`ExposureDirective`].
pub const EXPOSURE_ENV: &str = "TOOL_BENCH_EXPOSURE";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointExposure {
    pub id: String,
    /// Registered tool names in the order `tools/list` returns them. `None` registers
    /// every tool the endpoint serves, in its served order.
    pub tools: Option<Vec<String>>,
    /// [`sequence_sha256`] of the registered `(name, pinned definition hash)` sequence.
    pub sha256: String,
}

/// What the tools container registers for one task. Carries no condition label or seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposureDirective {
    pub schema_version: u32,
    /// Registered endpoints in realized order.
    pub endpoints: Vec<EndpointExposure>,
}

impl ExposureDirective {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let directive: Self = serde_json::from_str(text)?;
        let mut endpoints = std::collections::BTreeSet::new();
        if directive.schema_version != 1 || directive.endpoints.is_empty() {
            anyhow::bail!("unsupported or empty exposure directive");
        }
        for endpoint in &directive.endpoints {
            let mut tools = std::collections::BTreeSet::new();
            if !endpoints.insert(&endpoint.id)
                || endpoint.tools.as_ref().is_some_and(|names| {
                    names.is_empty() || names.iter().any(|name| !tools.insert(name))
                })
            {
                anyhow::bail!("exposure directive repeats or empties an endpoint or tool");
            }
        }
        Ok(directive)
    }

    pub fn endpoint(&self, id: &str) -> Option<&EndpointExposure> {
        self.endpoints.iter().find(|endpoint| endpoint.id == id)
    }
}

/// `sha256:` hash of a tool definition's compact JSON, the identity pinned by exposure
/// manifests and checked again when the definition is served.
pub fn definition_sha256(definition: &Value) -> String {
    format!(
        "sha256:{}",
        crate::snapshot::sha256_hex(
            &serde_json::to_vec(definition).expect("definitions serialize")
        )
    )
}

/// Hash of an ordered `(tool name, definition hash)` sequence.
pub fn sequence_sha256<'a>(tools: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut bytes = Vec::new();
    for (name, definition) in tools {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(definition.as_bytes());
        bytes.push(0);
    }
    format!("sha256:{}", crate::snapshot::sha256_hex(&bytes))
}

/// Restricts a backend to the registered tools. Unregistered tools are absent from
/// `tools/list` and unknown to `tools/call`.
pub struct ExposedBackend<B> {
    inner: B,
    exposure: EndpointExposure,
}

impl<B> ExposedBackend<B> {
    pub fn new(inner: B, exposure: &EndpointExposure) -> Self {
        Self {
            inner,
            exposure: exposure.clone(),
        }
    }

    /// Selects and orders a full `tools/list` result, failing closed on any drift.
    fn select(&self, response: &mut Value) -> Result<(), String> {
        let Some(result) = response.get_mut("result") else {
            return Ok(());
        };
        if !result["nextCursor"].is_null() {
            return Err(
                "a paginated tool list cannot be registered under controlled exposure".into(),
            );
        }
        let listed = result["tools"]
            .as_array()
            .ok_or("the server returned no tool list")?;
        let selected: Vec<Value> = match &self.exposure.tools {
            None => listed.clone(),
            Some(names) => names
                .iter()
                .map(|name| {
                    listed
                        .iter()
                        .find(|definition| definition["name"] == name.as_str())
                        .cloned()
                        .ok_or("a registered tool is not served by its endpoint")
                })
                .collect::<Result<_, _>>()?,
        };
        let hashes: Vec<String> = selected.iter().map(definition_sha256).collect();
        let served = sequence_sha256(selected.iter().zip(&hashes).map(|(definition, hash)| {
            (
                definition["name"].as_str().unwrap_or_default(),
                hash.as_str(),
            )
        }));
        if served != self.exposure.sha256 {
            return Err("registered tools differ from their pinned definitions".into());
        }
        result["tools"] = Value::Array(selected);
        Ok(())
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

impl<B: Backend> Backend for ExposedBackend<B> {
    type Session = B::Session;

    async fn open(&self) -> anyhow::Result<Self::Session> {
        self.inner.open().await
    }

    async fn request(&self, session: &Self::Session, msg: Value) -> anyhow::Result<Reply> {
        let id = msg["id"].clone();
        if msg["method"] == "tools/call" {
            let name = msg["params"]["name"].as_str().unwrap_or_default();
            if self
                .exposure
                .tools
                .as_ref()
                .is_some_and(|names| !names.iter().any(|registered| registered == name))
            {
                return Ok(Reply::Json(error(
                    id,
                    -32602,
                    &format!("Unknown tool: {name}"),
                )));
            }
            return self.inner.request(session, msg).await;
        }
        let is_list = msg["method"] == "tools/list";
        let reply = self.inner.request(session, msg).await?;
        if !is_list {
            return Ok(reply);
        }
        let Reply::Json(mut response) = reply else {
            return Ok(Reply::Json(error(
                id,
                -32603,
                "unsupported streamed tool list",
            )));
        };
        Ok(Reply::Json(match self.select(&mut response) {
            Ok(()) => response,
            Err(reason) => error(id, -32603, &reason),
        }))
    }

    async fn notify(&self, session: &Self::Session, msg: Value) -> anyhow::Result<()> {
        self.inner.notify(session, msg).await
    }

    async fn close(&self, session: Arc<Self::Session>) {
        self.inner.close(session).await;
    }
}
