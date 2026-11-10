use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    json_pointer,
    relations::{IdentitySource, RecordCheckError},
    rich_text::unsupported_control,
};

mod claims;
mod tables;
mod values;

use claims::{Matcher, Part};
use tables::Grid;
use values::{Aliases, Amount, CellMatch, Fact, header_key, normalized, strip_markup};

const MAX_COLUMNS: usize = 16;
const CLAIM_SEARCH_BUDGET: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFacts {
    pub requirement: String,
    pub source: IdentitySource,
    pub key: String,
    pub rows: FactRows,
    pub columns: BTreeMap<String, FactColumn>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aliases: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neutral_claims: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answer_headers: Vec<String>,
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
    raw: Vec<BTreeMap<String, String>>,
    targets: BTreeSet<usize>,
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

enum Heading<'contract> {
    Fact(&'contract str),
    Extra(String),
    Unknown,
}

enum Claim {
    Row(usize),
    Context(usize),
    Review(&'static str),
    Wrong(&'static str),
    Conflict,
}

fn strip_marker(line: &str) -> &str {
    let line = line.trim();
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Selected,
    Neutral,
    Rejected,
}

fn fenced(answer: &str) -> Option<&str> {
    let trimmed = answer.trim();
    let Some(opened) = trimmed.strip_prefix("```") else {
        return Some(trimmed);
    };
    let (info, body) = opened.split_once('\n')?;
    if !matches!(info.trim(), "" | "json") {
        return None;
    }
    body.trim_end().strip_suffix("```").map(str::trim)
}

impl SourceFacts {
    fn answer_headers(&self, name: &str, column: &FactColumn) -> BTreeSet<String> {
        std::iter::once(&column.header)
            .chain(&column.answer_headers)
            .map(|header| header_key(header))
            .chain(std::iter::once(header_key(name)))
            .collect()
    }

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
        let mut source_headers = BTreeSet::new();
        let mut answer_headers = BTreeSet::new();
        for (name, column) in &self.columns {
            if !claims::valid_name(name) {
                return Err(format!("invalid source fact column name {name:?}"));
            }
            if column.header.chars().any(unsupported_control)
                || !source_headers.insert(header_key(&column.header))
                || header_key(&column.header).is_empty()
            {
                return Err("source fact columns require distinct readable source headers".into());
            }
            if column.short_names && column.kind != FactKind::Name {
                return Err("short names apply only to name columns".into());
            }
            for header in self.answer_headers(name, column) {
                if header.is_empty() || !answer_headers.insert(header) {
                    return Err(
                        "answer headers must be nonempty and identify exactly one column".into(),
                    );
                }
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
        for (name, values) in &self.aliases {
            let mut group = BTreeSet::new();
            for alias in std::iter::once(name).chain(values) {
                let text = normalized(alias);
                if text.is_empty()
                    || text.contains(',')
                    || text.contains("${")
                    || alias.chars().any(unsupported_control)
                    || !group.insert(text)
                {
                    return Err(
                        "aliases must be distinct nonempty comma-free reviewed equivalents".into(),
                    );
                }
            }
            if group.len() < 2 {
                return Err("an alias group requires at least one equivalent".into());
            }
        }
        let mut seen = BTreeSet::new();
        for (accepted, template) in self
            .claims
            .iter()
            .map(|template| (true, template))
            .chain(self.neutral_claims.iter().map(|template| (true, template)))
            .chain(self.rejected.iter().map(|template| (false, template)))
        {
            if template.chars().any(unsupported_control) || !seen.insert(normalized(template)) {
                return Err("claim forms must be distinct readable whole-text examples".into());
            }
            let parts = claims::parse(template)?;
            if parts.is_empty() {
                return Err("claim forms must not be empty".into());
            }
            let used: BTreeSet<_> = claims::slots(&parts).collect();
            if used.iter().any(|slot| !self.columns.contains_key(*slot)) {
                return Err("claim forms may only reference declared source fact columns".into());
            }
            if accepted
                && self
                    .columns
                    .iter()
                    .any(|(name, column)| column.required && !used.contains(name.as_str()))
            {
                return Err(
                    "accepted claim forms must state the key and every required source fact".into(),
                );
            }
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
        let aliases = Aliases::new(&self.aliases);
        let wanted: BTreeMap<&str, String> = self
            .columns
            .iter()
            .map(|(name, column)| (name.as_str(), header_key(&column.header)))
            .collect();
        let markdown = tables::markdown(document);
        let candidates: Vec<&Grid> = markdown
            .tables
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
        let mut raw = Vec::new();
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
                let fact =
                    Fact::parse(column.kind, &cells[position], &aliases).ok_or_else(|| {
                        RecordCheckError::MissingEvidence(format!(
                            "source {name} cell is not a readable {:?} fact",
                            column.kind
                        ))
                    })?;
                facts.push((name.clone(), fact));
            }
            rows.push(facts);
            raw.push(
                keys.iter()
                    .zip(cells)
                    .map(|(key, cell)| (key.clone(), normalized(strip_markup(cell))))
                    .collect(),
            );
        }
        if rows.is_empty() {
            return Err(RecordCheckError::MissingEvidence(
                "source table has no rows".into(),
            ));
        }
        let mut source = Source {
            rows,
            raw,
            targets: BTreeSet::new(),
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

    fn subject<'source>(&self, source: &'source Source, row: usize) -> &'source str {
        &source.fact(row, &self.key).names()[0]
    }

    fn heading(&self, header: &str, source: &Source) -> Heading<'_> {
        let key = header_key(header);
        if let Some((name, _)) = self
            .columns
            .iter()
            .find(|(name, column)| self.answer_headers(name, column).contains(&key))
        {
            return Heading::Fact(name);
        }
        if !key.is_empty() && source.raw[0].contains_key(&key) {
            return Heading::Extra(key);
        }
        Heading::Unknown
    }

    fn identify(&self, cell: &str, source: &Source) -> Vec<usize> {
        (0..source.rows.len())
            .filter(|row| source.fact(*row, &self.key).cell(cell) == CellMatch::Exact)
            .collect()
    }

    fn fact_cell(
        &self,
        name: &str,
        cell: &str,
        row: usize,
        source: &Source,
        verdict: &mut Verdict,
    ) {
        if strip_markup(cell).is_empty() {
            if self.columns[name].required {
                verdict.fail("answer table leaves a required source fact empty");
            }
            return;
        }
        let outcome = source.fact(row, name).cell(cell);
        if outcome == CellMatch::Exact {
            return;
        }
        let subject = self.subject(source, row);
        if (0..source.rows.len())
            .any(|other| other != row && source.fact(other, name).cell(cell) == CellMatch::Exact)
        {
            verdict.fail(format!(
                "answer attributes another source row's {name} to {subject}"
            ));
        } else if outcome == CellMatch::Contradicted {
            verdict.fail(format!("answer contradicts the source {name} of {subject}"));
        } else {
            verdict.review(format!(
                "answer presents the {name} of {subject} in an unreviewed form"
            ));
        }
    }

    fn table(
        &self,
        grid: &Grid,
        source: &Source,
        transposable: bool,
        prose: bool,
        verdict: &mut Verdict,
    ) {
        let identifies = |cell: &str| self.identify(cell, source).len() == 1;
        let mut grid = grid.clone();
        let mut key_column = grid.headers.iter().position(|header| {
            matches!(self.heading(header, source), Heading::Fact(name) if name == self.key)
        });
        if key_column.is_none()
            && grid
                .headers
                .first()
                .is_some_and(|header| header_key(header).is_empty())
        {
            if !grid.rows.is_empty()
                && grid
                    .rows
                    .iter()
                    .all(|row| row.first().is_some_and(|cell| identifies(cell)))
            {
                key_column = Some(0);
            } else if transposable
                && grid.headers.len() > 1
                && grid.headers[1..].iter().all(|cell| identifies(cell))
            {
                grid = tables::transposed(&grid);
                key_column = Some(0);
            }
        }
        let Some(key_column) = key_column else {
            verdict.review("answer table has no recognizable source subject column");
            return;
        };
        let headings: Vec<Heading> = grid
            .headers
            .iter()
            .map(|header| self.heading(header, source))
            .collect();
        let mut stated = BTreeSet::new();
        let unknown_content = headings.iter().enumerate().any(|(index, heading)| {
            matches!(heading, Heading::Unknown)
                && grid.rows.iter().any(|row| {
                    row.get(index)
                        .is_some_and(|cell| !strip_markup(cell).is_empty())
                })
        });
        for (index, heading) in headings.iter().enumerate() {
            if let Heading::Fact(name) = heading
                && index != key_column
                && !stated.insert(*name)
            {
                verdict.review("answer table states one source fact in several columns");
            }
        }
        for (name, column) in &self.columns {
            if column.required && name != &self.key && !stated.contains(name.as_str()) {
                if prose || unknown_content {
                    verdict.review(format!("answer table does not state the source {name}"));
                } else {
                    verdict.fail(format!("answer table omits the source {name}"));
                }
            }
        }
        let mut seen = BTreeSet::new();
        let mut unidentified = false;
        for cells in &grid.rows {
            if cells.len() != grid.headers.len() {
                verdict.review("answer table contains a ragged row");
                unidentified = true;
                continue;
            }
            let [row] = self.identify(&cells[key_column], source)[..] else {
                verdict.review("answer table row does not identify exactly one source row");
                unidentified = true;
                continue;
            };
            if !seen.insert(row) {
                verdict.fail("answer table repeats a source row");
            }
            for (index, heading) in headings.iter().enumerate() {
                let cell = &cells[index];
                if index == key_column {
                    continue;
                }
                match heading {
                    Heading::Fact(name) => {
                        let supplied_elsewhere = strip_markup(cell).is_empty()
                            && headings.iter().enumerate().any(|(other_index, other)| {
                                other_index != index
                                    && matches!(other, Heading::Fact(other_name) if other_name == name)
                                    && !strip_markup(&cells[other_index]).is_empty()
                            });
                        if supplied_elsewhere {
                            verdict
                                .review("answer distributes one fact across alternative headers");
                        } else {
                            self.fact_cell(name, cell, row, source, verdict);
                        }
                    }
                    Heading::Extra(key) => {
                        if !strip_markup(cell).is_empty()
                            && normalized(strip_markup(cell)) != source.raw[row][key]
                        {
                            verdict
                                .review("answer table restates other source details differently");
                        }
                    }
                    Heading::Unknown => {
                        if !strip_markup(cell).is_empty() {
                            verdict.review("answer table contains an unreviewed column");
                        }
                    }
                }
            }
        }
        if !source.targets.is_subset(&seen) {
            if unidentified || prose {
                verdict.review("answer table may omit a requested source row");
            } else {
                verdict.fail("answer table omits a requested source row");
            }
        }
        if !seen.is_subset(&source.targets) {
            verdict.review("answer table lists source rows beyond the requested facts");
        }
    }

    fn classify(
        &self,
        text: &str,
        forms: &[(Form, Vec<Part>)],
        matcher: &mut Matcher,
        targets: &BTreeSet<usize>,
    ) -> Result<Option<Claim>, RecordCheckError> {
        let mut negative = false;
        let mut target = None;
        let mut other = false;
        let mut mixed = false;
        let mut context = None;
        let mut unreviewed_rejection = false;
        for (form, parts) in forms {
            if *form == Form::Rejected
                && matches!(parts.last(), Some(Part::Literal(literal)) if literal.ends_with('.'))
            {
                for (index, _) in text.match_indices(". ") {
                    for binding in matcher.bindings(parts, &text[..index + 1])? {
                        if binding.iter().all(|row| *row == binding[0])
                            && targets.contains(&binding[0])
                        {
                            negative = true;
                        }
                    }
                }
            }
            for binding in matcher.bindings(parts, text)? {
                let first = binding[0];
                let same_row = binding.iter().all(|row| *row == first);
                if *form == Form::Rejected {
                    if same_row && targets.contains(&first) {
                        negative = true;
                    } else {
                        unreviewed_rejection = true;
                    }
                    continue;
                }
                if same_row {
                    if targets.contains(&first) {
                        target.get_or_insert(first);
                    } else if *form == Form::Neutral {
                        context.get_or_insert(first);
                    } else {
                        other = true;
                    }
                } else {
                    mixed = true;
                }
            }
        }
        Ok(match target {
            Some(_) if negative => Some(Claim::Conflict),
            Some(_) if unreviewed_rejection || context.is_some() => Some(Claim::Review(
                "answer combines requested facts with unreviewed context",
            )),
            Some(row) => Some(Claim::Row(row)),
            None if negative => Some(Claim::Wrong(
                "answer matches a reviewed incorrect claim form",
            )),
            None if other => Some(Claim::Wrong(
                "reviewed claim form names a source row other than the requested one",
            )),
            None if mixed => Some(Claim::Wrong(
                "reviewed claim form attributes another source row's fact",
            )),
            None if unreviewed_rejection => Some(Claim::Review(
                "negated claim does not bind one requested source row",
            )),
            None if context.is_some() => context.map(Claim::Context),
            None => None,
        })
    }

    fn unbound_claim(
        &self,
        text: &str,
        forms: &[(Form, Vec<Part>)],
        matcher: &mut Matcher,
        source: &Source,
    ) -> Result<Option<Claim>, RecordCheckError> {
        let mut wrong = false;
        let mut uncertain = false;
        for (form, parts) in forms {
            if *form == Form::Rejected {
                continue;
            }
            for captured in matcher.captures(parts, text)? {
                let keys: Vec<&str> = captured
                    .iter()
                    .filter(|(name, _)| name == &self.key)
                    .map(|(_, value)| value.as_str())
                    .collect();
                let identified: Vec<Vec<usize>> =
                    keys.iter().map(|key| self.identify(key, source)).collect();
                let Some(&row) = identified.first().and_then(|rows| rows.first()) else {
                    if keys.iter().all(|key| {
                        key.split_whitespace().count() <= 4
                            && key.chars().all(|character| {
                                character.is_alphabetic() || matches!(character, ' ' | '-' | '&')
                            })
                    }) {
                        wrong = true;
                    }
                    continue;
                };
                let same_subject = identified.iter().all(|rows| rows.as_slice() == [row]);
                let mut contradiction =
                    !same_subject || (*form == Form::Selected && !source.targets.contains(&row));
                let mut unknown = false;
                for (name, value) in &captured {
                    match source.fact(row, name).cell(value) {
                        CellMatch::Exact => {}
                        CellMatch::Contradicted => contradiction = true,
                        CellMatch::Partial | CellMatch::Unknown => {
                            if (0..source.rows.len()).any(|other| {
                                other != row
                                    && source.fact(other, name).cell(value) == CellMatch::Exact
                            }) {
                                contradiction = true;
                            } else {
                                unknown = true;
                            }
                        }
                    }
                }
                wrong |= contradiction && !unknown;
                uncertain |= unknown || !contradiction;
            }
        }
        Ok((wrong && !uncertain).then_some(Claim::Wrong(
            "reviewed claim form contradicts or misattributes a source fact",
        )))
    }

    fn claims(
        &self,
        answer: &str,
        source: &Source,
        verdict: &mut Verdict,
    ) -> Result<(), RecordCheckError> {
        let forms: Vec<(Form, Vec<Part>)> = self
            .claims
            .iter()
            .map(|template| (Form::Selected, template))
            .chain(
                self.neutral_claims
                    .iter()
                    .map(|template| (Form::Neutral, template)),
            )
            .chain(
                self.rejected
                    .iter()
                    .map(|template| (Form::Rejected, template)),
            )
            .map(|(form, template)| {
                claims::parse(template)
                    .map(|parts| (form, parts))
                    .map_err(RecordCheckError::InvalidContract)
            })
            .collect::<Result<_, _>>()?;
        let mut matcher = Matcher {
            rows: &source.rows,
            budget: CLAIM_SEARCH_BUDGET,
        };
        let whole = normalized(answer);
        let whole_claim = match self.classify(&whole, &forms, &mut matcher, &source.targets)? {
            Some(claim) => Some(claim),
            None if answer.lines().count() <= 1 => {
                self.unbound_claim(&whole, &forms, &mut matcher, source)?
            }
            None => None,
        };
        let segments: Vec<Claim> = if let Some(claim) = whole_claim {
            vec![claim]
        } else {
            let mut found = Vec::new();
            for line in answer
                .lines()
                .map(strip_marker)
                .filter(|line| !line.is_empty())
            {
                let text = normalized(line);
                if text == whole {
                    found.clear();
                    break;
                }
                let claim = match self.classify(&text, &forms, &mut matcher, &source.targets)? {
                    Some(claim) => Some(claim),
                    None => self.unbound_claim(&text, &forms, &mut matcher, source)?,
                };
                match claim {
                    Some(claim) => found.push(claim),
                    None => {
                        verdict.review("answer wording is not a reviewed claim form");
                    }
                }
            }
            if found.is_empty() {
                verdict.review("answer wording is not a reviewed claim form");
            }
            found
        };
        let mut claimed = BTreeSet::new();
        let mut context = BTreeSet::new();
        for claim in segments {
            match claim {
                Claim::Row(row) => {
                    claimed.insert(row);
                }
                Claim::Context(row) => {
                    context.insert(row);
                }
                Claim::Review(reason) => verdict.review(reason),
                Claim::Wrong(reason) => verdict.fail(reason),
                Claim::Conflict => {
                    verdict.review("answer matches both reviewed correct and incorrect claim forms")
                }
            }
        }
        if !context.is_empty() && !claimed.is_empty() {
            verdict.review("answer includes neutral facts about additional source rows");
        }
        if verdict.review.is_none() && claimed != source.targets {
            verdict.fail("reviewed claims do not cover exactly the requested source rows");
        }
        Ok(())
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
        let mut verdict = Verdict::default();
        let Some(structured_text) = fenced(answer) else {
            return Err(RecordCheckError::NeedsAdjudication(
                "answer has unreviewed or incomplete fence metadata".into(),
            ));
        };
        match tables::json(structured_text) {
            Ok(value @ (Value::Array(_) | Value::Object(_))) => match tables::structured(&value) {
                Some(grid) => self.table(&grid, &source, false, false, &mut verdict),
                None => verdict.review("unrecognized structured answer shape"),
            },
            _ => {
                let markdown = tables::markdown(answer);
                match markdown.tables.as_slice() {
                    [] => self.claims(answer, &source, &mut verdict)?,
                    [grid] => {
                        self.table(grid, &source, true, markdown.prose, &mut verdict);
                        if markdown.prose {
                            verdict.review("answer text outside the table is unreviewed");
                        }
                    }
                    _ => verdict.review("answer contains several tables"),
                }
            }
        }
        verdict.finish(&self.requirement)
    }
}
