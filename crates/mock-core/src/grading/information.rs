use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    CallMatch, CheckDisposition, CheckFailure, CheckKind, GradeInput, call_matches, headers_match,
    request_parts_match, required_response_matches,
};
use crate::{
    clock::parse_rfc3339_nanos,
    events::{EffectCheck, EventKind, OperationEvent, OperationKind, audit_with_observer},
    journal::JournalEntry,
    json_pointer,
    relations::{RecordCheckError, RecordSelector},
    request,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InformationCheck {
    pub prerequisite: CallMatch,
    pub response_field: String,
    pub source: RecordSelector,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<InformationWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<InformationSeries>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projection: Vec<InformationField>,
    pub action: CallMatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_effect: Option<EffectCheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<InformationRead>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InformationRead {
    pub prerequisite: CallMatch,
    pub response_field: String,
    pub source: RecordSelector,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<InformationWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<InformationSeries>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projection: Vec<InformationField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InformationField {
    pub response: Vec<String>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excerpts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FieldFormat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldFormat {
    UnixSeconds,
}

fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn passage(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(folded(text)),
        Value::Array(parts) => parts
            .iter()
            .map(|part| part.as_str().map(folded))
            .collect::<Option<Vec<_>>>()
            .map(|parts| parts.join("\n")),
        _ => None,
    }
}

impl InformationField {
    fn validate(&self) -> Result<(), String> {
        if self.response.is_empty()
            || !self.response.iter().all(|pointer| literal_pointer(pointer))
            || !literal_pointer(&self.source)
            || self
                .excerpts
                .iter()
                .any(|excerpt| folded(excerpt).is_empty())
            || self.format.is_some() && !self.excerpts.is_empty()
        {
            return Err("information projections require literal response/source pointers and nonempty excerpts".into());
        }
        Ok(())
    }

    fn bound(&self, response: &Value, source: &Value) -> Result<bool, String> {
        let expected = source
            .pointer(&self.source)
            .ok_or("information projection source field is absent from the initial record")?;
        let candidates = self
            .response
            .iter()
            .filter_map(|pointer| response.pointer(pointer));
        if self.format == Some(FieldFormat::UnixSeconds) {
            let instant = expected
                .as_str()
                .and_then(parse_rfc3339_nanos)
                .ok_or("information projection source is not an exact RFC 3339 timestamp")?;
            let seconds = Value::String(format!("{instant}e-9"));
            return Ok(candidates
                .into_iter()
                .any(|actual| crate::decimal::equivalent(actual, &seconds)));
        }
        if self.excerpts.is_empty() {
            return Ok(candidates.into_iter().any(|actual| actual == expected));
        }
        let authored = passage(expected)
            .ok_or("information projection excerpts require a textual source field")?;
        let excerpts: Vec<String> = self
            .excerpts
            .iter()
            .map(|excerpt| folded(excerpt))
            .collect();
        if !excerpts
            .iter()
            .all(|excerpt| authored.contains(excerpt.as_str()))
        {
            return Err(
                "information projection excerpts are absent from the initial source text".into(),
            );
        }
        Ok(candidates.into_iter().any(|actual| {
            passage(actual).is_some_and(|returned| {
                excerpts
                    .iter()
                    .all(|excerpt| returned.contains(excerpt.as_str()))
            })
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InformationSeries {
    pub fields: BTreeMap<String, String>,
    pub scope: String,
    pub source_tags: String,
    pub points: String,
    pub interval: String,
    /// Widest response bucket, in source-timestamp seconds, that may merge several window
    /// points and still witness the window. Without it such a rollup needs adjudication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rollup_interval: Option<u64>,
    pub source_points: String,
    pub timestamp_scale: u64,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeriesEvidence {
    Witness,
    Ambiguous,
    Absent,
}

fn series_point(point: &Value) -> Option<(f64, f64)> {
    match point.as_array()?.as_slice() {
        [instant, value] => Some((instant.as_f64()?, value.as_f64()?)),
        _ => None,
    }
}

fn scope_terms(scope: &str) -> Option<Vec<&str>> {
    let scope = scope.trim();
    if scope == "*" {
        return Some(Vec::new());
    }
    scope
        .split(',')
        .map(str::trim)
        .map(|term| {
            term.split_once(':')
                .filter(|(key, value)| {
                    [key, value].iter().all(|part| {
                        !part.is_empty()
                            && part.chars().all(|character| {
                                character.is_ascii_alphanumeric()
                                    || matches!(character, '_' | '-' | '.' | '/')
                            })
                    })
                })
                .map(|_| term)
        })
        .collect()
}

impl InformationSeries {
    fn validate(&self) -> Result<(), String> {
        let start = parse_rfc3339_nanos(&self.start);
        let end = parse_rfc3339_nanos(&self.end);
        if self.fields.is_empty()
            || self
                .fields
                .iter()
                .any(|(response, source)| !literal_pointer(response) || !literal_pointer(source))
            || [
                &self.scope,
                &self.source_tags,
                &self.points,
                &self.interval,
                &self.source_points,
            ]
            .iter()
            .any(|pointer| !literal_pointer(pointer))
            || self.timestamp_scale == 0
            || self.max_rollup_interval == Some(0)
            || !matches!((start, end), (Some(start), Some(end)) if start < end)
        {
            return Err("information series require literal response/source bindings, a positive timestamp scale and rollup limit, and increasing exact bounds".into());
        }
        Ok(())
    }

    fn bounds(&self) -> (f64, f64) {
        let seconds = |instant: &str| parse_rfc3339_nanos(instant).unwrap() as f64 / 1e9;
        (seconds(&self.start), seconds(&self.end))
    }

    fn tags<'record>(&self, record: &'record Value) -> Vec<&'record str> {
        record
            .pointer(&self.source_tags)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect()
    }

    fn bound_fields(&self, response: &Value, source: &Value) -> bool {
        self.fields.iter().all(|(response_field, source_field)| {
            source
                .pointer(source_field)
                .is_some_and(|expected| response.pointer(response_field) == Some(expected))
        })
    }

    fn assess(
        &self,
        response: &Value,
        source: &Value,
        universe: &[&Value],
    ) -> Result<SeriesEvidence, String> {
        if !self.bound_fields(response, source) {
            return Ok(SeriesEvidence::Absent);
        }
        let scope = response
            .pointer(&self.scope)
            .and_then(Value::as_str)
            .ok_or("information series response has no declared scope")?;
        let Some(terms) = scope_terms(scope) else {
            return Ok(SeriesEvidence::Ambiguous);
        };
        let tags = self.tags(source);
        if !terms.iter().all(|term| tags.contains(term)) {
            return Ok(SeriesEvidence::Absent);
        }
        let contributors = universe
            .iter()
            .filter(|candidate| {
                self.bound_fields(response, candidate) && {
                    let candidate_tags = self.tags(candidate);
                    terms.iter().all(|term| candidate_tags.contains(term))
                }
            })
            .count();
        if contributors != 1 {
            return Ok(SeriesEvidence::Absent);
        }
        let interval = response
            .pointer(&self.interval)
            .and_then(Value::as_f64)
            .filter(|interval| *interval > 0.0)
            .ok_or("information series response has no positive bucket interval")?;
        let points = response
            .pointer(&self.points)
            .and_then(Value::as_array)
            .ok_or("information series response has no declared point array")?
            .iter()
            .map(series_point)
            .collect::<Option<Vec<_>>>()
            .ok_or("information series response points are not timestamp/value pairs")?;
        let observed = source
            .pointer(&self.source_points)
            .and_then(Value::as_array)
            .ok_or("information series source has no declared point array")?
            .iter()
            .map(series_point)
            .collect::<Option<Vec<_>>>()
            .ok_or("information series source points are not timestamp/value pairs")?;
        let (start, end) = self.bounds();
        let scale = self.timestamp_scale as f64;
        let mut buckets = Vec::new();
        for (instant, _) in observed
            .iter()
            .filter(|(instant, _)| *instant >= start && *instant < end)
        {
            let Some((bucket, value)) = points.iter().find(|(bucket, _)| {
                let opening = bucket / scale;
                opening <= *instant && *instant < opening + interval
            }) else {
                return Ok(SeriesEvidence::Absent);
            };
            let opening = bucket / scale;
            let contained: Vec<f64> = observed
                .iter()
                .filter(|(other, _)| opening <= *other && *other < opening + interval)
                .map(|(_, value)| *value)
                .collect();
            let lowest = contained.iter().copied().fold(f64::INFINITY, f64::min);
            let highest = contained.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if !(lowest <= *value && *value <= highest) {
                return Ok(SeriesEvidence::Absent);
            }
            buckets.push(*bucket);
        }
        if buckets.is_empty() {
            return Err("no initial source point lies inside the information series window".into());
        }
        let distinct = buckets
            .iter()
            .enumerate()
            .all(|(index, bucket)| !buckets[..index].contains(bucket));
        let accepted_rollup = self
            .max_rollup_interval
            .is_some_and(|accepted| interval <= accepted as f64);
        Ok(if distinct || accepted_rollup {
            SeriesEvidence::Witness
        } else {
            SeriesEvidence::Ambiguous
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InformationWindow {
    pub field: String,
    pub start: String,
    pub end: String,
}

fn literal_pointer(pointer: &str) -> bool {
    json_pointer::tokens(pointer).is_ok_and(|parts| {
        !parts.is_empty()
            && parts
                .iter()
                .all(|part| !part.is_empty() && !part.contains('*'))
    })
}

fn bounded_matcher(matcher: &CallMatch, name: &str) -> Result<(), String> {
    super::validation::validate_call(matcher, name)?;
    if matcher.method.is_none()
        || matcher.path.is_none() && matcher.path_regex.is_none()
        || matcher.min_count != 1
        || matcher.allow_tool_error
    {
        return Err(
            "information checks require bounded single-operation matchers without error allowances"
                .into(),
        );
    }
    Ok(())
}

impl InformationCheck {
    pub(super) fn validate(&self) -> Result<(), String> {
        bounded_matcher(&self.action, "action")?;
        if !self.action.response_status.is_empty() {
            return Err("information actions match request admission".into());
        }
        if let Some(effect) = &self.action_effect {
            effect.validate()?;
        }
        let reads = self.reads();
        for (index, read) in reads.iter().enumerate() {
            read.validate()?;
            if reads[..index].contains(read) {
                return Err("information alternatives must be distinct reads".into());
            }
        }
        Ok(())
    }

    fn reads(&self) -> Vec<InformationRead> {
        std::iter::once(InformationRead {
            prerequisite: self.prerequisite.clone(),
            response_field: self.response_field.clone(),
            source: self.source.clone(),
            window: self.window.clone(),
            series: self.series.clone(),
            projection: self.projection.clone(),
        })
        .chain(self.alternatives.iter().cloned())
        .collect()
    }
}

impl InformationRead {
    fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        bounded_matcher(&self.prerequisite, "prerequisite")?;
        if self.source.created
            || !literal_pointer(&self.response_field)
            || self
                .prerequisite
                .response_status
                .iter()
                .any(|status| !(200..300).contains(status))
        {
            return Err("information prerequisites require an existing source, literal response array and successful read; actions match request admission".into());
        }
        if let Some(series) = &self.series {
            if self.window.is_some() || !self.projection.is_empty() {
                return Err(
                    "information series declare their own point window and field bindings".into(),
                );
            }
            series.validate()?;
        }
        for field in &self.projection {
            field.validate()?;
        }
        if let Some(window) = &self.window {
            let start = parse_rfc3339_nanos(&window.start);
            let end = parse_rfc3339_nanos(&window.end);
            if !literal_pointer(&window.field)
                || !matches!((start, end), (Some(start), Some(end)) if start < end)
            {
                return Err("information windows require a literal timestamp field and increasing exact bounds".into());
            }
        }
        Ok(())
    }

    fn source_records<'state>(
        &self,
        initial: &'state Value,
    ) -> Result<Vec<&'state Value>, RecordCheckError> {
        let mut records = Vec::new();
        for (_, record) in self.source.select(None, initial)? {
            if let Some(window) = &self.window {
                let instant = record
                    .pointer(&window.field)
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339_nanos)
                    .ok_or_else(|| {
                        RecordCheckError::MissingEvidence(
                            "information source has no exact timestamp".into(),
                        )
                    })?;
                if instant < parse_rfc3339_nanos(&window.start).unwrap()
                    || instant >= parse_rfc3339_nanos(&window.end).unwrap()
                {
                    continue;
                }
            }
            records.push(record);
        }
        if records.is_empty() {
            return Err(RecordCheckError::MissingEvidence(
                "no initial source record establishes the required information".into(),
            ));
        }
        Ok(records)
    }

    fn universe<'state>(
        &self,
        initial: &'state Value,
    ) -> Result<Vec<&'state Value>, RecordCheckError> {
        initial
            .pointer(&self.source.collection)
            .and_then(Value::as_object)
            .map(|records| records.values().collect())
            .ok_or_else(|| {
                RecordCheckError::MissingEvidence(
                    "information series source collection is absent from initial state".into(),
                )
            })
    }

    fn evidence(
        &self,
        records: &[Value],
        sources: &[&Value],
        universe: &[&Value],
    ) -> Result<SeriesEvidence, String> {
        let Some(series) = &self.series else {
            for record in records {
                for source in sources {
                    let bound = if self.projection.is_empty() {
                        super::subset(source, record)
                    } else {
                        let mut all = true;
                        for field in &self.projection {
                            all &= field.bound(record, source)?;
                        }
                        all
                    };
                    if bound {
                        return Ok(SeriesEvidence::Witness);
                    }
                }
            }
            return Ok(SeriesEvidence::Absent);
        };
        let mut strongest = SeriesEvidence::Absent;
        for record in records {
            for source in sources {
                match series.assess(record, source, universe)? {
                    SeriesEvidence::Witness => return Ok(SeriesEvidence::Witness),
                    SeriesEvidence::Ambiguous => strongest = SeriesEvidence::Ambiguous,
                    SeriesEvidence::Absent => {}
                }
            }
        }
        Ok(strongest)
    }
}

fn environment(location: String, reason: String) -> CheckFailure {
    CheckFailure {
        check: CheckKind::Environment,
        disposition: CheckDisposition::Failed,
        location,
        reason,
        evidence: Vec::new(),
        event_evidence: Vec::new(),
    }
}

fn committing_requests(
    input: &GradeInput,
    initial: &Value,
    events: &[OperationEvent],
) -> Result<BTreeMap<usize, BTreeSet<u64>>, String> {
    let effects: Vec<(usize, &EffectCheck)> = input
        .expect
        .information
        .iter()
        .enumerate()
        .filter_map(|(index, check)| check.action_effect.as_ref().map(|effect| (index, effect)))
        .collect();
    let mut committed: BTreeMap<usize, BTreeSet<u64>> = BTreeMap::new();
    if effects.is_empty() {
        return Ok(committed);
    }
    let parents: BTreeMap<u64, Option<u64>> = events
        .iter()
        .filter(|event| matches!(event.event, EventKind::RequestStarted))
        .filter_map(|event| event.operation.as_ref())
        .map(|operation| (operation.id, operation.parent_id))
        .collect();
    let mut problem = None;
    audit_with_observer(
        initial,
        &input.state,
        events,
        None,
        &[],
        |sequence, before, after| {
            let Some(operation) = usize::try_from(sequence)
                .ok()
                .and_then(|sequence| events.get(sequence.checked_sub(1)?))
                .and_then(|event| event.operation.as_ref())
            else {
                problem.get_or_insert_with(|| {
                    "a committed effect has no operation identity".to_owned()
                });
                return;
            };
            for (index, effect) in &effects {
                match effect.any_transition(initial, before, after) {
                    Ok(false) => {}
                    Ok(true) => {
                        let operations = committed.entry(*index).or_default();
                        let mut lineage = Some(operation.id);
                        while let Some(identity) = lineage {
                            if !operations.insert(identity) {
                                break;
                            }
                            lineage = parents.get(&identity).copied().flatten();
                        }
                    }
                    Err(error) => {
                        problem.get_or_insert_with(|| error.to_string());
                    }
                }
            }
        },
    )
    .map_err(|error| error.to_string())?;
    match problem {
        Some(reason) => Err(reason),
        None => Ok(committed),
    }
}

pub(super) fn audit(input: &GradeInput, entries: &[&JournalEntry]) -> Vec<CheckFailure> {
    let (Some(initial), Some(events)) = (input.initial_state, input.events) else {
        return vec![environment(
            "information".into(),
            "information prerequisites lack initial state or operation events".into(),
        )];
    };
    if let Err(reason) = request::validate_journal(events, input.journal) {
        return vec![environment("information/requests".into(), reason)];
    }
    let observed = match request::observe(events) {
        Ok(observed) => observed,
        Err(reason) => return vec![environment("information/requests".into(), reason)],
    };
    let mut completions = BTreeMap::new();
    let lost: std::collections::BTreeSet<_> = events
        .iter()
        .filter(|event| matches!(event.event, EventKind::ResponseDeliveryLost))
        .filter_map(|event| event.operation.as_ref().map(|operation| operation.id))
        .collect();
    for event in events {
        if let EventKind::ResponseCompleted {
            status,
            body_complete,
        } = event.event
        {
            let Some(operation) = &event.operation else {
                return vec![environment(
                    "information/responses".into(),
                    "response has no operation identity".into(),
                )];
            };
            if observed
                .get(&operation.id)
                .is_none_or(|request| request.context != operation)
                || completions
                    .insert(operation.id, (event.seq, status, body_complete))
                    .is_some()
            {
                return vec![environment(
                    "information/responses".into(),
                    "response identities are inconsistent or duplicated".into(),
                )];
            }
        }
    }
    let committed = match committing_requests(input, initial, events) {
        Ok(committed) => committed,
        Err(reason) => return vec![environment("information/effects".into(), reason)],
    };
    let mut failures = Vec::new();
    for (index, check) in input.expect.information.iter().enumerate() {
        let location = format!("information/{index}");
        let mut witnesses = Vec::new();
        let mut ambiguous = Vec::new();
        let mut candidate_entries = Vec::new();
        let mut read_fault = false;
        for read in check.reads() {
            let sources = match read.source_records(initial) {
                Ok(records) => records,
                Err(error) => {
                    failures.push(environment(location.clone(), error.to_string()));
                    read_fault = true;
                    continue;
                }
            };
            let universe = if read.series.is_some() {
                match read.universe(initial) {
                    Ok(records) => records,
                    Err(error) => {
                        failures.push(environment(location.clone(), error.to_string()));
                        read_fault = true;
                        continue;
                    }
                }
            } else {
                Vec::new()
            };
            for entry in entries {
                if !call_matches(entry, &read.prerequisite)
                    || !required_response_matches(entry, &read.prerequisite)
                {
                    continue;
                }
                candidate_entries.push(entry.seq);
                let Some(identity) = entry.operation_id else {
                    failures.push(environment(
                        location.clone(),
                        "information read lacks its operation identity".into(),
                    ));
                    continue;
                };
                if lost.contains(&identity) {
                    continue;
                }
                let Some(request) = observed.get(&identity) else {
                    failures.push(environment(
                        location.clone(),
                        "information read lacks its admitted request".into(),
                    ));
                    continue;
                };
                if !headers_match(
                    &read.prerequisite,
                    request.head.map(|(_, head)| head),
                    &location,
                    &mut failures,
                ) {
                    continue;
                }
                let Some((sequence, status, true)) = completions.get(&identity).copied() else {
                    failures.push(environment(
                        location.clone(),
                        "information read lacks a complete response".into(),
                    ));
                    continue;
                };
                if status != entry.status {
                    failures.push(environment(
                        location.clone(),
                        "information response status differs from its journal".into(),
                    ));
                    continue;
                }
                let Some(records) = entry
                    .response_body
                    .pointer(&read.response_field)
                    .and_then(Value::as_array)
                else {
                    failures.push(environment(
                        location.clone(),
                        "information response has no declared record array".into(),
                    ));
                    continue;
                };
                match read.evidence(records, &sources, &universe) {
                    Ok(SeriesEvidence::Witness) => witnesses.push(sequence),
                    Ok(SeriesEvidence::Ambiguous) => ambiguous.push(sequence),
                    Ok(SeriesEvidence::Absent) => {}
                    Err(reason) => failures.push(environment(location.clone(), reason)),
                }
            }
        }
        if read_fault {
            continue;
        }
        for request in observed.values() {
            if request.context.kind != OperationKind::Http
                || input.expect.interfaces == super::InterfacePolicy::McpOnly
                    && !input.tools_ips.contains(&request.context.source)
                || !request_parts_match(
                    &request.context.service,
                    &request.context.method,
                    &request.context.path,
                    request.head.and_then(|(_, head)| head.query.as_deref()),
                    request.body.map(|(_, body)| body.value()).as_ref(),
                    &check.action,
                )
                || !headers_match(
                    &check.action,
                    request.head.map(|(_, head)| head),
                    &location,
                    &mut failures,
                )
                || check.action_effect.is_some()
                    && !committed
                        .get(&index)
                        .is_some_and(|operations| operations.contains(&request.context.id))
            {
                continue;
            }
            if !witnesses.iter().any(|sequence| *sequence < request.started) {
                let earlier: Vec<u64> = ambiguous
                    .iter()
                    .copied()
                    .filter(|sequence| *sequence < request.started)
                    .collect();
                let (disposition, reason) = if earlier.is_empty() {
                    (
                        CheckDisposition::Failed,
                        "no successful response supplied a source-bound record before the action was admitted",
                    )
                } else {
                    (
                        CheckDisposition::Unresolved,
                        "earlier responses covered the source only at a resolution or scope whose meaning needs adjudication",
                    )
                };
                failures.push(CheckFailure {
                    check: CheckKind::Ordering,
                    disposition,
                    location: location.clone(),
                    reason: reason.into(),
                    evidence: candidate_entries.clone(),
                    event_evidence: std::iter::once(request.started)
                        .chain(witnesses.iter().copied())
                        .chain(earlier)
                        .collect(),
                });
            }
        }
    }
    failures
}
