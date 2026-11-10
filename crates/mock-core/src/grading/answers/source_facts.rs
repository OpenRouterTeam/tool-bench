//! Answer grading bound to typed rows of an authored research table.
//!
//! The contract names one Markdown table in an authored baseline document,
//! its key column and the typed columns the answer must state.  The reply is
//! read as it comes: prose, bullets, a Markdown table (also transposed), JSON
//! or a fenced block are all turned into statements, and each stated value is
//! attributed to the source row named with it.
//!
//! - **Pass**: every requested row is named, each required fact is stated for
//!   its own row and nothing denies it.  With `rows = { largest = ... }` the
//!   reply must lead with the requested row; other rows may follow as
//!   context, or precede it when they are set aside or carry their own
//!   (smaller) ranking value.
//! - **Fail**: a row or fact is missing, denied or wrong, another row's value
//!   is attributed to a requested row, or another row is named first.
//! - **Unresolved**: a fact is both stated and disputed, the requested row is
//!   named only under a contradicting label ("runner-up"), or a figure is in
//!   a notation that cannot be read exactly (a range, number words, `4.200`).

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::prose::{self, Evidence, Stance};
use crate::{
    json_pointer,
    relations::{IdentitySource, RecordCheckError},
    rich_text::unsupported_control,
};

mod tables;
mod values;

use tables::Grid;
use values::{Amount, Fact, Quantity, header_key};

const MAX_COLUMNS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFacts {
    pub requirement: String,
    pub source: IdentitySource,
    pub key: String,
    pub rows: FactRows,
    pub columns: BTreeMap<String, FactColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactRows {
    All,
    Largest(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactColumn {
    pub header: String,
    pub kind: FactKind,
    #[serde(default = "required_default")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub short_names: bool,
}

fn required_default() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Name,
    Place,
    Number,
    Percent,
    Money,
}

struct Source {
    rows: Vec<Vec<(String, Fact)>>,
    targets: BTreeSet<usize>,
    /// Words of the column names and source headers: they label a value and
    /// claim nothing about it.
    labels: Vec<String>,
}

impl Source {
    fn fact(&self, row: usize, column: &str) -> &Fact {
        self.rows[row]
            .iter()
            .find(|(name, _)| name == column)
            .map(|(_, fact)| fact)
            .expect("every source row holds every declared column")
    }
}

#[derive(Default)]
struct Verdict {
    failure: Option<String>,
    review: Option<String>,
}

impl Verdict {
    fn fail(&mut self, reason: impl Into<String>) {
        self.failure.get_or_insert_with(|| reason.into());
    }

    fn review(&mut self, reason: impl Into<String>) {
        self.review.get_or_insert_with(|| reason.into());
    }

    fn finish(self, requirement: &str) -> Result<(), RecordCheckError> {
        if let Some(reason) = self.failure {
            return Err(RecordCheckError::Failed(format!(
                "{reason}; requirement: {requirement}"
            )));
        }
        if let Some(reason) = self.review {
            return Err(RecordCheckError::NeedsAdjudication(format!(
                "{reason}; adjudicate against: {requirement}"
            )));
        }
        Ok(())
    }
}

fn valid_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn strip_marker(line: &str) -> &str {
    let line = line.trim().trim_start_matches(['#', '>']).trim();
    for marker in ["- ", "* ", "+ ", "• "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest.trim();
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0
        && let Some(rest) = line[digits..]
            .strip_prefix(". ")
            .or_else(|| line[digits..].strip_prefix(") "))
    {
        return rest.trim();
    }
    line
}

impl SourceFacts {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.requirement.trim().is_empty() {
            return Err("source fact answers require a nonempty criterion".into());
        }
        self.source.selector.validate()?;
        let tokens = json_pointer::tokens(&self.source.field)?;
        if self.source.selector.created
            || self.source.separator.is_some()
            || tokens.is_empty()
            || tokens.iter().any(|token| token == "*")
        {
            return Err(
                "source fact answers must read one literal field of an authored baseline record"
                    .into(),
            );
        }
        if self.columns.is_empty() || self.columns.len() > MAX_COLUMNS {
            return Err("source fact answers require between one and sixteen columns".into());
        }
        let mut headers = BTreeSet::new();
        for (name, column) in &self.columns {
            if !valid_name(name) {
                return Err(format!("invalid source fact column name {name:?}"));
            }
            if column.header.chars().any(unsupported_control)
                || !headers.insert(header_key(&column.header))
                || header_key(&column.header).is_empty()
            {
                return Err("source fact columns require distinct readable source headers".into());
            }
            if column.short_names && column.kind != FactKind::Name {
                return Err("short names apply only to name columns".into());
            }
        }
        if !self
            .columns
            .get(&self.key)
            .is_some_and(|column| column.kind == FactKind::Name && column.required)
        {
            return Err("the source fact key must be a required name column".into());
        }
        if let FactRows::Largest(column) = &self.rows
            && !self.columns.get(column).is_some_and(|column| {
                matches!(
                    column.kind,
                    FactKind::Number | FactKind::Percent | FactKind::Money
                )
            })
        {
            return Err("largest-row selection requires a declared numeric column".into());
        }
        Ok(())
    }

    fn source(&self, initial: Option<&Value>) -> Result<Source, RecordCheckError> {
        let initial = initial.ok_or_else(|| {
            RecordCheckError::MissingEvidence(
                "source fact answers require authoritative initial business state".into(),
            )
        })?;
        let records = self.source.selector.select(Some(initial), initial)?;
        let [(_, record)] = records.as_slice() else {
            return Err(RecordCheckError::MissingEvidence(
                "source fact answers require one authored source record".into(),
            ));
        };
        let document = record
            .pointer(&self.source.field)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RecordCheckError::MissingEvidence(
                    "authored source record has no readable source document".into(),
                )
            })?;
        if document.chars().any(unsupported_control) {
            return Err(RecordCheckError::NeedsAdjudication(
                "source document contains directional or invisible controls".into(),
            ));
        }
        let wanted: BTreeMap<&str, String> = self
            .columns
            .iter()
            .map(|(name, column)| (name.as_str(), header_key(&column.header)))
            .collect();
        let lines: Vec<&str> = document.lines().collect();
        let grids: Vec<Grid> = (0..lines.len())
            .filter_map(|index| tables::table_at(&lines, index).map(|(grid, _)| grid))
            .collect();
        let candidates: Vec<&Grid> = grids
            .iter()
            .filter(|grid| {
                let keys: Vec<String> = grid
                    .headers
                    .iter()
                    .map(|header| header_key(header))
                    .collect();
                wanted
                    .values()
                    .all(|header| keys.iter().filter(|key| *key == header).count() == 1)
            })
            .collect();
        let [grid] = candidates[..] else {
            return Err(RecordCheckError::MissingEvidence(
                "source document lacks exactly one table with the declared columns".into(),
            ));
        };
        let keys: Vec<String> = grid
            .headers
            .iter()
            .map(|header| header_key(header))
            .collect();
        let mut rows = Vec::new();
        for cells in &grid.rows {
            if cells.len() != keys.len() {
                return Err(RecordCheckError::MissingEvidence(
                    "source table contains a ragged row".into(),
                ));
            }
            let mut facts = Vec::new();
            for (name, column) in &self.columns {
                let position = keys
                    .iter()
                    .position(|key| key == &wanted[name.as_str()])
                    .expect("the selected table has every declared column");
                let fact = Fact::parse(column.kind, &cells[position]).ok_or_else(|| {
                    RecordCheckError::MissingEvidence(format!(
                        "source {name} cell is not a readable {:?} fact",
                        column.kind
                    ))
                })?;
                facts.push((name.clone(), fact));
            }
            rows.push(facts);
        }
        if rows.is_empty() {
            return Err(RecordCheckError::MissingEvidence(
                "source table has no rows".into(),
            ));
        }
        let labels: BTreeSet<String> = self
            .columns
            .iter()
            .flat_map(|(name, column)| [name.as_str(), column.header.as_str()])
            .flat_map(|text| {
                prose::fold(text)
                    .split(|character: char| !character.is_alphanumeric())
                    .filter(|word| !word.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut source = Source {
            rows,
            targets: BTreeSet::new(),
            labels: labels.into_iter().collect(),
        };
        let subjects: Vec<String> = (0..source.rows.len())
            .map(|row| source.fact(row, &self.key).names()[0].clone())
            .collect();
        if subjects.iter().collect::<BTreeSet<_>>().len() != subjects.len() {
            return Err(RecordCheckError::MissingEvidence(
                "source table repeats a subject".into(),
            ));
        }
        for (name, column) in &self.columns {
            if !column.short_names {
                continue;
            }
            let full: Vec<String> = (0..source.rows.len())
                .map(|row| source.fact(row, name).names()[0].clone())
                .collect();
            let short: Vec<Option<&str>> = full
                .iter()
                .map(|text| {
                    let mut words = text.split(' ');
                    let first = words.next()?;
                    words.next().map(|_| first)
                })
                .collect();
            for (row, candidate) in short.iter().enumerate() {
                let Some(candidate) = candidate else {
                    continue;
                };
                let unique = short
                    .iter()
                    .enumerate()
                    .all(|(other, word)| other == row || word != &Some(*candidate))
                    && !full.iter().any(|text| text == candidate);
                if unique {
                    let entry = source.rows[row]
                        .iter_mut()
                        .find(|(column, _)| column == name)
                        .expect("every source row holds every declared column");
                    entry.1.add_name((*candidate).to_owned());
                }
            }
        }
        source.targets = match &self.rows {
            FactRows::All => (0..source.rows.len()).collect(),
            FactRows::Largest(column) => {
                let amounts: Vec<(&Amount, Option<&str>)> = (0..source.rows.len())
                    .map(|row| {
                        source
                            .fact(row, column)
                            .amount()
                            .expect("largest-row columns are numeric")
                    })
                    .collect();
                if amounts.iter().any(|(_, unit)| *unit != amounts[0].1) {
                    return Err(RecordCheckError::NeedsAdjudication(
                        "source values use different currencies and cannot be ranked".into(),
                    ));
                }
                let largest = amounts
                    .iter()
                    .map(|(amount, _)| *amount)
                    .max()
                    .expect("the source table has rows");
                let leaders: BTreeSet<usize> = amounts
                    .iter()
                    .enumerate()
                    .filter(|(_, (amount, _))| *amount == largest)
                    .map(|(row, _)| row)
                    .collect();
                if leaders.len() != 1 {
                    return Err(RecordCheckError::NeedsAdjudication(
                        "source ranking is tied; no single requested row exists".into(),
                    ));
                }
                leaders
            }
        };
        Ok(source)
    }
}

static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\bhttps?://[^\s<>"'`|]+"#).expect("URL pattern"));
/// Figures spelled out in words, which the reader does not evaluate.
static NUMBER_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:hundred|thousand|million|billion)\b",
        r"|\b(?:zero|one|two|three|four|five|six|seven|eight|nine)\s+point\s+",
        r"(?:zero|one|two|three|four|five|six|seven|eight|nine)\b"
    ))
    .expect("number word pattern")
});
/// "X is second", "X trails": the row is ranked below the requested one.
static RANKED_BELOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(?:\s*\([^)|]*\))?\s*(?:",
        r"(?:is|was|ranks|comes|came|sits|places|placed|finishes|finished)\s+",
        r"(?:only\s+|in\s+|the\s+|a\s+)*(?:second|third|2nd|3rd|runner-up|last|smaller|behind)\b",
        r"|trails\b|lags\b)"
    ))
    .expect("lower rank pattern")
});
/// Labels that rank a row below the largest one.
const LOWER_RANKS: &[&str] = &[
    "runner-up",
    "runners-up",
    "second",
    "third",
    "2nd",
    "3rd",
    "second-largest",
    "third-largest",
    "second-biggest",
    "smallest",
    "lowest",
    "least",
    "cheapest",
    "last",
    "worst",
    "smaller",
    "lesser",
    "minor",
    "challenger",
];

/// Whether the words leading up to a row's name rank it below the largest
/// row ("runner-up: X", "cheapest vendor: X", "ahead of X").
fn ranked_below(before: &str, labels: &[String]) -> bool {
    let tokens: Vec<&str> = before
        .split(' ')
        .filter(|token| !token.is_empty())
        .collect();
    let mut unknown = false;
    for (steps, index) in (0..tokens.len()).rev().enumerate() {
        let token = tokens[index];
        if steps >= 6 || token.ends_with([',', ';', '.']) {
            break;
        }
        let word = prose::bare(token);
        let pair = (index > 0).then(|| (prose::bare(tokens[index - 1]), word));
        if LOWER_RANKS.contains(&word) || matches!(pair, Some(("ahead", "of") | ("followed", "by")))
        {
            return true;
        }
        if word.is_empty() || prose::transparent(word) || labels.iter().any(|label| label == word) {
            continue;
        }
        if std::mem::replace(&mut unknown, true) {
            break;
        }
    }
    false
}

/// One source row named in a statement.
struct Mention {
    row: usize,
    start: usize,
    end: usize,
    stance: Stance,
}

/// One statement of the answer: a sentence, a table row or a JSON object.
struct Segment {
    text: String,
    subjects: Vec<Mention>,
    quantities: Vec<Quantity>,
    /// The row named most recently before this statement.
    carried: Option<usize>,
    /// A table row or JSON object: one unit, whatever the order of its cells.
    row: bool,
}

impl Segment {
    /// The source row a value at `start..end` is stated for.  In a table row
    /// that names one source row it is that row.  Otherwise it is the row
    /// named before the value in this statement; failing that the row named
    /// right after it in the same clause ("EUR 6,900 is what X charges",
    /// "EUR 6,200 (X)"), the row the answer was last talking about, or the
    /// first row named.
    fn owner(&self, start: usize, end: usize) -> Option<usize> {
        let first = self.subjects.first()?;
        if self.row && self.subjects.iter().all(|mention| mention.row == first.row) {
            return Some(first.row);
        }
        if let Some(before) = self
            .subjects
            .iter()
            .rev()
            .find(|mention| mention.start < start)
        {
            return Some(before.row);
        }
        let between = &self.text[end.min(first.start)..first.start];
        let bracketed =
            between.trim() == "(" && self.text[first.end..].trim_start().starts_with(')');
        if self.carried.is_none() || bracketed || !between.contains([',', ';', ':', '(', ')', '|'])
        {
            Some(first.row)
        } else {
            self.carried
        }
    }

    fn owned(&self, start: usize, end: usize) -> Option<usize> {
        if self.subjects.is_empty() {
            self.carried
        } else {
            self.owner(start, end)
        }
    }

    /// Where the words about a value ending at `end` stop.
    fn scope(&self, end: usize) -> usize {
        self.subjects
            .iter()
            .find(|mention| mention.start >= end)
            .map_or(self.text.len(), |mention| mention.start)
    }

    fn about(&self, row: usize) -> bool {
        if self.subjects.is_empty() {
            self.carried == Some(row)
        } else {
            self.subjects.iter().any(|mention| mention.row == row)
        }
    }
}

/// How the answer states one column of one source row.
struct Reading {
    evidence: Evidence,
    /// Another row's value is stated for this row before its own.
    foreign: bool,
    /// A figure for this row is in a notation that cannot be read exactly.
    unclear: bool,
}

fn clean(text: &str) -> String {
    prose::fold(&URL.replace_all(text, " "))
}

/// A whole JSON answer or fenced block as table-like lines.
fn structured(text: &str) -> Result<Option<Vec<String>>, RecordCheckError> {
    match tables::json(text) {
        Ok(value @ (Value::Array(_) | Value::Object(_))) => {
            let mut lines = Vec::new();
            tables::flatten(&value, "", &mut lines);
            Ok(Some(lines.iter().map(|line| clean(line)).collect()))
        }
        Err(error) if error.to_string().contains(tables::DUPLICATE_KEY) => {
            Err(RecordCheckError::NeedsAdjudication(
                "structured answer gives one key several values".into(),
            ))
        }
        _ => Ok(None),
    }
}

impl SourceFacts {
    fn subject<'source>(&self, source: &'source Source, row: usize) -> &'source str {
        &source.fact(row, &self.key).names()[0]
    }

    fn named(&self, text: &str, source: &Source) -> Vec<(usize, usize, usize)> {
        let mut candidates: Vec<(usize, usize, usize)> = (0..source.rows.len())
            .flat_map(|row| {
                source
                    .fact(row, &self.key)
                    .spans(text, &[])
                    .into_iter()
                    .map(move |(start, end)| (row, start, end))
            })
            .collect();
        candidates.sort_by_key(|(_, start, end)| (std::cmp::Reverse(end - start), *start));
        let mut found: Vec<(usize, usize, usize)> = Vec::new();
        for candidate in candidates {
            if !found
                .iter()
                .any(|other| other.1 < candidate.2 && candidate.1 < other.2)
            {
                found.push(candidate);
            }
        }
        found.sort_by_key(|(_, start, _)| *start);
        found
    }

    fn table(&self, grid: Grid, source: &Source, lines: &mut Vec<(String, bool)>) {
        let names_one = |cell: &String| {
            self.named(&clean(cell), source)
                .iter()
                .map(|(row, _, _)| *row)
                .collect::<BTreeSet<_>>()
                .len()
                == 1
        };
        let transposed = grid.headers.len() > 1
            && grid.headers[1..].iter().all(names_one)
            && !grid
                .rows
                .iter()
                .any(|row| row.first().is_some_and(names_one));
        let grid = if transposed {
            tables::transposed(&grid)
        } else {
            grid
        };
        let headers: Vec<String> = grid.headers.iter().map(|header| clean(header)).collect();
        if grid.rows.is_empty() {
            lines.push((headers.join(" | "), true));
        }
        for row in &grid.rows {
            let cells: Vec<String> = row
                .iter()
                .enumerate()
                .filter_map(|(index, cell)| {
                    let cell = clean(cell);
                    if cell.is_empty() {
                        return None;
                    }
                    Some(
                        match headers.get(index).filter(|header| !header.is_empty()) {
                            Some(header) => format!("{header}: {cell}"),
                            None => cell,
                        },
                    )
                })
                .collect();
            if !cells.is_empty() {
                lines.push((cells.join(" | "), true));
            }
        }
    }

    /// Turns answer lines into statements: fenced JSON and Markdown tables
    /// become one labelled line per row, everything else is split into
    /// sentences.
    fn statements(
        &self,
        lines: &[&str],
        source: &Source,
        found: &mut Vec<(String, bool)>,
    ) -> Result<(), RecordCheckError> {
        let sentences = |line: &str, found: &mut Vec<(String, bool)>| {
            let line = clean(strip_marker(line));
            found.extend(
                prose::sentences(&line)
                    .into_iter()
                    .map(|sentence| (sentence.to_owned(), false)),
            );
        };
        let mut index = 0;
        while index < lines.len() {
            if let Some(info) = lines[index].trim_start().strip_prefix("```") {
                let body = &lines[index + 1..];
                let close = body
                    .iter()
                    .position(|line| line.trim_start().starts_with("```"))
                    .unwrap_or(body.len());
                let body = &body[..close];
                // More than a language tag is wording, not fence metadata.
                if info.split_whitespace().count() > 1 {
                    sentences(info, found);
                }
                match structured(&body.join("\n"))? {
                    Some(rows) => found.extend(rows.into_iter().map(|row| (row, true))),
                    None => self.statements(body, source, found)?,
                }
                index += close + 2;
            } else if let Some((grid, next)) = tables::table_at(lines, index) {
                self.table(grid, source, found);
                index = next;
            } else {
                sentences(lines[index], found);
                index += 1;
            }
        }
        Ok(())
    }

    fn segments(&self, answer: &str, source: &Source) -> Result<Vec<Segment>, RecordCheckError> {
        let mut texts = Vec::new();
        match structured(answer.trim())? {
            Some(rows) => texts = rows.into_iter().map(|row| (row, true)).collect(),
            None => {
                let lines: Vec<&str> = answer.lines().collect();
                self.statements(&lines, source, &mut texts)?;
            }
        }
        let leader = match self.rows {
            FactRows::Largest(_) => source.targets.first().copied(),
            FactRows::All => None,
        };
        let mut carried = None;
        let mut segments = Vec::new();
        for (text, row) in texts {
            let named = self.named(&text, source);
            let subjects: Vec<Mention> = named
                .iter()
                .enumerate()
                .map(|(position, (row, start, end))| {
                    let scope = named[position + 1..]
                        .iter()
                        .find(|next| next.0 != *row)
                        .map_or(text.len(), |next| next.1);
                    let mut stance = prose::stance(&text, *start, *end, scope, &source.labels);
                    let cell = text[..*start].rfind('|').map_or(0, |index| index + 1);
                    if stance == Stance::Affirmed
                        && leader == Some(*row)
                        && (ranked_below(&text[cell..*start], &source.labels)
                            || RANKED_BELOW.is_match(&text[*end..scope]))
                    {
                        stance = Stance::Disputed;
                    }
                    Mention {
                        row: *row,
                        start: *start,
                        end: *end,
                        stance,
                    }
                })
                .collect();
            let next = subjects.last().map(|mention| mention.row).or(carried);
            segments.push(Segment {
                quantities: values::quantities(&text),
                text,
                subjects,
                carried,
                row,
            });
            carried = next;
        }
        Ok(segments)
    }

    fn reading(&self, segments: &[Segment], source: &Source, row: usize, column: &str) -> Reading {
        let own = source.fact(row, column);
        let mut reading = Reading {
            evidence: Evidence::default(),
            foreign: false,
            unclear: false,
        };
        for segment in segments {
            let stated = own.spans(&segment.text, &segment.quantities);
            let mut spans: Vec<(usize, usize, bool)> = stated
                .iter()
                .map(|(start, end)| (*start, *end, true))
                .collect();
            for other in (0..source.rows.len()).filter(|other| *other != row) {
                for span in source
                    .fact(other, column)
                    .spans(&segment.text, &segment.quantities)
                {
                    if !stated.contains(&span) && !spans.contains(&(span.0, span.1, false)) {
                        spans.push((span.0, span.1, false));
                    }
                }
            }
            spans.retain(|(start, end, _)| {
                segment.owned(*start, *end) == Some(row)
                    && !segment
                        .subjects
                        .iter()
                        .any(|mention| mention.start < *end && *start < mention.end)
            });
            spans.sort_unstable();
            // A statement that does not name the row and sets its subject
            // aside ("others charge ...") is about other rows.
            let aside = segment.subjects.is_empty() && prose::set_aside(&segment.text);
            for (start, end, own) in spans {
                let stance = prose::stance(
                    &segment.text,
                    start,
                    end,
                    segment.scope(end),
                    &source.labels,
                );
                if own {
                    reading.evidence.count(stance);
                } else if stance == Stance::Affirmed && reading.evidence.affirmed == 0 && !aside {
                    reading.foreign = true;
                }
            }
            if own.amount().is_some() {
                reading.unclear |= segment.quantities.iter().any(|quantity| {
                    segment.owned(quantity.start, quantity.end) == Some(row)
                        && own.unclear(quantity)
                }) || (segment.about(row)
                    && NUMBER_WORDS.is_match(&segment.text));
            }
        }
        reading
    }

    /// Checks every column of one requested row.
    fn row(&self, segments: &[Segment], source: &Source, row: usize, verdict: &mut Verdict) {
        let subject = self.subject(source, row);
        for (name, column) in &self.columns {
            if name == &self.key {
                continue;
            }
            let Reading {
                evidence,
                foreign,
                unclear,
            } = self.reading(segments, source, row, name);
            if evidence.affirmed > 0 {
                if evidence.denied + evidence.disputed > 0 {
                    verdict.review(format!(
                        "answer both states and disputes the source {name} of {subject}"
                    ));
                } else if foreign {
                    verdict.fail(format!(
                        "answer attributes another source row's {name} to {subject}"
                    ));
                }
            } else if evidence.denied > 0 {
                verdict.fail(format!("answer denies the source {name} of {subject}"));
            } else if foreign {
                verdict.fail(format!(
                    "answer attributes another source row's {name} to {subject}"
                ));
            } else if evidence.disputed > 0 {
                verdict.review(format!(
                    "answer mentions the source {name} of {subject} only in a negative remark"
                ));
            } else if !column.required {
                continue;
            } else if unclear {
                verdict.review(format!(
                    "answer may state the {name} of {subject} in a notation that cannot be read exactly"
                ));
            } else {
                verdict.fail(format!(
                    "answer does not state the source {name} of {subject}"
                ));
            }
        }
    }

    /// With one requested row, the answer must present that row and lead
    /// with it.
    fn leader(
        &self,
        segments: &[Segment],
        source: &Source,
        row: usize,
        ranking: &str,
        verdict: &mut Verdict,
    ) {
        let subject = self.subject(source, row);
        let mut evidence = Evidence::default();
        let mut first = None;
        for (index, segment) in segments.iter().enumerate() {
            for mention in segment.subjects.iter().filter(|mention| mention.row == row) {
                evidence.count(mention.stance);
                if mention.stance == Stance::Affirmed {
                    first.get_or_insert(index);
                }
            }
        }
        let Some(first) = first else {
            if evidence.denied > 0 {
                verdict.fail(format!(
                    "answer denies that {subject} is the requested source row"
                ));
            } else if evidence.disputed > 0 {
                verdict.review(format!(
                    "answer names {subject} only in a negative or lower-ranked remark"
                ));
            } else {
                verdict.fail("answer does not name the requested source row");
            }
            return;
        };
        if evidence.denied + evidence.disputed > 0 {
            verdict.review(format!(
                "answer both presents and disputes {subject} as the requested source row"
            ));
        }
        // A row named earlier is context only when it is set aside or carries
        // its own value of the ranking column.
        let rival = segments[..first].iter().any(|segment| {
            segment.subjects.iter().any(|mention| {
                mention.row != row
                    && mention.stance == Stance::Affirmed
                    && !prose::set_aside(&segment.text)
                    && !source
                        .fact(mention.row, ranking)
                        .spans(&segment.text, &segment.quantities)
                        .iter()
                        .any(|(start, end)| segment.owned(*start, *end) == Some(mention.row))
            })
        });
        if rival {
            verdict.fail("answer names a source row other than the requested one first");
        }
    }

    pub(crate) fn check(
        &self,
        answer: &str,
        initial: Option<&Value>,
    ) -> Result<(), RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;
        if answer.trim().is_empty() {
            return Err(RecordCheckError::Failed(
                "required source fact answer is empty".into(),
            ));
        }
        if answer.chars().any(unsupported_control) {
            return Err(RecordCheckError::NeedsAdjudication(
                "answer contains directional or invisible controls".into(),
            ));
        }
        let source = self.source(initial)?;
        let segments = self.segments(answer, &source)?;
        let mut verdict = Verdict::default();
        for row in &source.targets {
            if let FactRows::Largest(ranking) = &self.rows {
                self.leader(&segments, &source, *row, ranking, &mut verdict);
            } else if !segments
                .iter()
                .any(|segment| segment.subjects.iter().any(|mention| mention.row == *row))
            {
                verdict.fail(format!(
                    "answer omits the source row {}",
                    self.subject(&source, *row)
                ));
                continue;
            }
            self.row(&segments, &source, *row, &mut verdict);
        }
        verdict.finish(&self.requirement)
    }
}
