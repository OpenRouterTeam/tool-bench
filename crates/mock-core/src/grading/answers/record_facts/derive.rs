//! Bounded derivation of answer facts from authored baseline records.
//!
//! The interface is deliberately small: one primary record (a filtered
//! selection, an extreme, or one group of a bounded aggregate), named records
//! reached from it by equality joins, and facts read from those records
//! (optionally through one reviewed capture pattern for text sources).  Every
//! value comes from the initial business state; a contract never restates the
//! expected answer.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    RecordFact, RecordFactKind, RecordFacts, RecordSelection,
    numbers::{Date, Rational},
};
use crate::{json_pointer, relations::RecordCheckError};

/// Maximum number of related records in one contract.
pub(super) const MAX_RELATED: usize = 8;
/// Maximum number of rows a bounded aggregate may read.
const MAX_ROWS: usize = 20_000;
/// Maximum number of alternative values kept per fact.
const MAX_ALTERNATIVES: usize = 2_000;

/// Reviewed record filter beyond the selector's equality and text patterns.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordFilter {
    /// Keeps records that equal every listed field.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub matching: BTreeMap<String, Value>,
    /// Keeps records whose text field matches a reviewed pattern.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub patterns: BTreeMap<String, String>,
    /// Keeps records in which every listed field holds a value (not null).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub present: Vec<String>,
    /// Keeps records in which every listed field is null or missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub absent: Vec<String>,
    /// Drops a record that equals every field of any listed example.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluding: Vec<BTreeMap<String, Value>>,
    /// Keeps records whose numeric field lies in an inclusive range.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ranges: BTreeMap<String, NumericRange>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumericRange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
}

/// One group of a bounded aggregate over the selected records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// Array field to expand: each element becomes a row `{parent, item}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unnest: Option<String>,
    /// Row field that names the group; absent means one group of every row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// Reviewed filter on the rows (after `unnest`).
    #[serde(default, skip_serializing_if = "is_default")]
    pub rows: RecordFilter,
    pub measure: Measure,
    #[serde(default)]
    pub pick: Pick,
    /// Per-row related records: each row joins independently to these
    /// records, and their fields are available as `/row_related/<name>/<field>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub row_related: BTreeMap<String, RowRelated>,
    /// A second-level aggregation within the winning group's members.
    /// After the primary aggregate picks a group, this re-aggregates that
    /// group's rows.  The result is available as a related-like record
    /// named `_second_level` with `{key, value, count}` fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second_level: Option<Box<SecondLevel>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    Count,
    Sum(String),
    Average(String),
    /// Sum over rows of the product of the listed fields (price × quantity).
    SumProduct(Vec<String>),
    /// Like `SumProduct` but each row's product is divided by a value
    /// looked up from a field (e.g. interval → divisor for yearly→monthly).
    SumProductNormalized {
        fields: Vec<String>,
        divisor: DivisorMap,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    #[default]
    Only,
    Largest,
    Smallest,
}

/// A per-row related record inside an aggregate: each expanded row joins
/// independently to this collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowRelated {
    pub collection: String,
    /// Row field → related-record field.
    pub on: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub filter: RecordFilter,
}

/// Maps a text field's value to a numeric divisor.  Used for interval-to-
/// monthly normalization (`year → 12`, `month → 1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DivisorMap {
    /// JSON pointer into the (possibly row-related-enriched) row.
    pub field: String,
    /// Field value → divisor (as a decimal string).
    pub values: BTreeMap<String, String>,
}

/// A second-level aggregation within the winning group's rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecondLevel {
    /// Field to group the rows by for the second level.
    pub by: String,
    pub measure: Measure,
    pub pick: Pick,
}

/// A record reached from another named record by field equality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelatedRecord {
    /// No matching record is permitted for optional explanatory facts, but
    /// missing collections, malformed fields and ambiguous matches still fail.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_absence: bool,
    /// Name of the record joined from; absent means the primary record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Collection of authored records the join searches.
    pub collection: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub filter: RecordFilter,
    /// Related-record field → field of the `from` record that it must equal.
    /// Empty for a record chosen independently by its own `filter` (a second
    /// source the answer draws on).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub on: BTreeMap<String, String>,
    /// Related-record field → pattern whose one capture group is compared
    /// instead of the whole field (`^([^@]+)@` on an email).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capture: BTreeMap<String, String>,
    /// `from`-record field → pattern whose one capture group is compared
    /// instead of the whole field (`pipeline #(\d+)` in a commit message).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capture_from: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub pick: RelatedPick,
    /// When set, the related record is an independent bounded aggregate
    /// over its own matched candidates (after filter).  The result is
    /// `{key, value, count}` like a primary group.  Not combined with `on`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Box<Group>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelatedPick {
    #[default]
    Only,
    Largest(String),
    Smallest(String),
    /// Keep every match; facts read from it list one value per record.
    All,
    /// Resolve a PagerDuty-style schedule rotation at `now` and return a
    /// virtual record `{"id": user_id}`.  The `overrides` field names the
    /// collection that holds schedule overrides, keyed by `<schedule_id>/*`.
    Oncall {
        overrides: String,
    },
}

pub(super) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// One resolved record, the rows behind it and the records it was chosen over.
struct Resolved {
    record: Value,
    /// Every joined record when the join keeps all matches; otherwise empty.
    all: Vec<Value>,
    members: Vec<Value>,
    alternatives: Vec<(Value, Vec<Value>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Typed {
    Text(Vec<String>),
    Number(Rational),
    Date(Date),
}

pub(super) struct DerivedFact {
    /// Usually one value; several when the fact lists every joined record or
    /// every row of a group, each of which the answer must state.
    pub targets: Vec<Typed>,
    pub alternatives: Vec<Typed>,
}

/// Maximum number of values one listed fact may require.
const MAX_LISTED: usize = 16;

fn missing(message: impl Into<String>) -> RecordCheckError {
    RecordCheckError::MissingEvidence(message.into())
}

/// Text of a scalar field; BSON-style wrappers such as `{"$oid": ...}` unwrap.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Object(fields) if fields.len() == 1 => fields
            .iter()
            .next()
            .filter(|(name, _)| name.starts_with('$'))
            .and_then(|(_, inner)| scalar_text(inner)),
        _ => None,
    }
    .filter(|text| !text.trim().is_empty())
}

fn joins(related: &Value, source: &Value) -> bool {
    match source {
        Value::Array(values) => values.iter().any(|value| joins(related, value)),
        _ => match (scalar_text(related), scalar_text(source)) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        },
    }
}

impl RecordFilter {
    pub(super) fn validate(&self) -> Result<(), String> {
        for pointer in self
            .matching
            .keys()
            .chain(&self.present)
            .chain(&self.absent)
        {
            json_pointer::validate(pointer)?;
        }
        for (pointer, pattern) in &self.patterns {
            json_pointer::validate(pointer)?;
            let pattern = Regex::new(pattern)
                .map_err(|error| format!("invalid filter pattern at {pointer}: {error}"))?;
            if pattern.is_match("") {
                return Err("filter patterns must not match an empty value".into());
            }
        }
        for example in &self.excluding {
            if example.is_empty() {
                return Err("an excluded record example must name a field".into());
            }
            for pointer in example.keys() {
                json_pointer::validate(pointer)?;
            }
        }
        for (pointer, range) in &self.ranges {
            json_pointer::validate(pointer)?;
            let bound = |text: &Option<String>| match text {
                Some(text) => Rational::parse(text)
                    .map(Some)
                    .ok_or("range bounds must be plain decimal numbers"),
                None => Ok(None),
            };
            if (bound(&range.min)?, bound(&range.max)?) == (None, None) {
                return Err("a numeric range requires a bound".into());
            }
        }
        Ok(())
    }

    fn keeps(&self, record: &Value) -> Result<bool, RecordCheckError> {
        let equals = |example: &BTreeMap<String, Value>| {
            example.iter().all(|(pointer, expected)| {
                record.pointer(pointer).is_some_and(|actual| {
                    actual == expected || crate::decimal::equivalent(actual, expected)
                })
            })
        };
        let absent = |pointer: &String| record.pointer(pointer).is_none_or(Value::is_null);
        let unmatched = |(pointer, pattern): (&String, &String)| {
            !record
                .pointer(pointer)
                .and_then(Value::as_str)
                .is_some_and(|text| Regex::new(pattern).is_ok_and(|pattern| pattern.is_match(text)))
        };
        if !equals(&self.matching)
            || self.excluding.iter().any(equals)
            || self.present.iter().any(absent)
            || !self.absent.iter().all(absent)
            || self.patterns.iter().any(unmatched)
        {
            return Ok(false);
        }
        for (pointer, range) in &self.ranges {
            let value = record
                .pointer(pointer)
                .map(|value| match scalar_text(value) {
                    Some(text) => Rational::parse(&text),
                    None => None,
                })
                .ok_or_else(|| missing(format!("a candidate record lacks ranged field {pointer}")))?
                .ok_or_else(|| missing(format!("ranged field {pointer} is not an exact number")))?;
            let outside = |bound: &Option<String>, wrong: Ordering| {
                bound
                    .as_deref()
                    .and_then(Rational::parse)
                    .is_some_and(|bound| value.compare(bound) == Some(wrong))
            };
            if outside(&range.min, Ordering::Less) || outside(&range.max, Ordering::Greater) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl Group {
    pub(super) fn validate(&self) -> Result<(), String> {
        for pointer in self.unnest.iter().chain(&self.by) {
            json_pointer::validate(pointer)?;
        }
        self.rows.validate()?;
        match &self.measure {
            Measure::Count => {}
            Measure::Sum(field) | Measure::Average(field) => json_pointer::validate(field)?,
            Measure::SumProduct(fields) => {
                if !(2..=3).contains(&fields.len()) {
                    return Err("a product measure multiplies two or three fields".into());
                }
                for field in fields {
                    json_pointer::validate(field)?;
                }
            }
            Measure::SumProductNormalized { fields, divisor } => {
                if !(2..=3).contains(&fields.len()) {
                    return Err(
                        "a normalized product measure multiplies two or three fields".into(),
                    );
                }
                for field in fields {
                    json_pointer::validate(field)?;
                }
                json_pointer::validate(&divisor.field)?;
                if divisor.values.is_empty() {
                    return Err("a divisor map requires at least one mapping".into());
                }
                for value in divisor.values.values() {
                    if Rational::parse(value).is_none() {
                        return Err(format!("divisor value {value:?} is not a number"));
                    }
                }
            }
        }
        if self.by.is_none() && self.pick != Pick::Only {
            return Err("an ungrouped aggregate has exactly one group".into());
        }
        for (name, rr) in &self.row_related {
            if !super::valid_slot(name) {
                return Err(format!("invalid row-related name {name:?}"));
            }
            json_pointer::validate(&rr.collection)?;
            rr.filter.validate()?;
            if rr.on.is_empty() {
                return Err("a row-related record requires a join field".into());
            }
            for (related, source) in &rr.on {
                json_pointer::validate(related)?;
                json_pointer::validate(source)?;
            }
        }
        if let Some(sl) = &self.second_level {
            json_pointer::validate(&sl.by)?;
            match &sl.measure {
                Measure::Count => {}
                Measure::Sum(field) | Measure::Average(field) => {
                    json_pointer::validate(field)?;
                }
                Measure::SumProduct(fields) | Measure::SumProductNormalized { fields, .. } => {
                    for field in fields {
                        json_pointer::validate(field)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl RelatedRecord {
    pub(super) fn validate(&self) -> Result<(), String> {
        json_pointer::validate(&self.collection)?;
        self.filter.validate()?;
        if !self.collection.starts_with("/services/") {
            return Err("related records must be authored service records".into());
        }
        if self.on.is_empty() && self.filter == RecordFilter::default() && self.group.is_none() {
            return Err("a related record requires a join field or its own filter".into());
        }
        if let Some(group) = &self.group {
            group.validate()?;
            if !self.on.is_empty() {
                return Err("a related aggregate uses its own filter, not a join".into());
            }
        }
        for (side, captures) in [(true, &self.capture), (false, &self.capture_from)] {
            for (pointer, pattern) in captures {
                let joined = if side {
                    self.on.contains_key(pointer)
                } else {
                    self.on.values().any(|field| field == pointer)
                };
                let pattern = Regex::new(pattern)
                    .map_err(|error| format!("invalid join capture: {error}"))?;
                if !joined || pattern.captures_len() != 2 {
                    return Err(
                        "a join capture names a joined field and has exactly one capture group"
                            .into(),
                    );
                }
            }
        }
        for (related, source) in &self.on {
            json_pointer::validate(related)?;
            json_pointer::validate(source)?;
        }
        match &self.pick {
            RelatedPick::Only | RelatedPick::All => {}
            RelatedPick::Oncall { overrides } => {
                json_pointer::validate(overrides)?;
            }
            RelatedPick::Largest(field) | RelatedPick::Smallest(field) => {
                json_pointer::validate(field)?
            }
        }
        Ok(())
    }
}

fn number_at(record: &Value, pointer: &str) -> Result<Rational, RecordCheckError> {
    record
        .pointer(pointer)
        .and_then(scalar_text)
        .and_then(|text| Rational::parse(&text))
        .ok_or_else(|| {
            missing(format!(
                "field {pointer} is not an exact number in a source record"
            ))
        })
}

/// Picks the unique extreme of `candidates` ordered by `value`.
fn extreme<T>(
    candidates: Vec<T>,
    value: impl Fn(&T) -> Result<Rational, RecordCheckError>,
    largest: bool,
) -> Result<(T, Vec<T>), RecordCheckError> {
    let mut best: Option<(usize, Rational)> = None;
    let mut tied = false;
    for (index, candidate) in candidates.iter().enumerate() {
        let amount = value(candidate)?;
        match best {
            None => best = Some((index, amount)),
            Some((_, current)) => {
                let order = amount
                    .compare(current)
                    .ok_or_else(|| missing("source values are too large to rank exactly"))?;
                let wanted = if largest {
                    Ordering::Greater
                } else {
                    Ordering::Less
                };
                if order == wanted {
                    best = Some((index, amount));
                    tied = false;
                } else if order == Ordering::Equal {
                    tied = true;
                }
            }
        }
    }
    let (index, _) = best.ok_or_else(|| missing("no source record is available to rank"))?;
    if tied {
        return Err(RecordCheckError::NeedsAdjudication(
            "the source ranking is tied; no single requested record exists".into(),
        ));
    }
    let mut rest = candidates;
    let chosen = rest.remove(index);
    Ok((chosen, rest))
}

fn only<T>(candidates: Vec<T>, label: &str) -> Result<T, RecordCheckError> {
    let count = candidates.len();
    let mut candidates = candidates.into_iter();
    match (candidates.next(), candidates.next()) {
        (Some(record), None) => Ok(record),
        _ => Err(missing(format!(
            "{label} must identify exactly one authored record; found {count}"
        ))),
    }
}

/// A record with its collection key readable as `/_key`.
fn keyed(key: &str, record: &Value) -> Value {
    let mut record = record.clone();
    if let Value::Object(fields) = &mut record
        && !fields.contains_key("_key")
    {
        fields.insert("_key".into(), Value::String(key.to_owned()));
    }
    record
}

/// Every record of a collection, regardless of any filter.
fn collection(pointer: &str, initial: &Value) -> Vec<Value> {
    match initial.pointer(pointer) {
        Some(Value::Object(records)) => records
            .iter()
            .map(|(key, record)| keyed(key, record))
            .collect(),
        Some(Value::Array(records)) => records.clone(),
        _ => Vec::new(),
    }
}

/// The one distinct value `pattern` captures in a scalar field.
fn captured(value: &Value, pattern: &str) -> Option<Value> {
    let text = scalar_text(value)?;
    let pattern = Regex::new(pattern).ok()?;
    let found: BTreeSet<&str> = pattern
        .captures_iter(&text)
        .filter_map(|captures| captures.get(1).map(|group| group.as_str().trim()))
        .filter(|value| !value.is_empty())
        .collect();
    let mut found = found.into_iter();
    match (found.next(), found.next()) {
        (Some(value), None) => Some(Value::String(value.to_owned())),
        _ => None,
    }
}

fn groups(
    group: &Group,
    candidates: &[Value],
    initial: Option<&Value>,
) -> Result<Vec<(Value, Vec<Value>)>, RecordCheckError> {
    let mut rows = Vec::new();
    for candidate in candidates {
        match &group.unnest {
            None => rows.push(candidate.clone()),
            Some(pointer) => {
                let items = candidate
                    .pointer(pointer)
                    .and_then(Value::as_array)
                    .ok_or_else(|| missing(format!("a source record lacks the list {pointer}")))?;
                rows.extend(
                    items
                        .iter()
                        .map(|item| json!({"parent": candidate, "item": item})),
                );
            }
        }
        if rows.len() > MAX_ROWS {
            return Err(missing("the aggregate reads too many source rows"));
        }
    }
    // Resolve per-row related records: enrich each row.
    if !group.row_related.is_empty() {
        let state =
            initial.ok_or_else(|| missing("row-related joins require initial business state"))?;
        let mut enriched = Vec::with_capacity(rows.len());
        for row in rows {
            let mut enriched_row = row.clone();
            for (name, rr) in &group.row_related {
                let candidates = collection(&rr.collection, state);
                let mut matched: Option<Value> = None;
                for record in &candidates {
                    if !rr.filter.keeps(record)? {
                        continue;
                    }
                    let mut ok = true;
                    for (related_field, row_field) in &rr.on {
                        let wanted = row.pointer(row_field).ok_or_else(|| {
                            missing(format!(
                                "a row lacks field {row_field} for row-related {name}"
                            ))
                        })?;
                        ok &= record
                            .pointer(related_field)
                            .is_some_and(|actual| joins(actual, wanted));
                    }
                    if ok {
                        matched = Some(record.clone());
                        break;
                    }
                }
                let matched = matched.ok_or_else(|| {
                    missing(format!("row-related {name} found no match for a row"))
                })?;
                if let Value::Object(fields) = &mut enriched_row {
                    let rr_obj = fields
                        .entry("row_related".to_owned())
                        .or_insert_with(|| json!({}));
                    if let Value::Object(rr_fields) = rr_obj {
                        rr_fields.insert(name.clone(), matched);
                    }
                }
            }
            enriched.push(enriched_row);
        }
        rows = enriched;
    }
    let mut keyed: BTreeMap<String, (Value, Vec<Value>)> = BTreeMap::new();
    for row in rows {
        if !group.rows.keeps(&row)? {
            continue;
        }
        let key = match &group.by {
            None => Value::String("all".into()),
            Some(pointer) => row
                .pointer(pointer)
                .filter(|value| scalar_text(value).is_some())
                .cloned()
                .ok_or_else(|| missing(format!("a source row lacks group field {pointer}")))?,
        };
        keyed
            .entry(scalar_text(&key).expect("group keys are scalar"))
            .or_insert_with(|| (key, Vec::new()))
            .1
            .push(row);
    }
    let overflow = || missing("an aggregate exceeds exact arithmetic");
    keyed
        .into_values()
        .map(|(key, members)| {
            let count = i128::try_from(members.len()).map_err(|_| overflow())?;
            let mut total = Rational::ZERO;
            for row in &members {
                let amount = match &group.measure {
                    Measure::Count => Rational::integer(1),
                    Measure::Sum(field) | Measure::Average(field) => number_at(row, field)?,
                    Measure::SumProduct(fields) => {
                        let mut product = Rational::integer(1);
                        for field in fields {
                            product = product.mul(number_at(row, field)?).ok_or_else(overflow)?;
                        }
                        product
                    }
                    Measure::SumProductNormalized { fields, divisor } => {
                        let mut product = Rational::integer(1);
                        for field in fields {
                            product = product.mul(number_at(row, field)?).ok_or_else(overflow)?;
                        }
                        let div_key = row
                            .pointer(&divisor.field)
                            .and_then(|v| v.as_str().map(str::to_owned))
                            .ok_or_else(|| {
                                missing(format!("a row lacks divisor field {}", divisor.field))
                            })?;
                        let div_str = divisor.values.get(&div_key).ok_or_else(|| {
                            missing(format!("divisor map has no entry for {div_key:?}"))
                        })?;
                        let div_val = Rational::parse(div_str).ok_or_else(|| {
                            missing(format!("divisor {div_str:?} is not a number"))
                        })?;
                        product.div(div_val).ok_or_else(overflow)?
                    }
                };
                total = total.add(amount).ok_or_else(overflow)?;
            }
            if matches!(group.measure, Measure::Average(_)) {
                total = total.div(Rational::integer(count)).ok_or_else(overflow)?;
            }
            Ok((
                json!({"key": key, "value": total.text(), "count": members.len()}),
                members,
            ))
        })
        .collect()
}

impl RecordFacts {
    fn primary(&self, initial: &Value) -> Result<Resolved, RecordCheckError> {
        let mut candidates = Vec::new();
        for (key, record) in self.selector.select(Some(initial), initial)? {
            if self.filter.keeps(record)? {
                candidates.push(keyed(&key, record));
            }
        }
        if candidates.is_empty() {
            return Err(missing("record fact selector matched no authored records"));
        }
        let plain = |chosen: Value| {
            let alternatives = collection(&self.selector.collection, initial)
                .into_iter()
                .filter(|record| *record != chosen)
                .map(|record| (record, Vec::new()))
                .collect();
            Resolved {
                record: chosen,
                all: Vec::new(),
                members: Vec::new(),
                alternatives,
            }
        };
        match &self.selection {
            RecordSelection::Only => Ok(plain(only(candidates, "the record fact selector")?)),
            RecordSelection::Largest { field } | RecordSelection::Smallest { field } => {
                let largest = matches!(self.selection, RecordSelection::Largest { .. });
                let (chosen, _) = extreme(candidates, |record| number_at(record, field), largest)?;
                Ok(plain(chosen))
            }
            RecordSelection::Group(group) => {
                let groups = groups(group, &candidates, Some(initial))?;
                let value = |entry: &(Value, Vec<Value>)| number_at(&entry.0, "/value");
                let ((record, members), mut alternatives) = match group.pick {
                    Pick::Only => (only(groups, "the aggregate")?, Vec::new()),
                    Pick::Largest => extreme(groups, value, true)?,
                    Pick::Smallest => extreme(groups, value, false)?,
                };
                // Groups that the selector excluded (another environment, a
                // closed status) are also things the answer could name
                // instead; they carry a key but no measure.
                if let Some(by) = &group.by {
                    let mut seen: BTreeSet<String> = std::iter::once(&record)
                        .chain(alternatives.iter().map(|(group, _)| group))
                        .filter_map(|group| scalar_text(&group["key"]))
                        .collect();
                    for excluded in collection(&self.selector.collection, initial) {
                        let rows: Vec<Value> = match &group.unnest {
                            None => vec![excluded],
                            Some(pointer) => excluded
                                .pointer(pointer)
                                .and_then(Value::as_array)
                                .map(|items| {
                                    items
                                        .iter()
                                        .map(|item| json!({"parent": excluded, "item": item}))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        };
                        for row in rows {
                            if let Some(key) = row.pointer(by)
                                && let Some(text) = scalar_text(key)
                                && seen.len() < MAX_ALTERNATIVES
                                && seen.insert(text)
                            {
                                alternatives.push((json!({"key": key}), vec![row]));
                            }
                        }
                    }
                }
                Ok(Resolved {
                    record,
                    all: Vec::new(),
                    members,
                    alternatives,
                })
            }
        }
    }

    /// Resolves the second-level aggregate if declared, returning a virtual
    /// related record.
    fn second_level_record(
        members: &[Value],
        second_level: &SecondLevel,
    ) -> Result<Resolved, RecordCheckError> {
        // Build a sub-group over the winning group's members.
        let sub_group = Group {
            unnest: None,
            by: Some(second_level.by.clone()),
            rows: RecordFilter::default(),
            measure: second_level.measure.clone(),
            pick: second_level.pick,
            row_related: BTreeMap::new(),
            second_level: None,
        };
        let sub_groups = groups(&sub_group, members, None)?;
        let value = |entry: &(Value, Vec<Value>)| number_at(&entry.0, "/value");
        let ((record, sub_members), alternatives) = match second_level.pick {
            Pick::Only => (only(sub_groups, "the second-level aggregate")?, Vec::new()),
            Pick::Largest => extreme(sub_groups, value, true)?,
            Pick::Smallest => extreme(sub_groups, value, false)?,
        };
        let alternatives = alternatives
            .into_iter()
            .map(|(record, _)| (record, Vec::new()))
            .collect();
        Ok(Resolved {
            record,
            all: Vec::new(),
            members: sub_members,
            alternatives,
        })
    }

    fn related(
        &self,
        related: &RelatedRecord,
        source: &Resolved,
        name: &str,
        initial: &Value,
    ) -> Result<Option<Resolved>, RecordCheckError> {
        let mut candidates = Vec::new();
        let records = collection(&related.collection, initial);
        if !initial
            .pointer(&related.collection)
            .is_some_and(|value| value.is_object() || value.is_array())
            || records.is_empty() && !related.allow_absence
        {
            return Err(missing(format!(
                "related record {name} has no authored source collection"
            )));
        }
        for record in &records {
            let mut joined = related.filter.keeps(record)?;
            for (field, source_field) in &related.on {
                let wanted = source.record.pointer(source_field).ok_or_else(|| {
                    missing(format!(
                        "the record joined to {name} lacks field {source_field}"
                    ))
                })?;
                let wanted = match related.capture_from.get(source_field) {
                    Some(pattern) => captured(wanted, pattern).ok_or_else(|| {
                        missing(format!(
                            "the record joined to {name} holds no single captured value in {source_field}"
                        ))
                    })?,
                    None => wanted.clone(),
                };
                joined &=
                    record
                        .pointer(field)
                        .is_some_and(|actual| match related.capture.get(field) {
                            Some(pattern) => captured(actual, pattern)
                                .is_some_and(|actual| joins(&actual, &wanted)),
                            None => joins(actual, &wanted),
                        });
            }
            if joined {
                candidates.push(record.clone());
            }
        }
        if !source.all.is_empty() {
            return Err(RecordCheckError::InvalidContract(format!(
                "related record {name} joins from a record that keeps every match"
            )));
        }
        if candidates.is_empty() && related.allow_absence {
            return Ok(None);
        }
        // Independent aggregate: run the group over all candidates.
        if let Some(group) = &related.group {
            let all_groups = groups(group, &candidates, Some(initial))?;
            let value = |entry: &(Value, Vec<Value>)| number_at(&entry.0, "/value");
            let ((record, members), alternatives) = match group.pick {
                Pick::Only => (
                    only(all_groups, &format!("related aggregate {name}"))?,
                    Vec::new(),
                ),
                Pick::Largest => extreme(all_groups, value, true)?,
                Pick::Smallest => extreme(all_groups, value, false)?,
            };
            return Ok(Some(Resolved {
                record,
                all: Vec::new(),
                members,
                alternatives,
            }));
        }
        let mut all = Vec::new();
        let chosen = match &related.pick {
            RelatedPick::All => {
                if candidates.is_empty() || candidates.len() > MAX_LISTED {
                    return Err(missing(format!(
                        "related record {name} must list between 1 and {MAX_LISTED} authored records; found {}",
                        candidates.len()
                    )));
                }
                all = candidates;
                all[0].clone()
            }
            RelatedPick::Only => only(candidates, &format!("related record {name}"))?,
            RelatedPick::Largest(field) => {
                extreme(candidates, |record| number_at(record, field), true)?.0
            }
            RelatedPick::Smallest(field) => {
                extreme(candidates, |record| number_at(record, field), false)?.0
            }
            RelatedPick::Oncall { overrides } => {
                // Each candidate is a schedule.  Resolve oncall at `now`.
                let now_str = initial["now"]
                    .as_str()
                    .ok_or_else(|| missing("initial state lacks a /now timestamp"))?;
                let now_epoch = parse_epoch(now_str)
                    .ok_or_else(|| missing("initial /now is not a valid timestamp"))?;
                // Try each candidate schedule; first one with an oncall wins.
                let mut resolved_user: Option<String> = None;
                let mut resolved_schedule: Option<Value> = None;
                for schedule in &candidates {
                    if let Some(uid) = resolve_oncall(schedule, overrides, initial, now_epoch) {
                        resolved_user = Some(uid);
                        resolved_schedule = Some(schedule.clone());
                        break;
                    }
                }
                let uid = resolved_user.ok_or_else(|| {
                    missing(format!(
                        "no schedule in related record {name} has someone on call at now"
                    ))
                })?;
                // Return a virtual record with the user ID.
                resolved_schedule
                    .map(|s| json!({"id": uid, "_schedule": s["id"]}))
                    .unwrap_or_else(|| json!({"id": uid}))
            }
        };
        let alternatives = records
            .into_iter()
            .filter(|record| *record != chosen && !all.contains(record))
            .map(|record| (record, Vec::new()))
            .collect();
        Ok(Some(Resolved {
            record: chosen,
            all,
            members: Vec::new(),
            alternatives,
        }))
    }

    /// Derives every declared fact and its alternatives from `initial`.
    pub(super) fn derive(
        &self,
        initial: Option<&Value>,
    ) -> Result<BTreeMap<String, DerivedFact>, RecordCheckError> {
        let initial = initial
            .filter(|state| state["services"].is_object())
            .ok_or_else(|| {
                missing("record fact answers require authoritative initial business state")
            })?;
        let mut resolved: BTreeMap<&str, Option<Resolved>> = BTreeMap::new();
        resolved.insert("", Some(self.primary(initial)?));
        // Resolve second-level aggregate if declared.
        if let RecordSelection::Group(group) = &self.selection
            && let Some(second_level) = &group.second_level
        {
            let primary = resolved[""].as_ref().unwrap();
            let resolved_second_level = Self::second_level_record(&primary.members, second_level)?;
            resolved.insert("_second_level", Some(resolved_second_level));
        }
        let mut pending: BTreeSet<&str> = self.related.keys().map(String::as_str).collect();
        while !pending.is_empty() {
            let ready: Vec<&str> = pending
                .iter()
                .copied()
                .filter(|name| {
                    resolved.contains_key(self.related[*name].from.as_deref().unwrap_or(""))
                })
                .collect();
            if ready.is_empty() {
                return Err(RecordCheckError::InvalidContract(
                    "related records form a cycle or join from an undeclared record".into(),
                ));
            }
            for name in ready {
                let related = &self.related[name];
                let source = &resolved[related.from.as_deref().unwrap_or("")];
                let record = match source {
                    Some(source) => self.related(related, source, name, initial)?,
                    None => {
                        if !initial
                            .pointer(&related.collection)
                            .is_some_and(|value| value.is_object() || value.is_array())
                        {
                            return Err(missing(format!(
                                "related record {name} has no authored source collection"
                            )));
                        }
                        None
                    }
                };
                resolved.insert(name, record);
                pending.remove(name);
            }
        }
        let mut derived = BTreeMap::new();
        for (name, fact) in &self.facts {
            let Some(source) = &resolved[fact.record.as_deref().unwrap_or("")] else {
                derived.insert(
                    name.clone(),
                    DerivedFact {
                        targets: Vec::new(),
                        alternatives: Vec::new(),
                    },
                );
                continue;
            };
            // `/members/<field>` lists the field of every row of the group.
            let per_row = fact
                .field
                .strip_prefix("/members/")
                .map(|field| RecordFact {
                    field: format!("/{field}"),
                    ..fact.clone()
                });
            let read = |record: &Value, members: &[Value]| -> Result<Vec<Typed>, String> {
                match &per_row {
                    Some(row_fact) => members.iter().map(|row| row_fact.read(row, &[])).collect(),
                    None => fact.read(record, members).map(|value| vec![value]),
                }
            };
            let mut targets = Vec::new();
            let listed: Vec<&Value> = if source.all.is_empty() {
                vec![&source.record]
            } else {
                source.all.iter().collect()
            };
            for record in listed {
                for value in read(record, &source.members)
                    .map_err(|reason| missing(format!("source fact {name}: {reason}")))?
                {
                    if !targets.contains(&value) {
                        targets.push(value);
                    }
                }
            }
            if targets.is_empty() || targets.len() > MAX_LISTED {
                return Err(missing(format!(
                    "source fact {name} must hold between 1 and {MAX_LISTED} values"
                )));
            }
            let mut alternatives = Vec::new();
            for (record, members) in source.alternatives.iter().take(MAX_ALTERNATIVES) {
                for value in read(record, members).unwrap_or_default() {
                    if !targets.contains(&value) && !alternatives.contains(&value) {
                        alternatives.push(value);
                    }
                }
            }
            derived.insert(
                name.clone(),
                DerivedFact {
                    targets,
                    alternatives,
                },
            );
        }
        Ok(derived)
    }
}

fn timestamp(text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let text = text.trim();
    text.parse::<i64>()
        .ok()
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(text)
                .ok()
                .map(|instant| instant.with_timezone(&chrono::Utc))
        })
}

fn clock_forms(text: &str) -> Option<Vec<String>> {
    let instant = timestamp(text)?;
    Some(vec![
        instant.format("%H:%M").to_string(),
        instant.format("%H:%M:%S%.f").to_string(),
        instant.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        instant.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, false),
    ])
}

impl RecordFact {
    fn text_at(&self, pointer: &str, record: &Value, members: &[Value]) -> Result<String, String> {
        if let Some(member_field) = pointer.strip_prefix("/member/") {
            let member_field = format!("/{member_field}");
            let values: BTreeSet<String> = members
                .iter()
                .filter_map(|row| row.pointer(&member_field).and_then(scalar_text))
                .collect();
            let count = values.len();
            return only(values.into_iter().collect(), "a group")
                .map_err(|_| format!("group rows hold {count} values of {member_field}"));
        }
        record
            .pointer(pointer)
            .and_then(scalar_text)
            .ok_or_else(|| format!("the authored record has no readable field {pointer}"))
    }

    /// Reads this fact from one record (and, for groups, its member rows).
    fn read(&self, record: &Value, members: &[Value]) -> Result<Typed, String> {
        let mut parts = Vec::new();
        for pointer in std::iter::once(&self.field).chain(&self.append) {
            parts.push(self.text_at(pointer, record, members)?);
        }
        let mut text = parts.join(" ");
        if let Some(pattern) = &self.capture {
            let pattern = Regex::new(pattern).map_err(|error| error.to_string())?;
            let captured: BTreeSet<&str> = pattern
                .captures_iter(&text)
                .filter_map(|captures| captures.get(1).map(|group| group.as_str().trim()))
                .filter(|value| !value.is_empty())
                .collect();
            let count = captured.len();
            text = only(captured.into_iter().collect(), "a capture")
                .map_err(|_| format!("the capture pattern finds {count} distinct values"))?
                .to_owned();
        }
        match self.kind {
            RecordFactKind::Number | RecordFactKind::Money => Rational::parse(&text)
                .map(Typed::Number)
                .ok_or_else(|| format!("{text:?} is not an exact number")),
            RecordFactKind::Date => Date::parse_iso(&text)
                .or_else(|| {
                    // An authored prose date ("January 15, 2027") with a year.
                    let stated = super::numbers::dates(&text.to_lowercase());
                    match stated.as_slice() {
                        [token] if token.date.year.is_some() => Some(token.date),
                        _ => None,
                    }
                })
                .map(Typed::Date)
                .ok_or_else(|| format!("{text:?} does not hold exactly one full date")),
            RecordFactKind::UtcDate => timestamp(&text)
                .and_then(|instant| Date::parse_iso(&instant.format("%Y-%m-%d").to_string()))
                .map(Typed::Date)
                .ok_or_else(|| format!("{text:?} is not a supported UTC timestamp")),
            RecordFactKind::Text | RecordFactKind::Identifier => {
                let mut forms = if self.clock {
                    clock_forms(&text).ok_or_else(|| format!("{text:?} is not a UTC timestamp"))?
                } else {
                    vec![text]
                };
                for pointer in &self.also {
                    let form = self.text_at(pointer, record, members)?;
                    if !forms.contains(&form) {
                        forms.push(form);
                    }
                }
                Ok(Typed::Text(forms))
            }
            RecordFactKind::Phone => {
                let extra = super::numbers::phone_forms(&text);
                if extra.is_empty() {
                    return Err(format!("{text:?} is not a recognizable phone number"));
                }
                let mut forms = vec![text];
                for form in extra {
                    if !forms.contains(&form) {
                        forms.push(form);
                    }
                }
                Ok(Typed::Text(forms))
            }
            RecordFactKind::TimeOfDay => {
                let extra = super::numbers::time_of_day_forms(&text);
                if extra.is_empty() {
                    return Err(format!("{text:?} is not a valid HH:MM time"));
                }
                let mut forms = vec![text];
                for form in extra {
                    if !forms.contains(&form) {
                        forms.push(form);
                    }
                }
                Ok(Typed::Text(forms))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PagerDuty-style schedule rotation resolution
// ---------------------------------------------------------------------------

/// Parses an ISO-8601 timestamp to epoch seconds.
fn parse_epoch(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text.trim())
        .ok()
        .map(|instant| instant.timestamp())
}

/// Resolves who is on call on a PagerDuty-style schedule at `now_epoch`.
/// Returns the user ID string or None if nobody is on call.
fn resolve_oncall(
    schedule: &Value,
    overrides_collection: &str,
    initial: &Value,
    now_epoch: i64,
) -> Option<String> {
    let schedule_id = schedule["id"].as_str()?;
    let layers = schedule["schedule_layers"].as_array()?;
    // Resolve layers: later layers win over earlier ones.
    // Each layer has rotation_virtual_start, rotation_turn_length_seconds, users[].
    let mut layer_result: Option<String> = None;
    for layer in layers {
        let users: Vec<&str> = layer["users"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|u| u["user"]["id"].as_str().or_else(|| u["id"].as_str()))
            .collect();
        if users.is_empty() {
            continue;
        }
        let start = parse_epoch(layer["start"].as_str().unwrap_or(""))?;
        let end = layer["end"].as_str().and_then(parse_epoch);
        if now_epoch < start || end.is_some_and(|e| now_epoch >= e) {
            continue;
        }
        let rvs = layer["rotation_virtual_start"]
            .as_str()
            .and_then(parse_epoch)
            .unwrap_or(start);
        let turn = layer["rotation_turn_length_seconds"]
            .as_i64()
            .filter(|t| *t > 0)
            .unwrap_or(7 * 86400);
        let elapsed = now_epoch - rvs;
        let index = elapsed.div_euclid(turn);
        let user_index = index.rem_euclid(users.len() as i64) as usize;
        layer_result = Some(users[user_index].to_owned());
    }
    // Check overrides: keyed as schedule_id/*.
    if let Some(Value::Object(all_overrides)) = initial.pointer(overrides_collection) {
        let prefix = format!("{schedule_id}/");
        let mut best_override: Option<(usize, String)> = None;
        for (index, (key, entry)) in all_overrides.iter().enumerate() {
            if !key.starts_with(&prefix) {
                continue;
            }
            let override_start = entry["start"].as_str().and_then(parse_epoch);
            let override_end = entry["end"].as_str().and_then(parse_epoch);
            if let (Some(start), Some(end)) = (override_start, override_end)
                && now_epoch >= start
                && now_epoch < end
                && let Some(user_id) = entry["user"]["id"].as_str()
            {
                match &best_override {
                    None => best_override = Some((index, user_id.to_owned())),
                    Some((previous_index, _)) if index > *previous_index => {
                        best_override = Some((index, user_id.to_owned()));
                    }
                    _ => {}
                }
            }
        }
        if let Some((_, uid)) = best_override {
            return Some(uid);
        }
    }
    layer_result
}
