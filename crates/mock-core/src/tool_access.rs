//! What reached the MCP tool servers, as the tools container saw it.
//!
//! The world sees a call only after a tool server turns it into an API request, and every
//! such request comes from the tools container whoever asked for it. The agent's own
//! shell is on the same network as the tool servers, so it can speak MCP to them directly,
//! outside the harness and outside whatever serves the tools to the model. The tools
//! container therefore reports each MCP request it receives to the world, which seals the
//! reports with the rest of the evidence.
//!
//! The reports identify a client by what it sent: its address, its `User-Agent` and the
//! name it gave when it opened its session. That is what a client says about itself, not
//! an authentication, so the summary flags and counts; it never fails a trial.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The most reports a world keeps. A trial makes a few hundred requests.
pub const MAX_RECORDS: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAccessEvent {
    /// A tool server started and reports here. Its absence means nothing was observed.
    Registered,
    /// One MCP request arrived.
    Request,
    /// A `tools/call` was answered with an error: a JSON-RPC error or `isError`.
    ToolError,
}

/// One report, as the tools container sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAccessReport {
    pub endpoint: String,
    pub event: ToolAccessEvent,
    /// Address of the client, without its port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// The `Mcp-Session-Id` the request carried or was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The JSON-RPC method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// `clientInfo.name` of an `initialize` request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    /// The tool a `tools/call` named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

/// A report in the order the world received it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAccess {
    pub seq: u64,
    #[serde(flatten)]
    pub report: ToolAccessReport,
}

/// Who a client said it was.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ClientIdentity {
    pub peer: Option<String>,
    pub user_agent: Option<String>,
    pub client: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientRequests {
    #[serde(flatten)]
    pub identity: ClientIdentity,
    pub requests: usize,
    pub tool_calls: usize,
}

/// What the sealed reports add up to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolServerSummary {
    /// At least one tool server registered: the counts below are an observation, not an
    /// absence of one.
    pub observed: bool,
    pub requests: usize,
    /// `tools/call` requests, by any client.
    pub tool_calls: usize,
    /// `tools/call` requests answered with an error, by any client. This counts tool
    /// calls, where the world's `refused_calls` and `service_error_calls` count API
    /// requests; a call a tool server rejects before it asks the service is only here.
    pub tool_call_errors: usize,
    /// Every client that made a request, the first to open a session first.
    pub clients: Vec<ClientRequests>,
    /// Requests by clients other than the first one. The harness connects before the
    /// model can act, so the first client is the serving path; a later, different client
    /// reached the tool servers around it.
    pub outside_requests: usize,
    /// Reports the world dropped because it already held [`MAX_RECORDS`].
    pub dropped: usize,
}

/// Sums the reports. A request belongs to the client that opened its session; a request
/// whose session was never seen opening belongs to the client its own headers describe.
pub fn summarize(records: &[ToolAccess], dropped: usize) -> ToolServerSummary {
    let mut sessions: BTreeMap<(&str, &str), ClientIdentity> = BTreeMap::new();
    let mut order: Vec<ClientIdentity> = Vec::new();
    let mut counts: BTreeMap<ClientIdentity, (usize, usize)> = BTreeMap::new();
    let (mut requests, mut tool_calls, mut tool_call_errors) = (0, 0, 0);
    let mut observed = false;
    for record in records {
        let report = &record.report;
        match report.event {
            ToolAccessEvent::Registered => observed = true,
            ToolAccessEvent::ToolError => tool_call_errors += 1,
            ToolAccessEvent::Request => {
                let own = ClientIdentity {
                    peer: report.peer.clone(),
                    user_agent: report.user_agent.clone(),
                    client: report.client.clone(),
                };
                let key = report
                    .session
                    .as_deref()
                    .map(|session| (report.endpoint.as_str(), session));
                let identity = if report.method.as_deref() == Some("initialize") {
                    if let Some(key) = key {
                        sessions.insert(key, own.clone());
                    }
                    own
                } else {
                    key.and_then(|key| sessions.get(&key).cloned())
                        .unwrap_or(own)
                };
                if !order.contains(&identity) {
                    order.push(identity.clone());
                }
                let count = counts.entry(identity).or_default();
                count.0 += 1;
                requests += 1;
                if report.method.as_deref() == Some("tools/call") {
                    count.1 += 1;
                    tool_calls += 1;
                }
            }
        }
    }
    let clients: Vec<ClientRequests> = order
        .into_iter()
        .map(|identity| {
            let (requests, tool_calls) = counts[&identity];
            ClientRequests {
                identity,
                requests,
                tool_calls,
            }
        })
        .collect();
    let outside_requests = clients.iter().skip(1).map(|client| client.requests).sum();
    ToolServerSummary {
        observed: observed || requests > 0,
        requests,
        tool_calls,
        tool_call_errors,
        clients,
        outside_requests,
        dropped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        seq: u64,
        session: &str,
        method: &str,
        agent: &str,
        client: Option<&str>,
    ) -> ToolAccess {
        ToolAccess {
            seq,
            report: ToolAccessReport {
                endpoint: "github".into(),
                event: ToolAccessEvent::Request,
                peer: Some("172.20.0.4".into()),
                user_agent: Some(agent.into()),
                session: Some(session.into()),
                method: Some(method.into()),
                client: client.map(str::to_owned),
                tool: None,
            },
        }
    }

    fn event(seq: u64, event: ToolAccessEvent) -> ToolAccess {
        ToolAccess {
            seq,
            report: ToolAccessReport {
                endpoint: "github".into(),
                event,
                peer: None,
                user_agent: None,
                session: None,
                method: None,
                client: None,
                tool: None,
            },
        }
    }

    #[test]
    fn nothing_registered_is_not_an_observation() {
        let summary = summarize(&[], 0);
        assert!(!summary.observed);
        assert_eq!((summary.requests, summary.outside_requests), (0, 0));
        assert!(summarize(&[event(1, ToolAccessEvent::Registered)], 0).observed);
    }

    #[test]
    fn one_client_is_the_serving_path_however_many_sessions_it_opens() {
        let records = [
            event(1, ToolAccessEvent::Registered),
            request(
                2,
                "tb-000001",
                "initialize",
                "claude-code/2.1",
                Some("claude-code"),
            ),
            request(3, "tb-000001", "tools/list", "claude-code/2.1", None),
            request(
                4,
                "tb-000002",
                "initialize",
                "claude-code/2.1",
                Some("claude-code"),
            ),
            request(5, "tb-000002", "tools/call", "claude-code/2.1", None),
            event(6, ToolAccessEvent::ToolError),
        ];
        let summary = summarize(&records, 0);
        assert_eq!(summary.clients.len(), 1);
        assert_eq!(
            summary.clients[0].identity.client.as_deref(),
            Some("claude-code")
        );
        assert_eq!((summary.requests, summary.tool_calls), (4, 1));
        assert_eq!(summary.tool_call_errors, 1);
        assert_eq!(summary.outside_requests, 0);
    }

    #[test]
    fn a_second_client_is_counted_as_outside_the_serving_path() {
        let records = [
            event(1, ToolAccessEvent::Registered),
            request(
                2,
                "tb-000001",
                "initialize",
                "claude-code/2.1",
                Some("claude-code"),
            ),
            request(3, "tb-000001", "tools/list", "claude-code/2.1", None),
            // The shell: its own session, its own name. Its later requests count as its
            // own even though they carry no client name.
            request(4, "tb-000002", "initialize", "curl/8.5.0", Some("shell")),
            request(5, "tb-000002", "tools/list", "curl/8.5.0", None),
            request(6, "tb-000002", "tools/call", "curl/8.5.0", None),
            request(7, "tb-000001", "tools/call", "claude-code/2.1", None),
        ];
        let summary = summarize(&records, 0);
        assert_eq!(summary.clients.len(), 2);
        assert_eq!(
            summary.clients[1].identity.user_agent.as_deref(),
            Some("curl/8.5.0")
        );
        assert_eq!(
            (summary.clients[1].requests, summary.clients[1].tool_calls),
            (3, 1)
        );
        assert_eq!(summary.outside_requests, 3);
        assert_eq!(summary.tool_calls, 2);
    }

    #[test]
    fn the_reference_program_speaking_from_the_shell_is_its_own_serving_path() {
        // The scripted reference solution calls the tool servers with curl and nothing
        // else does: one client, nothing outside it.
        let records = [
            request(
                1,
                "tb-000001",
                "initialize",
                "curl/7.88.1",
                Some("tool-bench-oracle"),
            ),
            request(2, "tb-000001", "tools/call", "curl/7.88.1", None),
            request(
                3,
                "tb-000002",
                "initialize",
                "curl/7.88.1",
                Some("tool-bench-oracle"),
            ),
            request(4, "tb-000002", "tools/call", "curl/7.88.1", None),
        ];
        let summary = summarize(&records, 0);
        assert!(summary.observed);
        assert_eq!((summary.clients.len(), summary.outside_requests), (1, 0));
    }
}
