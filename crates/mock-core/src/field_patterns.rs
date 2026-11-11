use std::collections::BTreeMap;

use regex::Regex;
use serde_json::Value;

use crate::json_pointer::tokens;

pub(crate) struct FieldPatterns(Vec<(String, Regex)>);

impl FieldPatterns {
    pub(crate) fn compile(fields: &BTreeMap<String, String>) -> Result<Self, String> {
        let mut patterns = Vec::new();
        for (field, pattern) in fields {
            let path = tokens(field)?;
            if path.is_empty()
                || path
                    .iter()
                    .any(|token| token.is_empty() || token.contains('*'))
            {
                return Err("string patterns require literal fields inside a record".into());
            }
            let pattern = Regex::new(pattern)
                .map_err(|error| format!("invalid string pattern at {field}: {error}"))?;
            if pattern.is_match("") {
                return Err("string patterns must not match an empty value".into());
            }
            patterns.push((field.clone(), pattern));
        }
        Ok(Self(patterns))
    }

    pub(crate) fn matches(&self, record: &Value) -> bool {
        self.0.iter().all(|(field, pattern)| {
            record
                .pointer(field)
                .and_then(Value::as_str)
                .is_some_and(|value| pattern.is_match(value))
        })
    }
}
