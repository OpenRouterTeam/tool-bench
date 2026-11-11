use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use crate::events::{EventKind, OperationContext, OperationEvent};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub struct ObservedRequest<'events> {
    pub context: &'events OperationContext,
    pub started: u64,
    pub head: Option<(u64, &'events RequestHead)>,
    pub body: Option<(u64, &'events CapturedBody)>,
}

pub fn observe(events: &[OperationEvent]) -> Result<BTreeMap<u64, ObservedRequest<'_>>, String> {
    let mut requests: BTreeMap<u64, ObservedRequest<'_>> = BTreeMap::new();
    for event in events {
        match &event.event {
            EventKind::RequestStarted => {
                let context = event
                    .operation
                    .as_ref()
                    .ok_or("request start has no identity")?;
                if context.id != event.seq || requests.contains_key(&context.id) {
                    return Err("request-start identities are inconsistent".into());
                }
                requests.insert(
                    context.id,
                    ObservedRequest {
                        context,
                        started: event.seq,
                        head: None,
                        body: None,
                    },
                );
            }
            EventKind::RequestHeadReceived { head } => {
                let context = event
                    .operation
                    .as_ref()
                    .ok_or("request head has no identity")?;
                let observed = requests
                    .get_mut(&context.id)
                    .filter(|observed| observed.context == context)
                    .ok_or("request head has no matching admission")?;
                if observed.head.replace((event.seq, head)).is_some() || observed.body.is_some() {
                    return Err("request head was observed more than once or after its body".into());
                }
            }
            EventKind::RequestBodyReceived { body } => {
                let context = event
                    .operation
                    .as_ref()
                    .ok_or("request body has no identity")?;
                let observed = requests
                    .get_mut(&context.id)
                    .filter(|observed| observed.context == context)
                    .ok_or("request body has no matching admission")?;
                if observed.head.is_none() || observed.body.replace((event.seq, body)).is_some() {
                    return Err(
                        "request body was observed more than once or before its head".into(),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(requests)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "encoding",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CapturedBody {
    Utf8(String),
    Bytes(Vec<u8>),
}

impl CapturedBody {
    pub fn capture(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::Utf8(text.to_owned()),
            Err(_) => Self::Bytes(bytes.to_vec()),
        }
    }

    pub fn bytes(&self) -> Cow<'_, [u8]> {
        match self {
            Self::Utf8(text) => Cow::Borrowed(text.as_bytes()),
            Self::Bytes(bytes) => Cow::Borrowed(bytes),
        }
    }

    pub fn value(&self) -> Value {
        crate::journal::body_value(&self.bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapturedHeaderValue {
    Public { value: String },
    Synthetic { reference: String, sha256: String },
    Redacted { sha256: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedHeader {
    pub name: String,
    pub value: CapturedHeaderValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestHead {
    pub host: Option<String>,
    pub scheme: String,
    pub content_type: Option<String>,
    pub query: Option<String>,
    pub headers: Vec<CapturedHeader>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayContext {
    pub parent_operation_id: Option<u64>,
    pub headers: Vec<CapturedHeader>,
    pub body: CapturedBody,
}

pub fn validate_journal(
    events: &[OperationEvent],
    journal: &[crate::journal::JournalEntry],
) -> Result<(), String> {
    let observed = observe(events)?;
    let mut completed = BTreeMap::new();
    for event in events {
        match &event.event {
            EventKind::StateCommitted { .. } => {
                let context = event
                    .operation
                    .as_ref()
                    .ok_or("mutation has no request identity")?;
                let request = observed
                    .get(&context.id)
                    .ok_or("mutation has no admitted request")?;
                if request.body.is_none_or(|(seq, _)| seq >= event.seq) {
                    return Err("business mutation preceded complete request observation".into());
                }
            }
            EventKind::ResponseCompleted { status, .. } => {
                let context = event
                    .operation
                    .as_ref()
                    .ok_or("response has no request identity")?;
                let request = observed
                    .get(&context.id)
                    .ok_or("response has no admitted request")?;
                if request.head.is_none_or(|(seq, _)| seq >= event.seq)
                    || (*status != 413 && request.body.is_none_or(|(seq, _)| seq >= event.seq))
                {
                    return Err("response preceded required request observation".into());
                }
                completed.insert(context.id, *status);
            }
            _ => {}
        }
    }
    let mut recorded = BTreeSet::new();
    for entry in journal {
        let id = entry
            .operation_id
            .ok_or("journal entry has no operation identity")?;
        let request = observed
            .get(&id)
            .ok_or("journal entry has no admitted request")?;
        if !recorded.insert(id) || completed.get(&id) != Some(&entry.status) {
            return Err("journal response does not match terminal request evidence".into());
        }
        let (_, head) = request.head.ok_or("journal entry has no request head")?;
        let (_, body) = request
            .body
            .ok_or("journal entry has no complete request body")?;
        if entry.service != request.context.service
            || entry.source != request.context.source
            || entry.method != request.context.method
            || entry.path != request.context.path
            || entry.host != head.host
            || entry.scheme != head.scheme
            || entry.content_type != head.content_type
            || entry.query != head.query
            || entry.request_body != body.value()
            || entry.replay.as_ref()
                != Some(&ReplayContext {
                    parent_operation_id: request.context.parent_id,
                    headers: head.headers.clone(),
                    body: body.clone(),
                })
        {
            return Err("journal request differs from its pre-dispatch receipt".into());
        }
    }
    for (id, status) in completed {
        if !recorded.contains(&id) && (status != 413 || observed[&id].body.is_some()) {
            return Err("completed response has no journal observation".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct HeaderRegistry {
    synthetic: BTreeMap<String, SyntheticHeader>,
}

#[derive(Debug, Clone)]
struct SyntheticHeader {
    service: String,
    name: String,
    value: String,
}

impl HeaderRegistry {
    pub fn insert_synthetic(
        &mut self,
        service: &str,
        name: &str,
        reference: &str,
        value: &str,
    ) -> Result<(), String> {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| error.to_string())?;
        HeaderValue::from_str(value).map_err(|error| error.to_string())?;
        if service.is_empty()
            || reference.is_empty()
            || value.is_empty()
            || hop_header(name.as_str())
        {
            return Err(
                "synthetic replay headers require nonempty, non-transport identities".into(),
            );
        }
        if self.synthetic.contains_key(reference) {
            return Err(format!("duplicate synthetic header reference {reference}"));
        }
        self.synthetic.insert(
            reference.into(),
            SyntheticHeader {
                service: service.into(),
                name: name.as_str().into(),
                value: value.into(),
            },
        );
        Ok(())
    }

    pub fn capture(&self, service: &str, headers: &HeaderMap) -> Vec<CapturedHeader> {
        let names: BTreeSet<_> = headers.keys().map(HeaderName::as_str).collect();
        let mut captured = Vec::new();
        for name in names {
            if hop_header(name) {
                continue;
            }
            for value in headers.get_all(name).iter() {
                let known = self.synthetic.iter().find(|(_, header)| {
                    header.service == service
                        && header.name == name
                        && header.value.as_bytes() == value.as_bytes()
                });
                let value = if let Some((reference, _)) = known {
                    CapturedHeaderValue::Synthetic {
                        reference: reference.clone(),
                        sha256: digest(value.as_bytes()),
                    }
                } else if public_header(name) && value.to_str().is_ok() {
                    CapturedHeaderValue::Public {
                        value: value.to_str().expect("validated header").into(),
                    }
                } else {
                    CapturedHeaderValue::Redacted {
                        sha256: digest(value.as_bytes()),
                    }
                };
                captured.push(CapturedHeader {
                    name: name.into(),
                    value,
                });
            }
        }
        captured
    }

    pub fn restore(&self, service: &str, headers: &[CapturedHeader]) -> Result<HeaderMap, String> {
        let mut restored = HeaderMap::new();
        for header in headers {
            let name = HeaderName::from_bytes(header.name.as_bytes())
                .map_err(|error| error.to_string())?;
            if hop_header(name.as_str()) {
                return Err(format!(
                    "transport header {} cannot be restored",
                    header.name
                ));
            }
            let value = match &header.value {
                CapturedHeaderValue::Public { value } if public_header(name.as_str()) => value,
                CapturedHeaderValue::Synthetic { reference, sha256 } => {
                    let known = self.synthetic.get(reference).filter(|known| {
                        known.service == service && known.name == name.as_str()
                            && digest(known.value.as_bytes()) == *sha256
                    }).ok_or_else(|| format!("synthetic header reference {reference} is unavailable or incompatible"))?;
                    &known.value
                }
                CapturedHeaderValue::Redacted { .. } => {
                    return Err(format!(
                        "header {} was redacted without a synthetic replay reference",
                        header.name
                    ));
                }
                _ => {
                    return Err(format!(
                        "header {} is not a declared public header",
                        header.name
                    ));
                }
            };
            restored.append(
                name,
                HeaderValue::from_str(value).map_err(|error| error.to_string())?,
            );
        }
        Ok(restored)
    }
}

fn digest(bytes: &[u8]) -> String {
    let hexadecimal: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256:{hexadecimal}")
}

fn hop_header(name: &str) -> bool {
    matches!(
        name,
        "host"
            | "content-length"
            | "connection"
            | "keep-alive"
            | "proxy-connection"
            | "transfer-encoding"
            | "te"
            | "trailer"
            | "upgrade"
    )
}

fn public_header(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "accept-encoding"
            | "accept-language"
            | "accept-charset"
            | "content-type"
            | "user-agent"
            | "range"
            | "if-match"
            | "if-none-match"
            | "if-modified-since"
            | "cache-control"
            | "pragma"
            | "notion-version"
            | "stripe-version"
            | "x-github-api-version"
            | "x-forwarded-proto"
            | "mcp-protocol-version"
            | "sec-fetch-mode"
            | "sec-fetch-site"
            | "sec-fetch-dest"
            | "priority"
            | "x-stainless-lang"
            | "x-stainless-package-version"
            | "x-stainless-os"
            | "x-stainless-arch"
            | "x-stainless-runtime"
            | "x-stainless-runtime-version"
            | "x-stainless-retry-count"
            | "x-stainless-timeout"
            | "x-disable-provenance"
            | "amz-sdk-request"
            | "amz-sdk-invocation-id"
            | "x-amz-user-agent"
    )
}
