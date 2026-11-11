//! No-action and clarification outcomes (improvement-plan items 12 and 13).
//!
//! `[expect.interaction]` makes "do nothing and say why" or "ask first, then
//! act" the correct outcome. Both are judged from authoritative operation
//! events: committed business changes and the replayed scripted-user turns.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{CheckDisposition, CheckFailure, CheckKind, Expect, Grade, GradeInput, Outcome};
use crate::{events::EventKind, scripted_user};

pub const INVALID_ACTION: &str = "interaction/invalid_action";
pub const ACTED_WITHOUT_CLARIFICATION: &str = "interaction/acted_without_clarification";
pub const CLARIFICATION_MISSING: &str = "interaction/clarification_missing";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InteractionCheck {
    /// The request is already satisfied: nothing may be committed, and the
    /// answer contract must establish that the reply says so.
    NoAction,
    /// The request is underspecified: the scripted user must have revealed
    /// every listed fact before the first business change is committed.
    ClarifyThenAct { required_facts: Vec<String> },
}

impl InteractionCheck {
    pub(super) fn validate(&self, contract: &Expect) -> Result<(), String> {
        if contract.version < 2 || !contract.audit_history {
            return Err("interaction outcomes require an audited version-2 contract".into());
        }
        match self {
            Self::NoAction => {
                if contract.answer.is_none() {
                    return Err(
                        "a no-action outcome needs an answer contract for the explanation".into(),
                    );
                }
                if contract
                    .preservation
                    .as_ref()
                    .is_some_and(|policy| !policy.allow.is_empty())
                {
                    return Err("a no-action outcome cannot permit business changes".into());
                }
            }
            Self::ClarifyThenAct { required_facts } => {
                let distinct: BTreeSet<_> = required_facts.iter().collect();
                if required_facts.is_empty()
                    || distinct.len() != required_facts.len()
                    || required_facts.iter().any(|fact| fact.trim().is_empty())
                {
                    return Err("clarify_then_act needs distinct required facts".into());
                }
            }
        }
        Ok(())
    }
}

fn failure(check: CheckKind, location: &str, reason: String, events: Vec<u64>) -> CheckFailure {
    CheckFailure {
        check,
        disposition: CheckDisposition::Failed,
        location: location.into(),
        reason,
        evidence: Vec::new(),
        event_evidence: events,
    }
}

/// Replays any scripted-user evidence and applies the interaction contract.
pub(super) fn audit(input: &GradeInput) -> Vec<CheckFailure> {
    let mut failures = Vec::new();
    let check = input.expect.interaction.as_ref();
    let (Some(initial), Some(events)) = (input.initial_state, input.events) else {
        if check.is_some() {
            failures.push(failure(
                CheckKind::Environment,
                "evidence/interaction",
                "interaction outcomes require authoritative initial state and operation events"
                    .into(),
                Vec::new(),
            ));
        }
        return failures;
    };
    if let Err(reason) = scripted_user::validate_evidence(initial, &input.state, events) {
        failures.push(failure(
            CheckKind::Environment,
            "evidence/scripted-user",
            reason,
            Vec::new(),
        ));
        return failures;
    }
    let Some(check) = check else {
        return failures;
    };
    let commits: Vec<u64> = events
        .iter()
        .filter(|event| matches!(event.event, EventKind::StateCommitted { .. }))
        .map(|event| event.seq)
        .collect();
    match check {
        InteractionCheck::NoAction => {
            if !commits.is_empty() {
                failures.push(failure(
                    CheckKind::Interaction,
                    INVALID_ACTION,
                    "the request was already satisfied, yet a business change was committed".into(),
                    commits,
                ));
            }
        }
        InteractionCheck::ClarifyThenAct { required_facts } => {
            let script = match scripted_user::script_of(initial) {
                Ok(Some(script)) => script,
                _ => {
                    failures.push(failure(
                        CheckKind::Contract,
                        "expect/interaction",
                        "clarify_then_act requires the scenario's scripted user".into(),
                        Vec::new(),
                    ));
                    return failures;
                }
            };
            if let Some(unknown) = required_facts
                .iter()
                .find(|fact| !script.facts.iter().any(|known| &known.id == *fact))
            {
                failures.push(failure(
                    CheckKind::Contract,
                    "expect/interaction",
                    format!("required fact {unknown:?} is not a scripted-user fact"),
                    Vec::new(),
                ));
                return failures;
            }
            let exchanges = scripted_user::exchanges(events);
            let mut answered_at = 0;
            let mut missing = Vec::new();
            for fact in required_facts {
                match exchanges
                    .iter()
                    .find(|(_, exchange)| exchange.revealed.contains(fact))
                {
                    Some((seq, _)) => answered_at = answered_at.max(*seq),
                    None => missing.push(fact.as_str()),
                }
            }
            let early: Vec<u64> = commits
                .iter()
                .copied()
                .filter(|seq| !missing.is_empty() || *seq < answered_at)
                .collect();
            if !early.is_empty() {
                failures.push(failure(
                    CheckKind::Interaction,
                    ACTED_WITHOUT_CLARIFICATION,
                    "a business change was committed before the user answered the required clarification"
                        .into(),
                    early,
                ));
            } else if !missing.is_empty() {
                failures.push(failure(
                    CheckKind::Interaction,
                    CLARIFICATION_MISSING,
                    format!(
                        "the user was never asked a question that yields: {}",
                        missing.join(", ")
                    ),
                    exchanges.iter().map(|(seq, _)| *seq).collect(),
                ));
            }
        }
    }
    failures
}

/// How a graded interaction trial ended, so an invalid action, a valid
/// no-action/clarified completion and an incomplete execution stay distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionOutcome {
    /// The contract passed: correct no-action or correct action after asking.
    Correct,
    /// A business change was committed although none was appropriate, or a
    /// prohibited operation was attempted.
    InvalidAction,
    /// A business change was committed on a guess, before asking.
    ActedWithoutClarification,
    /// Nothing wrong was done, but the required result or explanation is absent.
    IncompleteExecution,
    /// Environment, contract or integrity findings; not a model outcome.
    NotScorable,
}

pub fn classify(grade: &Grade) -> InteractionOutcome {
    let located = |location: &str| {
        grade
            .failures
            .iter()
            .any(|failure| failure.location == location)
    };
    match grade.outcome {
        Outcome::Pass => InteractionOutcome::Correct,
        Outcome::Fail | Outcome::Unresolved | Outcome::ContextOverflow => {
            if located(INVALID_ACTION)
                || grade
                    .failures
                    .iter()
                    .any(|failure| failure.check == CheckKind::ForbiddenCalls)
            {
                InteractionOutcome::InvalidAction
            } else if located(ACTED_WITHOUT_CLARIFICATION) {
                InteractionOutcome::ActedWithoutClarification
            } else {
                InteractionOutcome::IncompleteExecution
            }
        }
        Outcome::IntegrityViolation | Outcome::InvalidContract | Outcome::EnvironmentError => {
            InteractionOutcome::NotScorable
        }
    }
}
