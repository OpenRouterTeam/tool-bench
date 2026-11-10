//! Scores a trial from the world's final state and journal. No LLM judges: every check is
//! a JSON pointer, a call pattern, or a text match.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod answers;
mod information;
mod validation;

pub use answers::{FactColumn, FactKind, FactRows, OncallAnswer, SentryDsn, SourceFacts};
pub use information::{
    FieldFormat, InformationCheck, InformationField, InformationRead, InformationSeries,
    InformationWindow,
};

use crate::{
    events::{self, HistoryError, HistoryFailureKind, OperationEvent, OrderingCheck},
    journal::JournalEntry,
    preservation::Preservation,
    relations::{RecordCheck, RecordCheckError, RelationCheck},
    request,
    startup::{StartupCall, without_startup},
};

pub const EVALUATOR_VERSION: u32 = 44;

/// What a scenario expects (generated from `scenario.toml` into the task's `tests/expect.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default = "default_contract_version")]
    pub version: u32,
    #[serde(default)]
    pub state: Vec<StateCheck>,
    #[serde(default)]
    pub records: Vec<RecordCheck>,
    #[serde(default)]
    pub relations: Vec<RelationCheck>,
    #[serde(default)]
    pub audit_history: bool,
    #[serde(default)]
    pub ordering: Vec<OrderingCheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub information: Vec<InformationCheck>,
    #[serde(default)]
    pub required_calls: Vec<CallMatch>,
    #[serde(default)]
    pub forbidden_calls: Vec<CallMatch>,
    #[serde(default)]
    pub answer: Option<AnswerCheck>,
    #[serde(default)]
    pub preservation: Option<Preservation>,
    #[serde(default)]
    pub interfaces: InterfacePolicy,
    #[serde(default)]
    pub environment_faults: Vec<EnvironmentFault>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentFault {
    pub code: String,
    pub reason: String,
}

fn default_contract_version() -> u32 {
    1
}

impl Default for Expect {
    fn default() -> Self {
        Self {
            version: default_contract_version(),
            state: Vec::new(),
            records: Vec::new(),
            relations: Vec::new(),
            audit_history: false,
            ordering: Vec::new(),
            information: Vec::new(),
            required_calls: Vec::new(),
            forbidden_calls: Vec::new(),
            answer: None,
            preservation: None,
            interfaces: InterfacePolicy::default(),
            environment_faults: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfacePolicy {
    #[default]
    McpOnly,
    DirectAllowed,
}

/// A check on the final state snapshot (`/_bench/state`), addressed by RFC 6901 pointer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateCheck {
    pub pointer: String,
    /// The value equals this exactly.
    #[serde(default)]
    pub equals: Option<Value>,
    /// The value exists (true) or is absent (false).
    #[serde(default)]
    pub exists: Option<bool>,
    /// Array: some element matches this (subset match). String: contains this text.
    #[serde(default)]
    pub contains: Option<Value>,
    /// Some element of the array, or some value of the object, loosely matches this:
    /// objects by key subset, strings by substring, other values by equality. Use it for
    /// records whose keys an agent's actions decide (e.g. the n-th comment on a ticket).
    #[serde(default)]
    pub any: Option<Value>,
    /// With `any` on an object: only entries whose key starts with this.
    #[serde(default)]
    pub key_prefix: Option<String>,
    #[serde(default)]
    pub is_null: Option<bool>,
    #[serde(default)]
    pub text_equals: Option<String>,
    #[serde(default)]
    pub member_ascii_ci: Option<String>,
}

/// A pattern over journal entries that came from the tools container.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallMatch {
    pub service: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub path_regex: Option<String>,
    #[serde(default)]
    pub query_contains: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query_fields: BTreeMap<String, String>,
    /// The request body contains this JSON (objects: subset; arrays: each element found).
    #[serde(default)]
    pub body: Option<Value>,
    /// The raw request body text contains this (for GraphQL, form bodies).
    #[serde(default)]
    pub body_contains: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default = "one")]
    pub min_count: usize,
    #[serde(default)]
    pub response_status: Vec<u16>,
    #[serde(default)]
    pub allow_tool_error: bool,
}

fn one() -> usize {
    1
}

/// Checks on the agent's final answer (`/app/answer.md`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerCheck {
    /// Each string must appear, case-insensitively.
    #[serde(default)]
    pub contains: Vec<String>,
    #[serde(default)]
    pub regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentry_dsn: Option<SentryDsn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oncall: Option<OncallAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_facts: Option<SourceFacts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    /// The harness or provider rejected the request for length before any tool ran.
    ContextOverflow,
    IntegrityViolation,
    InvalidContract,
    EnvironmentError,
    Unresolved,
}

impl Outcome {
    pub fn code(self) -> f64 {
        match self {
            Self::Pass => 0.0,
            Self::Fail => 1.0,
            Self::ContextOverflow => 2.0,
            Self::IntegrityViolation => 3.0,
            Self::InvalidContract => 4.0,
            Self::EnvironmentError => 5.0,
            Self::Unresolved => 6.0,
        }
    }
}

pub struct GradeInput<'a> {
    pub expect: &'a Expect,
    /// `WorldState::snapshot()` at the end of the agent phase.
    pub state: Value,
    pub journal: &'a [JournalEntry],
    pub answer: Option<&'a str>,
    /// Peer IPs of the tools container; anything else reaching the world is an MCP bypass.
    pub tools_ips: Vec<String>,
    /// Agent logs contained a context-length rejection.
    pub context_overflow: bool,
    /// Requests MCP servers make when a session opens; not counted as agent calls.
    pub startup: &'a [StartupCall],
    pub initial_state: Option<&'a Value>,
    pub events: Option<&'a [OperationEvent]>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckFailure {
    pub check: CheckKind,
    pub disposition: CheckDisposition,
    pub location: String,
    pub reason: String,
    pub evidence: Vec<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub event_evidence: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckDisposition {
    Failed,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    State,
    Records,
    Relations,
    Ordering,
    RequiredCalls,
    ForbiddenCalls,
    Answer,
    Integrity,
    Preservation,
    Contract,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Grade {
    pub evaluator_version: u32,
    pub contract_version: u32,
    pub reward: f64,
    pub outcome: Outcome,
    pub state_ok: bool,
    pub records_ok: bool,
    pub relations_ok: bool,
    pub ordering_ok: bool,
    pub history_ok: bool,
    pub history_checked: bool,
    pub required_calls_ok: bool,
    pub forbidden_calls_ok: bool,
    pub answer_ok: bool,
    pub integrity_ok: bool,
    pub preservation_ok: bool,
    pub preservation_checked: bool,
    pub contract_ok: bool,
    pub environment_ok: bool,
    pub runner_admission_checked: bool,
    pub runner_admission_ok: bool,
    pub adjudication_needed: bool,
    pub failures: Vec<CheckFailure>,
    /// World requests made through MCP servers, excluding their startup requests.
    pub api_calls: usize,
    /// World requests that did not come from the tools container.
    pub mcp_bypass: usize,
}

impl Grade {
    pub fn admit_runner(&mut self, admission: Result<(), String>) {
        self.runner_admission_checked = true;
        if let Err(reason) = admission {
            self.runner_admission_ok = false;
            self.invalidate_environment("evidence/runner-admission", reason);
        }
    }

    pub fn invalidate_contract(&mut self, location: impl Into<String>, reason: impl Into<String>) {
        self.contract_ok = false;
        self.outcome = Outcome::InvalidContract;
        self.reward = 0.0;
        self.failures.push(CheckFailure {
            check: CheckKind::Contract,
            disposition: CheckDisposition::Failed,
            location: location.into(),
            reason: reason.into(),
            evidence: Vec::new(),
            event_evidence: Vec::new(),
        });
    }

    pub fn invalidate_environment(
        &mut self,
        location: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.environment_ok = false;
        self.outcome = if self.contract_ok {
            Outcome::EnvironmentError
        } else {
            Outcome::InvalidContract
        };
        self.reward = 0.0;
        self.failures.push(CheckFailure {
            check: CheckKind::Environment,
            disposition: CheckDisposition::Failed,
            location: location.into(),
            reason: reason.into(),
            evidence: Vec::new(),
            event_evidence: Vec::new(),
        });
    }

    /// Harbor `reward.json`: a flat map of numbers.
    pub fn metrics(&self) -> BTreeMap<&'static str, f64> {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        BTreeMap::from([
            ("reward", self.reward),
            ("outcome_code", self.outcome.code()),
            ("state_ok", b(self.state_ok)),
            ("records_ok", b(self.records_ok)),
            ("relations_ok", b(self.relations_ok)),
            ("ordering_ok", b(self.ordering_ok)),
            ("history_ok", b(self.history_ok)),
            ("history_checked", b(self.history_checked)),
            ("required_calls_ok", b(self.required_calls_ok)),
            ("forbidden_calls_ok", b(self.forbidden_calls_ok)),
            ("answer_ok", b(self.answer_ok)),
            ("integrity_ok", b(self.integrity_ok)),
            ("preservation_ok", b(self.preservation_ok)),
            ("preservation_checked", b(self.preservation_checked)),
            ("contract_ok", b(self.contract_ok)),
            ("environment_ok", b(self.environment_ok)),
            ("runner_admission_checked", b(self.runner_admission_checked)),
            ("runner_admission_ok", b(self.runner_admission_ok)),
            ("adjudication_needed", b(self.adjudication_needed)),
            ("evaluator_version", f64::from(self.evaluator_version)),
            ("contract_version", f64::from(self.contract_version)),
            ("api_calls", self.api_calls as f64),
            ("mcp_bypass", self.mcp_bypass as f64),
        ])
    }
}

pub fn grade(input: &GradeInput) -> Grade {
    let (via_tools, bypass): (Vec<&JournalEntry>, Vec<&JournalEntry>) = input
        .journal
        .iter()
        .partition(|e| input.tools_ips.contains(&e.source));
    let agent_calls = without_startup(&via_tools, input.startup);
    let completed_bypasses: BTreeSet<_> = bypass
        .iter()
        .filter_map(|entry| entry.operation_id)
        .collect();
    let bypass_starts: BTreeSet<_> = input
        .events
        .into_iter()
        .flatten()
        .filter(|event| matches!(&event.event, events::EventKind::RequestStarted))
        .filter_map(|event| event.operation.as_ref())
        .filter(|operation| {
            operation.kind == events::OperationKind::Http
                && !input.tools_ips.contains(&operation.source)
        })
        .map(|operation| operation.id)
        .collect();
    let mcp_bypass = bypass.len() + bypass_starts.difference(&completed_bypasses).count();

    let required_entries = if input.expect.interfaces == InterfacePolicy::DirectAllowed {
        let entries: Vec<_> = input.journal.iter().collect();
        without_startup(&entries, input.startup)
    } else {
        agent_calls.clone()
    };
    let answer_result = input.expect.answer.as_ref().map_or(Ok(()), |check| {
        check.check(input.answer, input.initial_state, &input.state)
    });

    let integrity_ok = mcp_bypass == 0 || input.expect.interfaces == InterfacePolicy::DirectAllowed;
    let mut failures = Vec::new();
    for event in input.events.into_iter().flatten() {
        if let events::EventKind::EnvironmentFailed { code, reason } = &event.event {
            failures.push(CheckFailure {
                check: CheckKind::Environment,
                disposition: CheckDisposition::Failed,
                location: format!("environment/{code}"),
                reason: reason.clone(),
                evidence: Vec::new(),
                event_evidence: vec![event.seq],
            });
        }
    }
    let contract_ok = match input.expect.validate() {
        Ok(()) => true,
        Err(reason) => {
            failures.push(CheckFailure {
                check: CheckKind::Contract,
                disposition: CheckDisposition::Failed,
                location: "expect".into(),
                reason,
                evidence: Vec::new(),
                event_evidence: Vec::new(),
            });
            false
        }
    };
    if contract_ok {
        for fault in &input.expect.environment_faults {
            failures.push(CheckFailure {
                check: CheckKind::Environment,
                disposition: CheckDisposition::Failed,
                location: format!("environment/declared/{}", fault.code),
                reason: fault.reason.clone(),
                evidence: Vec::new(),
                event_evidence: Vec::new(),
            });
        }
    }
    let preservation_checked = input.expect.version >= 2 || input.expect.preservation.is_some();
    let preservation_ok = if !contract_ok {
        false
    } else if preservation_checked {
        let default_policy = Preservation::default();
        let policy = input
            .expect
            .preservation
            .as_ref()
            .unwrap_or(&default_policy);
        match input.initial_state {
            Some(initial) => match policy.check(initial, &input.state) {
                Ok(violations) => {
                    let passed = violations.is_empty();
                    failures.extend(violations.into_iter().map(|violation| CheckFailure {
                        check: CheckKind::Preservation,
                        disposition: CheckDisposition::Failed,
                        location: violation.pointer,
                        reason: violation.reason,
                        evidence: Vec::new(),
                        event_evidence: Vec::new(),
                    }));
                    passed
                }
                Err(reason) => {
                    failures.push(CheckFailure {
                        check: CheckKind::Environment,
                        disposition: CheckDisposition::Failed,
                        location: "state".into(),
                        reason,
                        evidence: Vec::new(),
                        event_evidence: Vec::new(),
                    });
                    false
                }
            },
            None => {
                failures.push(CheckFailure {
                    check: CheckKind::Environment,
                    disposition: CheckDisposition::Failed,
                    location: "initial_state".into(),
                    reason: "preservation requires authoritative initial state".into(),
                    evidence: Vec::new(),
                    event_evidence: Vec::new(),
                });
                false
            }
        }
    } else {
        true
    };
    let mut historical_state_checks = Vec::new();
    for (index, check) in input.expect.state.iter().enumerate() {
        if !state_check(&input.state, check) {
            failures.push(CheckFailure {
                check: CheckKind::State,
                disposition: CheckDisposition::Failed,
                location: check.pointer.clone(),
                reason: format!("state expectation {index} was not satisfied"),
                evidence: Vec::new(),
                event_evidence: Vec::new(),
            });
        } else if check.pointer.starts_with("/services/") && check.text_equals.is_some() {
            historical_state_checks.push((index, check));
        }
    }
    let history_checked = input.expect.audit_history || !input.expect.ordering.is_empty();
    let mut historical_checks = Vec::new();
    for (index, check) in input.expect.records.iter().enumerate() {
        let eligible = match check.check(input.initial_state, &input.state) {
            Ok(()) => true,
            Err(error) => {
                let eligible = matches!(&error, RecordCheckError::NeedsAdjudication(_));
                failures.push(record_failure(
                    error,
                    CheckKind::Records,
                    format!("records/{index}"),
                ));
                eligible
            }
        };
        if contract_ok && history_checked && eligible && check.has_committed_requirements() {
            match check.selector.select(input.initial_state, &input.state) {
                Ok(records) => historical_checks.push((
                    index,
                    check,
                    records
                        .into_iter()
                        .map(|(key, _)| key)
                        .collect::<BTreeSet<_>>(),
                )),
                Err(error) => failures.push(record_failure(
                    error,
                    CheckKind::Records,
                    format!("records/{index}/history"),
                )),
            }
        }
    }
    for (index, check) in input.expect.relations.iter().enumerate() {
        if let Err(error) = check.check(input.initial_state, &input.state) {
            failures.push(record_failure(
                error,
                CheckKind::Relations,
                format!("relations/{index}"),
            ));
        }
    }
    let mut history_verified = !history_checked;
    if contract_ok && history_checked {
        let default_policy = Preservation::default();
        let policy = preservation_checked.then(|| {
            input
                .expect
                .preservation
                .as_ref()
                .unwrap_or(&default_policy)
        });
        let mut historical_failures = Vec::new();
        let result = match (input.initial_state, input.events) {
            (Some(initial), Some(events)) => events::audit_with_observer(
                initial,
                &input.state,
                events,
                policy,
                &input.expect.ordering,
                |sequence, before, after| {
                    for (index, check) in &historical_state_checks {
                        if before.pointer(&check.pointer) == after.pointer(&check.pointer) {
                            continue;
                        }
                        let Some(expected) = &check.text_equals else {
                            continue;
                        };
                        let unreviewed = match after.pointer(&check.pointer) {
                            None | Some(Value::Null) => false,
                            Some(Value::String(text)) => {
                                !text.trim().is_empty()
                                    && text.split_whitespace().ne(expected.split_whitespace())
                            }
                            Some(_) => true,
                        };
                        if unreviewed {
                            historical_failures.push(CheckFailure {
                                check: CheckKind::State,
                                disposition: CheckDisposition::Unresolved,
                                location: format!("state/{index}/history"),
                                reason: format!(
                                    "committed content at {} requires review against the requested literal: {expected}",
                                    check.pointer
                                ),
                                evidence: Vec::new(),
                                event_evidence: vec![sequence],
                            });
                        }
                    }
                    for (index, check, targets) in &historical_checks {
                        let previous = match check.selector.entries(before) {
                            Ok(records) => records.into_iter().collect::<BTreeMap<_, _>>(),
                            Err(error) => {
                                let mut failure = record_failure(
                                    error,
                                    CheckKind::Records,
                                    format!("records/{index}/history"),
                                );
                                failure.event_evidence.push(sequence);
                                historical_failures.push(failure);
                                continue;
                            }
                        };
                        let current = match check.selector.entries(after) {
                            Ok(records) => records,
                            Err(error) => {
                                let mut failure = record_failure(
                                    error,
                                    CheckKind::Records,
                                    format!("records/{index}/history"),
                                );
                                failure.event_evidence.push(sequence);
                                historical_failures.push(failure);
                                continue;
                            }
                        };
                        for (key, record) in current {
                            if targets.contains(&key)
                                && let Err(error) = check.check_committed_requirements(
                                    input.initial_state,
                                    after,
                                    previous.get(&key).copied(),
                                    record,
                                )
                            {
                                let mut failure = record_failure(
                                    error,
                                    CheckKind::Records,
                                    format!("records/{index}/history/{key}"),
                                );
                                failure.event_evidence.push(sequence);
                                historical_failures.push(failure);
                            }
                        }
                    }
                },
            ),
            _ => Err(HistoryError::MissingEvidence {
                event: None,
                reason: "history checks require authoritative initial state and mutation events"
                    .into(),
            }),
        };
        match result {
            Ok(violations) => {
                history_verified = true;
                failures.extend(historical_failures);
                failures.extend(violations.into_iter().map(|violation| CheckFailure {
                    check: match violation.kind {
                        HistoryFailureKind::Preservation => CheckKind::Preservation,
                        HistoryFailureKind::Ordering => CheckKind::Ordering,
                    },
                    disposition: CheckDisposition::Failed,
                    location: violation.location,
                    reason: violation.reason,
                    evidence: Vec::new(),
                    event_evidence: violation.events,
                }));
            }
            Err(error) => {
                let (check, reason, event_evidence) = match error {
                    HistoryError::InvalidContract(reason) => {
                        (CheckKind::Contract, reason, Vec::new())
                    }
                    HistoryError::MissingEvidence { event, reason } => {
                        (CheckKind::Environment, reason, event.into_iter().collect())
                    }
                };
                failures.push(CheckFailure {
                    check,
                    disposition: CheckDisposition::Failed,
                    location: "events".into(),
                    reason,
                    evidence: Vec::new(),
                    event_evidence,
                });
            }
        }
    }
    let requests = input.events.map(request::observe).transpose();
    if let Err(reason) = &requests {
        failures.push(CheckFailure {
            check: CheckKind::Environment,
            disposition: CheckDisposition::Failed,
            location: "evidence/requests".into(),
            reason: reason.clone(),
            evidence: Vec::new(),
            event_evidence: Vec::new(),
        });
    }
    let observed_requests = requests.as_ref().ok().and_then(Option::as_ref);
    let request_head = |entry: &JournalEntry| {
        entry
            .operation_id
            .and_then(|identity| observed_requests?.get(&identity))
            .and_then(|request| request.head.map(|(_, head)| head))
    };
    for (index, matcher) in input.expect.required_calls.iter().enumerate() {
        let candidates: Vec<_> = required_entries
            .iter()
            .filter(|entry| {
                call_matches(entry, matcher)
                    && headers_match(
                        matcher,
                        request_head(entry),
                        &format!("required_calls/{index}"),
                        &mut failures,
                    )
            })
            .collect();
        if candidates
            .iter()
            .filter(|entry| required_response_matches(entry, matcher))
            .count()
            < matcher.min_count
        {
            failures.push(CheckFailure {
                check: CheckKind::RequiredCalls,
                disposition: CheckDisposition::Failed,
                location: format!("required_calls/{index}"),
                reason: format!(
                    "fewer than {} accepted {} operations were observed",
                    matcher.min_count, matcher.service
                ),
                evidence: candidates.iter().map(|entry| entry.seq).collect(),
                event_evidence: Vec::new(),
            });
        }
    }
    for (index, matcher) in input.expect.forbidden_calls.iter().enumerate() {
        let evidence: Vec<_> = input
            .journal
            .iter()
            .filter(|entry| {
                call_matches(entry, matcher)
                    && headers_match(
                        matcher,
                        request_head(entry),
                        &format!("forbidden_calls/{index}"),
                        &mut failures,
                    )
            })
            .map(|entry| entry.seq)
            .collect();
        let event_evidence: Vec<_> = requests
            .as_ref()
            .ok()
            .and_then(|observed| observed.as_ref())
            .into_iter()
            .flat_map(|observed| observed.values())
            .filter(|observed| {
                matcher.response_status.is_empty()
                    && request_parts_match(
                        &observed.context.service,
                        &observed.context.method,
                        &observed.context.path,
                        observed.head.and_then(|(_, head)| head.query.as_deref()),
                        observed.body.map(|(_, body)| body.value()).as_ref(),
                        matcher,
                    )
                    && headers_match(
                        matcher,
                        observed.head.map(|(_, head)| head),
                        &format!("forbidden_calls/{index}"),
                        &mut failures,
                    )
            })
            .map(|observed| {
                if matcher.body.is_some() || matcher.body_contains.is_some() {
                    observed.body.map_or(observed.started, |(seq, _)| seq)
                } else if matcher.query_contains.is_some()
                    || !matcher.query_fields.is_empty()
                    || !matcher.headers.is_empty()
                {
                    observed.head.map_or(observed.started, |(seq, _)| seq)
                } else {
                    observed.started
                }
            })
            .collect();
        if !evidence.is_empty() || !event_evidence.is_empty() {
            failures.push(CheckFailure {
                check: CheckKind::ForbiddenCalls,
                disposition: CheckDisposition::Failed,
                location: format!("forbidden_calls/{index}"),
                reason: "a prohibited request was observed".into(),
                evidence,
                event_evidence,
            });
        }
    }
    if contract_ok && !input.expect.information.is_empty() {
        failures.extend(information::audit(input, &required_entries));
    }
    if !integrity_ok {
        failures.push(CheckFailure {
            check: CheckKind::Integrity,
            disposition: CheckDisposition::Failed,
            location: "interfaces".into(),
            reason: "requests bypassed the permitted MCP interface".into(),
            evidence: bypass.iter().map(|entry| entry.seq).collect(),
            event_evidence: bypass_starts.iter().copied().collect(),
        });
    }
    let answer_ok = answer_result.is_ok();
    if let Err(error) = answer_result {
        failures.push(record_failure(error, CheckKind::Answer, "answer".into()));
    }
    let failed = |check: CheckKind| failures.iter().any(|failure| failure.check == check);
    let state_ok = !failed(CheckKind::State);
    let records_ok = !failed(CheckKind::Records);
    let relations_ok = !failed(CheckKind::Relations);
    let ordering_ok = !failed(CheckKind::Ordering)
        && (input.expect.ordering.is_empty() && input.expect.information.is_empty()
            || history_verified);
    let history_ok =
        history_verified && !failed(CheckKind::Preservation) && !failed(CheckKind::Ordering);
    let preservation_ok = preservation_ok
        && !failed(CheckKind::Preservation)
        && (!history_checked || history_verified);
    let contract_ok = !failed(CheckKind::Contract);
    let environment_ok = !failed(CheckKind::Environment);
    let required_calls_ok = !failed(CheckKind::RequiredCalls);
    let forbidden_calls_ok = !failed(CheckKind::ForbiddenCalls);
    let passed = failures.is_empty();
    let adjudication_needed = failures
        .iter()
        .any(|failure| failure.disposition == CheckDisposition::Unresolved);
    let proven_failure = failures
        .iter()
        .any(|failure| failure.disposition == CheckDisposition::Failed);
    let outcome = if !contract_ok {
        Outcome::InvalidContract
    } else if !environment_ok {
        Outcome::EnvironmentError
    } else if !integrity_ok {
        Outcome::IntegrityViolation
    } else if passed {
        Outcome::Pass
    } else if input.context_overflow && agent_calls.is_empty() {
        Outcome::ContextOverflow
    } else if adjudication_needed && !proven_failure {
        Outcome::Unresolved
    } else {
        Outcome::Fail
    };
    Grade {
        evaluator_version: EVALUATOR_VERSION,
        contract_version: input.expect.version,
        reward: if passed { 1.0 } else { 0.0 },
        outcome,
        state_ok,
        records_ok,
        relations_ok,
        ordering_ok,
        history_ok,
        history_checked,
        required_calls_ok,
        forbidden_calls_ok,
        answer_ok,
        integrity_ok,
        preservation_ok,
        preservation_checked,
        contract_ok,
        environment_ok,
        runner_admission_checked: false,
        runner_admission_ok: true,
        adjudication_needed,
        failures,
        api_calls: agent_calls.len(),
        mcp_bypass,
    }
}

fn record_failure(error: RecordCheckError, kind: CheckKind, location: String) -> CheckFailure {
    let (check, disposition, reason) = match error {
        RecordCheckError::InvalidContract(reason) => {
            (CheckKind::Contract, CheckDisposition::Failed, reason)
        }
        RecordCheckError::MissingEvidence(reason) => {
            (CheckKind::Environment, CheckDisposition::Failed, reason)
        }
        RecordCheckError::NeedsAdjudication(reason) => (kind, CheckDisposition::Unresolved, reason),
        RecordCheckError::Failed(reason) => (kind, CheckDisposition::Failed, reason),
    };
    CheckFailure {
        check,
        disposition,
        location,
        reason,
        evidence: Vec::new(),
        event_evidence: Vec::new(),
    }
}

fn state_check(state: &Value, check: &StateCheck) -> bool {
    let found = state.pointer(&check.pointer);
    if let Some(expected) = &check.text_equals
        && found
            .and_then(Value::as_str)
            .is_none_or(|actual| actual.split_whitespace().ne(expected.split_whitespace()))
    {
        return false;
    }
    if let Some(expected) = &check.member_ascii_ci
        && found.and_then(Value::as_array).is_none_or(|items| {
            !items.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
            })
        })
    {
        return false;
    }
    if let Some(expected_null) = check.is_null
        && !found.is_some_and(|value| value.is_null() == expected_null)
    {
        return false;
    }
    if let Some(should_exist) = check.exists
        && found.is_some() != should_exist
    {
        return false;
    }
    if let Some(expected) = &check.equals
        && found != Some(expected)
    {
        return false;
    }
    if let Some(needle) = &check.any {
        let prefix = check.key_prefix.as_deref().unwrap_or("");
        let ok = match found {
            Some(Value::Array(items)) => items.iter().any(|i| loose(needle, i)),
            Some(Value::Object(map)) => map
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .any(|(_, v)| loose(needle, v)),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    if let Some(needle) = &check.contains {
        let ok = match (found, needle) {
            (Some(Value::Array(items)), _) => items.iter().any(|i| subset(needle, i)),
            (Some(Value::String(s)), Value::String(n)) => s.contains(n.as_str()),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}

fn call_matches(entry: &JournalEntry, matcher: &CallMatch) -> bool {
    request_parts_match(
        &entry.service,
        &entry.method,
        &entry.path,
        entry.query.as_deref(),
        Some(&entry.request_body),
        matcher,
    ) && (matcher.response_status.is_empty() || matcher.response_status.contains(&entry.status))
}

fn headers_match(
    matcher: &CallMatch,
    head: Option<&request::RequestHead>,
    location: &str,
    failures: &mut Vec<CheckFailure>,
) -> bool {
    if matcher.headers.is_empty() {
        return true;
    }
    let Some(head) = head else {
        failures.push(CheckFailure {
            check: CheckKind::Environment,
            disposition: CheckDisposition::Failed,
            location: format!("{location}/request_headers"),
            reason: "header-scoped call matching lacks correlated request-head evidence".into(),
            evidence: Vec::new(),
            event_evidence: Vec::new(),
        });
        return false;
    };
    matcher.headers.iter().all(|(name, expected)| {
        head.headers
            .iter()
            .find(|header| header.name == *name)
            .is_some_and(|header| match &header.value {
                request::CapturedHeaderValue::Public { value } => value == expected,
                request::CapturedHeaderValue::Redacted { sha256 }
                | request::CapturedHeaderValue::Synthetic { sha256, .. } => {
                    *sha256 == crate::runner::sha256(expected.as_bytes())
                }
            })
    })
}

fn request_parts_match(
    service: &str,
    method: &str,
    path: &str,
    query: Option<&str>,
    body: Option<&Value>,
    matcher: &CallMatch,
) -> bool {
    service == matcher.service
        && matcher
            .method
            .as_ref()
            .is_none_or(|expected_method| expected_method.eq_ignore_ascii_case(method))
        && matcher
            .path
            .as_ref()
            .is_none_or(|expected| expected == path)
        && matcher.path_regex.as_ref().is_none_or(|pattern| {
            Regex::new(pattern).is_ok_and(|expression| expression.is_match(path))
        })
        && matcher
            .query_contains
            .as_ref()
            .is_none_or(|expected_query| {
                query.is_some_and(|actual| actual.contains(expected_query.as_str()))
            })
        && matcher
            .query_fields
            .iter()
            .all(|(expected_key, expected_value)| {
                let values: Vec<_> = url::form_urlencoded::parse(query.unwrap_or("").as_bytes())
                    .filter(|(key, _)| key == expected_key.as_str())
                    .map(|(_, value)| value.into_owned())
                    .collect();
                !values.is_empty() && values.iter().all(|value| value == expected_value)
            })
        && matcher
            .body
            .as_ref()
            .is_none_or(|expected| body.is_some_and(|actual| subset(expected, actual)))
        && matcher
            .body_contains
            .as_ref()
            .is_none_or(|text| body.is_some_and(|actual| body_text(actual).contains(text.as_str())))
}

fn required_response_matches(entry: &JournalEntry, matcher: &CallMatch) -> bool {
    let accepted_status = if matcher.response_status.is_empty() {
        (200..300).contains(&entry.status)
    } else {
        matcher.response_status.contains(&entry.status)
    };
    let body = &entry.response_body;
    let tool_error = body["isError"] == true
        || (body.get("jsonrpc").is_some()
            && body.get("error").is_some_and(|value| !value.is_null()))
        || (entry.path.ends_with("/graphql")
            && body["errors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty()))
        || (entry.service == "cloudflare" && body["success"] == false);
    accepted_status && (matcher.allow_tool_error || !tool_error)
}

fn body_text(body: &Value) -> String {
    match body {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `expected` is contained in `actual`: objects by key subset, arrays by element-wise
/// containment, everything else by equality.
pub fn subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => e
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|av| subset(v, av))),
        (Value::Array(e), Value::Array(a)) => e.iter().all(|ev| a.iter().any(|av| subset(ev, av))),
        _ => expected == actual,
    }
}

/// Like [`subset`], but strings match by substring.
fn loose(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => e
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|av| loose(v, av))),
        (Value::Array(e), Value::Array(a)) => e.iter().all(|ev| a.iter().any(|av| loose(ev, av))),
        (Value::String(e), Value::String(a)) => a.contains(e.as_str()),
        _ => expected == actual,
    }
}

fn answer_check(answer: Option<&str>, check: &AnswerCheck) -> bool {
    let Some(answer) = answer else {
        return false;
    };
    let lower = answer.to_lowercase();
    check
        .contains
        .iter()
        .all(|c| lower.contains(&c.to_lowercase()))
        && check
            .regex
            .as_ref()
            .is_none_or(|r| Regex::new(r).is_ok_and(|re| re.is_match(answer)))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::journal::JournalEntry;

    fn entry(seq: u64, source: &str, method: &str, path: &str, body: Value) -> JournalEntry {
        JournalEntry {
            seq,
            operation_id: None,
            replay: None,
            time: "2026-03-14T09:00:00Z".into(),
            service: "github".into(),
            source: source.into(),
            method: method.into(),
            host: None,
            scheme: "http".into(),
            content_type: None,
            path: path.into(),
            query: Some("state=open".into()),
            request_body: body,
            status: 200,
            response_body: Value::Null,
        }
    }

    fn state() -> Value {
        json!({"services": {"github": {"issues": {"octo-org/app#3": {"state": "closed", "labels": [{"name": "bug"}], "title": "Flaky login test"}}}}})
    }

    fn expect(v: Value) -> Expect {
        serde_json::from_value(v).unwrap()
    }

    const TOOLS: &str = "10.0.0.5";

    fn input<'a>(
        expect: &'a Expect,
        journal: &'a [JournalEntry],
        answer: Option<&'a str>,
    ) -> GradeInput<'a> {
        GradeInput {
            expect,
            state: state(),
            journal,
            answer,
            tools_ips: vec![TOOLS.to_owned()],
            context_overflow: false,
            startup: &[],
            initial_state: None,
            events: None,
        }
    }

    #[test]
    fn passes_when_every_check_holds() {
        let e = expect(json!({
            "state": [
                {"pointer": "/services/github/issues/octo-org~1app#3/state", "equals": "closed"},
                {"pointer": "/services/github/issues/octo-org~1app#3/labels", "contains": {"name": "bug"}},
                {"pointer": "/services/github/issues/octo-org~1app#9", "exists": false}
            ],
            "required_calls": [{"service": "github", "method": "PATCH", "path": "/api/v3/repos/octo-org/app/issues/3", "body": {"state": "closed"}}],
            "forbidden_calls": [{"service": "github", "method": "DELETE"}],
            "answer": {"contains": ["flaky LOGIN"]}
        }));
        let journal = [entry(
            1,
            TOOLS,
            "PATCH",
            "/api/v3/repos/octo-org/app/issues/3",
            json!({"state": "closed", "state_reason": "completed"}),
        )];
        let g = grade(&input(
            &e,
            &journal,
            Some("Closed issue #3 (Flaky login test)."),
        ));
        assert_eq!(g.reward, 1.0);
        assert_eq!(g.outcome, Outcome::Pass);
        assert_eq!(
            (
                g.state_ok,
                g.required_calls_ok,
                g.forbidden_calls_ok,
                g.answer_ok
            ),
            (true, true, true, true)
        );
        assert_eq!(g.api_calls, 1);
        assert_eq!(g.mcp_bypass, 0);
    }

    #[test]
    fn each_failed_component_fails_the_trial() {
        let e = expect(json!({
            "state": [{"pointer": "/services/github/issues/octo-org~1app#3/state", "equals": "open"}],
            "required_calls": [{"service": "github", "method": "POST", "path_regex": "^/api/v3/repos/[^/]+/app/issues$", "min_count": 1}],
            "forbidden_calls": [{"service": "github", "method": "PATCH"}]
        }));
        let journal = [entry(
            1,
            TOOLS,
            "PATCH",
            "/api/v3/repos/octo-org/app/issues/3",
            Value::Null,
        )];
        let g = grade(&input(&e, &journal, None));
        assert_eq!(g.reward, 0.0);
        assert_eq!(g.outcome, Outcome::Fail);
        assert_eq!(
            (g.state_ok, g.required_calls_ok, g.forbidden_calls_ok),
            (false, false, false)
        );
        assert!(g.answer_ok, "no answer check means answer_ok");
    }

    #[test]
    fn calls_not_from_the_tools_container_are_bypasses_and_do_not_count() {
        let e = expect(json!({"required_calls": [{"service": "github", "method": "PATCH"}]}));
        let journal = [entry(
            1,
            "10.0.0.9",
            "PATCH",
            "/api/v3/repos/octo-org/app/issues/3",
            Value::Null,
        )];
        let g = grade(&input(&e, &journal, None));
        assert_eq!(g.mcp_bypass, 1);
        assert!(
            !g.required_calls_ok,
            "a curl from the agent container is not a tool call"
        );
        assert_eq!(g.outcome, Outcome::IntegrityViolation);
    }

    #[test]
    fn context_overflow_without_any_tool_activity_is_its_own_outcome() {
        let e = expect(json!({"required_calls": [{"service": "github"}]}));
        let mut i = input(&e, &[], None);
        i.context_overflow = true;
        let g = grade(&i);
        assert_eq!(g.outcome, Outcome::ContextOverflow);
        assert_eq!(g.reward, 0.0);
        assert_eq!(g.metrics()["outcome_code"], 2.0);
    }

    #[test]
    fn startup_requests_are_not_agent_calls() {
        let e = expect(json!({"required_calls": [{"service": "github", "method": "POST"}]}));
        let startup = [StartupCall {
            service: "github".into(),
            method: "GET".into(),
            path: "/ping".into(),
            query: Some("state=open".into()),
        }];
        let handshake = entry(1, TOOLS, "GET", "/ping", json!(null));
        let mut i = input(&e, std::slice::from_ref(&handshake), None);
        i.startup = &startup;
        i.context_overflow = true;
        let g = grade(&i);
        assert_eq!(g.api_calls, 0);
        assert_eq!(g.outcome, Outcome::ContextOverflow);

        let journal = [
            handshake.clone(),
            entry(2, TOOLS, "GET", "/ping", json!(null)),
        ];
        let mut i = input(&e, &journal, None);
        i.startup = &startup;
        i.context_overflow = true;
        let g = grade(&i);
        assert_eq!(
            g.api_calls, 1,
            "an agent repeating a startup request is counted"
        );
        assert_eq!(g.outcome, Outcome::Fail);
    }

    #[test]
    fn answer_regex_and_missing_answer() {
        let e = expect(json!({"answer": {"regex": "(?i)total:\\s*\\$?1,?204"}}));
        assert!(grade(&input(&e, &[], Some("Total: $1,204"))).answer_ok);
        assert!(!grade(&input(&e, &[], None)).answer_ok);
    }

    #[test]
    fn query_and_body_text_matchers() {
        let e = expect(
            json!({"required_calls": [{"service": "github", "query_contains": "state=open", "body_contains": "closed"}]}),
        );
        let journal = [entry(1, TOOLS, "PATCH", "/x", json!({"state": "closed"}))];
        assert!(grade(&input(&e, &journal, None)).required_calls_ok);
    }

    #[test]
    fn any_matches_some_entry_by_key_prefix_with_substring_strings() {
        let state = json!({"services": {"zendesk": {"comments": {
            "4217/1": {"public": true, "body": "Original request"},
            "4217/2": {"public": false, "body": "internal: refund RF-88213 issued"},
            "4217/3": {"public": true, "body": "Your refund RF-88213 has been issued."},
            "9999/1": {"public": true, "body": "RF-11111"}
        }}}});
        let check = |any: Value, prefix: &str| {
            let e: Expect = serde_json::from_value(json!({"state": [{"pointer": "/services/zendesk/comments", "any": any, "key_prefix": prefix}]})).unwrap();
            let mut i = input(&e, &[], None);
            i.state = state.clone();
            grade(&i).state_ok
        };
        assert!(check(json!({"public": true, "body": "RF-88213"}), "4217/"));
        assert!(
            !check(json!({"public": true, "body": "RF-11111"}), "4217/"),
            "prefix scopes the search"
        );
        assert!(!check(
            json!({"public": false, "body": "has been issued"}),
            "4217/"
        ));
    }

    #[test]
    fn metrics_are_all_numeric() {
        let e = expect(json!({}));
        let m = grade(&input(&e, &[], None)).metrics();
        for key in [
            "reward",
            "state_ok",
            "required_calls_ok",
            "forbidden_calls_ok",
            "answer_ok",
            "api_calls",
            "mcp_bypass",
            "outcome_code",
        ] {
            assert!(m.contains_key(key), "{key}");
        }
    }
}
