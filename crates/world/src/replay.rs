//! Determinism check: replays a recorded journal against a fresh world started from the
//! same scenario and reports every response that differs from the recording.

use std::{
    collections::{BTreeMap, BTreeSet},
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

use mock_core::{
    dataset::Dataset,
    journal::{JournalEntry, body_value},
    world::{WorldState, shared},
};

use crate::{PortChoice, spawn};

#[derive(Debug, Clone, PartialEq)]
pub struct Mismatch {
    pub seq: u64,
    pub kind: ReplayIssue,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayIssue {
    ContextUnavailable,
    UnsupportedService,
    ResponseMismatch,
}

pub async fn replay(
    scenario_dir: &Path,
    entries: &[JournalEntry],
) -> anyhow::Result<Vec<Mismatch>> {
    let world = shared(WorldState::from_dataset(Dataset::load(scenario_dir)?)?);
    let running = spawn(
        world,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        PortChoice::Ephemeral,
    )
    .await?;
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut mismatches = Vec::new();
    let registry = crate::replay_headers::registry().map_err(anyhow::Error::msg)?;
    let mut prepared = BTreeMap::new();
    let mut operations = BTreeMap::new();
    let mut previous_external = None;
    for (index, entry) in entries.iter().enumerate() {
        let unavailable = |detail: String| Mismatch {
            seq: entry.seq,
            kind: ReplayIssue::ContextUnavailable,
            detail,
        };
        if entry.seq != index as u64 + 1 {
            mismatches.push(unavailable(
                "journal sequence is incomplete or inconsistent".into(),
            ));
        }
        let Some(context) = &entry.replay else {
            mismatches.push(unavailable("complete replay context was not recorded; this legacy journal cannot be faithfully replayed".into()));
            continue;
        };
        let Some(operation_id) = entry.operation_id else {
            mismatches.push(unavailable("request admission identity is missing".into()));
            continue;
        };
        if operations.insert(operation_id, entry).is_some()
            || context.body.value() != entry.request_body
        {
            mismatches.push(unavailable(
                "request identity or raw body is inconsistent".into(),
            ));
        }
        if !running.services.contains_key(entry.service.as_str()) {
            mismatches.push(Mismatch {
                seq: entry.seq,
                kind: ReplayIssue::UnsupportedService,
                detail: format!("unknown service {}", entry.service),
            });
        }
        if entry.scheme == "internal" {
            continue;
        }
        if context.parent_operation_id.is_some()
            || previous_external.is_some_and(|previous| previous >= operation_id)
        {
            mismatches.push(unavailable("response order is not a serial request-admission order; scheduler-aware replay is required".into()));
        }
        previous_external = Some(operation_id);
        if !entry.path.starts_with('/') || !matches!(entry.scheme.as_str(), "http" | "https") {
            mismatches.push(unavailable(
                "request target or scheme is unsupported".into(),
            ));
        }
        match registry.restore(&entry.service, &context.headers) {
            Ok(headers) => {
                if headers
                    .get("content-type")
                    .and_then(|value| value.to_str().ok())
                    != entry.content_type.as_deref()
                {
                    mismatches.push(unavailable(
                        "content type differs from its captured header".into(),
                    ));
                }
                if headers
                    .get("x-forwarded-proto")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("http")
                    != entry.scheme
                {
                    mismatches.push(unavailable(
                        "request scheme differs from its captured header".into(),
                    ));
                }
                prepared.insert(entry.seq, headers);
            }
            Err(reason) => mismatches.push(unavailable(reason)),
        }
    }
    for entry in entries.iter().filter(|entry| entry.scheme == "internal") {
        let mut parent = entry
            .replay
            .as_ref()
            .and_then(|context| context.parent_operation_id);
        let mut seen = BTreeSet::new();
        let mut rooted = false;
        while let Some(parent_id) = parent {
            if !seen.insert(parent_id) {
                break;
            }
            let Some(ancestor) = operations.get(&parent_id) else {
                break;
            };
            if ancestor.scheme != "internal" {
                rooted = true;
                break;
            }
            parent = ancestor
                .replay
                .as_ref()
                .and_then(|context| context.parent_operation_id);
        }
        if !rooted {
            mismatches.push(Mismatch {
                seq: entry.seq,
                kind: ReplayIssue::ContextUnavailable,
                detail: "internal operation has no fully recorded external request to reproduce it"
                    .into(),
            });
        }
    }
    if !mismatches.is_empty() {
        return Ok(mismatches);
    }
    for entry in entries {
        // Sub-requests recorded inside a tool call are reproduced by replaying that call.
        if entry.scheme == "internal" {
            continue;
        }
        let addr = running.services[entry.service.as_str()];
        let url = match &entry.query {
            Some(q) => format!("http://{addr}{}?{q}", entry.path),
            None => format!("http://{addr}{}", entry.path),
        };
        let method = reqwest::Method::from_bytes(entry.method.as_bytes())?;
        let mut request = http.request(method, url);
        request = request.headers(
            prepared
                .remove(&entry.seq)
                .expect("validated replay headers"),
        );
        if let Some(host) = &entry.host {
            request = request.header("host", host);
        }
        request = request.body(
            entry
                .replay
                .as_ref()
                .expect("validated replay context")
                .body
                .bytes()
                .into_owned(),
        );
        let response = request.send().await?;
        let status = response.status().as_u16();
        let body = body_value(&response.bytes().await?);
        if status != entry.status {
            mismatches.push(Mismatch {
                seq: entry.seq,
                kind: ReplayIssue::ResponseMismatch,
                detail: format!("status {status} != recorded {}", entry.status),
            });
        } else if body != entry.response_body {
            mismatches.push(Mismatch {
                seq: entry.seq,
                kind: ReplayIssue::ResponseMismatch,
                detail: format!("{} {} body differs", entry.method, entry.path),
            });
        }
    }
    Ok(mismatches)
}
