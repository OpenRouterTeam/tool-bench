//! Source-bound answer grading for tasks whose reference answer derives from
//! structured JSON record fields.  Unlike `source_facts` (Markdown table
//! extraction) this module reads field values directly from the world state
//! records selected via `RecordSelector`, optionally computing a selection
//! (e.g. "record with the largest value in field X").
//!
//! ## Semantics
//! - **Pass**: answer matches a reviewed claim form with correct source values.
//! - **Fail**: answer contradicts source facts (wrong value, misattribution,
//!   rejected claim form matches).
//! - **Unresolved**: answer wording not covered by any reviewed form (needs
//!   human adjudication).
//! - **EnvironmentError**: source record or field missing from initial state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    json_pointer,
    relations::{RecordCheckError, RecordSelector},
    rich_text::unsupported_control,
};

/// Maximum number of fact slots in one contract.
const MAX_FACTS: usize = 16;
/// Maximum number of claim forms (accepted + rejected).
const MAX_CLAIMS: usize = 64;
/// Maximum number of records the selector may return for `Largest` selection
/// before we bail.
const MAX_CANDIDATES: usize = 1024;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Answer grading bound to structured JSON record facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordFacts {
    /// Human-readable grading requirement (shown in adjudication messages).
    pub requirement: String,
    /// Selects candidate records from initial state.
    pub selector: RecordSelector,
    /// How to narrow candidates to the target record(s).
    #[serde(default)]
    pub selection: RecordSelection,
    /// Named facts to extract.  Keys are slot names used in claim templates.
    pub facts: BTreeMap<String, RecordFact>,
    /// How to match the answer: `claim` requires exact claim form match,
    /// `contains` requires all fact values to appear as substrings.
    #[serde(default)]
    pub mode: MatchMode,
    /// Reviewed correct claim forms with `${slot}` placeholders.
    /// Required when mode is `claim`; optional (for contradiction detection)
    /// when mode is `contains`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<String>,
    /// Reviewed incorrect claim forms (if matched → Fail).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<String>,
}

/// How to pick the target record(s) from the candidates returned by `selector`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordSelection {
    /// Expect exactly one candidate.
    Only,
    /// Pick the candidate with the largest numeric value at `field` (JSON
    /// pointer into the record).  Ties are an environment error.
    Largest { field: String },
}

impl Default for RecordSelection {
    fn default() -> Self {
        Self::Only
    }
}

/// One fact extracted from a record field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordFact {
    /// JSON pointer into the record (e.g. `/properties/amount`).
    pub field: String,
    /// How to interpret the field value when matching the answer.
    pub kind: RecordFactKind,
    /// Additional acceptable text representations of this value that are not
    /// derivable from the kind alone.  These are reviewed alternatives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presentations: Vec<String>,
}

/// How a record field value maps to answer text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    /// Answer must exactly match one of the reviewed claim forms.
    #[default]
    Claim,
    /// Answer must contain all fact values as substrings (case-insensitive).
    Contains,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordFactKind {
    /// Exact text identity (case-insensitive whitespace-normalized match).
    Text,
    /// An integer or decimal number.  Accepted presentations include
    /// locale-grouped digits (1,234 / 1.234 / 1 234) and optional currency
    /// prefix/suffix.
    Number,
    /// A monetary amount with currency.  The field value must be numeric; the
    /// currency is inferred from presentations or claim context.
    Money,
    /// ISO-8601 date.  Common reformats (Nov 15, 2026 etc.) are accepted.
    Date,
    /// URL or identifier — exact substring match (case-sensitive).
    Identifier,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn valid_slot(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn normalize(s: &str) -> String {
    let mut result = String::new();
    let mut whitespace = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            whitespace = true;
            continue;
        }
        if whitespace {
            result.push(' ');
            whitespace = false;
        }
        result.extend(ch.to_lowercase());
    }
    if whitespace {
        result.push(' ');
    }
    result
}

/// Parse `${slot}` templates into literal/slot parts.
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
            parts.push(Part::Literal(normalize(&remaining[..start])));
        }
        let rest = &remaining[start + 2..];
        let end = rest
            .find('}')
            .ok_or("unterminated placeholder in claim template")?;
        let name = &rest[..end];
        if !valid_slot(name) {
            return Err(format!("invalid slot name {name:?} in claim template"));
        }
        if matches!(parts.last(), Some(Part::Slot(_))) {
            return Err("adjacent placeholders must be separated by literal text".into());
        }
        parts.push(Part::Slot(name.to_owned()));
        remaining = &rest[end + 1..];
    }
    if !remaining.is_empty() {
        parts.push(Part::Literal(normalize(remaining)));
    }
    Ok(parts)
}

fn template_slots(parts: &[Part]) -> BTreeSet<&str> {
    parts
        .iter()
        .filter_map(|p| match p {
            Part::Slot(name) => Some(name.as_str()),
            Part::Literal(_) => None,
        })
        .collect()
}

impl RecordFacts {
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
            RecordSelection::Largest { field } => {
                json_pointer::validate(field)?;
            }
        }
        if self.facts.is_empty() || self.facts.len() > MAX_FACTS {
            return Err(format!(
                "record fact answers require between 1 and {MAX_FACTS} facts"
            ));
        }
        for (name, fact) in &self.facts {
            if !valid_slot(name) {
                return Err(format!("invalid fact slot name {name:?}"));
            }
            json_pointer::validate(&fact.field)?;
            for presentation in &fact.presentations {
                if presentation.trim().is_empty() || presentation.chars().any(unsupported_control) {
                    return Err("fact presentations must be nonempty readable text".into());
                }
            }
        }
        if self.mode == MatchMode::Claim && self.claims.is_empty() {
            return Err("claim mode requires at least one claim form".into());
        }
        if self.claims.len() + self.rejected.len() > MAX_CLAIMS {
            return Err("too many claim forms".into());
        }
        let mut seen = BTreeSet::new();
        for (accepted, template) in self
            .claims
            .iter()
            .map(|t| (true, t))
            .chain(self.rejected.iter().map(|t| (false, t)))
        {
            if template.chars().any(unsupported_control) || !seen.insert(normalize(template)) {
                return Err("claim forms must be distinct readable text".into());
            }
            let parts = parse_template(template)?;
            if parts.is_empty() {
                return Err("claim forms must not be empty".into());
            }
            let used = template_slots(&parts);
            if used.iter().any(|s| !self.facts.contains_key(*s)) {
                return Err("claim forms may only reference declared fact slots".into());
            }
            if accepted
                && self.mode == MatchMode::Claim
                && !self.facts.keys().all(|k| used.contains(k.as_str()))
            {
                return Err("accepted claim forms must reference every declared fact slot".into());
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Source extraction
    // -----------------------------------------------------------------------

    fn extract(
        &self,
        initial: Option<&Value>,
    ) -> Result<(Value, BTreeMap<String, FactExtract>), RecordCheckError> {
        let initial = initial.ok_or_else(|| {
            RecordCheckError::MissingEvidence(
                "record fact answers require authoritative initial business state".into(),
            )
        })?;
        let candidates = self.selector.select(Some(initial), initial)?;
        if candidates.is_empty() {
            return Err(RecordCheckError::MissingEvidence(
                "record fact selector matched no records".into(),
            ));
        }
        if candidates.len() > MAX_CANDIDATES {
            return Err(RecordCheckError::MissingEvidence(
                "record fact selector matched too many candidates".into(),
            ));
        }

        let record = match &self.selection {
            RecordSelection::Only => {
                if candidates.len() != 1 {
                    return Err(RecordCheckError::MissingEvidence(format!(
                        "record fact selection 'only' expects 1 candidate, got {}",
                        candidates.len()
                    )));
                }
                candidates.into_iter().next().unwrap().1.clone()
            }
            RecordSelection::Largest { field } => {
                let mut best: Option<(f64, &Value)> = None;
                let mut ties = false;
                for (_, record) in &candidates {
                    let value = record.pointer(field).ok_or_else(|| {
                        RecordCheckError::MissingEvidence(format!(
                            "candidate record lacks field {field}"
                        ))
                    })?;
                    let num = value_as_f64(value).ok_or_else(|| {
                        RecordCheckError::MissingEvidence(format!(
                            "field {field} is not numeric in candidate record"
                        ))
                    })?;
                    match &best {
                        None => best = Some((num, record)),
                        Some((current_best, _)) => {
                            if num > *current_best {
                                best = Some((num, record));
                                ties = false;
                            } else if (num - current_best).abs() < f64::EPSILON {
                                ties = true;
                            }
                        }
                    }
                }
                if ties {
                    return Err(RecordCheckError::NeedsAdjudication(
                        "record fact largest selection has a tie".into(),
                    ));
                }
                best.map(|(_, r)| r.clone()).ok_or_else(|| {
                    RecordCheckError::MissingEvidence("no candidates for largest selection".into())
                })?
            }
        };

        // Extract facts
        let mut extracts = BTreeMap::new();
        for (name, fact) in &self.facts {
            let value = record.pointer(&fact.field).ok_or_else(|| {
                RecordCheckError::MissingEvidence(format!(
                    "selected record lacks fact field {}",
                    fact.field
                ))
            })?;
            let extract = FactExtract::from_value(value, fact)?;
            extracts.insert(name.clone(), extract);
        }

        // Also extract distractors: values from non-selected records for the same facts
        // (used to detect misattribution)
        Ok((record, extracts))
    }

    // -----------------------------------------------------------------------
    // Answer checking
    // -----------------------------------------------------------------------

    pub(crate) fn check(
        &self,
        answer: &str,
        initial: Option<&Value>,
    ) -> Result<(), RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;

        if answer.trim().is_empty() {
            return Err(RecordCheckError::Failed(
                "required record fact answer is empty".into(),
            ));
        }
        if answer.chars().any(unsupported_control) {
            return Err(RecordCheckError::NeedsAdjudication(
                "answer contains directional or invisible controls".into(),
            ));
        }

        let initial_value = initial.ok_or_else(|| {
            RecordCheckError::MissingEvidence(
                "record fact answers require authoritative initial business state".into(),
            )
        })?;
        let (_record, extracts) = self.extract(initial)?;

        // Collect distractor values from other candidates
        let all_candidates = self.selector.select(Some(initial_value), initial_value)?;
        let mut distractors: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (_, candidate) in &all_candidates {
            for (name, fact) in &self.facts {
                if let Some(val) = candidate.pointer(&fact.field) {
                    let text = value_to_text(val);
                    let correct = &extracts[name];
                    if !correct.matches_text(&text) {
                        distractors.entry(name.clone()).or_default().push(text);
                    }
                }
            }
        }

        let answer_norm = normalize(answer);

        // In contains mode, just verify all fact values are present
        if self.mode == MatchMode::Contains {
            let mut all_contained = true;
            let mut any_contradicted = false;
            for (name, extract) in &extracts {
                if !extract.contained_in(&answer_norm) {
                    all_contained = false;
                    if let Some(wrong_values) = distractors.get(name) {
                        for wrong in wrong_values {
                            if answer_norm.contains(&normalize(wrong)) {
                                any_contradicted = true;
                            }
                        }
                    }
                }
            }
            if any_contradicted {
                return Err(RecordCheckError::Failed(format!(
                    "answer contains a fact value from the wrong source record; requirement: {}",
                    self.requirement
                )));
            }
            if !all_contained {
                return Err(RecordCheckError::Failed(format!(
                    "answer does not contain all required source facts; requirement: {}",
                    self.requirement
                )));
            }
            return Ok(());
        }

        // Check rejected claim forms first
        for template in &self.rejected {
            let parts = parse_template(template).map_err(RecordCheckError::InvalidContract)?;
            if matches_claim(&answer_norm, &parts, &extracts) {
                return Err(RecordCheckError::Failed(format!(
                    "answer matches a reviewed incorrect claim form; requirement: {}",
                    self.requirement
                )));
            }
        }

        // Check accepted claim forms
        // Try whole answer as one claim
        for template in &self.claims {
            let parts = parse_template(template).map_err(RecordCheckError::InvalidContract)?;
            if matches_claim(&answer_norm, &parts, &extracts) {
                return Ok(());
            }
        }

        // Try line-by-line for multi-line answers
        if answer.lines().count() > 1 {
            let mut all_matched = true;
            let mut any_matched = false;
            for line in answer.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let line_norm = normalize(strip_markers(line));
                let mut line_matched = false;
                for template in &self.claims {
                    let parts =
                        parse_template(template).map_err(RecordCheckError::InvalidContract)?;
                    if matches_claim(&line_norm, &parts, &extracts) {
                        line_matched = true;
                        any_matched = true;
                        break;
                    }
                }
                if !line_matched {
                    all_matched = false;
                }
            }
            if any_matched && all_matched {
                return Ok(());
            }
        }

        // Check if answer contains all fact values even without matching a claim form
        // (substring match — basic containment check)
        let mut all_contained = true;
        let mut any_contradicted = false;
        for (name, extract) in &extracts {
            if !extract.contained_in(&answer_norm) {
                all_contained = false;
                // Check if a distractor value appears instead
                if let Some(wrong_values) = distractors.get(name) {
                    for wrong in wrong_values {
                        if answer_norm.contains(&normalize(wrong)) {
                            any_contradicted = true;
                        }
                    }
                }
            }
        }

        if any_contradicted {
            return Err(RecordCheckError::Failed(format!(
                "answer contains a fact value from the wrong source record; requirement: {}",
                self.requirement
            )));
        }

        if all_contained {
            // All source values are present but wording doesn't match a reviewed form
            return Err(RecordCheckError::NeedsAdjudication(format!(
                "answer contains all source facts but wording is not a reviewed claim form; adjudicate against: {}",
                self.requirement
            )));
        }

        // Check for structural contradiction: answer structurally matches a claim
        // form but slot values disagree with the source (e.g., wrong number).
        for template in &self.claims {
            let parts = parse_template(template).map_err(RecordCheckError::InvalidContract)?;
            if structural_contradiction(&answer_norm, &parts, &extracts) {
                return Err(RecordCheckError::Failed(format!(
                    "answer contradicts source facts; requirement: {}",
                    self.requirement
                )));
            }
        }

        // Not all facts present and no reviewed form matched
        Err(RecordCheckError::NeedsAdjudication(format!(
            "answer does not match any reviewed claim form; adjudicate against: {}",
            self.requirement
        )))
    }
}

// ---------------------------------------------------------------------------
// Extracted fact value
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct FactExtract {
    /// The canonical text representation of the value.
    canonical: String,
    /// Normalized lowercase canonical.
    canonical_norm: String,
    /// Additional reviewed presentations.
    presentations: Vec<String>,
    /// For numbers: the raw numeric value for comparison.
    numeric: Option<f64>,
    kind: RecordFactKind,
}

impl FactExtract {
    fn from_value(value: &Value, fact: &RecordFact) -> Result<Self, RecordCheckError> {
        let canonical = value_to_text(value);
        if canonical.is_empty() {
            return Err(RecordCheckError::MissingEvidence(format!(
                "fact field {} has an empty value",
                fact.field
            )));
        }
        let numeric = match fact.kind {
            RecordFactKind::Number | RecordFactKind::Money => value_as_f64(value),
            _ => None,
        };
        Ok(Self {
            canonical_norm: normalize(&canonical),
            canonical,
            presentations: fact.presentations.clone(),
            numeric,
            kind: fact.kind,
        })
    }

    /// Check if the extracted value matches some text (case-insensitive).
    fn matches_text(&self, text: &str) -> bool {
        let norm = normalize(text);
        if self.canonical_norm == norm {
            return true;
        }
        if self.presentations.iter().any(|p| normalize(p) == norm) {
            return true;
        }
        // Numeric equivalence (412000 == 412,000 == 412.000)
        if let Some(expected) = self.numeric {
            if let Some(found) = parse_number_text(&norm) {
                if (expected - found).abs() < 0.01 {
                    return true;
                }
            }
        }
        false
    }

    /// Check if the answer text contains this fact value as a substring.
    fn contained_in(&self, answer_norm: &str) -> bool {
        // Direct substring
        if answer_norm.contains(&self.canonical_norm) {
            return true;
        }
        // Check presentations
        if self
            .presentations
            .iter()
            .any(|p| answer_norm.contains(&normalize(p)))
        {
            return true;
        }
        // For numbers, check common formats
        if let Some(num) = self.numeric {
            if contains_number(answer_norm, num) {
                return true;
            }
        }
        // For identifiers, check case-sensitive
        // (handled separately since answer_norm is lowercase)
        false
    }

    /// Try to consume this fact's value from the start of `text`, returning
    /// the number of characters consumed (in the normalized text).
    fn consume(&self, text: &str) -> Vec<usize> {
        let mut lengths = Vec::new();
        // Try canonical
        if text.starts_with(&self.canonical_norm) {
            lengths.push(self.canonical_norm.len());
        }
        // Try presentations
        for p in &self.presentations {
            let pn = normalize(p);
            if text.starts_with(&pn) && !lengths.contains(&pn.len()) {
                lengths.push(pn.len());
            }
        }
        // For numbers, try all common formats
        if let Some(num) = self.numeric {
            for len in consume_number_all(text, num) {
                if !lengths.contains(&len) {
                    lengths.push(len);
                }
            }
        }
        // Sort longest first for better matching
        lengths.sort_by(|a, b| b.cmp(a));
        lengths
    }
}

// ---------------------------------------------------------------------------
// Template matching
// ---------------------------------------------------------------------------

fn matches_claim(
    answer_norm: &str,
    parts: &[Part],
    extracts: &BTreeMap<String, FactExtract>,
) -> bool {
    match_parts(answer_norm, parts, extracts)
}

fn match_parts(text: &str, parts: &[Part], extracts: &BTreeMap<String, FactExtract>) -> bool {
    let Some((first, rest)) = parts.split_first() else {
        return text.is_empty();
    };
    match first {
        Part::Literal(lit) => {
            text.starts_with(lit.as_str()) && match_parts(&text[lit.len()..], rest, extracts)
        }
        Part::Slot(name) => {
            let Some(extract) = extracts.get(name) else {
                return false;
            };
            for consumed in extract.consume(text) {
                if match_parts(&text[consumed..], rest, extracts) {
                    return true;
                }
            }
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Number formatting helpers
// ---------------------------------------------------------------------------

fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.replace(',', "").parse::<f64>().ok(),
        _ => None,
    }
}

/// Parse a normalized text into a number, handling common groupings.
fn parse_number_text(text: &str) -> Option<f64> {
    // Strip currency symbols
    let cleaned: String = text
        .trim_start_matches(|c: char| matches!(c, '$' | '€' | '£'))
        .trim()
        .replace([',', ' ', '\u{a0}'], "")
        .replace(".", "."); // keep decimal point
    cleaned.parse::<f64>().ok()
}

/// Check if normalized answer text contains a number approximately equal to `expected`.
fn contains_number(text: &str, expected: f64) -> bool {
    // Generate common string representations
    let representations = number_representations(expected);
    representations
        .iter()
        .any(|repr| text.contains(repr.as_str()))
}

/// Try to consume a number equal to `expected` from the start of `text`.
fn consume_number_all(text: &str, expected: f64) -> Vec<usize> {
    let representations = number_representations(expected);
    let mut results = Vec::new();
    for repr in &representations {
        if text.starts_with(repr.as_str()) {
            let after = &text[repr.len()..];
            if after.is_empty() || !after.starts_with(|c: char| c.is_ascii_digit()) {
                if !results.contains(&repr.len()) {
                    results.push(repr.len());
                }
            }
        }
    }
    results
}

/// Generate common text representations of a number.
fn number_representations(value: f64) -> Vec<String> {
    let mut reps = Vec::new();

    // Integer representation
    if value == value.floor() && value.abs() < 1e15 {
        let int = value as i64;
        reps.push(int.to_string());

        // With comma grouping (US style)
        let s = int.to_string();
        if s.len() > 3 {
            let mut grouped = String::new();
            for (i, c) in s.chars().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    grouped.push(',');
                }
                grouped.push(c);
            }
            reps.push(grouped.chars().rev().collect());
        }
    }

    // Decimal representation
    let decimal = format!("{value}");
    if !reps.contains(&decimal) {
        reps.push(decimal);
    }

    // With .00 suffix for integers
    if value == value.floor() && value.abs() < 1e15 {
        let int = value as i64;
        reps.push(format!("{int}.00"));
    }

    // Normalize to lowercase for comparison
    reps.into_iter().map(|r| r.to_lowercase()).collect()
}

/// Strip markdown list markers.
fn strip_markers(line: &str) -> &str {
    let line = line.trim();
    for marker in ["- ", "* ", "+ ", "• "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest.trim();
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        if let Some(rest) = line[digits..]
            .strip_prefix(". ")
            .or_else(|| line[digits..].strip_prefix(") "))
        {
            return rest.trim();
        }
    }
    line
}

/// Check if the answer structurally matches a claim template (all literals
/// present) but with different values in the slots.
fn structural_contradiction(
    text: &str,
    parts: &[Part],
    extracts: &BTreeMap<String, FactExtract>,
) -> bool {
    struct_match(text, parts, extracts, false)
}

fn struct_match(
    text: &str,
    parts: &[Part],
    extracts: &BTreeMap<String, FactExtract>,
    any_wrong: bool,
) -> bool {
    let Some((first, rest)) = parts.split_first() else {
        return text.is_empty() && any_wrong;
    };
    match first {
        Part::Literal(lit) => {
            text.starts_with(lit.as_str())
                && struct_match(&text[lit.len()..], rest, extracts, any_wrong)
        }
        Part::Slot(name) => {
            let Some(extract) = extracts.get(name) else {
                return false;
            };
            // Try consuming with correct values
            let correct_lengths = extract.consume(text);
            for consumed in &correct_lengths {
                if struct_match(&text[*consumed..], rest, extracts, any_wrong) {
                    // Correct value was used for THIS slot, but a later slot
                    // may still have wrong values (any_wrong propagated).
                    return true;
                }
            }
            // Also check: if correct value matches and completes cleanly,
            // this is NOT a contradiction.
            for consumed in &correct_lengths {
                if match_parts(&text[*consumed..], rest, extracts) {
                    return false; // Fully correct match, no contradiction
                }
            }
            // Try consuming any plausible value (non-empty text up to next literal)
            let next_lit = rest.iter().find_map(|p| match p {
                Part::Literal(l) => Some(l.as_str()),
                _ => None,
            });
            match next_lit {
                Some(lit) => {
                    for (idx, _) in text.match_indices(lit) {
                        if idx > 0 && struct_match(&text[idx..], rest, extracts, true) {
                            return true;
                        }
                    }
                    false
                }
                None => {
                    // Last slot — consume rest
                    if rest.is_empty() && !text.is_empty() {
                        true // Wrong value in last slot
                    } else {
                        false
                    }
                }
            }
        }
    }
}
