use serde_json::Value;
use std::collections::BTreeSet;

use super::{RecordCheck, RecordCheckError};

fn same_set(first: &[Value], second: &[Value]) -> bool {
    first.len() == second.len() && first.iter().all(|value| second.contains(value))
}

impl RecordCheck {
    pub(super) fn validate_structured(&self) -> Result<(), String> {
        let mut url_fields = BTreeSet::new();
        if self
            .url_host_fields
            .iter()
            .any(|field| !url_fields.insert(field) || !self.value_sets.contains_key(field))
        {
            return Err(
                "URL host normalization requires distinct fields with explicit value sets".into(),
            );
        }
        for field in self
            .json_fields
            .keys()
            .chain(self.value_sets.keys())
            .chain(self.quantity_fields.keys())
        {
            if self.fields.contains_key(field)
                || self.optional_fields.contains_key(field)
                || self.optional_choices.contains_key(field)
                || self.null_fields.contains(field)
                || self.html_text.contains_key(field)
                || self.reviewed_text.contains_key(field)
                || self.numeric_fields.contains_key(field)
                || self.array_lengths.contains_key(field)
                || self.totals.contains_key(field)
                || self.integer_bounds.contains_key(field)
                || self
                    .any_fields
                    .iter()
                    .any(|fields| fields.contains_key(field))
                || self.reviewed_fields.as_ref().is_some_and(|check| {
                    check
                        .accepted
                        .iter()
                        .any(|fields| fields.contains_key(field))
                })
                || self.json_fields.contains_key(field) && self.value_sets.contains_key(field)
                || self.quantity_fields.contains_key(field)
                    && (self.json_fields.contains_key(field) || self.value_sets.contains_key(field))
            {
                return Err(
                    "structured field predicates must not compete with other field predicates"
                        .into(),
                );
            }
        }
        for expected in self.quantity_fields.values() {
            if crate::quantity::Quantity::parse(expected).is_err() {
                return Err("quantity fields require a supported exact resource quantity".into());
            }
        }
        for (field, alternatives) in &self.value_sets {
            if alternatives.is_empty() {
                return Err("record value sets require at least one complete alternative".into());
            }
            for (index, values) in alternatives.iter().enumerate() {
                let values = self
                    .normalized_set(field, values)
                    .ok_or("URL host value sets require supported hostname/path strings")?;
                if values.is_empty()
                    || values
                        .iter()
                        .enumerate()
                        .any(|(position, value)| values[..position].contains(value))
                    || alternatives[..index].iter().any(|previous| {
                        self.normalized_set(field, previous)
                            .is_some_and(|previous| same_set(&previous, &values))
                    })
                {
                    return Err("record value-set alternatives must be nonempty, distinct sets without duplicate members".into());
                }
            }
        }
        Ok(())
    }

    fn normalized_set(&self, field: &str, values: &[Value]) -> Option<Vec<Value>> {
        if self
            .url_host_fields
            .iter()
            .any(|declared| declared == field)
        {
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .and_then(crate::url_host::normalized)
                        .map(Value::String)
                })
                .collect()
        } else {
            Some(values.to_vec())
        }
    }

    pub(super) fn check_structured(
        &self,
        records: &[(String, &Value)],
    ) -> Result<(), RecordCheckError> {
        for (identity, record) in records {
            for (field, expected) in &self.quantity_fields {
                let expected = crate::quantity::Quantity::parse(expected).map_err(|_| {
                    RecordCheckError::InvalidContract("unsupported required quantity".into())
                })?;
                let actual = record.pointer(field).ok_or_else(|| {
                    RecordCheckError::Failed(format!("missing quantity field {field}"))
                })?;
                match crate::quantity::Quantity::parse(actual) {
                    Ok(actual) if actual == expected => {}
                    Err(crate::quantity::ParseError::Unsupported) => {
                        return Err(RecordCheckError::NeedsAdjudication(format!(
                            "quantity field {field} is outside the reviewed parser bounds"
                        )));
                    }
                    _ => {
                        return Err(RecordCheckError::Failed(format!(
                            "{} record {identity} field {field} does not equal the required resource quantity",
                            self.selector.collection
                        )));
                    }
                }
            }
            for (field, expected) in &self.json_fields {
                let parsed = record
                    .pointer(field)
                    .and_then(Value::as_str)
                    .and_then(|source| crate::json_value::parse(source).ok());
                if !parsed
                    .as_ref()
                    .is_some_and(|actual| crate::json_value::equivalent(expected, actual))
                {
                    return Err(RecordCheckError::Failed(format!(
                        "{} record {identity} field {field} must encode the complete required JSON value with distinct object members",
                        self.selector.collection
                    )));
                }
            }
        }
        for (field, alternatives) in &self.value_sets {
            let values = records
                .iter()
                .map(|(_, record)| record.pointer(field).cloned())
                .collect::<Option<Vec<_>>>();
            let matched = values
                .and_then(|values| self.normalized_set(field, &values))
                .is_some_and(|values| {
                    values
                        .iter()
                        .enumerate()
                        .all(|(index, value)| !values[..index].contains(value))
                        && alternatives.iter().any(|expected| {
                            self.normalized_set(field, expected)
                                .is_some_and(|expected| same_set(&expected, &values))
                        })
                });
            if !matched {
                return Err(RecordCheckError::Failed(format!(
                    "{} field {field} does not form one declared complete, distinct value set",
                    self.selector.collection
                )));
            }
        }
        Ok(())
    }
}
