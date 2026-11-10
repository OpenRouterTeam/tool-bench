//! Answer grading bound to the current assignees of one on-call schedule.
//!
//! The current assignees come from the selected schedule's `on_call_now`
//! and the directory collection; the reply is read as ordinary prose
//! (`prose`) or as a structured user list.
//!
//! - **Pass**: every current assignee is named (username, email, full name
//!   or an unambiguous surname) and not denied, and nobody else is presented
//!   as on call in their place.  Other people may appear as context: after
//!   the answer, set aside, or on the other schedule they really cover.
//! - **Fail**: an assignee is missing or denied, or the reply names another
//!   directory person first.
//! - **Unresolved**: the reply cannot be decided: a name shared by several
//!   people, a first name only, a statement both made and disputed, or an
//!   extra person joined to the assignee with no separate plain statement.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::prose::{self, Evidence, Stance};
use crate::relations::{RecordCheckError, RecordSelector};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OncallAnswer {
    pub requirement: String,
    pub schedule: RecordSelector,
    pub directory: String,
    #[serde(default)]
    pub allow_unambiguous_surnames: bool,
}

struct Person {
    record: Value,
    name: String,
}

struct Facts {
    schedule: Value,
    people: BTreeMap<String, Person>,
    current: BTreeSet<String>,
    /// Every way a directory person may be written, longest first, with the
    /// people it identifies.
    forms: Vec<(String, BTreeSet<String>)>,
    /// Other schedules of the same collection: the words that tell each one
    /// apart from the selected schedule, and who is on call there.
    others: Vec<(Vec<String>, BTreeSet<String>)>,
}

struct Mention {
    people: BTreeSet<String>,
    start: usize,
    end: usize,
}

/// A statement that nobody is on call.
static EMPTY_DUTY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\b(?:nobody|no one|no-one|none|not anyone|n't anyone|empty|unassigned|unstaffed",
        r"|no (?:\w+ ){0,3}(?:users?|person|people|engineers?|assignees?|responders?|on-call|oncall|on call))\b"
    ))
    .expect("empty duty pattern")
});
/// Text that only joins two names into one group.
static COORDINATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\s*\([^)]*\))?\s*(?:,|&|/|\+|and|or|plus|as well as|along with|together with|, and)\s*$")
        .expect("coordination pattern")
});

fn identity(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(format!("string:{value}")),
        Value::Number(value) if value.is_i64() || value.is_u64() => {
            Some(format!("integer:{value}"))
        }
        _ => None,
    }
}

fn words(text: &str) -> Vec<String> {
    prose::fold(text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

fn presentations(record: &Value, surnames: bool) -> Vec<String> {
    let value = |field: &str| {
        record[field].as_str().filter(|text| {
            !text.trim().is_empty() && !text.chars().any(crate::rich_text::unsupported_control)
        })
    };
    let mut values: Vec<String> = ["username", "email", "name"]
        .iter()
        .filter_map(|field| value(field).map(prose::fold))
        .collect();
    if surnames && let Some(name) = value("name") {
        let words: Vec<_> = name.split_whitespace().collect();
        if words.len() > 1 {
            values.push(prose::fold(words.last().expect("a multiword name")));
        }
    }
    values.retain(|value| !value.is_empty());
    values
}

impl OncallAnswer {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.requirement.trim().is_empty() {
            return Err("on-call answers require a nonempty criterion".into());
        }
        self.schedule.validate()?;
        let directory = crate::json_pointer::tokens(&self.directory)?;
        if directory.len() != 3
            || directory[0] != "services"
            || directory
                .iter()
                .any(|part| part.is_empty() || part.contains('*'))
        {
            return Err("on-call answers require one literal business directory collection".into());
        }
        Ok(())
    }

    fn facts(&self, initial: Option<&Value>, state: &Value) -> Result<Facts, RecordCheckError> {
        let schedules = self.schedule.select(initial, state)?;
        let [(_, schedule)] = schedules.as_slice() else {
            return Err(RecordCheckError::MissingEvidence(
                "on-call answer requires one authoritative selected schedule".into(),
            ));
        };
        let duty = |schedule: &Value| -> Result<BTreeSet<String>, RecordCheckError> {
            let observed = schedule["on_call_now"].as_array().ok_or_else(|| {
                RecordCheckError::MissingEvidence(
                    "selected schedule has no authoritative current-duty observation".into(),
                )
            })?;
            let mut current = BTreeSet::new();
            for value in observed {
                let person = identity(value).ok_or_else(|| {
                    RecordCheckError::MissingEvidence(
                        "current-duty observation contains an invalid identity".into(),
                    )
                })?;
                if !current.insert(person) {
                    return Err(RecordCheckError::MissingEvidence(
                        "current-duty observation contains duplicate identities".into(),
                    ));
                }
            }
            Ok(current)
        };
        let current = duty(schedule)?;
        if current.len() > 32 {
            return Err(RecordCheckError::NeedsAdjudication(
                "current-duty group exceeds bounded answer matching".into(),
            ));
        }
        let mut people = BTreeMap::new();
        let mut owners = BTreeMap::<String, BTreeSet<String>>::new();
        let directory = state
            .pointer(&self.directory)
            .and_then(Value::as_object)
            .ok_or_else(|| {
                RecordCheckError::MissingEvidence(
                    "authoritative on-call directory collection is missing".into(),
                )
            })?;
        for record in directory.values() {
            let person = identity(&record["id"]).ok_or_else(|| {
                RecordCheckError::MissingEvidence(
                    "on-call directory record has no stable typed identity".into(),
                )
            })?;
            for form in presentations(record, self.allow_unambiguous_surnames) {
                owners.entry(form).or_default().insert(person.clone());
            }
            let name = ["name", "username", "email"]
                .iter()
                .find_map(|field| record[*field].as_str())
                .unwrap_or_default()
                .to_owned();
            let record = record.clone();
            if people.insert(person, Person { record, name }).is_some() {
                return Err(RecordCheckError::MissingEvidence(
                    "on-call directory contains duplicate identities".into(),
                ));
            }
        }
        if current.iter().any(|person| !people.contains_key(person)) {
            return Err(RecordCheckError::MissingEvidence(
                "current-duty observation references a missing directory person".into(),
            ));
        }
        let mut forms: Vec<_> = owners.into_iter().collect();
        forms.sort_by_key(|(form, _)| std::cmp::Reverse(form.len()));
        let selected = words(schedule["name"].as_str().unwrap_or_default());
        let others = state
            .pointer(&self.schedule.collection)
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|records| records.values())
            .filter(|other| other["id"] != schedule["id"])
            .filter_map(|other| {
                let mut marks: Vec<String> = words(other["name"].as_str().unwrap_or_default())
                    .into_iter()
                    .filter(|word| !selected.contains(word))
                    .collect();
                marks.extend(other["id"].as_str().map(prose::fold));
                Some((marks, duty(other).ok()?))
            })
            .collect();
        Ok(Facts {
            schedule: (*schedule).clone(),
            people,
            current,
            forms,
            others,
        })
    }

    /// A user list in the native result shape, or `None` when the answer is
    /// not one.
    fn structured(&self, value: &Value, facts: &Facts) -> Option<Result<(), RecordCheckError>> {
        let rows = match value {
            Value::Array(rows) => rows,
            Value::Object(object) => object.get("users")?.as_array()?,
            _ => return None,
        };
        if !rows.iter().all(Value::is_object) {
            return None;
        }
        let failed = |reason: &str| Some(Err(RecordCheckError::Failed(reason.into())));
        if let Value::Object(object) = value {
            for (field, source) in [("scheduleId", "id"), ("scheduleName", "name")] {
                if object
                    .get(field)
                    .is_some_and(|value| value != &facts.schedule[source])
                {
                    return failed("structured answer identifies another schedule");
                }
            }
        }
        let mut actual = BTreeSet::new();
        for row in rows {
            let fields = row.as_object().expect("rows were checked to be objects");
            let candidates: Vec<_> = if let Some(value) = fields.get("id") {
                identity(value)
                    .and_then(|person| facts.people.get_key_value(&person))
                    .into_iter()
                    .collect()
            } else {
                let identifying: Vec<_> = ["username", "email", "name"]
                    .into_iter()
                    .filter_map(|field| fields.get(field).map(|value| (field, value)))
                    .collect();
                if identifying.is_empty() {
                    return failed("structured person has no identity fields");
                }
                facts
                    .people
                    .iter()
                    .filter(|(_, person)| {
                        identifying
                            .iter()
                            .all(|(field, value)| person.record[*field] == **value)
                    })
                    .collect()
            };
            if candidates.is_empty() {
                return failed(
                    "structured answer names a person outside the authoritative directory",
                );
            }
            let [(person_id, person)] = candidates.as_slice() else {
                return Some(Err(RecordCheckError::NeedsAdjudication(
                    "structured person identity is ambiguous".into(),
                )));
            };
            if fields.iter().any(|(field, value)| {
                person
                    .record
                    .get(field)
                    .is_some_and(|expected| expected != value)
            }) {
                return failed("structured answer contradicts a person record");
            }
            if !actual.insert((*person_id).clone()) {
                return failed("structured on-call answer duplicates a person");
            }
        }
        if actual != facts.current {
            return failed(
                "structured on-call answer does not identify exactly the current primary assignees",
            );
        }
        Some(Ok(()))
    }

    fn mentions(text: &str, facts: &Facts) -> Vec<Mention> {
        let mut found: Vec<Mention> = Vec::new();
        for (form, people) in &facts.forms {
            for (start, end) in prose::find(text, form) {
                if !found
                    .iter()
                    .any(|other| other.start < end && start < other.end)
                {
                    found.push(Mention {
                        people: people.clone(),
                        start,
                        end,
                    });
                }
            }
        }
        found.sort_by_key(|mention| mention.start);
        found
    }

    fn prose(&self, answer: &str, facts: &Facts) -> Result<(), RecordCheckError> {
        let folded: Vec<String> = answer.lines().map(prose::fold).collect();
        let segments: Vec<&str> = folded
            .iter()
            .flat_map(|line| prose::sentences(line))
            .collect();
        let labels: Vec<String> = ["call", "on-call", "oncall"].map(String::from).into();
        let mut evidence: BTreeMap<&String, Evidence> = BTreeMap::new();
        let mut ambiguous = false;
        // Segment of the first affirmation, of each competing mention, and of
        // each plain statement of assignees alone.
        let mut first = None;
        let mut rivals = Vec::new();
        let (mut joined, mut plain) = (false, false);
        for (index, segment) in segments.iter().enumerate() {
            let mentions = Self::mentions(segment, facts);
            let stances: Vec<Stance> = mentions
                .iter()
                .enumerate()
                .map(|(position, mention)| {
                    let scope = mentions[position + 1..]
                        .iter()
                        .find(|next| next.people != mention.people)
                        .map_or(segment.len(), |next| next.start);
                    prose::stance(segment, mention.start, mention.end, scope, &labels)
                })
                .collect();
            let assignee =
                |mention: &Mention| mention.people.iter().all(|id| facts.current.contains(id));
            let (mut named, mut extra) = (false, false);
            for (position, (mention, stance)) in mentions.iter().zip(&stances).enumerate() {
                let only = (mention.people.len() == 1)
                    .then(|| mention.people.first())
                    .flatten();
                if let Some(person) = only.and_then(|id| facts.current.get(id)) {
                    evidence.entry(person).or_default().count(*stance);
                    if *stance == Stance::Affirmed {
                        first.get_or_insert(index);
                        named = true;
                    }
                    continue;
                }
                if *stance != Stance::Affirmed {
                    continue;
                }
                if mention.people.iter().any(|id| facts.current.contains(id)) {
                    ambiguous = true;
                    continue;
                }
                let elsewhere = facts.others.iter().any(|(marks, duty)| {
                    mention.people.iter().all(|id| duty.contains(id))
                        && marks
                            .iter()
                            .any(|mark| !prose::find(segment, mark).is_empty())
                });
                if elsewhere || prose::set_aside(segment) {
                    continue;
                }
                extra = true;
                rivals.push(index);
                let neighbour = |other: usize, between: &str| {
                    stances[other] == Stance::Affirmed
                        && assignee(&mentions[other])
                        && COORDINATION.is_match(between)
                };
                joined |= (position > 0
                    && neighbour(
                        position - 1,
                        &segment[mentions[position - 1].end..mention.start],
                    ))
                    || (position + 1 < mentions.len()
                        && neighbour(
                            position + 1,
                            &segment[mention.end..mentions[position + 1].start],
                        ));
            }
            plain |= named && !extra;
        }
        let failed = |reason: String| {
            Err(RecordCheckError::Failed(format!(
                "{reason}; requirement: {}",
                self.requirement
            )))
        };
        let review = |reason: String| {
            Err(RecordCheckError::NeedsAdjudication(format!(
                "{reason}; adjudicate against: {}",
                self.requirement
            )))
        };
        if facts.current.is_empty() {
            return if !rivals.is_empty() {
                failed("answer names an assignee although nobody is on call".into())
            } else if folded.iter().any(|line| EMPTY_DUTY.is_match(line)) {
                Ok(())
            } else {
                review("answer does not say whether anyone is on call".into())
            };
        }
        let whole = folded.join(" ");
        let mut pending = None;
        for person in &facts.current {
            let name = &facts.people[person].name;
            let seen = evidence.get(person).copied().unwrap_or_default();
            if seen.affirmed == 0 {
                if seen.denied > 0 {
                    return failed(format!("answer denies that {name} is on call"));
                }
                let partial = words(name)
                    .iter()
                    .any(|word| word.len() >= 3 && !prose::find(&whole, word).is_empty());
                pending.get_or_insert(if seen.disputed > 0 {
                    review(format!("answer mentions {name} only in a negative remark"))
                } else if ambiguous || partial {
                    review(format!(
                        "answer may identify {name} by a partial or shared name"
                    ))
                } else {
                    failed("answer does not name a current assignee".into())
                });
            } else if seen.denied + seen.disputed > 0 {
                pending.get_or_insert(review(format!(
                    "answer both states and disputes that {name} is on call"
                )));
            }
        }
        if matches!(pending, Some(Err(RecordCheckError::Failed(_)))) {
            return pending.expect("a pending failure");
        }
        if first.is_some_and(|first| rivals.iter().any(|rival| *rival < first)) {
            return failed("answer names another person before the current assignee".into());
        }
        if let Some(verdict) = pending {
            return verdict;
        }
        if joined && !plain {
            return review("answer joins another person to the current assignees".into());
        }
        Ok(())
    }

    pub(crate) fn check(
        &self,
        answer: &str,
        initial: Option<&Value>,
        state: &Value,
    ) -> Result<(), RecordCheckError> {
        self.validate().map_err(RecordCheckError::InvalidContract)?;
        if answer.trim().is_empty() {
            return Err(RecordCheckError::Failed(
                "required on-call answer is empty".into(),
            ));
        }
        if answer.chars().any(crate::rich_text::unsupported_control) {
            return Err(RecordCheckError::NeedsAdjudication(
                "answer contains directional or invisible controls".into(),
            ));
        }
        let facts = self.facts(initial, state)?;
        if let Ok(value) = serde_json::from_str::<Value>(answer)
            && let Some(verdict) = self.structured(&value, &facts)
        {
            return verdict;
        }
        self.prose(answer, &facts)
    }
}
