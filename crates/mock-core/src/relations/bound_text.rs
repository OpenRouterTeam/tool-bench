use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    RecordCheckError, RecordSelector, contains_reference, identifier_character, normalized_text,
};
use crate::json_pointer::tokens as pointer_tokens;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundText {
    pub identities: BTreeMap<String, IdentitySource>,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentitySource {
    pub selector: RecordSelector,
    pub field: String,
    #[serde(default)]
    pub separator: Option<String>,
}

enum Part<'source> {
    Literal(&'source str),
    Identity(&'source str),
}

struct ResolvedIdentity {
    values: Vec<String>,
    separator: Option<String>,
}

impl ResolvedIdentity {
    fn consume<'candidate>(&self, candidate: &'candidate str) -> Option<&'candidate str> {
        if self.values.len() == 1 {
            return candidate.strip_prefix(self.values[0].as_str());
        }
        let separator = self.separator.as_deref()?;
        let mut remaining = candidate;
        let mut pending: BTreeSet<_> = self.values.iter().map(String::as_str).collect();
        while pending.len() > 1 {
            let boundary = remaining.find(separator)?;
            if !pending.remove(&remaining[..boundary]) {
                return None;
            }
            remaining = &remaining[boundary + separator.len()..];
        }
        remaining.strip_prefix(*pending.first()?)
    }
}

fn normalized_fragment(source: &str) -> String {
    let mut result = String::new();
    let mut whitespace = false;
    for character in source.chars() {
        if character.is_whitespace() {
            if !whitespace {
                result.push(' ');
            }
            whitespace = true;
        } else {
            result.push(character);
            whitespace = false;
        }
    }
    result
}

fn valid_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn parts(template: &str) -> Result<Vec<Part<'_>>, String> {
    let mut result = Vec::new();
    let mut remaining = template;
    while let Some(start) = remaining.find("${") {
        result.push(Part::Literal(&remaining[..start]));
        let variable = &remaining[start + 2..];
        let end = variable
            .find('}')
            .ok_or("unterminated source identity placeholder")?;
        let name = &variable[..end];
        if !valid_name(name) {
            return Err(format!("invalid source identity name {name:?}"));
        }
        result.push(Part::Identity(name));
        remaining = &variable[end + 1..];
    }
    result.push(Part::Literal(remaining));
    Ok(result)
}

impl IdentitySource {
    fn validate(&self) -> Result<(), String> {
        self.selector.validate()?;
        if pointer_tokens(&self.field)?.is_empty() {
            return Err("a source identity must identify a field inside a record".into());
        }
        if let Some(separator) = &self.separator
            && (separator.is_empty()
                || separator.chars().next().is_some_and(identifier_character)
                || separator
                    .chars()
                    .next_back()
                    .is_some_and(identifier_character)
                || separator.chars().any(crate::rich_text::unsupported_control))
        {
            return Err(
                "identity separators must be nonempty and retain complete identity boundaries"
                    .into(),
            );
        }
        Ok(())
    }

    fn values(
        &self,
        initial: Option<&Value>,
        state: &Value,
    ) -> Result<Vec<String>, RecordCheckError> {
        let records = self.selector.select(initial, state)?;
        if records.is_empty() {
            return Err(RecordCheckError::Failed(
                "a required source identity does not exist".into(),
            ));
        }
        let mut values = BTreeMap::new();
        for (_, record) in records {
            let text = match record.pointer(&self.field) {
                Some(Value::String(value))
                    if !value.is_empty() && !value.chars().any(char::is_whitespace) =>
                {
                    value.clone()
                }
                Some(Value::Number(value)) if value.is_i64() || value.is_u64() => value.to_string(),
                _ => {
                    return Err(RecordCheckError::Failed(
                        "source identities require nonempty whitespace-free strings or integers"
                            .into(),
                    ));
                }
            };
            if text.chars().any(crate::rich_text::unsupported_control) {
                return Err(RecordCheckError::NeedsAdjudication(
                    "source identity directional or invisible controls require adjudication".into(),
                ));
            }
            let typed_identity = super::identity(record.pointer(&self.field))?;
            if values
                .get(&text)
                .is_some_and(|previous| previous != &typed_identity)
            {
                return Err(RecordCheckError::NeedsAdjudication(
                    "distinct typed source identities have an ambiguous textual representation"
                        .into(),
                ));
            }
            values.insert(text, typed_identity);
        }
        Ok(values.into_keys().collect())
    }
}

impl BoundText {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.identities.is_empty() || self.examples.is_empty() {
            return Err(
                "bound text requires identity sources and positive whole-text examples".into(),
            );
        }
        for (name, source) in &self.identities {
            if !valid_name(name) {
                return Err(format!("invalid source identity name {name:?}"));
            }
            source.validate()?;
        }
        let required: BTreeSet<_> = self.identities.keys().map(String::as_str).collect();
        let mut seen = BTreeSet::new();
        for example in &self.examples {
            let normalized = normalized_text(example);
            if normalized.is_empty()
                || example.chars().any(crate::rich_text::unsupported_control)
                || !seen.insert(normalized)
            {
                return Err("bound whole-text examples must be nonempty and distinct after whitespace folding".into());
            }
            let parsed = parts(example)?;
            let mut used = BTreeSet::new();
            for (index, part) in parsed.iter().enumerate() {
                if let Part::Identity(name) = part {
                    used.insert(*name);
                    let preceding = match parsed.get(index.saturating_sub(1)) {
                        Some(Part::Literal(text)) => text.chars().next_back(),
                        _ => None,
                    };
                    let following = match parsed.get(index + 1) {
                        Some(Part::Literal(text)) => text.chars().next(),
                        _ => None,
                    };
                    if (preceding.is_none() && index != 1)
                        || (following.is_none() && index + 2 != parsed.len())
                        || preceding.is_some_and(identifier_character)
                        || following.is_some_and(identifier_character)
                    {
                        return Err(
                            "source identity placeholders require complete identity boundaries"
                                .into(),
                        );
                    }
                }
            }
            if used != required {
                return Err(
                    "every bound example must use exactly the declared identity sources".into(),
                );
            }
        }
        Ok(())
    }

    pub(super) fn matches(
        &self,
        candidate: &str,
        initial: Option<&Value>,
        state: &Value,
    ) -> Result<bool, RecordCheckError> {
        let mut bindings = BTreeMap::new();
        for (name, source) in &self.identities {
            let identities = source.values(initial, state)?;
            let separator = match &source.separator {
                Some(separator) => Some(normalized_fragment(separator)),
                None if identities.len() == 1 => None,
                None => {
                    return Err(RecordCheckError::NeedsAdjudication(format!(
                        "source identity {name} is ambiguous; no record may be guessed"
                    )));
                }
            };
            if separator.as_ref().is_some_and(|separator| {
                identities
                    .iter()
                    .any(|identity| identity.contains(separator))
            }) {
                return Err(RecordCheckError::NeedsAdjudication(format!(
                    "source identity {name} contains its presentation separator"
                )));
            }
            if !identities
                .iter()
                .any(|identity| contains_reference(candidate, identity))
            {
                return Err(RecordCheckError::Failed(format!(
                    "the text does not mention an actual complete identity from source {name}"
                )));
            }
            bindings.insert(
                name.as_str(),
                ResolvedIdentity {
                    values: identities,
                    separator,
                },
            );
        }
        for example in &self.examples {
            let canonical = normalized_text(example);
            let mut remaining = Some(candidate);
            for part in parts(&canonical).map_err(RecordCheckError::InvalidContract)? {
                let Some(candidate) = remaining else { break };
                remaining = match part {
                    Part::Literal(text) => candidate.strip_prefix(text),
                    Part::Identity(name) => bindings
                        .get(name)
                        .ok_or_else(|| {
                            RecordCheckError::InvalidContract(format!(
                                "undeclared source identity {name}"
                            ))
                        })?
                        .consume(candidate),
                };
            }
            if remaining == Some("") {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
