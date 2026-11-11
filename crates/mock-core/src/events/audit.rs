use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use super::{EventKind, OperationContext, OperationEvent};
use crate::{
    json_pointer::tokens as pointer_tokens,
    preservation::{CountScope, Preservation, StateChange, business_state, record_changes},
    relations::{CalendarMeeting, RecordCheckError, RecordSelector},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectCheck {
    pub selector: RecordSelector,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_meeting: Option<CalendarMeeting>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub equals: Option<Value>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub has_value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonempty_text: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeric_equals: Option<Value>,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub quantity_equals: Option<Value>,
}

fn present_value<'de, Decoder: serde::Deserializer<'de>>(
    decoder: Decoder,
) -> Result<Option<Value>, Decoder::Error> {
    Value::deserialize(decoder).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderingCheck {
    pub prerequisite: EffectCheck,
    pub action: EffectCheck,
    #[serde(default)]
    pub require_transition: bool,
    #[serde(default)]
    pub allow_same_commit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFailureKind {
    Preservation,
    Ordering,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryFailure {
    pub kind: HistoryFailureKind,
    pub location: String,
    pub reason: String,
    pub events: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum HistoryError {
    #[error("{0}")]
    InvalidContract(String),
    #[error("{reason}")]
    MissingEvidence { event: Option<u64>, reason: String },
}

impl EffectCheck {
    pub fn validate(&self) -> Result<(), String> {
        self.selector.validate()?;
        if self.record_count.is_some() || self.calendar_meeting.is_some() {
            if !self.field.is_empty() {
                return Err(
                    "whole-record and record-count effects cannot also select a record field"
                        .into(),
                );
            }
        } else if pointer_tokens(&self.field)?.is_empty() {
            return Err("an effect check must identify a field inside its selected record".into());
        }
        if usize::from(self.record_count.is_some())
            + usize::from(self.calendar_meeting.is_some())
            + usize::from(self.equals.is_some())
            + usize::from(self.has_value.is_some())
            + usize::from(self.nonempty_text.is_some())
            + usize::from(self.numeric_equals.is_some())
            + usize::from(self.quantity_equals.is_some())
            != 1
            || self.nonempty_text == Some(false)
        {
            return Err("effect checks require exactly one calendar meeting, record count, equality, numeric equality, quantity equality, array membership or nonempty-text predicate".into());
        }
        if self
            .numeric_equals
            .as_ref()
            .is_some_and(|value| !crate::decimal::equivalent(value, value))
        {
            return Err("numeric causal predicates require a valid exact decimal value".into());
        }
        if self
            .quantity_equals
            .as_ref()
            .is_some_and(|value| crate::quantity::Quantity::parse(value).is_err())
        {
            return Err("quantity causal predicates require a supported resource quantity".into());
        }
        if let Some(meeting) = &self.calendar_meeting {
            meeting.validate()?;
        }
        Ok(())
    }

    fn matches(&self, value: Option<&Value>) -> Result<bool, HistoryError> {
        if let Some(expected) = &self.equals {
            return Ok(value == Some(expected));
        }
        if let Some(expected) = &self.has_value {
            return Ok(value
                .and_then(Value::as_array)
                .is_some_and(|values| values.contains(expected)));
        }
        if let Some(expected) = &self.numeric_equals {
            return Ok(value.is_some_and(|value| crate::decimal::equivalent(value, expected)));
        }
        if let Some(expected) = &self.quantity_equals {
            let expected = crate::quantity::Quantity::parse(expected).map_err(|_| {
                HistoryError::InvalidContract("unsupported required causal quantity".into())
            })?;
            return match value.map(crate::quantity::Quantity::parse) {
                Some(Ok(actual)) => Ok(actual == expected),
                Some(Err(crate::quantity::ParseError::Unsupported)) => Err(missing(
                    None,
                    "causal quantity evidence is outside the reviewed parser bounds",
                )),
                Some(Err(crate::quantity::ParseError::Invalid)) | None => Ok(false),
            };
        }
        Ok(self.nonempty_text == Some(true)
            && value
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty()))
    }

    fn record_matches(&self, record: Option<&Value>, state: &Value) -> Result<bool, HistoryError> {
        if let Some(meeting) = &self.calendar_meeting {
            let Some(record) = record else {
                return Ok(false);
            };
            return match meeting.check(record, state, true) {
                Ok(()) => Ok(true),
                Err(RecordCheckError::Failed(_)) => Ok(false),
                Err(RecordCheckError::InvalidContract(reason)) => {
                    Err(HistoryError::InvalidContract(reason))
                }
                Err(
                    RecordCheckError::MissingEvidence(reason)
                    | RecordCheckError::NeedsAdjudication(reason),
                ) => Err(missing(None, reason)),
            };
        }
        self.matches(record.and_then(|record| record.pointer(&self.field)))
    }

    fn holds(&self, initial: &Value, state: &Value) -> Result<bool, HistoryError> {
        if self.record_count.is_some()
            && !matches!(
                state.pointer(&self.selector.collection),
                Some(Value::Object(_) | Value::Array(_))
            )
            && !(state.pointer(&self.selector.collection).is_none()
                && matches!(
                    initial.pointer(&self.selector.collection),
                    Some(Value::Object(_) | Value::Array(_))
                ))
        {
            return Err(missing(
                None,
                "record-count prerequisite lacks its authoritative collection",
            ));
        }
        let selected = self
            .selector
            .select(Some(initial), state)
            .map_err(|error| missing(None, error.to_string()))?;
        if let Some(expected) = self.record_count {
            return Ok(selected.len() == expected);
        }
        Ok(selected.len() == 1 && self.record_matches(Some(selected[0].1), state)?)
    }

    fn transition(
        &self,
        initial: &Value,
        before: &Value,
        after: &Value,
    ) -> Result<bool, HistoryError> {
        if self.record_count.is_some() {
            return Ok(self.holds(initial, after)? && !self.holds(initial, before)?);
        }
        let selected = self
            .selector
            .select(Some(initial), after)
            .map_err(|error| missing(None, error.to_string()))?;
        if selected.len() != 1 {
            return Ok(false);
        }
        self.any_transition(initial, before, after)
    }

    pub(crate) fn any_transition(
        &self,
        initial: &Value,
        before: &Value,
        after: &Value,
    ) -> Result<bool, HistoryError> {
        if self.record_count.is_some() {
            return Ok(self.holds(initial, after)? && !self.holds(initial, before)?);
        }
        let selected = self
            .selector
            .select(Some(initial), after)
            .map_err(|error| missing(None, error.to_string()))?;
        let previous = self
            .selector
            .entries(before)
            .map_err(|error| missing(None, error.to_string()))?;
        for (identity, record) in selected {
            if !self.record_matches(Some(record), after)? {
                continue;
            }
            let previous = previous
                .iter()
                .find(|(key, _)| key == &identity)
                .map(|(_, record)| *record);
            if !self.record_matches(previous, before)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl OrderingCheck {
    pub fn validate(&self) -> Result<(), String> {
        self.prerequisite.validate()?;
        self.action.validate()
    }
}

pub fn audit(
    initial: &Value,
    final_state: &Value,
    events: &[OperationEvent],
    preservation: Option<&Preservation>,
    ordering: &[OrderingCheck],
) -> Result<Vec<HistoryFailure>, HistoryError> {
    audit_with_observer(
        initial,
        final_state,
        events,
        preservation,
        ordering,
        |_, _, _| {},
    )
}

pub(crate) fn audit_with_observer(
    initial: &Value,
    final_state: &Value,
    events: &[OperationEvent],
    preservation: Option<&Preservation>,
    ordering: &[OrderingCheck],
    mut observe: impl FnMut(u64, &Value, &Value),
) -> Result<Vec<HistoryFailure>, HistoryError> {
    let scenario = initial.get("scenario").and_then(Value::as_str);
    if scenario.is_some()
        && final_state
            .get("scenario")
            .and_then(Value::as_str)
            .is_some()
        && scenario != final_state.get("scenario").and_then(Value::as_str)
    {
        return Err(missing(
            None,
            "initial and final state belong to different scenarios",
        ));
    }
    if let Some(policy) = preservation {
        policy.validate().map_err(HistoryError::InvalidContract)?;
    }
    for check in ordering {
        check.validate().map_err(HistoryError::InvalidContract)?;
    }
    let mut current = business_state(initial).map_err(|reason| missing(None, reason))?;
    let final_state = business_state(final_state).map_err(|reason| missing(None, reason))?;
    let mut operations: BTreeMap<u64, OperationContext> = BTreeMap::new();
    let mut completed = BTreeSet::new();
    let mut failures = Vec::new();
    let mut seen_prerequisites = BTreeSet::new();
    let mut affected: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    let mut exceeded = BTreeSet::new();
    for (index, event) in events.iter().enumerate() {
        if event.seq != index as u64 + 1 {
            return Err(missing(Some(event.seq), "event sequence is not contiguous"));
        }
        match &event.event {
            EventKind::RequestStarted => {
                let operation = event.operation.as_ref().ok_or_else(|| {
                    missing(Some(event.seq), "a request-start event has no operation")
                })?;
                if operation.id != event.seq
                    || operations.contains_key(&operation.id)
                    || operation
                        .parent_id
                        .is_some_and(|parent| !operations.contains_key(&parent))
                    || scenario.is_some_and(|scenario| scenario != operation.scenario)
                {
                    return Err(missing(
                        Some(event.seq),
                        "request identity or parentage is inconsistent",
                    ));
                }
                operations.insert(operation.id, operation.clone());
            }
            EventKind::ResponseCompleted {
                status,
                body_complete,
            } => {
                let operation = known_operation(event, &operations)?;
                if !completed.insert(operation.id) || !(100..600).contains(status) {
                    return Err(missing(
                        Some(event.seq),
                        "response evidence is inconsistent",
                    ));
                }
                if !body_complete {
                    return Err(missing(
                        Some(event.seq),
                        "response evidence was not fully captured",
                    ));
                }
            }
            EventKind::EnvironmentFailed { code, reason } => {
                known_operation(event, &operations)?;
                if code.is_empty() || reason.is_empty() {
                    return Err(missing(
                        Some(event.seq),
                        "environment failure evidence is inconsistent",
                    ));
                }
            }
            EventKind::OperationAborted => {
                let operation = known_operation(event, &operations)?;
                if !completed.insert(operation.id) {
                    return Err(missing(
                        Some(event.seq),
                        "operation cancellation evidence is inconsistent",
                    ));
                }
            }
            EventKind::ExecutionRetained { .. } | EventKind::ExecutionReleased { .. } => {
                known_operation(event, &operations)?;
            }
            EventKind::RequestHeadReceived { .. }
            | EventKind::RequestBodyReceived { .. }
            | EventKind::ResponseDeliveryLost
            | EventKind::FaultSelected { .. }
            | EventKind::UserExchanged { .. } => {
                known_operation(event, &operations)?;
            }
            EventKind::StateCommitted { records } => {
                let operation = known_operation(event, &operations)?;
                if records.is_empty() {
                    return Err(missing(
                        Some(event.seq),
                        "a commit contains no record changes",
                    ));
                }
                let before = current.clone();
                let mut changed = BTreeSet::new();
                for record in records {
                    if !changed.insert(&record.pointer)
                        || current.pointer(&record.pointer) != record.before.as_ref()
                        || record.before == record.after
                    {
                        return Err(missing(
                            Some(event.seq),
                            format!("record history is inconsistent at {}", record.pointer),
                        ));
                    }
                    apply_record(&mut current, record, event.seq)?;
                }
                if let Some(policy) = preservation {
                    let assessment = policy
                        .assess_commit(initial, &before, &current, operation)
                        .map_err(|reason| missing(Some(event.seq), reason))?;
                    failures.extend(assessment.failures.into_iter().map(|failure| {
                        HistoryFailure {
                            kind: HistoryFailureKind::Preservation,
                            location: failure.pointer,
                            reason: failure.reason,
                            events: vec![event.seq],
                        }
                    }));
                    for (rule_index, records) in assessment.matched_records {
                        let all_records = affected.entry(rule_index).or_default();
                        all_records.extend(records);
                        let rule = &policy.allow[rule_index];
                        if rule.count_scope == CountScope::PerCommit {
                            continue;
                        }
                        if let Some(maximum) = rule.max_count.maximum()
                            && all_records.len() > maximum
                            && exceeded.insert(rule_index)
                        {
                            failures.push(HistoryFailure {
                                kind: HistoryFailureKind::Preservation,
                                location: rule.pointer.clone(),
                                reason: format!(
                                    "{} distinct records changed across history; at most {} are allowed",
                                    all_records.len(), maximum
                                ),
                                events: vec![event.seq],
                            });
                        }
                    }
                }
                for (check_index, check) in ordering.iter().enumerate() {
                    if check.action.any_transition(initial, &before, &current)? {
                        let holds_before = check.prerequisite.holds(initial, &before)?;
                        let transitions_now =
                            check.prerequisite.transition(initial, &before, &current)?;
                        if !check.allow_same_commit
                            && !holds_before
                            && check.prerequisite.holds(initial, &current)?
                        {
                            return Err(missing(
                                Some(event.seq),
                                "the prerequisite and action share an atomic commit; their internal order was not observed",
                            ));
                        }
                        let held_before = holds_before
                            && (!check.require_transition
                                || seen_prerequisites.contains(&check_index));
                        if !held_before && !(check.allow_same_commit && transitions_now) {
                            failures.push(HistoryFailure {
                                kind: HistoryFailureKind::Ordering,
                                location: format!("ordering/{check_index}"),
                                reason: if check.allow_same_commit {
                                    "the action occurred without its prerequisite holding before or becoming true in the same atomic commit"
                                } else {
                                    "the action occurred before its required prerequisite held"
                                }.into(),
                                events: vec![event.seq],
                            });
                        }
                    }
                    if check.prerequisite.transition(initial, &before, &current)? {
                        seen_prerequisites.insert(check_index);
                    }
                }
                observe(event.seq, &before, &current);
            }
        }
    }
    if business_state(&current).map_err(|reason| missing(None, reason))? != final_state {
        let pointers: Vec<_> = record_changes(&current, &final_state)
            .into_iter()
            .map(|change| change.pointer)
            .take(8)
            .collect();
        return Err(missing(
            None,
            format!("history does not reconstruct final business state: {pointers:?}"),
        ));
    }
    Ok(failures)
}

fn known_operation<'event>(
    event: &'event OperationEvent,
    operations: &BTreeMap<u64, OperationContext>,
) -> Result<&'event OperationContext, HistoryError> {
    let operation = event
        .operation
        .as_ref()
        .ok_or_else(|| missing(Some(event.seq), "business evidence has no causal operation"))?;
    if operations.get(&operation.id) != Some(operation) {
        return Err(missing(
            Some(event.seq),
            "operation provenance is inconsistent",
        ));
    }
    Ok(operation)
}

fn apply_record(
    state: &mut Value,
    change: &StateChange,
    event_seq: u64,
) -> Result<(), HistoryError> {
    let tokens =
        pointer_tokens(&change.pointer).map_err(|reason| missing(Some(event_seq), reason))?;
    if tokens.len() != 4 || tokens[0] != "services" {
        return Err(missing(
            Some(event_seq),
            "commit changes must identify complete business records",
        ));
    }
    let services = state["services"]
        .as_object_mut()
        .ok_or_else(|| missing(Some(event_seq), "business services are not a map"))?;
    let service = services
        .entry(tokens[1].clone())
        .or_insert_with(|| json!({}));
    let collections = service
        .as_object_mut()
        .ok_or_else(|| missing(Some(event_seq), "business collections are not a map"))?;
    let collection = collections
        .entry(tokens[2].clone())
        .or_insert_with(|| json!({}));
    let records = collection
        .as_object_mut()
        .ok_or_else(|| missing(Some(event_seq), "business records are not a map"))?;
    match &change.after {
        Some(value) => {
            records.insert(tokens[3].clone(), value.clone());
        }
        None => {
            records.remove(&tokens[3]);
        }
    }
    Ok(())
}

fn missing(event: Option<u64>, reason: impl Into<String>) -> HistoryError {
    HistoryError::MissingEvidence {
        event,
        reason: reason.into(),
    }
}
