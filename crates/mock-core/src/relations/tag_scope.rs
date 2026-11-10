use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{RecordCheckError, RecordSelector};
use crate::{json_pointer, label_matchers::LabelMatchersGoal};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagScopeGoal {
    pub field: String,
    pub equals: BTreeMap<String, String>,
    pub context: TagContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagContext {
    pub selector: RecordSelector,
    pub field: String,
}

fn failed(reason: impl Into<String>) -> RecordCheckError {
    RecordCheckError::Failed(reason.into())
}

fn unresolved(reason: impl Into<String>) -> RecordCheckError {
    RecordCheckError::NeedsAdjudication(reason.into())
}

fn supported_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn supported_value(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'*')
        })
}

fn expression(value: &str) -> String {
    value
        .split('*')
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(".*")
}

impl TagScopeGoal {
    pub(super) fn validate(&self) -> Result<(), String> {
        LabelMatchersGoal {
            field: self.field.clone(),
            equals: self.equals.clone(),
        }
        .validate()?;
        self.context.selector.validate()?;
        for reference in &self.context.selector.references {
            reference.validate_initial()?;
        }
        if self.context.selector.created
            || self.equals.iter().any(|(name, value)| {
                !supported_name(name) || !supported_value(value) || value.contains('*')
            })
            || json_pointer::tokens(&self.context.field)?
                .iter()
                .any(|part| part.is_empty() || part.contains('*'))
            || json_pointer::tokens(&self.context.field)?.is_empty()
        {
            return Err(
                "tag scopes require literal target equalities and an existing source tag context"
                    .into(),
            );
        }
        Ok(())
    }

    pub(super) fn owns_field(&self, pointer: &str) -> bool {
        let scope = json_pointer::tokens(&self.field);
        let field = json_pointer::tokens(pointer);
        matches!((scope, field), (Ok(scope), Ok(field))
            if scope.starts_with(&field) || field.starts_with(&scope))
    }

    fn tags(
        &self,
        initial: Option<&Value>,
    ) -> Result<BTreeMap<String, Vec<String>>, RecordCheckError> {
        let initial = initial.ok_or_else(|| {
            RecordCheckError::MissingEvidence("tag scope lacks its initial source context".into())
        })?;
        let selected = self.context.selector.select(None, initial)?;
        if selected.len() != 1 {
            return Err(RecordCheckError::MissingEvidence(
                "tag scope source context is not uniquely established".into(),
            ));
        }
        let source = selected[0].1.pointer(&self.context.field).ok_or_else(|| {
            RecordCheckError::MissingEvidence("tag context has no tag field".into())
        })?;
        let arrays = if let Some(groups) = source.as_object() {
            groups.values().collect::<Vec<_>>()
        } else {
            vec![source]
        };
        let mut tags = BTreeMap::<String, Vec<String>>::new();
        for array in arrays {
            let values = array.as_array().ok_or_else(|| {
                RecordCheckError::MissingEvidence("source tags are not string arrays".into())
            })?;
            for value in values {
                let (name, value) = value
                    .as_str()
                    .and_then(|tag| tag.split_once(':'))
                    .ok_or_else(|| {
                        RecordCheckError::MissingEvidence(
                            "source tag has no key/value identity".into(),
                        )
                    })?;
                if name.is_empty() || value.is_empty() {
                    return Err(RecordCheckError::MissingEvidence(
                        "source tag is empty".into(),
                    ));
                }
                tags.entry(name.into()).or_default().push(value.into());
            }
        }
        if self.equals.iter().any(|(name, expected)| {
            !tags
                .get(name)
                .is_some_and(|values| values.contains(expected))
        }) {
            return Err(RecordCheckError::MissingEvidence(
                "requested tag equality is absent from the bound source context".into(),
            ));
        }
        Ok(tags)
    }

    pub(super) fn check(
        &self,
        initial: Option<&Value>,
        record: &Value,
    ) -> Result<(), RecordCheckError> {
        let tags = self.tags(initial)?;
        let scope = record
            .pointer(&self.field)
            .and_then(Value::as_array)
            .ok_or_else(|| failed("tag scope is not an array"))?;
        if scope.is_empty() {
            return Err(failed("tag scope is empty"));
        }
        if scope.len() > 16 {
            return Err(unresolved(
                "tag scope exceeds the admitted conjunction bound",
            ));
        }
        let mut matchers = Vec::new();
        for scope in scope {
            let scope = scope
                .as_str()
                .ok_or_else(|| failed("tag scope is not text"))?;
            for term in scope.split(',').map(str::trim) {
                if term == "*" {
                    continue;
                }
                let (name, value) = term.split_once(':').ok_or_else(|| {
                    unresolved("non-tag downtime scopes require separate semantic qualification")
                })?;
                if !supported_name(name) || !supported_value(value) || value.len() > 1024 {
                    return Err(unresolved(
                        "downtime tag syntax is outside the admitted positive glob subset",
                    ));
                }
                let source = expression(value);
                let matcher =
                    regex::Regex::new(&format!("\\A(?:{source})\\z")).map_err(|error| {
                        unresolved(format!("tag predicate cannot be qualified: {error}"))
                    })?;
                if !tags
                    .get(name)
                    .is_some_and(|values| values.iter().any(|value| matcher.is_match(value)))
                {
                    return Err(failed(format!(
                        "scope conjunct {term} excludes the source target"
                    )));
                }
                if self.equals.contains_key(name) {
                    matchers.push(json!({
                        "name":name,"value":source,"isRegex":true,"isEqual":true
                    }));
                }
            }
        }
        LabelMatchersGoal {
            field: "/matchers".into(),
            equals: self.equals.clone(),
        }
        .check(&json!({"matchers":matchers}))
    }
}
