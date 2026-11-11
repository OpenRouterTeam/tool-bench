use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{RecordCheckError, RecordSelector, identities, identity};

pub(super) const MAX_REFERENCE_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordReference {
    pub source_field: String,
    pub target: RecordSelector,
    pub target_field: String,
}

impl RecordReference {
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.validate_depth(1)
    }

    pub(super) fn validate_depth(&self, depth: usize) -> Result<(), String> {
        for field in [&self.source_field, &self.target_field] {
            let tokens = crate::json_pointer::tokens(field)?;
            if tokens.is_empty()
                || tokens
                    .iter()
                    .any(|token| token.is_empty() || token.contains('*'))
            {
                return Err("record references require literal fields inside both records".into());
            }
        }
        self.target.validate_depth(depth)
    }

    pub(crate) fn validate_initial(&self) -> Result<(), String> {
        self.validate()?;
        fn baseline(selector: &RecordSelector) -> bool {
            !selector.created
                && selector
                    .references
                    .iter()
                    .all(|reference| baseline(&reference.target))
        }
        if !baseline(&self.target) {
            return Err("initial references cannot select newly created targets".into());
        }
        Ok(())
    }
}

pub(crate) struct ReferenceSet(Vec<(String, BTreeSet<String>)>);

impl ReferenceSet {
    pub(crate) fn resolve(
        references: &[RecordReference],
        initial: Option<&Value>,
        state: &Value,
    ) -> Result<Self, RecordCheckError> {
        let mut resolved = Vec::new();
        for reference in references {
            reference
                .validate()
                .map_err(RecordCheckError::InvalidContract)?;
            let targets = identities(&reference.target, &reference.target_field, initial, state)?;
            resolved.push((
                reference.source_field.clone(),
                targets.into_keys().collect(),
            ));
        }
        Ok(Self(resolved))
    }

    pub(crate) fn matches(&self, record: &Value) -> bool {
        self.0.iter().all(|(field, targets)| {
            identity(record.pointer(field))
                .ok()
                .is_some_and(|identity| targets.contains(&identity))
        })
    }
}
