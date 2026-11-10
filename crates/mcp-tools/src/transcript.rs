//! Passive transcript of what an MCP endpoint actually served: `initialize`, `tools/list`
//! and `tools/call` response bodies, appended as JSON lines inside the tools container.
//!
//! The strict runner collects it after the agent stops and scans it on the host. Recording
//! never issues a request of its own, so it cannot change what the world observes.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    sync::Mutex,
};

use serde_json::json;

use crate::front::Observer;

/// Default transcript directory inside the tools container.
pub const SURFACE_DIR: &str = "/var/lib/tool-bench/surface";
/// Per-endpoint cap. Beyond it a single overflow record marks the transcript incomplete.
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;

pub struct Transcript {
    endpoint: String,
    limit: u64,
    state: Mutex<(File, u64, bool)>,
}

impl Transcript {
    /// Starts a fresh transcript for `endpoint`, with a first record proving it is live.
    pub fn create(directory: &Path, endpoint: &str) -> std::io::Result<Self> {
        Self::with_limit(directory, endpoint, MAX_BYTES)
    }

    pub fn with_limit(directory: &Path, endpoint: &str, limit: u64) -> std::io::Result<Self> {
        fs::create_dir_all(directory)?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(directory.join(format!("{endpoint}.jsonl")))?;
        writeln!(
            file,
            "{}",
            json!({"endpoint": endpoint, "registered": true})
        )?;
        Ok(Self {
            endpoint: endpoint.to_owned(),
            limit,
            state: Mutex::new((file, 0, false)),
        })
    }
}

impl Observer for Transcript {
    fn served(&self, method: &str, body: &str) {
        if !matches!(method, "initialize" | "tools/list" | "tools/call") {
            return;
        }
        let mut state = self.state.lock().expect("transcript lock");
        if state.2 {
            return;
        }
        let line = if state.1 + body.len() as u64 > self.limit {
            state.2 = true;
            json!({"endpoint": self.endpoint, "overflow": true})
        } else {
            state.1 += body.len() as u64;
            let mut line = json!({"endpoint": self.endpoint, "method": method, "body": body});
            if method == "tools/list" {
                // Definition hashes are computed here so the host audit compares the same
                // identity the exposure manifest pins, without re-serializing JSON.
                line["tools"] = serde_json::from_str::<serde_json::Value>(body)
                    .ok()
                    .and_then(|response| response["result"]["tools"].as_array().cloned())
                    .map(|tools| {
                        tools
                            .iter()
                            .map(|tool| {
                                json!([tool["name"], crate::exposure::definition_sha256(tool)])
                            })
                            .collect()
                    })
                    .unwrap_or(serde_json::Value::Null);
            }
            line
        };
        // A failed write leaves the transcript short; the host audit then lacks the
        // response it expects and reports the surface as uncaptured.
        if writeln!(state.0, "{line}").is_err() {
            state.2 = true;
        }
    }
}
