use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::relations::{RecordCheckError, RecordSelector, ReviewedText, ReviewedTextFormat};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OncallAnswer {
    pub schedule: RecordSelector,
    pub directory: String,
    pub content: ReviewedText,
    #[serde(default)]
    pub allow_unambiguous_surnames: bool,
    #[serde(default)]
    pub empty_answers: Vec<String>,
}

struct Person {
    record: Value,
    presentations: Vec<String>,
}

struct Facts {
    schedule: Value,
    people: BTreeMap<String, Person>,
    current: BTreeSet<String>,
}

fn normalized(source: &str) -> String {
    source
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn identity(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(format!("string:{value}")),
        Value::Number(value) if value.is_i64() || value.is_u64() => {
            Some(format!("integer:{value}"))
        }
        _ => None,
    }
}

fn presentations(record: &Value, surnames: bool) -> Vec<String> {
    let value = |field: &str| {
        record[field].as_str().filter(|text| {
            !text.trim().is_empty() && !text.chars().any(crate::rich_text::unsupported_control)
        })
    };
    let mut values: Vec<_> = ["username", "email", "name"]
        .iter()
        .filter_map(|field| value(field).map(str::to_owned))
        .collect();
    for (left, right) in [
        ("username", "email"),
        ("name", "email"),
        ("name", "username"),
    ] {
        if let (Some(left), Some(right)) = (value(left), value(right)) {
            values.push(format!("{left} ({right})"));
        }
    }
    if surnames && let Some(name) = value("name") {
        let words: Vec<_> = name.split_whitespace().collect();
        if words.len() > 1 {
            values.push(words.last().expect("a multiword name").to_string());
        }
    }
    values
        .into_iter()
        .map(|value| normalized(&value))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn match_people(
    source: &str,
    remaining: &BTreeSet<String>,
    people: &BTreeMap<String, Person>,
    budget: &mut usize,
) -> Result<bool, RecordCheckError> {
    if remaining.is_empty() {
        return Ok(source.is_empty());
    }
    if *budget == 0 {
        return Err(RecordCheckError::NeedsAdjudication(
            "on-call presentation matching exceeded its bounded search".into(),
        ));
    }
    *budget -= 1;
    for person in remaining {
        for presentation in &people[person].presentations {
            if let Some(tail) = source.strip_prefix(presentation) {
                let mut pending = remaining.clone();
                pending.remove(person);
                if pending.is_empty() && tail.is_empty() {
                    return Ok(true);
                }
                for separator in [", ", "; ", " and "] {
                    if let Some(tail) = tail.strip_prefix(separator)
                        && match_people(tail, &pending, people, budget)?
                    {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

impl OncallAnswer {
    pub(crate) fn validate(&self) -> Result<(), String> {
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
        self.content.validate()?;
        if self.content.optional
            || self.content.bound.is_some()
            || self.content.format != ReviewedTextFormat::Plain
            || self
                .content
                .accepted
                .iter()
                .chain(&self.content.rejected)
                .any(|example| {
                    example.matches("${oncall}").count() != 1
                        || example.replace("${oncall}", "").contains("${")
                })
            || self
                .empty_answers
                .iter()
                .enumerate()
                .any(|(index, answer)| {
                    answer.trim().is_empty()
                        || answer.contains("${")
                        || answer.chars().any(crate::rich_text::unsupported_control)
                        || self.empty_answers[..index]
                            .iter()
                            .any(|previous| normalized(previous) == normalized(answer))
                })
        {
            return Err("on-call answers require plain whole-text examples with one ${oncall} placeholder and distinct explicit empty-duty answers".into());
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
            let variants = presentations(record, self.allow_unambiguous_surnames);
            for variant in &variants {
                owners
                    .entry(variant.clone())
                    .or_default()
                    .insert(person.clone());
            }
            if people
                .insert(
                    person,
                    Person {
                        record: record.clone(),
                        presentations: variants,
                    },
                )
                .is_some()
            {
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
        for person in people.values_mut() {
            person
                .presentations
                .retain(|variant| owners[variant].len() == 1);
            person
                .presentations
                .sort_by_key(|variant| std::cmp::Reverse(variant.len()));
        }
        Ok(Facts {
            schedule: (*schedule).clone(),
            people,
            current,
        })
    }

    fn structured(&self, value: &Value, facts: &Facts) -> Result<(), RecordCheckError> {
        let mut unreviewed_metadata = false;
        let rows = match value {
            Value::Array(rows) => rows,
            Value::Object(object) => {
                unreviewed_metadata = object.keys().any(|field| {
                    !matches!(field.as_str(), "users" | "scheduleId" | "scheduleName")
                });
                for (field, source) in [("scheduleId", "id"), ("scheduleName", "name")] {
                    if object
                        .get(field)
                        .is_some_and(|value| value != &facts.schedule[source])
                    {
                        return Err(RecordCheckError::Failed(
                            "structured answer identifies another schedule".into(),
                        ));
                    }
                }
                object
                    .get("users")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        RecordCheckError::Failed(
                            "structured on-call answer must contain a users array".into(),
                        )
                    })?
            }
            _ => {
                return Err(RecordCheckError::NeedsAdjudication(
                    "unrecognized structured on-call presentation".into(),
                ));
            }
        };
        let mut actual = BTreeSet::new();
        for row in rows {
            let fields = row.as_object().ok_or_else(|| {
                RecordCheckError::Failed("structured on-call users must be person records".into())
            })?;
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
                    return Err(RecordCheckError::Failed(
                        "structured person has no identity fields".into(),
                    ));
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
                return Err(RecordCheckError::Failed(
                    "structured answer names a person outside the authoritative directory".into(),
                ));
            }
            let [(person_id, person)] = candidates.as_slice() else {
                return Err(RecordCheckError::NeedsAdjudication(
                    "structured person identity is ambiguous".into(),
                ));
            };
            for (field, value) in fields {
                if let Some(expected) = person.record.get(field) {
                    if expected != value {
                        return Err(RecordCheckError::Failed(
                            "structured answer contradicts a person record".into(),
                        ));
                    }
                } else {
                    unreviewed_metadata = true;
                }
            }
            if !actual.insert((*person_id).clone()) {
                return Err(RecordCheckError::Failed(
                    "structured on-call answer duplicates a person".into(),
                ));
            }
        }
        if actual != facts.current {
            return Err(RecordCheckError::Failed(
                "structured on-call answer does not identify exactly the current primary assignees"
                    .into(),
            ));
        }
        if unreviewed_metadata {
            return Err(RecordCheckError::NeedsAdjudication(
                "unreviewed structured answer metadata requires adjudication".into(),
            ));
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
        if let Ok(value @ (Value::Array(_) | Value::Object(_))) =
            serde_json::from_str::<Value>(answer)
        {
            return self.structured(&value, &facts);
        }
        let candidate = normalized(answer);
        if facts.current.is_empty() {
            return if self
                .empty_answers
                .iter()
                .any(|answer| normalized(answer) == candidate)
            {
                Ok(())
            } else {
                Err(RecordCheckError::NeedsAdjudication(
                    "empty-duty answer wording requires review".into(),
                ))
            };
        }
        for (accepted, examples) in [
            (true, &self.content.accepted),
            (false, &self.content.rejected),
        ] {
            for example in examples {
                let example = normalized(example);
                let (before, after) = example
                    .split_once("${oncall}")
                    .expect("validated placeholder");
                if let Some(middle) = candidate
                    .strip_prefix(before)
                    .and_then(|text| text.strip_suffix(after))
                    && match_people(middle, &facts.current, &facts.people, &mut 10_000)?
                {
                    return if accepted {
                        Ok(())
                    } else {
                        Err(RecordCheckError::Failed(format!(
                            "reviewed incorrect on-call claim violates: {}",
                            self.content.requirement
                        )))
                    };
                }
            }
        }
        Err(RecordCheckError::NeedsAdjudication(format!(
            "on-call answer wording requires adjudication against: {}",
            self.content.requirement,
        )))
    }
}
