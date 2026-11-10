//! Source-bound answer grading for lookup tasks.
//!
//! A contract names facts that are *derived from the authored baseline
//! records* (`derive`): one primary record chosen by a selector, an extreme
//! or one group of a bounded aggregate; named records reached from it by
//! equality joins; and reviewed captures from authored text.  The agent's
//! natural-prose answer is then read conservatively (`statements`).
//!
//! ## Semantics
//! - **Pass**: every fact is affirmed in a reviewed notation and nothing in
//!   the answer competes with it.
//! - **Fail**: a fact is denied, another authored record's value is asserted
//!   in its place, a value with the reviewed unit or currency differs, a
//!   reviewed incorrect form matches, or the fact is absent.
//! - **Unresolved**: wording the reader cannot classify (hedges, unreviewed
//!   notations, disputed or competing statements); needs adjudication.
//! - **EnvironmentError**: a source record, field, join or capture is
//!   missing from the initial state.
//!
//! See `docs/answer-fact-contracts.md`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    json_pointer,
    relations::{RecordCheckError, RecordSelector},
    rich_text::unsupported_control,
};

mod derive;
mod numbers;
mod statements;

pub use derive::{Group, RecordFilter, RelatedRecord};

/// Maximum number of fact slots in one contract.
const MAX_FACTS: usize = 16;
/// Maximum number of reviewed forms (rejected + context).
const MAX_FORMS: usize = 64;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Answer grading bound to facts derived from authored baseline records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordFacts {
    /// Human-readable grading requirement (shown in adjudication messages).
    pub requirement: String,
    /// Selects candidate records from initial state.
    pub selector: RecordSelector,
    /// How to narrow candidates to the primary record.
    #[serde(default)]
    pub selection: RecordSelection,
    /// Named facts to derive.  Keys are slot names used in reviewed forms.
    pub facts: BTreeMap<String, RecordFact>,
    /// How the answer is read.  Only `statements` exists: earlier substring
    /// and whole-text modes could not establish negation or relationships.
    #[serde(default)]
    pub mode: MatchMode,
    /// Reviewed incorrect forms with `${slot}` placeholders (matched → Fail),
    /// such as a role swap between two declared facts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<String>,
    /// Reviewed record filter applied after `selector`.
    #[serde(default, skip_serializing_if = "derive::is_default")]
    pub filter: RecordFilter,
    /// Named records reached from the primary record by bounded equality
    /// joins.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub related: BTreeMap<String, RelatedRecord>,
    /// Reviewed neutral remarks that may accompany the facts.  They are set
    /// aside before the answer is read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<String>,
    /// Reviewed negative predicates that are true of the facts, such as
    /// `does not match`; they are not read as denials.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predicates: Vec<String>,
    /// Reviewed words that state the opposite of what the task asks about
    /// the facts (`fastest` when the slowest region is asked for).  A fact
    /// stated in the same sentence as one of them is disputed, never affirmed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contradictions: Vec<String>,
    /// Reviewed affirmative clauses. Slots bind typed source facts, not fixed
    /// expected values; unsupported additional clauses require adjudication.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<String>,
}

/// How to pick the primary record from the candidates returned by `selector`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordSelection {
    /// Expect exactly one candidate.
    #[default]
    Only,
    /// Pick the candidate with the largest numeric value at `field` (JSON
    /// pointer into the record).  A tie needs adjudication.
    Largest { field: String },
    /// Pick the candidate with the smallest numeric value at `field`.
    Smallest { field: String },
    /// Aggregate the candidates and pick one group; the primary record is
    /// then `{key, value, count}`, `/member/<field>` reads a field that
    /// every row of the group shares and `/members/<field>` lists it.
    Group(Box<Group>),
}

/// One fact derived from an authored record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordFact {
    /// JSON pointer into the record (e.g. `/properties/amount`).
    pub field: String,
    /// How to interpret the field value when reading the answer.
    pub kind: RecordFactKind,
    /// Omission is allowed; any stated value still needs source binding and
    /// reviewed affirmative clauses.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// Name of the related record to read; absent means the primary record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
    /// Further fields joined to `field` with one space (first + last name).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub append: Vec<String>,
    /// Other fields of the same record that identify the same thing, such as
    /// a login next to a display name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
    /// Reviewed pattern with one capture group that extracts the fact from an
    /// authored text source.  It must capture exactly one distinct value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<String>,
    /// Reviewed wordings of a coded source value, keyed by that value
    /// (`UNFULFILLED` → "has not shipped").  A wording applies only while the
    /// authored record holds its key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub wordings: BTreeMap<String, Vec<String>>,
    /// Render an epoch-seconds or ISO-8601 UTC timestamp as `HH:MM`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clock: bool,
    /// Reviewed unit labels for a number.  The first names the unit of the
    /// source value; convertible units (ms/s/min/h) are converted exactly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<String>,
    /// The number only counts when it carries one of `units`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_unit: bool,
    /// Accept a correct rounding of the source value (averages and rates).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rounding: bool,
    /// Accept an abbreviation of at least this many leading characters
    /// (short commit hashes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_prefix: Option<usize>,
}

/// How the answer is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    /// Natural prose is read conservatively: every fact must be affirmed in a
    /// reviewed notation, and denials, competing records' values, unit or
    /// currency conflicts and hedges are recognized.  See `statements`.
    #[default]
    Statements,
}

/// How a source value maps to answer text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordFactKind {
    /// A name or phrase (case-insensitive, whole-name match).
    Text,
    /// An exact integer or decimal quantity, optionally with reviewed units.
    Number,
    /// A US-dollar amount: `$` / `USD` are optional, scale suffixes
    /// (`k`, `M`, `million`) are read, another currency never matches.
    Money,
    /// A calendar date, read from ISO or a full prose date in the source and
    /// accepted in ISO and month-name notations in the answer.
    Date,
    /// A UTC calendar date derived from integer Unix seconds or an RFC 3339
    /// timestamp, with ISO and month-name answer notations.
    UtcDate,
    /// An identifier, URL or code (whole-token match).
    Identifier,
    /// A telephone number.  The source is normalized: common formatted
    /// variants (dashes, parens, dots, country prefix) are generated
    /// automatically and matched as text.
    Phone,
    /// An `HH:MM` 24-hour time.  The source is expanded to 12-hour and
    /// 24-hour display variants and matched as text.
    TimeOfDay,
}

// ---------------------------------------------------------------------------
// Reviewed forms
// ---------------------------------------------------------------------------

fn valid_slot(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// One piece of a `${slot}` template.
#[derive(Debug, Clone)]
enum Part {
    Literal(String),
    Slot(String),
}

fn parse_template(template: &str) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    let mut remaining = template;
    while let Some(start) = remaining.find("${") {
        if start > 0 {
            parts.push(Part::Literal(remaining[..start].to_owned()));
        }
        let rest = &remaining[start + 2..];
        let end = rest
            .find('}')
            .ok_or("unterminated placeholder in a reviewed form")?;
        let name = &rest[..end];
        if !valid_slot(name) {
            return Err(format!("invalid slot name {name:?} in a reviewed form"));
        }
        if matches!(parts.last(), Some(Part::Slot(_))) {
            return Err("adjacent placeholders must be separated by literal text".into());
        }
        parts.push(Part::Slot(name.to_owned()));
        remaining = &rest[end + 1..];
    }
    if !remaining.is_empty() {
        parts.push(Part::Literal(remaining.to_owned()));
    }
    Ok(parts)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

impl RecordFacts {
    /// Proposes a typed clause template for author review. This verifies value
    /// bindings only; it never certifies that the candidate's prose is true.
    pub fn suggest_assertion(&self, answer: &str, initial: &Value) -> Result<String, String> {
        self.validate()?;
        statements::suggest_assertion(self, answer, initial).map_err(|error| format!("{error:?}"))
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.requirement.trim().is_empty() {
            return Err("record fact answers require a nonempty requirement".into());
        }
        self.selector.validate()?;
        if self.selector.created {
            return Err("record fact answers must read authored baseline records".into());
        }
        match &self.selection {
            RecordSelection::Only => {}
            RecordSelection::Largest { field } | RecordSelection::Smallest { field } => {
                json_pointer::validate(field)?;
            }
            RecordSelection::Group(group) => group.validate()?,
        }
        self.filter.validate()?;
        if self.related.len() > derive::MAX_RELATED {
            return Err("too many related records".into());
        }
        for (name, related) in &self.related {
            if !valid_slot(name) {
                return Err(format!("invalid related record name {name:?}"));
            }
            related.validate()?;
            if related
                .from
                .as_ref()
                .and_then(|from| self.related.get(from))
                .is_some_and(|parent| parent.allow_absence && !related.allow_absence)
            {
                return Err("a record joined from an absent record must allow absence".into());
            }
            if related
                .from
                .as_ref()
                .is_some_and(|from| from == name || !self.related.contains_key(from))
            {
                return Err(format!(
                    "related record {name} joins from an undeclared record"
                ));
            }
        }
        if self.facts.is_empty() || self.facts.len() > MAX_FACTS {
            return Err(format!(
                "record fact answers require between 1 and {MAX_FACTS} facts"
            ));
        }
        if self.facts.values().all(|fact| fact.optional) {
            return Err("record fact answers require at least one required fact".into());
        }
        if self.assertions.is_empty() && self.facts.values().any(|fact| fact.optional) {
            return Err("optional facts require reviewed affirmative clauses".into());
        }
        for (name, fact) in &self.facts {
            if !valid_slot(name) {
                return Err(format!("invalid fact slot name {name:?}"));
            }
            fact.validate(&self.related)?;
            if !fact.optional
                && fact
                    .record
                    .as_ref()
                    .and_then(|record| self.related.get(record))
                    .is_some_and(|related| related.allow_absence)
            {
                return Err("a required fact cannot read a record that allows absence".into());
            }
        }
        if self
            .predicates
            .iter()
            .chain(&self.contradictions)
            .any(|phrase| {
                phrase.trim().is_empty()
                    || phrase.contains("${")
                    || phrase.chars().any(unsupported_control)
            })
        {
            return Err(
                "reviewed predicates and contradictions must be nonempty literal phrases".into(),
            );
        }
        if self.rejected.len() + self.context.len() + self.assertions.len() > MAX_FORMS {
            return Err("too many reviewed forms".into());
        }
        let mut seen = BTreeSet::new();
        for template in self
            .rejected
            .iter()
            .chain(&self.context)
            .chain(&self.assertions)
        {
            if template.chars().any(unsupported_control)
                || !seen.insert(statements::fold(template, false))
            {
                return Err("reviewed forms must be distinct readable text".into());
            }
            let parts = parse_template(template)?;
            let mut literal = false;
            for part in &parts {
                match part {
                    Part::Slot(name) if !self.facts.contains_key(name) => {
                        return Err("reviewed forms may only reference declared fact slots".into());
                    }
                    Part::Slot(_) => {}
                    Part::Literal(text) => literal |= text.chars().any(char::is_alphanumeric),
                }
            }
            if !literal && !self.assertions.contains(template) {
                return Err("reviewed forms must contain literal wording".into());
            }
        }
        for assertion in &self.assertions {
            if !parse_template(assertion)?
                .iter()
                .any(|part| matches!(part, Part::Slot(_)))
            {
                return Err("reviewed assertions must bind at least one source fact".into());
            }
        }
        Ok(())
    }

    pub(crate) fn check(
        &self,
        answer: &str,
        initial: Option<&Value>,
    ) -> Result<(), RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;
        statements::check(self, answer, initial)
    }
}

impl RecordFact {
    fn validate(&self, related: &BTreeMap<String, RelatedRecord>) -> Result<(), String> {
        json_pointer::validate(&self.field)?;
        if self
            .record
            .as_ref()
            .is_some_and(|record| record != "_second_level" && !related.contains_key(record))
        {
            return Err("a fact reads an undeclared related record".into());
        }
        for pointer in self.append.iter().chain(&self.also) {
            json_pointer::validate(pointer)?;
        }
        if let Some(pattern) = &self.capture {
            let pattern = regex::Regex::new(pattern)
                .map_err(|error| format!("invalid capture pattern: {error}"))?;
            if pattern.captures_len() != 2 {
                return Err("a capture pattern requires exactly one capture group".into());
            }
        }
        let numeric = matches!(self.kind, RecordFactKind::Number | RecordFactKind::Money);
        let textual = matches!(
            self.kind,
            RecordFactKind::Text
                | RecordFactKind::Identifier
                | RecordFactKind::Phone
                | RecordFactKind::TimeOfDay
        );
        let unreadable =
            |text: &String| text.trim().is_empty() || text.chars().any(unsupported_control);
        if (!numeric && (!self.units.is_empty() || self.require_unit || self.rounding))
            || (!textual
                && (!self.also.is_empty()
                    || self.min_prefix.is_some()
                    || !self.wordings.is_empty()
                    || self.clock))
            || (self.require_unit && self.units.is_empty())
            || self.min_prefix.is_some_and(|minimum| minimum < 6)
            || self.units.iter().any(unreadable)
            || self.wordings.iter().any(|(value, wordings)| {
                value.is_empty() || wordings.is_empty() || wordings.iter().any(unreadable)
            })
        {
            return Err(
                "units and rounding apply to numbers; aliases, prefixes, wordings and clock times to text; each must be reviewable"
                    .into(),
            );
        }
        Ok(())
    }
}
