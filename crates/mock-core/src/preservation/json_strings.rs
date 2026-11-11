use serde_json::Value;

use super::{Preservation, PreservationFailure, business_state};
use crate::json_pointer::tokens;

impl Preservation {
    pub(super) fn validate_json_strings(&self) -> Result<(), String> {
        let mut observed: Vec<Vec<String>> = Vec::new();
        for pointer in &self.json_strings {
            let parts = tokens(pointer)?;
            if parts.len() < 5
                || parts[0] != "services"
                || parts
                    .iter()
                    .any(|part| part.is_empty() || part.contains('*'))
                || observed
                    .iter()
                    .any(|previous| parts.starts_with(previous) || previous.starts_with(&parts))
                || self.equivalent_numbers.contains(pointer)
            {
                return Err("encoded JSON requires distinct, nonoverlapping literal business-field pointers without competing numeric equivalence".into());
            }
            observed.push(parts);
        }
        Ok(())
    }

    pub(super) fn original_json_view(&self, original: &Value) -> Result<Option<Value>, String> {
        if self.json_strings.is_empty() {
            return Ok(None);
        }
        let mut original = business_state(original)?;
        for pointer in &self.json_strings {
            let source = original.pointer(pointer).and_then(Value::as_str)
                .ok_or_else(|| format!("encoded JSON projection requires an existing authoritative initial string at {pointer}"))?;
            let parsed = crate::json_value::parse(source).map_err(|_| {
                format!(
                    "encoded JSON projection requires unambiguous valid initial JSON at {pointer}"
                )
            })?;
            *original.pointer_mut(pointer).expect("existing JSON source") = parsed;
        }
        Ok(Some(original))
    }

    pub(super) fn project_json_strings(&self, state: &mut Value) -> Vec<PreservationFailure> {
        let mut failures = Vec::new();
        for pointer in &self.json_strings {
            let Some(value) = state.pointer_mut(pointer) else {
                continue;
            };
            match value.as_str().and_then(|source| crate::json_value::parse(source).ok()) {
                Some(parsed) => *value = parsed,
                None => failures.push(PreservationFailure {
                    pointer: pointer.clone(),
                    reason: "declared encoded business JSON is malformed, ambiguous or no longer a string".into(),
                }),
            }
        }
        failures
    }
}
