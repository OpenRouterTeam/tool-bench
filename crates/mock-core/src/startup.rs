//! World requests MCP servers make on their own when a session opens (auth checks, API
//! discovery, schema fetches). `deploy/capture-stack.sh` records them per pinned server
//! version; grading leaves them out of the agent's call count.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::journal::JournalEntry;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StartupCall {
    pub service: String,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub query: Option<String>,
}

impl StartupCall {
    fn of(entry: &JournalEntry) -> Self {
        Self {
            service: entry.service.clone(),
            method: entry.method.clone(),
            path: entry.path.clone(),
            query: entry.query.clone(),
        }
    }
}

/// `entries` minus one match per recorded startup call, so an agent repeating a startup
/// request still has that call counted.
pub fn without_startup<'a>(
    entries: &[&'a JournalEntry],
    startup: &[StartupCall],
) -> Vec<&'a JournalEntry> {
    let mut budget: BTreeMap<StartupCall, usize> = BTreeMap::new();
    for call in startup {
        *budget.entry(call.clone()).or_default() += 1;
    }
    entries
        .iter()
        .copied()
        .filter(|e| match budget.get_mut(&StartupCall::of(e)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        })
        .collect()
}
