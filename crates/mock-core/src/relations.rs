use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{grading::subset, json_pointer::tokens as pointer_tokens};

mod bound_text;
mod calendar_meeting;
mod firecrawl;
mod interval_coverage;
mod metric_alarm;
mod references;
mod structured;
mod tag_scope;
pub use bound_text::{BoundText, IdentitySource};
pub use calendar_meeting::{CalendarMeeting, meet_uri, meeting_uri};
pub use firecrawl::{FirecrawlCrawlGoal, FirecrawlMonitorGoal};
pub use interval_coverage::IntervalCoverage;
pub use metric_alarm::MetricAlarmGoal;
pub use references::RecordReference;
pub(crate) use references::ReferenceSet;
pub use tag_scope::{TagContext, TagScopeGoal};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordSelector {
    pub collection: String,
    #[serde(default)]
    pub matching: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub field_patterns: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<RecordReference>,
    #[serde(default)]
    pub key_prefix: Option<String>,
    #[serde(default)]
    pub created: bool,
    #[serde(default)]
    pub identity_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordCheck {
    pub selector: RecordSelector,
    #[serde(default)]
    pub equals: Option<usize>,
    #[serde(default)]
    pub min_count: Option<usize>,
    #[serde(default)]
    pub max_count: Option<usize>,
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
    #[serde(default)]
    pub optional_fields: BTreeMap<String, Value>,
    #[serde(default)]
    pub optional_choices: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    pub null_fields: Vec<String>,
    #[serde(default)]
    pub html_text: BTreeMap<String, String>,
    #[serde(default)]
    pub reviewed_text: BTreeMap<String, ReviewedText>,
    #[serde(default)]
    pub reviewed_fields: Option<ReviewedFields>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_alarm: Option<MetricAlarmGoal>,
    #[serde(default)]
    pub any_fields: Vec<BTreeMap<String, Value>>,
    #[serde(default)]
    pub totals: BTreeMap<String, i64>,
    #[serde(default)]
    pub integer_bounds: BTreeMap<String, IntegerBounds>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub array_lengths: BTreeMap<String, usize>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub numeric_fields: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub timestamp_fields: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sheet_date_fields: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_meeting: Option<CalendarMeeting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firecrawl_crawl: Option<FirecrawlCrawlGoal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firecrawl_monitor: Option<FirecrawlMonitorGoal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_coverage: Option<IntervalCoverage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_matchers: Option<crate::label_matchers::LabelMatchersGoal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_scope: Option<TagScopeGoal>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub quantity_fields: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub json_fields: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub value_sets: BTreeMap<String, Vec<Vec<Value>>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub url_host_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedText {
    pub requirement: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
    #[serde(default)]
    pub accepted: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<String>,
    #[serde(default)]
    pub format: ReviewedTextFormat,
    #[serde(default)]
    pub bound: Option<BoundText>,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedFields {
    pub requirement: String,
    pub accepted: Vec<BTreeMap<String, String>>,
}

impl ReviewedFields {
    fn validate(&self) -> Result<(), String> {
        let first = self
            .accepted
            .first()
            .ok_or("reviewed fields require positive examples")?;
        if self.requirement.trim().is_empty() || first.is_empty() {
            return Err("reviewed fields require a criterion and nonempty projections".into());
        }
        for pointer in first.keys() {
            if pointer_tokens(pointer)?.is_empty() {
                return Err("reviewed fields must identify fields inside a record".into());
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for example in &self.accepted {
            if example.keys().ne(first.keys()) {
                return Err(
                    "reviewed field examples must cover the same complete projection".into(),
                );
            }
            let normalized: BTreeMap<_, _> = example
                .iter()
                .map(|(pointer, text)| (pointer.clone(), normalized_text(text)))
                .collect();
            if normalized.values().all(String::is_empty) || !seen.insert(normalized) {
                return Err("reviewed field examples must be nonempty and distinct after whitespace folding".into());
            }
        }
        Ok(())
    }

    fn check(&self, record: &Value) -> Result<(), RecordCheckError> {
        let mut projection = BTreeMap::new();
        for pointer in self.accepted[0].keys() {
            let text = match record.pointer(pointer) {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(text)) => normalized_text(text),
                Some(_) => {
                    return Err(RecordCheckError::Failed(format!(
                        "reviewed content field {pointer} is not text or absent"
                    )));
                }
            };
            projection.insert(pointer.clone(), text);
        }
        if projection.values().all(String::is_empty) {
            return Err(RecordCheckError::Failed(
                "required reviewed content is empty".into(),
            ));
        }
        if self.accepted.iter().any(|example| {
            example
                .iter()
                .all(|(pointer, text)| projection[pointer] == normalized_text(text))
        }) {
            return Ok(());
        }
        Err(RecordCheckError::NeedsAdjudication(format!(
            "unreviewed field projection requires adjudication against: {}",
            self.requirement
        )))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewedTextFormat {
    #[default]
    Plain,
    Html,
    ZendeskHtml,
    NotionRichText,
}

fn normalized_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl ReviewedTextFormat {
    fn text(self, source: &Value) -> Result<String, String> {
        if self == Self::NotionRichText {
            return crate::rich_text::notion_text(source);
        }
        let source = source
            .as_str()
            .ok_or("content has an unsupported nontext representation")?;
        match self {
            Self::Plain => Ok(normalized_text(source)),
            Self::Html => crate::rich_text::passive_text(source),
            Self::ZendeskHtml => crate::rich_text::zendesk_text(source),
            Self::NotionRichText => unreachable!(),
        }
    }
}

fn committed_text(
    value: Option<&Value>,
    format: ReviewedTextFormat,
) -> Result<Option<String>, RecordCheckError> {
    let source = match value {
        None | Some(Value::Null) => return Ok(None),
        Some(source) => source,
    };
    let text = format
        .text(source)
        .map_err(RecordCheckError::NeedsAdjudication)?;
    Ok((!text.is_empty()).then_some(text))
}

impl ReviewedText {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.requirement.trim().is_empty() {
            return Err("reviewed text requires a nonempty criterion".into());
        }
        if let Some(bound) = &self.bound {
            if !self.accepted.is_empty() || !self.rejected.is_empty() {
                return Err(
                    "reviewed text must choose static examples or source-bound examples".into(),
                );
            }
            return bound.validate();
        }
        if self.accepted.is_empty() {
            return Err(
                "reviewed text requires a criterion and positive whole-text examples".into(),
            );
        }
        let mut seen = std::collections::BTreeSet::new();
        for example in &self.accepted {
            let normalized = normalized_text(example);
            if normalized.is_empty() || !seen.insert(normalized) {
                return Err(
                    "reviewed text examples must be nonempty and distinct after whitespace folding"
                        .into(),
                );
            }
        }
        for example in &self.rejected {
            let normalized = normalized_text(example);
            if normalized.is_empty() || !seen.insert(normalized) {
                return Err(
                    "rejected examples must be nonempty, distinct and disjoint from accepted examples"
                        .into(),
                );
            }
        }
        Ok(())
    }

    fn rejects_text(&self, text: &str) -> bool {
        self.rejected
            .iter()
            .any(|example| normalized_text(example) == text)
    }

    fn matches_text(
        &self,
        text: &str,
        initial: Option<&Value>,
        state: &Value,
    ) -> Result<bool, RecordCheckError> {
        match &self.bound {
            Some(bound) => bound.matches(text, initial, state),
            None => Ok(self
                .accepted
                .iter()
                .any(|example| normalized_text(example) == text)),
        }
    }

    pub(crate) fn check(
        &self,
        value: Option<&Value>,
        initial: Option<&Value>,
        state: &Value,
    ) -> Result<(), RecordCheckError> {
        if self.optional && matches!(value, None | Some(Value::Null)) {
            return Ok(());
        }
        let text = value
            .filter(|value| !value.is_null())
            .ok_or_else(|| RecordCheckError::Failed("required text is missing".into()))?;
        if self.format != ReviewedTextFormat::NotionRichText && !text.is_string() {
            return Err(RecordCheckError::Failed(
                "required text is not a string".into(),
            ));
        }
        let normalized = self
            .format
            .text(text)
            .map_err(RecordCheckError::NeedsAdjudication)?;
        if normalized.is_empty() {
            if self.optional {
                return Ok(());
            }
            return Err(RecordCheckError::Failed("required text is empty".into()));
        }
        if self.matches_text(&normalized, initial, state)? {
            return Ok(());
        }
        if self.rejects_text(&normalized) {
            return Err(RecordCheckError::Failed(format!(
                "reviewed incorrect text violates: {}",
                self.requirement
            )));
        }
        Err(RecordCheckError::NeedsAdjudication(format!(
            "unreviewed wording requires adjudication against: {}",
            self.requirement
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegerBounds {
    pub min: i64,
    pub max: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationCheck {
    pub source: RecordSelector,
    pub source_field: String,
    pub target: RecordSelector,
    #[serde(default)]
    pub target_field: String,
    #[serde(default)]
    pub kind: RelationKind,
    #[serde(default)]
    pub fields: Vec<JoinedFieldCheck>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinedFieldCheck {
    pub source_field: String,
    pub target_field: String,
    pub kind: JoinedFieldKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinedFieldKind {
    Equals,
    NegatedInteger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    #[default]
    Equals,
    NumericEquals,
    BsonNumericEquals,
    ContainsReference,
    Bijection,
    ForeignKey,
    KeyReference,
    CountEquals,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum RecordCheckError {
    #[error("{0}")]
    InvalidContract(String),
    #[error("{0}")]
    MissingEvidence(String),
    #[error("{0}")]
    NeedsAdjudication(String),
    #[error("{0}")]
    Failed(String),
}

fn check_sheet_date(
    actual: Option<&Value>,
    expected: &str,
    field: &str,
) -> Result<(), RecordCheckError> {
    let actual = actual
        .ok_or_else(|| RecordCheckError::Failed(format!("missing spreadsheet date at {field}")))?;
    match crate::sheet_date::matches(actual, expected) {
        Ok(true) => Ok(()),
        Ok(false) => Err(RecordCheckError::Failed(format!(
            "spreadsheet date at {field} differs from {expected}"
        ))),
        Err(reason) => Err(RecordCheckError::NeedsAdjudication(reason)),
    }
}

impl RecordSelector {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_depth(0)
    }

    fn validate_depth(&self, depth: usize) -> Result<(), String> {
        if depth > references::MAX_REFERENCE_DEPTH {
            return Err("record reference nesting exceeds the supported depth".into());
        }
        crate::field_patterns::FieldPatterns::compile(&self.field_patterns)?;
        for (index, reference) in self.references.iter().enumerate() {
            reference.validate_depth(depth + 1)?;
            if self.references[..index].contains(reference) {
                return Err("record selectors contain a duplicate reference".into());
            }
        }
        let tokens = pointer_tokens(&self.collection)?;
        if tokens.len() < 3
            || tokens[0] != "services"
            || tokens
                .iter()
                .any(|token| token.is_empty() || token.contains('*'))
        {
            return Err("record selectors must identify a literal business collection".into());
        }
        if tokens.len() > 3 && (tokens.len() < 5 || self.identity_field.is_none()) {
            return Err("nested array collections require an explicit identity field".into());
        }
        if let Some(field) = &self.identity_field
            && pointer_tokens(field)?.is_empty()
        {
            return Err("array identities must identify a field inside the element".into());
        }
        if let Some(field) = &self.latest_by
            && pointer_tokens(field)?.is_empty()
        {
            return Err("latest-record selection needs an explicit integer field".into());
        }
        if self.key_prefix.as_ref().is_some_and(String::is_empty) {
            return Err("record selector key_prefix must not be empty".into());
        }
        if self.matching.is_empty()
            && self.field_patterns.is_empty()
            && self.references.is_empty()
            && self.key_prefix.is_none()
            && !self.created
        {
            return Err(
                "record selectors need matching fields, string patterns, references, a key prefix, or created=true".into(),
            );
        }
        Ok(())
    }

    pub fn selected_identities(
        &self,
        initial: Option<&Value>,
        final_state: &Value,
    ) -> Result<Vec<String>, RecordCheckError> {
        Ok(self
            .select(initial, final_state)?
            .into_iter()
            .map(|(identity, _)| identity)
            .collect())
    }

    pub(crate) fn select<'state>(
        &self,
        initial: Option<&Value>,
        final_state: &'state Value,
    ) -> Result<Vec<(String, &'state Value)>, RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;
        if !final_state["services"].is_object() {
            return Err(RecordCheckError::MissingEvidence(
                "record checks require authoritative final business state".into(),
            ));
        }
        let original = if self.created {
            let initial = initial.ok_or_else(|| {
                RecordCheckError::MissingEvidence(
                    "created-record checks require authoritative initial state".into(),
                )
            })?;
            if !initial["services"].is_object() {
                return Err(RecordCheckError::MissingEvidence(
                    "created-record checks require authoritative initial business state".into(),
                ));
            }
            Some(
                self.entries(initial)?
                    .into_iter()
                    .map(|(key, _)| key)
                    .collect::<std::collections::BTreeSet<_>>(),
            )
        } else {
            None
        };
        let records = self.entries(final_state)?;
        let matcher =
            serde_json::to_value(&self.matching).expect("a map of JSON values is serializable");
        let patterns = crate::field_patterns::FieldPatterns::compile(&self.field_patterns)
            .map_err(RecordCheckError::InvalidContract)?;
        let references = ReferenceSet::resolve(&self.references, initial, final_state)?;
        let mut selected: Vec<_> = records
            .into_iter()
            .filter(|(key, record)| {
                self.key_prefix
                    .as_ref()
                    .is_none_or(|prefix| key.starts_with(prefix))
                    && (!self.created
                        || original
                            .as_ref()
                            .is_none_or(|records| !records.contains(key)))
                    && (self.matching.is_empty() || subset(&matcher, record))
                    && patterns.matches(record)
                    && references.matches(record)
            })
            .collect();
        if let Some(field) = &self.latest_by {
            let mut maximum = None;
            for (_, record) in &selected {
                let value = record
                    .pointer(field)
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        RecordCheckError::Failed(format!(
                            "latest-record field {field} must be a signed integer"
                        ))
                    })?;
                maximum = Some(maximum.map_or(value, |previous: i64| previous.max(value)));
            }
            selected.retain(|(_, record)| record.pointer(field).and_then(Value::as_i64) == maximum);
            if selected.len() > 1 {
                return Err(RecordCheckError::Failed(format!(
                    "latest-record field {field} has an ambiguous maximum"
                )));
            }
        }
        Ok(selected)
    }

    pub(crate) fn entries<'state>(
        &self,
        state: &'state Value,
    ) -> Result<Vec<(String, &'state Value)>, RecordCheckError> {
        match state.pointer(&self.collection) {
            None => Ok(Vec::new()),
            Some(Value::Object(records)) => Ok(records
                .iter()
                .map(|(key, value)| (key.clone(), value))
                .collect()),
            Some(Value::Array(records)) => {
                let field = self.identity_field.as_deref().ok_or_else(|| {
                    RecordCheckError::InvalidContract(
                        "array selectors require identity_field".into(),
                    )
                })?;
                let mut seen = std::collections::BTreeSet::new();
                records
                    .iter()
                    .map(|record| {
                        let key = identity(record.pointer(field))?;
                        if !seen.insert(key.clone()) {
                            return Err(RecordCheckError::Failed(
                                "array selection contains duplicate identities".into(),
                            ));
                        }
                        Ok((key, record))
                    })
                    .collect()
            }
            Some(_) => Err(RecordCheckError::MissingEvidence(format!(
                "{} is not a record map or identity array",
                self.collection
            ))),
        }
    }
}

impl RecordCheck {
    pub(crate) fn has_committed_requirements(&self) -> bool {
        !self.html_text.is_empty()
            || !self.reviewed_text.is_empty()
            || self.reviewed_fields.is_some()
            || !self.timestamp_fields.is_empty()
            || !self.sheet_date_fields.is_empty()
            || self.calendar_meeting.is_some()
            || self.firecrawl_crawl.is_some()
            || self.firecrawl_monitor.is_some()
            || self.interval_coverage.is_some()
            || self.label_matchers.is_some()
            || self.tag_scope.is_some()
    }

    pub(crate) fn check_committed_requirements(
        &self,
        initial: Option<&Value>,
        state: &Value,
        before: Option<&Value>,
        after: &Value,
    ) -> Result<(), RecordCheckError> {
        if let Some(goal) = &self.calendar_meeting {
            goal.check(after, state, false)?;
        }
        if let Some(goal) = &self.firecrawl_crawl {
            goal.check(initial, after)?;
        }
        if let Some(goal) = &self.firecrawl_monitor {
            goal.check(initial, after, state, false)?;
        }
        if let Some(goal) = &self.tag_scope {
            goal.check(initial, after)?;
        }
        if let Some(goal) = &self.interval_coverage {
            goal.check_record(after)?;
        }
        for (field, expected) in &self.timestamp_fields {
            if before.is_some_and(|record| record.pointer(field) == after.pointer(field)) {
                continue;
            }
            let actual = after
                .pointer(field)
                .and_then(Value::as_str)
                .and_then(crate::clock::parse_rfc3339_nanos);
            if actual != crate::clock::parse_rfc3339_nanos(expected) {
                return Err(RecordCheckError::Failed(format!(
                    "committed timestamp at {field} differs from the requested instant"
                )));
            }
        }
        for (field, expected) in &self.sheet_date_fields {
            if before.is_some_and(|record| record.pointer(field) == after.pointer(field)) {
                continue;
            }
            if matches!(after.pointer(field), None | Some(Value::Null))
                || after.pointer(field).and_then(Value::as_str) == Some("")
            {
                continue;
            }
            check_sheet_date(after.pointer(field), expected, field)?;
        }
        if let Some(goal) = &self.label_matchers
            && before.is_none_or(|record| record.pointer(&goal.field) != after.pointer(&goal.field))
        {
            goal.check(after)?;
        }
        for (field, expected) in &self.html_text {
            if before.is_some_and(|record| record.pointer(field) == after.pointer(field)) {
                continue;
            }
            if let Some(text) = committed_text(after.pointer(field), ReviewedTextFormat::Html)?
                && text != normalized_text(expected)
            {
                return Err(RecordCheckError::NeedsAdjudication(format!(
                    "committed content at {field} requires review against the requested literal: {expected}"
                )));
            }
        }
        for (field, check) in &self.reviewed_text {
            if before.is_some_and(|record| record.pointer(field) == after.pointer(field)) {
                continue;
            }
            if let Some(text) = committed_text(after.pointer(field), check.format)? {
                if before
                    .and_then(|record| committed_text(record.pointer(field), check.format).ok())
                    .flatten()
                    .as_ref()
                    == Some(&text)
                {
                    continue;
                }
                if check.rejects_text(&text) {
                    return Err(RecordCheckError::Failed(format!(
                        "committed reviewed incorrect text at {field} violates: {}",
                        check.requirement
                    )));
                }
                match check.matches_text(&text, initial, state) {
                    Ok(true) => {}
                    Ok(false) | Err(RecordCheckError::Failed(_)) => {
                        return Err(RecordCheckError::NeedsAdjudication(format!(
                            "committed content at {field} requires adjudication against: {}",
                            check.requirement
                        )));
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        if let Some(check) = &self.reviewed_fields {
            let fields = &check.accepted[0];
            let changed = before.is_none_or(|record| {
                fields
                    .keys()
                    .any(|field| record.pointer(field) != after.pointer(field))
            });
            let nonempty = fields.keys().any(|field| match after.pointer(field) {
                None | Some(Value::Null) => false,
                Some(Value::String(text)) => !text.trim().is_empty(),
                Some(_) => true,
            });
            if changed && nonempty {
                match check.check(after) {
                    Err(RecordCheckError::Failed(reason)) => {
                        return Err(RecordCheckError::NeedsAdjudication(format!(
                            "committed content requires adjudication against: {} ({reason})",
                            check.requirement
                        )));
                    }
                    result => result?,
                }
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.validate_firecrawl()?;
        let competing = |field: &str| {
            self.fields.contains_key(field)
                || self.numeric_fields.contains_key(field)
                || self.sheet_date_fields.contains_key(field)
                || self.quantity_fields.contains_key(field)
                || self.json_fields.contains_key(field)
                || self.value_sets.contains_key(field)
                || self.array_lengths.contains_key(field)
                || self.reviewed_text.contains_key(field)
                || self.html_text.contains_key(field)
                || self.optional_fields.contains_key(field)
                || self.optional_choices.contains_key(field)
                || self.null_fields.iter().any(|other| other == field)
                || self
                    .any_fields
                    .iter()
                    .any(|fields| fields.contains_key(field))
        };
        if let Some(goal) = &self.interval_coverage {
            goal.validate()?;
            for field in [&goal.start_field, &goal.end_field] {
                if competing(field)
                    || self.timestamp_fields.contains_key(field)
                    || self.integer_bounds.contains_key(field)
                    || self.totals.contains_key(field)
                    || self
                        .label_matchers
                        .as_ref()
                        .is_some_and(|other| other.field == *field)
                    || self.reviewed_fields.as_ref().is_some_and(|other| {
                        other
                            .accepted
                            .iter()
                            .any(|fields| fields.contains_key(field))
                    })
                {
                    return Err(
                        "interval coverage cannot compete with another boundary predicate".into(),
                    );
                }
            }
        }
        for (field, expected) in &self.timestamp_fields {
            let path = pointer_tokens(field)?;
            if path.is_empty()
                || path
                    .iter()
                    .any(|part| part.is_empty() || part.contains('*'))
                || crate::clock::parse_rfc3339_nanos(expected).is_none()
                || competing(field)
                || self
                    .label_matchers
                    .as_ref()
                    .is_some_and(|goal| goal.field == *field)
            {
                return Err("timestamp checks require literal fields, exact supported RFC3339 values and no competing predicates".into());
            }
        }
        for (field, expected) in &self.sheet_date_fields {
            let path = pointer_tokens(field)?;
            if path.is_empty()
                || path
                    .iter()
                    .any(|part| part.is_empty() || part.contains('*'))
                || crate::sheet_date::serial(expected).is_none()
                || self.fields.contains_key(field)
                || self.timestamp_fields.contains_key(field)
                || self.numeric_fields.contains_key(field)
                || self.quantity_fields.contains_key(field)
                || self.json_fields.contains_key(field)
                || self.value_sets.contains_key(field)
                || self.array_lengths.contains_key(field)
                || self.optional_fields.contains_key(field)
                || self.optional_choices.contains_key(field)
                || self.null_fields.contains(field)
                || self.html_text.contains_key(field)
                || self.reviewed_text.contains_key(field)
                || self.integer_bounds.contains_key(field)
                || self.totals.contains_key(field)
                || self
                    .any_fields
                    .iter()
                    .any(|fields| fields.contains_key(field))
                || self.selector.field_patterns.contains_key(field)
                || serde_json::to_value(&self.selector.matching)
                    .ok()
                    .is_some_and(|matching| matching.pointer(field).is_some())
                || self.reviewed_fields.as_ref().is_some_and(|check| {
                    check
                        .accepted
                        .iter()
                        .any(|fields| fields.contains_key(field))
                })
            {
                return Err("spreadsheet dates require literal fields, canonical required dates and no competing predicates".into());
            }
        }
        if let Some(goal) = &self.label_matchers {
            goal.validate()?;
            if competing(&goal.field) {
                return Err(
                    "semantic matcher goals must not compete with spelling or array predicates"
                        .into(),
                );
            }
        }
        self.selector.validate()?;
        if let Some(goal) = &self.calendar_meeting {
            goal.validate()?;
            if self.selector.collection != "/services/google/calendar_events"
                || !self.selector.created
            {
                return Err(
                    "calendar meeting goals require the complete created-event collection".into(),
                );
            }
            let pointers = self
                .fields
                .keys()
                .chain(self.optional_fields.keys())
                .chain(self.optional_choices.keys())
                .chain(self.null_fields.iter())
                .chain(self.numeric_fields.keys())
                .chain(self.timestamp_fields.keys())
                .chain(self.sheet_date_fields.keys())
                .chain(self.array_lengths.keys())
                .chain(self.quantity_fields.keys())
                .chain(self.json_fields.keys())
                .chain(self.value_sets.keys())
                .chain(self.integer_bounds.keys())
                .chain(self.totals.keys())
                .chain(self.html_text.keys())
                .chain(self.reviewed_text.keys())
                .chain(self.selector.field_patterns.keys())
                .chain(self.url_host_fields.iter())
                .chain(
                    self.reviewed_fields
                        .iter()
                        .flat_map(|review| &review.accepted)
                        .flat_map(BTreeMap::keys),
                )
                .chain(self.any_fields.iter().flat_map(BTreeMap::keys));
            if !self.selector.matching.is_empty()
                || !self.selector.references.is_empty()
                || self.selector.key_prefix.is_some()
                || self.selector.latest_by.is_some()
                || self.selector.identity_field.is_some()
                || pointers.into_iter().any(|field| goal.owns_field(field))
                || self.interval_coverage.is_some()
                || self.tag_scope.is_some()
                || self.label_matchers.is_some()
            {
                return Err("calendar meeting goals cannot compete with raw event selection or owned-field predicates".into());
            }
        }
        self.validate_structured()?;
        if let Some(goal) = &self.metric_alarm {
            goal.validate()?;
        }
        if let Some(goal) = &self.tag_scope {
            goal.validate()?;
            let pointers = self
                .fields
                .keys()
                .chain(self.optional_fields.keys())
                .chain(self.optional_choices.keys())
                .chain(self.null_fields.iter())
                .chain(self.array_lengths.keys())
                .chain(self.numeric_fields.keys())
                .chain(self.timestamp_fields.keys())
                .chain(self.sheet_date_fields.keys())
                .chain(self.value_sets.keys())
                .chain(self.json_fields.keys())
                .chain(self.quantity_fields.keys())
                .chain(self.integer_bounds.keys())
                .chain(self.totals.keys())
                .chain(self.html_text.keys())
                .chain(self.reviewed_text.keys())
                .chain(self.selector.field_patterns.keys())
                .chain(self.any_fields.iter().flat_map(BTreeMap::keys));
            if pointers.into_iter().any(|pointer| goal.owns_field(pointer))
                || serde_json::to_value(&self.selector.matching)
                    .ok()
                    .is_some_and(|matching| matching.pointer(&goal.field).is_some())
                || self
                    .label_matchers
                    .as_ref()
                    .is_some_and(|other| goal.owns_field(&other.field))
                || self.interval_coverage.as_ref().is_some_and(|other| {
                    goal.owns_field(&other.start_field) || goal.owns_field(&other.end_field)
                })
            {
                return Err(
                    "semantic tag scopes must not compete with raw scope spelling predicates"
                        .into(),
                );
            }
        }
        if self.equals.is_some() && (self.min_count.is_some() || self.max_count.is_some()) {
            return Err("record counts must use equals or a range, not both".into());
        }
        if self.equals.is_none() && self.min_count.is_none() && self.max_count.is_none() {
            return Err("record checks require an explicit count predicate".into());
        }
        if self.equals.is_none() && self.min_count.unwrap_or(0) == 0 && self.max_count.is_none() {
            return Err("record count predicate cannot be an unbounded minimum of zero".into());
        }
        if self
            .min_count
            .zip(self.max_count)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
        {
            return Err("record count range is reversed".into());
        }
        if (self.equals == Some(0) || self.max_count == Some(0))
            && (!self.fields.is_empty()
                || !self.optional_fields.is_empty()
                || !self.optional_choices.is_empty()
                || !self.null_fields.is_empty()
                || !self.integer_bounds.is_empty()
                || !self.array_lengths.is_empty()
                || !self.numeric_fields.is_empty()
                || !self.timestamp_fields.is_empty()
                || !self.sheet_date_fields.is_empty()
                || self.calendar_meeting.is_some()
                || self.firecrawl_crawl.is_some()
                || self.firecrawl_monitor.is_some()
                || self.interval_coverage.is_some()
                || self.label_matchers.is_some()
                || self.tag_scope.is_some()
                || !self.quantity_fields.is_empty()
                || !self.json_fields.is_empty()
                || !self.value_sets.is_empty()
                || !self.html_text.is_empty()
                || !self.reviewed_text.is_empty()
                || self.reviewed_fields.is_some()
                || self.metric_alarm.is_some()
                || !self.any_fields.is_empty())
        {
            return Err("zero-record checks cannot assert fields on nonexistent records".into());
        }
        for pointer in self
            .fields
            .keys()
            .chain(self.optional_fields.keys())
            .chain(self.optional_choices.keys())
            .chain(self.null_fields.iter())
            .chain(self.totals.keys())
            .chain(self.integer_bounds.keys())
            .chain(self.array_lengths.keys())
            .chain(self.numeric_fields.keys())
            .chain(self.sheet_date_fields.keys())
            .chain(self.quantity_fields.keys())
            .chain(self.json_fields.keys())
            .chain(self.value_sets.keys())
            .chain(self.json_fields.keys())
            .chain(self.value_sets.keys())
            .chain(self.html_text.keys())
            .chain(self.reviewed_text.keys())
            .chain(self.any_fields.iter().flat_map(BTreeMap::keys))
        {
            if pointer_tokens(pointer)?.is_empty() {
                return Err("record field checks must identify a field inside the record".into());
            }
            if self.metric_alarm.is_some() && MetricAlarmGoal::owns_field(pointer) {
                return Err(
                    "metric-alarm semantics must not compete with raw field predicates".into(),
                );
            }
        }
        for (index, fields) in self.any_fields.iter().enumerate() {
            if fields.is_empty() || self.any_fields[..index].contains(fields) {
                return Err("field alternatives must be nonempty and distinct".into());
            }
        }
        for choices in self.optional_choices.values() {
            if choices.is_empty()
                || choices
                    .iter()
                    .enumerate()
                    .any(|(index, value)| choices[..index].contains(value))
            {
                return Err("optional field choices must be nonempty and distinct".into());
            }
        }
        for (field, check) in &self.reviewed_text {
            if self.fields.contains_key(field)
                || self.numeric_fields.contains_key(field)
                || self.array_lengths.contains_key(field)
                || self.optional_fields.contains_key(field)
                || self.optional_choices.contains_key(field)
                || self.html_text.contains_key(field)
                || self.null_fields.contains(field)
                || self
                    .any_fields
                    .iter()
                    .any(|fields| fields.contains_key(field))
            {
                return Err("reviewed text must not also have another field predicate".into());
            }
            check.validate()?;
        }
        for (field, expected) in &self.numeric_fields {
            if !crate::decimal::equivalent(expected, expected)
                || self.fields.contains_key(field)
                || self.optional_fields.contains_key(field)
                || self.optional_choices.contains_key(field)
                || self.null_fields.contains(field)
                || self.html_text.contains_key(field)
                || self.array_lengths.contains_key(field)
                || self
                    .any_fields
                    .iter()
                    .any(|fields| fields.contains_key(field))
            {
                return Err("numeric fields require valid exact decimal values without competing field predicates".into());
            }
        }
        if let Some(check) = &self.reviewed_fields {
            check.validate()?;
            for field in check.accepted[0].keys() {
                if self.metric_alarm.is_some() && MetricAlarmGoal::owns_field(field) {
                    return Err(
                        "metric-alarm semantics must not compete with reviewed fields".into(),
                    );
                }
                if self.fields.contains_key(field)
                    || self.numeric_fields.contains_key(field)
                    || self.optional_fields.contains_key(field)
                    || self.optional_choices.contains_key(field)
                    || self.null_fields.contains(field)
                    || self.html_text.contains_key(field)
                    || self.reviewed_text.contains_key(field)
                    || self.totals.contains_key(field)
                    || self.integer_bounds.contains_key(field)
                    || self
                        .any_fields
                        .iter()
                        .any(|fields| fields.contains_key(field))
                {
                    return Err("reviewed fields must not also have another field predicate".into());
                }
            }
        }
        if self
            .integer_bounds
            .values()
            .any(|bounds| bounds.min > bounds.max)
        {
            return Err("integer field bounds are reversed".into());
        }
        Ok(())
    }

    pub fn check(
        &self,
        initial: Option<&Value>,
        final_state: &Value,
    ) -> Result<(), RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;
        let records = self.selector.select(initial, final_state)?;
        if self
            .equals
            .is_some_and(|expected| records.len() != expected)
            || self
                .min_count
                .is_some_and(|minimum| records.len() < minimum)
            || self
                .max_count
                .is_some_and(|maximum| records.len() > maximum)
        {
            return Err(RecordCheckError::Failed(format!(
                "{} selected {} records; required count is equals={:?}, min={:?}, max={:?}",
                self.selector.collection,
                records.len(),
                self.equals,
                self.min_count,
                self.max_count,
            )));
        }
        let mut totals = BTreeMap::<&str, i128>::new();
        let mut unresolved = None;
        self.check_structured(&records)?;
        if let Some(goal) = &self.interval_coverage {
            goal.check(&records)?;
        }
        for (key, record) in records {
            if let Some(goal) = &self.calendar_meeting {
                goal.check(record, final_state, true)?;
            }
            if let Some(goal) = &self.firecrawl_crawl {
                goal.check(initial, record)?;
            }
            if let Some(goal) = &self.firecrawl_monitor {
                goal.check(initial, record, final_state, true)?;
            }
            for (field, expected) in &self.sheet_date_fields {
                check_sheet_date(record.pointer(field), expected, field)?;
            }
            if let Some(goal) = &self.tag_scope {
                goal.check(initial, record)?;
            }
            for (field, expected) in &self.timestamp_fields {
                if record
                    .pointer(field)
                    .and_then(Value::as_str)
                    .and_then(crate::clock::parse_rfc3339_nanos)
                    != crate::clock::parse_rfc3339_nanos(expected)
                {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {key} has the wrong exact timestamp at {field}",
                        self.selector.collection
                    )));
                }
            }
            if let Some(goal) = &self.label_matchers {
                goal.check(record)?;
            }
            for (field, expected) in &self.fields {
                if record.pointer(field) != Some(expected) {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {key} did not satisfy field {field}",
                        self.selector.collection
                    )));
                }
            }
            for (field, expected) in &self.optional_fields {
                if record
                    .pointer(field)
                    .is_some_and(|value| !value.is_null() && value != expected)
                {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {key} has an inconsistent optional field {field}",
                        self.selector.collection
                    )));
                }
            }
            for (field, expected) in &self.array_lengths {
                if record
                    .pointer(field)
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    != Some(*expected)
                {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {key} field {field} must be an array of length {expected}",
                        self.selector.collection
                    )));
                }
            }
            for (field, expected) in &self.numeric_fields {
                if !record
                    .pointer(field)
                    .is_some_and(|value| crate::decimal::equivalent(value, expected))
                {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {key} did not satisfy exact numeric field {field}",
                        self.selector.collection
                    )));
                }
            }
            for (field, choices) in &self.optional_choices {
                if record
                    .pointer(field)
                    .is_some_and(|value| !value.is_null() && !choices.contains(value))
                {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {key} has an inconsistent optional choice at {field}",
                        self.selector.collection
                    )));
                }
            }
            for field in &self.null_fields {
                if record.pointer(field) != Some(&Value::Null) {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {key} field {field} must be explicitly null",
                        self.selector.collection
                    )));
                }
            }
            if !self.any_fields.is_empty()
                && !self.any_fields.iter().any(|fields| {
                    fields
                        .iter()
                        .all(|(field, expected)| record.pointer(field) == Some(expected))
                })
            {
                return Err(RecordCheckError::Failed(format!(
                    "{} record {key} did not satisfy any complete field alternative",
                    self.selector.collection
                )));
            }
            for (field, bounds) in &self.integer_bounds {
                let value = record.pointer(field).and_then(Value::as_i64);
                if value.is_none_or(|value| value < bounds.min || value > bounds.max) {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {key} field {field} is not an integer in [{}, {}]",
                        self.selector.collection, bounds.min, bounds.max,
                    )));
                }
            }
            for field in self.totals.keys() {
                let value = record
                    .pointer(field)
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        RecordCheckError::Failed(format!(
                            "{} record {key} field {field} is not an integer",
                            self.selector.collection
                        ))
                    })?;
                let total = totals.entry(field).or_default();
                *total = total.checked_add(i128::from(value)).ok_or_else(|| {
                    RecordCheckError::MissingEvidence(
                        "record aggregation exceeded the supported integer range".into(),
                    )
                })?;
            }
            if let Some(goal) = &self.metric_alarm {
                match goal.check(record) {
                    Ok(()) => {}
                    Err(RecordCheckError::NeedsAdjudication(reason)) => {
                        unresolved.get_or_insert_with(|| {
                            format!("{} record {key}: {reason}", self.selector.collection)
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
            for (field, expected) in &self.html_text {
                let value = record
                    .pointer(field)
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        RecordCheckError::Failed(format!(
                            "{} record {key} field {field} is not rich-text content",
                            self.selector.collection
                        ))
                    })?;
                match crate::rich_text::equivalent(value, expected) {
                    Ok(true) => {}
                    Ok(false) => {
                        return Err(RecordCheckError::Failed(format!(
                            "{} record {key} field {field} does not match the requested text",
                            self.selector.collection
                        )));
                    }
                    Err(reason) => {
                        unresolved.get_or_insert_with(|| {
                            format!(
                                "{} record {key} field {field}: {reason}",
                                self.selector.collection
                            )
                        });
                    }
                }
            }
            for (field, check) in &self.reviewed_text {
                match check.check(record.pointer(field), initial, final_state) {
                    Ok(()) => {}
                    Err(RecordCheckError::NeedsAdjudication(reason)) => {
                        unresolved.get_or_insert_with(|| {
                            format!(
                                "{} record {key} field {field}: {reason}",
                                self.selector.collection
                            )
                        });
                    }
                    Err(RecordCheckError::Failed(reason)) => {
                        return Err(RecordCheckError::Failed(format!(
                            "{} record {key} field {field}: {reason}",
                            self.selector.collection
                        )));
                    }
                    Err(error) => return Err(error),
                }
            }
            if let Some(check) = &self.reviewed_fields {
                match check.check(record) {
                    Ok(()) => {}
                    Err(RecordCheckError::NeedsAdjudication(reason)) => {
                        unresolved.get_or_insert_with(|| {
                            format!("{} record {key}: {reason}", self.selector.collection)
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        for (field, expected) in &self.totals {
            let actual = totals.get(field.as_str()).copied().unwrap_or(0);
            if actual != i128::from(*expected) {
                return Err(RecordCheckError::Failed(format!(
                    "{} total at {field} was {actual}; expected {expected}",
                    self.selector.collection,
                )));
            }
        }
        if let Some(reason) = unresolved {
            return Err(RecordCheckError::NeedsAdjudication(reason));
        }
        Ok(())
    }
}

impl RelationCheck {
    pub fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        self.target.validate()?;
        if pointer_tokens(&self.source_field)?.is_empty() {
            return Err("relations require a source field inside the selected record".into());
        }
        if self.kind == RelationKind::KeyReference {
            if !self.target_field.is_empty()
                || self.target.identity_field.is_some()
                || pointer_tokens(&self.target.collection)?.len() != 3
            {
                return Err(
                    "key references require an object-record target and no target_field".into(),
                );
            }
        } else if pointer_tokens(&self.target_field)?.is_empty() {
            return Err("field relations require a target field inside the selected record".into());
        }
        if !self.fields.is_empty()
            && !matches!(
                self.kind,
                RelationKind::Bijection | RelationKind::ForeignKey
            )
        {
            return Err("joined field checks require an identity-bound relationship".into());
        }
        if self.allow_empty && self.kind != RelationKind::Bijection {
            return Err("empty relationship permission requires an explicit bijection".into());
        }
        for (index, field) in self.fields.iter().enumerate() {
            if pointer_tokens(&field.source_field)?.is_empty()
                || pointer_tokens(&field.target_field)?.is_empty()
            {
                return Err("joined field checks must identify fields inside both records".into());
            }
            if self.fields[..index].contains(field) {
                return Err("joined field checks contain a duplicate predicate".into());
            }
        }
        Ok(())
    }

    pub fn check(
        &self,
        initial: Option<&Value>,
        final_state: &Value,
    ) -> Result<(), RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;
        if self.kind == RelationKind::CountEquals {
            let sources = identities(&self.source, &self.source_field, initial, final_state)?;
            let target = select_one(&self.target, initial, final_state, "target")?;
            let expected = target
                .pointer(&self.target_field)
                .filter(|value| value.is_number())
                .ok_or_else(|| {
                    RecordCheckError::Failed("count targets require an actual numeric count".into())
                })?;
            return if crate::decimal::equivalent(&serde_json::json!(sources.len()), expected) {
                Ok(())
            } else {
                Err(RecordCheckError::Failed(format!(
                    "{} distinct source identities do not equal target count {expected}",
                    sources.len()
                )))
            };
        }
        if self.kind == RelationKind::KeyReference {
            let targets: BTreeSet<_> = self
                .target
                .select(initial, final_state)?
                .into_iter()
                .map(|(key, _record)| key)
                .collect();
            if targets.is_empty() || targets.iter().any(String::is_empty) {
                return Err(RecordCheckError::Failed(
                    "key references require nonempty selected target keys".into(),
                ));
            }
            for (key, record) in self.source.select(initial, final_state)? {
                let reference = record.pointer(&self.source_field).and_then(Value::as_str);
                if !reference
                    .is_some_and(|reference| !reference.is_empty() && targets.contains(reference))
                {
                    return Err(RecordCheckError::Failed(format!(
                        "source record {key} does not reference an actual selected record key"
                    )));
                }
            }
            return Ok(());
        }
        if self.kind == RelationKind::ForeignKey {
            let target = identities(&self.target, &self.target_field, initial, final_state)?;
            if target.is_empty() {
                return Err(RecordCheckError::Failed(
                    "foreign-key references require a nonempty target selection".into(),
                ));
            }
            for (key, record) in self.source.select(initial, final_state)? {
                let identity = identity(record.pointer(&self.source_field))?;
                if !target.contains_key(&identity) {
                    return Err(RecordCheckError::Failed(format!(
                        "source record {key} references an identity outside the selected targets"
                    )));
                }
                for field in &self.fields {
                    let source_value = record.pointer(&field.source_field);
                    let target_value = target[&identity].pointer(&field.target_field);
                    let matched = match field.kind {
                        JoinedFieldKind::Equals => {
                            source_value.is_some() && source_value == target_value
                        }
                        JoinedFieldKind::NegatedInteger => source_value
                            .and_then(Value::as_i64)
                            .zip(target_value.and_then(Value::as_i64))
                            .is_some_and(|(source, target)| {
                                i128::from(source) == -i128::from(target)
                            }),
                    };
                    if !matched {
                        return Err(RecordCheckError::Failed(format!(
                            "source record {key} does not satisfy the joined field {} on its actual parent",
                            field.source_field
                        )));
                    }
                }
            }
            return Ok(());
        }
        if self.kind == RelationKind::Bijection {
            let source = identities(&self.source, &self.source_field, initial, final_state)?;
            let target = identities(&self.target, &self.target_field, initial, final_state)?;
            if self.allow_empty && source.is_empty() && target.is_empty() {
                return Ok(());
            }
            if source.is_empty() || !source.keys().eq(target.keys()) {
                return Err(RecordCheckError::Failed(
                    "selected identity sets do not form a nonempty bijection".into(),
                ));
            }
            for (identity, source_record) in source {
                let target_record = target[&identity];
                for field in &self.fields {
                    let source_value = source_record.pointer(&field.source_field);
                    let target_value = target_record.pointer(&field.target_field);
                    let matches = match field.kind {
                        JoinedFieldKind::Equals => {
                            source_value.is_some() && source_value == target_value
                        }
                        JoinedFieldKind::NegatedInteger => source_value
                            .and_then(Value::as_i64)
                            .zip(target_value.and_then(Value::as_i64))
                            .is_some_and(|(source, target)| {
                                i128::from(source) == -i128::from(target)
                            }),
                    };
                    if !matches {
                        return Err(RecordCheckError::Failed(format!(
                            "joined record {identity} did not satisfy {:?} between {} and {}",
                            field.kind, field.source_field, field.target_field,
                        )));
                    }
                }
            }
            return Ok(());
        }
        let source = select_one(&self.source, initial, final_state, "source")?;
        let target = select_one(&self.target, initial, final_state, "target")?;
        let source_value = source.pointer(&self.source_field).ok_or_else(|| {
            RecordCheckError::Failed(format!("source field {} is missing", self.source_field))
        })?;
        let target_value = target.pointer(&self.target_field).ok_or_else(|| {
            RecordCheckError::Failed(format!("target field {} is missing", self.target_field))
        })?;
        let matched = match self.kind {
            RelationKind::Equals => source_value == target_value,
            RelationKind::NumericEquals => crate::decimal::equivalent(source_value, target_value),
            RelationKind::BsonNumericEquals => {
                crate::decimal::bson_equivalent(source_value, target_value)
            }
            RelationKind::Bijection => unreachable!("handled before single-record selection"),
            RelationKind::ForeignKey => unreachable!("handled before single-record selection"),
            RelationKind::KeyReference => unreachable!("handled before single-record selection"),
            RelationKind::CountEquals => unreachable!("handled before single-record selection"),
            RelationKind::ContainsReference => match (source_value.as_str(), target_value.as_str())
            {
                (Some(reference), Some(text))
                    if !reference.is_empty() && !reference.chars().any(char::is_whitespace) =>
                {
                    contains_reference(text, reference)
                }
                _ => false,
            },
        };
        if !matched {
            return Err(RecordCheckError::Failed(format!(
                "{:?} relation between {}{} and {}{} was not satisfied",
                self.kind,
                self.source.collection,
                self.source_field,
                self.target.collection,
                self.target_field
            )));
        }
        Ok(())
    }
}

fn identities<'state>(
    selector: &RecordSelector,
    field: &str,
    initial: Option<&Value>,
    final_state: &'state Value,
) -> Result<BTreeMap<String, &'state Value>, RecordCheckError> {
    let records = selector.select(initial, final_state)?;
    let mut identities = BTreeMap::new();
    for (_, record) in records {
        let identity = identity(record.pointer(field))?;
        if identities.insert(identity, record).is_some() {
            return Err(RecordCheckError::Failed(
                "relation selection contains duplicate identities".into(),
            ));
        }
    }
    Ok(identities)
}

fn identity(value: Option<&Value>) -> Result<String, RecordCheckError> {
    match value {
        Some(Value::String(value)) if !value.is_empty() => Ok(format!("string:{value}")),
        Some(Value::Number(value)) if value.is_i64() || value.is_u64() => {
            Ok(format!("integer:{value}"))
        }
        _ => Err(RecordCheckError::Failed(
            "relation identity fields require nonempty string or integer values".into(),
        )),
    }
}

fn select_one<'state>(
    selector: &RecordSelector,
    initial: Option<&Value>,
    final_state: &'state Value,
    label: &str,
) -> Result<&'state Value, RecordCheckError> {
    let selected = selector.select(initial, final_state)?;
    if selected.len() != 1 {
        return Err(RecordCheckError::Failed(format!(
            "{label} selector {} selected {} records; exactly one is required",
            selector.collection,
            selected.len()
        )));
    }
    Ok(selected[0].1)
}

fn contains_reference(text: &str, reference: &str) -> bool {
    text.match_indices(reference).any(|(offset, matched)| {
        let before = text[..offset].chars().next_back();
        let after = text[offset + matched.len()..].chars().next();
        before.is_none_or(|character| !identifier_character(character))
            && after.is_none_or(|character| !identifier_character(character))
    })
}

fn identifier_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-')
}
