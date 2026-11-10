use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::{
    grading::subset,
    json_pointer::{encode as encode_pointer, escape, tokens as pointer_tokens},
};

mod json_strings;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preservation {
    #[serde(default)]
    pub allow: Vec<ChangeRule>,
    #[serde(default)]
    pub equivalent_numbers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equivalent_quantities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub json_strings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeRule {
    pub pointer: String,
    #[serde(default)]
    pub kind: ChangeKind,
    #[serde(default)]
    pub matching: Option<Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub field_patterns: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub numeric_fields: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub array_lengths: BTreeMap<String, usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<crate::relations::RecordReference>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub initial_matching: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_references: Vec<crate::relations::RecordReference>,
    #[serde(default = "one")]
    pub max_count: ChangeLimit,
    #[serde(default)]
    pub count_scope: CountScope,
    #[serde(default)]
    pub add_set_values: Option<Vec<String>>,
    #[serde(default)]
    pub ascii_case_insensitive: bool,
    #[serde(default)]
    pub element_matching: Option<Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub element_field_patterns: BTreeMap<String, String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub value_equals: Option<Value>,
    #[serde(default)]
    pub identity_field: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub url_host_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<OperationMatch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationMatch {
    pub service: String,
    pub methods: Vec<String>,
    pub path_regex: String,
}

impl OperationMatch {
    fn validate(&self) -> Result<(), String> {
        let anchored = self
            .path_regex
            .strip_prefix("(?i)")
            .unwrap_or(&self.path_regex);
        if self.service.is_empty()
            || self.methods.is_empty()
            || self.methods.iter().enumerate().any(|(index, method)| {
                !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE")
                    || self.methods[..index].contains(method)
            })
            || !anchored.starts_with('^')
            || !anchored.ends_with('$')
            || regex::Regex::new(&self.path_regex).map_or(true, |regex| regex.is_match(""))
        {
            return Err("mutation causes require a service, distinct supported HTTP methods and an anchored nonempty path pattern".into());
        }
        Ok(())
    }

    fn matches(&self, operation: &crate::events::OperationContext) -> bool {
        operation.kind == crate::events::OperationKind::Http
            && operation.service == self.service
            && self.methods.contains(&operation.method)
            && regex::Regex::new(&format!("\\A(?:{})\\z", self.path_regex))
                .is_ok_and(|pattern| pattern.is_match(&operation.path))
    }
}

fn one() -> ChangeLimit {
    ChangeLimit::Count(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChangeLimit {
    Count(usize),
    Matching(MatchingLimit),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchingLimit {
    AllMatching,
}

impl ChangeLimit {
    pub(crate) fn maximum(self) -> Option<usize> {
        match self {
            Self::Count(count) => Some(count),
            Self::Matching(MatchingLimit::AllMatching) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    #[default]
    Update,
    Create,
    Delete,
    Append,
    RemoveField,
    RestoreField,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CountScope {
    #[default]
    Trial,
    PerCommit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateChange {
    pub pointer: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub before: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub after: Option<Value>,
}

fn present_value<'de, Decoder: Deserializer<'de>>(
    decoder: Decoder,
) -> Result<Option<Value>, Decoder::Error> {
    Value::deserialize(decoder).map(Some)
}

pub(crate) fn record_changes(initial: &Value, final_state: &Value) -> Vec<StateChange> {
    let mut result = Vec::new();
    let previous = &initial["services"];
    let current = &final_state["services"];
    let services: BTreeSet<_> = previous
        .as_object()
        .into_iter()
        .flat_map(|services| services.keys())
        .chain(
            current
                .as_object()
                .into_iter()
                .flat_map(|services| services.keys()),
        )
        .collect();
    for service in services {
        let collections: BTreeSet<_> = previous[service]
            .as_object()
            .into_iter()
            .flat_map(|collections| collections.keys())
            .chain(
                current[service]
                    .as_object()
                    .into_iter()
                    .flat_map(|collections| collections.keys()),
            )
            .collect();
        for collection in collections {
            let previous_records = previous[service][collection].as_object();
            let current_records = current[service][collection].as_object();
            let keys: BTreeSet<_> = previous_records
                .into_iter()
                .flat_map(|records| records.keys())
                .chain(
                    current_records
                        .into_iter()
                        .flat_map(|records| records.keys()),
                )
                .collect();
            for key in keys {
                let before = previous_records.and_then(|records| records.get(key));
                let after = current_records.and_then(|records| records.get(key));
                if before != after {
                    result.push(StateChange {
                        pointer: format!(
                            "/services/{}/{}/{}",
                            escape(service),
                            escape(collection),
                            escape(key)
                        ),
                        before: before.cloned(),
                        after: after.cloned(),
                    });
                }
            }
        }
    }
    result
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreservationFailure {
    pub pointer: String,
    pub reason: String,
}

pub(crate) struct PreservationAssessment {
    pub failures: Vec<PreservationFailure>,
    pub matched_records: BTreeMap<usize, BTreeSet<String>>,
}

impl Preservation {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_json_strings()?;
        let mut numeric_fields = BTreeSet::new();
        for pointer in self
            .equivalent_numbers
            .iter()
            .chain(&self.equivalent_quantities)
        {
            let tokens = pointer_tokens(pointer)?;
            if tokens.len() < 5
                || tokens[0] != "services"
                || tokens
                    .iter()
                    .any(|token| token.is_empty() || token.contains('*'))
                || !numeric_fields.insert(pointer)
            {
                return Err(
                    "numeric and quantity equivalence require distinct literal business-field pointers".into(),
                );
            }
        }
        for (index, rule) in self.allow.iter().enumerate() {
            crate::field_patterns::FieldPatterns::compile(&rule.field_patterns)?;
            crate::field_patterns::FieldPatterns::compile(&rule.element_field_patterns)?;
            if rule.value_equals.is_some()
                && (rule.kind != ChangeKind::Update || rule.add_set_values.is_some())
            {
                return Err(
                    "field post-values require an update without a competing set predicate".into(),
                );
            }
            for field in rule.array_lengths.keys() {
                let tokens = pointer_tokens(field)?;
                if tokens.is_empty()
                    || tokens
                        .iter()
                        .any(|token| token.is_empty() || token.contains('*'))
                    || rule.field_patterns.contains_key(field)
                    || rule.numeric_fields.contains_key(field)
                    || rule.kind == ChangeKind::Append
                {
                    return Err("array-length preservation predicates require literal record fields without competing predicates or append rules".into());
                }
            }
            for (field, expected) in &rule.numeric_fields {
                let tokens = pointer_tokens(field)?;
                if tokens.is_empty()
                    || tokens
                        .iter()
                        .any(|token| token.is_empty() || token.contains('*'))
                    || rule.field_patterns.contains_key(field)
                    || !crate::decimal::equivalent(expected, expected)
                {
                    return Err("numeric preservation predicates require literal record fields and exact numeric values without competing string patterns".into());
                }
            }
            if rule
                .initial_matching
                .as_ref()
                .is_some_and(|matching| matching.as_object().is_none_or(serde_json::Map::is_empty))
            {
                return Err("initial matching requires a nonempty record matcher".into());
            }
            for (reference_index, reference) in rule.initial_references.iter().enumerate() {
                reference.validate_initial()?;
                if rule.initial_references[..reference_index].contains(reference) {
                    return Err("initial record references contain a duplicate predicate".into());
                }
            }
            for (reference_index, reference) in rule.references.iter().enumerate() {
                reference.validate()?;
                if rule.references[..reference_index].contains(reference) {
                    return Err("current record references contain a duplicate predicate".into());
                }
            }
            if rule.kind == ChangeKind::Append && !rule.references.is_empty() {
                return Err(
                    "current record references are not supported for append permissions".into(),
                );
            }
            let initially_scoped =
                rule.initial_matching.is_some() || !rule.initial_references.is_empty();
            if initially_scoped && matches!(rule.kind, ChangeKind::Create | ChangeKind::Append) {
                return Err("initial record scopes require changes to existing records".into());
            }
            if let Some(cause) = &rule.cause {
                cause.validate()?;
            }
            let tokens = pointer_tokens(&rule.pointer)?;
            if tokens.len() < 4 || tokens[0] != "services" {
                return Err(format!(
                    "preservation.allow[{index}] must identify a business record"
                ));
            }
            if matches!(
                rule.kind,
                ChangeKind::Update | ChangeKind::RemoveField | ChangeKind::RestoreField
            ) && tokens.len() < 5
            {
                return Err(format!(
                    "preservation.allow[{index}] must identify fields, not an entire existing record"
                ));
            }
            if rule.kind == ChangeKind::Append {
                if tokens.len() < 5
                    || tokens.iter().any(|token| token.contains('*'))
                    || !rule
                        .matching
                        .as_ref()
                        .and_then(Value::as_object)
                        .is_some_and(|value| !value.is_empty())
                    || rule.identity_field.as_ref().is_none_or(|field| {
                        pointer_tokens(field).map_or(true, |tokens| {
                            tokens.is_empty() || tokens.iter().any(|token| token.contains('*'))
                        })
                    })
                    || rule.element_matching.is_some()
                    || rule.add_set_values.is_some()
                    || !rule.numeric_fields.is_empty()
                {
                    return Err("append rules require a literal array, an explicit identity field and a nonempty new-row matcher".into());
                }
            } else if rule.identity_field.is_some() {
                return Err("array identity fields are only supported for append rules".into());
            }
            if tokens
                .iter()
                .enumerate()
                .any(|(position, token)| token.contains('*') && (token != "*" || position < 3))
            {
                return Err(format!(
                    "preservation.allow[{index}] permits a wildcard only for the record key"
                ));
            }
            let array_slots: Vec<_> = tokens
                .iter()
                .enumerate()
                .filter(|(position, token)| *position > 3 && token.as_str() == "*")
                .map(|(position, _)| position)
                .collect();
            if array_slots.len() > 1
                || !array_slots.is_empty()
                    && (array_slots[0] == tokens.len() - 1
                        || rule.kind != ChangeKind::Update
                        || (!rule
                            .element_matching
                            .as_ref()
                            .and_then(Value::as_object)
                            .is_some_and(|value| !value.is_empty())
                            && rule.element_field_patterns.is_empty()))
                || array_slots.is_empty()
                    && (rule.element_matching.is_some() || !rule.element_field_patterns.is_empty())
                || rule
                    .element_matching
                    .as_ref()
                    .is_some_and(|value| value.as_object().is_none_or(serde_json::Map::is_empty))
            {
                return Err(format!(
                    "preservation.allow[{index}] needs one identity-scoped array element and a leaf field"
                ));
            }
            if tokens[3] == "*"
                && !rule
                    .matching
                    .as_ref()
                    .and_then(Value::as_object)
                    .is_some_and(|object| !object.is_empty())
                && rule.field_patterns.is_empty()
                && !initially_scoped
                && rule.references.is_empty()
            {
                return Err(format!(
                    "preservation.allow[{index}] needs a nonempty record matcher for its wildcard"
                ));
            }
            if rule.max_count.maximum() == Some(0) {
                return Err(format!(
                    "preservation.allow[{index}].max_count must be positive"
                ));
            }
            if rule.max_count.maximum().is_none()
                && (tokens[3] != "*" || rule.count_scope != CountScope::Trial)
            {
                return Err(
                    "all_matching limits require a scoped record wildcard at trial scope".into(),
                );
            }
            if rule.max_count.maximum().is_none() {
                match rule.kind {
                    ChangeKind::Delete if tokens.len() == 4 => {}
                    ChangeKind::Update if tokens.len() >= 5 && initially_scoped => {}
                    ChangeKind::Create if tokens.len() == 4 && rule.cause.is_some() => {}
                    _ => {
                        return Err(
                            "all_matching permits baseline-record deletions, explicitly baseline-scoped field updates or cause-scoped record creations".into(),
                        );
                    }
                }
            }
            let mut url_fields = BTreeSet::new();
            for field in &rule.url_host_fields {
                if rule.kind != ChangeKind::Create
                    || pointer_tokens(field)?.is_empty()
                    || !url_fields.insert(field)
                    || rule
                        .matching
                        .as_ref()
                        .and_then(|matching| matching.pointer(field))
                        .and_then(Value::as_str)
                        .and_then(crate::url_host::normalized)
                        .is_none()
                {
                    return Err("URL host matching requires distinct record-field pointers in an explicit creation matcher".into());
                }
            }
            if rule.count_scope == CountScope::PerCommit
                && !matches!(rule.kind, ChangeKind::Create | ChangeKind::Delete)
            {
                return Err(format!(
                    "preservation.allow[{index}] permits per_commit limits only for created or deleted side-effect records"
                ));
            }
            let additions = rule.add_set_values.as_deref().unwrap_or(&[]);
            let normalized: BTreeSet<_> = additions
                .iter()
                .map(|value| {
                    if rule.ascii_case_insensitive {
                        value.to_ascii_lowercase()
                    } else {
                        value.clone()
                    }
                })
                .collect();
            if rule.add_set_values.is_some() && rule.kind != ChangeKind::Update
                || rule.ascii_case_insensitive && rule.add_set_values.is_none()
                || normalized.len() != additions.len()
                || additions
                    .iter()
                    .any(|value| value.is_empty() || value.trim() != value)
            {
                return Err(format!(
                    "preservation.allow[{index}] has an invalid additive string-set policy"
                ));
            }
        }
        Ok(())
    }

    pub fn check(
        &self,
        initial: &Value,
        final_state: &Value,
    ) -> Result<Vec<PreservationFailure>, String> {
        self.assess(initial, initial, final_state, None)
            .map(|assessment| assessment.failures)
    }

    pub(crate) fn assess_commit(
        &self,
        original: &Value,
        before: &Value,
        after: &Value,
        operation: &crate::events::OperationContext,
    ) -> Result<PreservationAssessment, String> {
        self.assess(original, before, after, Some(operation))
    }

    fn assess(
        &self,
        original: &Value,
        initial: &Value,
        final_state: &Value,
        operation: Option<&crate::events::OperationContext>,
    ) -> Result<PreservationAssessment, String> {
        let commit = operation.is_some();
        self.validate()?;
        if !original["services"].is_object() {
            return Err("preservation requires an authoritative original business state".into());
        }
        let projected_original = self.original_json_view(original)?;
        let original = projected_original.as_ref().unwrap_or(original);
        let mut initial = business_state(initial)?;
        let mut final_state = business_state(final_state)?;
        let mut failures = self.project_json_strings(&mut initial);
        failures.extend(self.project_json_strings(&mut final_state));
        if !failures.is_empty() {
            return Ok(PreservationAssessment {
                failures,
                matched_records: BTreeMap::new(),
            });
        }
        for pointer in &self.json_strings {
            if let (Some(before), Some(after)) =
                (initial.pointer(pointer), final_state.pointer_mut(pointer))
            {
                crate::json_value::normalize_numbers(before, after);
            }
        }
        for pointer in &self.equivalent_numbers {
            if let (Some(before), Some(after)) =
                (initial.pointer(pointer), final_state.pointer_mut(pointer))
                && crate::decimal::equivalent(before, after)
            {
                *after = before.clone();
            }
        }
        for pointer in &self.equivalent_quantities {
            if let (Some(before), Some(after)) =
                (initial.pointer(pointer), final_state.pointer_mut(pointer))
                && crate::quantity::Quantity::parse(before)
                    .ok()
                    .zip(crate::quantity::Quantity::parse(after).ok())
                    .is_some_and(|(before, after)| before == after)
            {
                *after = before.clone();
            }
        }
        let mut failures = Vec::new();
        let mut matched_records: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
        let mut set_permissions = BTreeMap::new();
        let rules = self
            .allow
            .iter()
            .map(|rule| pointer_tokens(&rule.pointer))
            .collect::<Result<Vec<_>, _>>()?;
        let patterns = self
            .allow
            .iter()
            .map(|rule| crate::field_patterns::FieldPatterns::compile(&rule.field_patterns))
            .collect::<Result<Vec<_>, _>>()?;
        let element_patterns = self
            .allow
            .iter()
            .map(|rule| crate::field_patterns::FieldPatterns::compile(&rule.element_field_patterns))
            .collect::<Result<Vec<_>, _>>()?;
        let initial_references = self
            .allow
            .iter()
            .map(|rule| {
                crate::relations::ReferenceSet::resolve(
                    &rule.initial_references,
                    Some(original),
                    original,
                )
                .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let references = self
            .allow
            .iter()
            .map(|rule| {
                crate::relations::ReferenceSet::resolve(
                    &rule.references,
                    Some(original),
                    &final_state,
                )
                .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (index, rule) in self
            .allow
            .iter()
            .enumerate()
            .filter(|(_, rule)| rule.kind == ChangeKind::Append)
        {
            if let Some(operation) = operation
                && rule
                    .cause
                    .as_ref()
                    .is_some_and(|cause| !cause.matches(operation))
            {
                continue;
            }
            let baseline = original
                .pointer(&rule.pointer)
                .and_then(Value::as_array)
                .ok_or("append rules require an existing initial array")?;
            let before = initial
                .pointer(&rule.pointer)
                .and_then(Value::as_array)
                .ok_or("append rules require an authoritative prior array")?;
            let Some(after) = final_state.pointer(&rule.pointer).and_then(Value::as_array) else {
                continue;
            };
            if before == after {
                continue;
            }
            let identity_field = rule
                .identity_field
                .as_ref()
                .expect("validated append identity");
            if let Some(identities) = append_rows(
                baseline,
                before,
                after,
                identity_field,
                rule.matching.as_ref().expect("validated append matcher"),
                &patterns[index],
            ) {
                matched_records.entry(index).or_default().extend(
                    identities
                        .into_iter()
                        .map(|identity| format!("{}/{}", rule.pointer, escape(&identity))),
                );
                *final_state
                    .pointer_mut(&rule.pointer)
                    .expect("observed array") = Value::Array(before.clone());
            }
        }
        let changes = changes(&initial, &final_state);
        for change in changes {
            let change_tokens = pointer_tokens(&change.pointer)?;
            let record_pointer = encode_pointer(&change_tokens[..change_tokens.len().min(4)]);
            let before = initial.pointer(&record_pointer);
            let after = final_state.pointer(&record_pointer);
            let mut accepted = false;
            for (index, (rule, tokens)) in self.allow.iter().zip(&rules).enumerate() {
                if let Some(operation) = operation
                    && rule
                        .cause
                        .as_ref()
                        .is_some_and(|cause| !cause.matches(operation))
                {
                    continue;
                }
                if rule.kind == ChangeKind::Append {
                    continue;
                }
                if !prefix_matches(tokens, &change_tokens) {
                    continue;
                }
                let correct_kind = match rule.kind {
                    ChangeKind::Update => before.is_some() && after.is_some(),
                    ChangeKind::RemoveField => before.is_some() && after.is_some(),
                    ChangeKind::RestoreField => before.is_some() && after.is_some(),
                    ChangeKind::Create => {
                        original.pointer(&record_pointer).is_none() && after.is_some()
                    }
                    ChangeKind::Delete => before.is_some() && after.is_none(),
                    ChangeKind::Append => unreachable!("handled as an array policy"),
                };
                let record =
                    if rule.max_count.maximum().is_none() && rule.kind == ChangeKind::Delete {
                        original.pointer(&record_pointer)
                    } else {
                        after.or(before)
                    };
                let initially_allowed =
                    if rule.initial_matching.is_some() || !rule.initial_references.is_empty() {
                        original.pointer(&record_pointer).is_some_and(|record| {
                            rule.initial_matching
                                .as_ref()
                                .is_none_or(|matching| subset(matching, record))
                                && initial_references[index].matches(record)
                        })
                    } else {
                        true
                    };
                let element_allowed = tokens
                    .iter()
                    .enumerate()
                    .find(|(position, token)| *position > 3 && token.as_str() == "*")
                    .is_none_or(|(position, _)| {
                        let prefix = encode_pointer(&change_tokens[..position]);
                        let offset = change_tokens[position].parse::<usize>().ok();
                        [initial.pointer(&prefix), final_state.pointer(&prefix)]
                            .iter()
                            .all(|array| {
                                array
                                    .and_then(Value::as_array)
                                    .and_then(|values| offset.and_then(|offset| values.get(offset)))
                                    .is_some_and(|element| {
                                        rule.element_matching
                                            .as_ref()
                                            .is_none_or(|matcher| subset(matcher, element))
                                            && element_patterns[index].matches(element)
                                    })
                            })
                    });
                let mut resolved_path = tokens.clone();
                for (position, token) in resolved_path.iter_mut().enumerate() {
                    if token == "*" {
                        *token = change_tokens[position].clone();
                    }
                }
                let resolved_pointer = encode_pointer(&resolved_path);
                if rule.kind == ChangeKind::RemoveField
                    && (initial.pointer(&resolved_pointer).is_none()
                        || final_state.pointer(&resolved_pointer).is_some())
                {
                    continue;
                }
                if rule.kind == ChangeKind::RestoreField
                    && !original
                        .pointer(&resolved_pointer)
                        .zip(final_state.pointer(&resolved_pointer))
                        .is_some_and(|(expected, actual)| {
                            crate::json_value::equivalent(expected, actual)
                        })
                {
                    continue;
                }
                let set_allowed = rule.add_set_values.as_ref().is_none_or(|additions| {
                    *set_permissions
                        .entry((index, resolved_pointer.clone()))
                        .or_insert_with(|| {
                            additive_set(
                                initial.pointer(&resolved_pointer),
                                final_state.pointer(&resolved_pointer),
                                additions,
                                rule.ascii_case_insensitive,
                            )
                        })
                });
                if correct_kind
                    && rule.value_equals.as_ref().is_none_or(|expected| {
                        final_state.pointer(&resolved_pointer) == Some(expected)
                    })
                    && initially_allowed
                    && element_allowed
                    && set_allowed
                    && record.is_some_and(|record| patterns[index].matches(record))
                    && record.is_some_and(|record| {
                        rule.numeric_fields.iter().all(|(field, expected)| {
                            record
                                .pointer(field)
                                .is_some_and(|actual| crate::decimal::equivalent(actual, expected))
                        })
                    })
                    && record.is_some_and(|record| {
                        rule.array_lengths.iter().all(|(field, expected)| {
                            record
                                .pointer(field)
                                .and_then(Value::as_array)
                                .is_some_and(|values| values.len() == *expected)
                        })
                    })
                    && record.is_some_and(|record| references[index].matches(record))
                    && rule.matching.as_ref().is_none_or(|matcher| {
                        record.is_some_and(|value| {
                            if rule.url_host_fields.is_empty() {
                                subset(matcher, value)
                            } else {
                                crate::url_host::record(matcher, &rule.url_host_fields)
                                    .zip(crate::url_host::record(value, &rule.url_host_fields))
                                    .is_some_and(|(matcher, value)| subset(&matcher, &value))
                            }
                        })
                    })
                {
                    accepted = true;
                    matched_records
                        .entry(index)
                        .or_default()
                        .insert(record_pointer.clone());
                }
            }
            if !accepted {
                failures.push(PreservationFailure {
                    pointer: change.pointer,
                    reason: "business state changed outside the allowed fields and records".into(),
                });
            }
        }
        for (index, records) in &matched_records {
            let rule = &self.allow[*index];
            if (commit || rule.count_scope == CountScope::Trial)
                && let Some(maximum) = rule.max_count.maximum()
                && records.len() > maximum
            {
                failures.push(PreservationFailure {
                    pointer: rule.pointer.clone(),
                    reason: format!(
                        "{} records changed; at most {} are allowed",
                        records.len(),
                        maximum
                    ),
                });
            }
        }
        Ok(PreservationAssessment {
            failures,
            matched_records,
        })
    }
}

fn append_rows(
    original: &[Value],
    before: &[Value],
    after: &[Value],
    field: &str,
    matcher: &Value,
    patterns: &crate::field_patterns::FieldPatterns,
) -> Option<BTreeSet<String>> {
    let identities = |rows: &[Value]| -> Option<Vec<String>> {
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        for row in rows {
            let value = row.pointer(field)?;
            let identity = match value {
                Value::String(value) if !value.is_empty() => serde_json::to_string(value).ok()?,
                Value::Number(value) if value.is_i64() || value.is_u64() => value.to_string(),
                _ => return None,
            };
            if !seen.insert(identity.clone()) {
                return None;
            }
            result.push(identity);
        }
        Some(result)
    };
    let original_ids: BTreeSet<_> = identities(original)?.into_iter().collect();
    let before_ids = identities(before)?;
    let after_ids = identities(after)?;
    if after_ids.len() < before_ids.len() || !after_ids.starts_with(&before_ids) {
        return None;
    }
    let mut created = BTreeSet::new();
    for (index, (identity, row)) in after_ids.into_iter().zip(after).enumerate() {
        if original_ids.contains(&identity) {
            if before.get(index) != Some(row) {
                return None;
            }
        } else {
            if !subset(matcher, row) || !patterns.matches(row) {
                return None;
            }
            created.insert(identity);
        }
    }
    Some(created)
}

fn additive_set(
    before: Option<&Value>,
    after: Option<&Value>,
    allowed: &[String],
    case_insensitive: bool,
) -> bool {
    let normalize = |value: &str| {
        if case_insensitive {
            value.to_ascii_lowercase()
        } else {
            value.to_owned()
        }
    };
    let values = |value: Option<&Value>| -> Option<BTreeSet<String>> {
        match value {
            None | Some(Value::Null) => Some(BTreeSet::new()),
            Some(Value::Array(values)) => values
                .iter()
                .map(|value| value.as_str().map(normalize))
                .collect(),
            _ => None,
        }
    };
    let (Some(before), Some(after)) = (values(before), values(after)) else {
        return false;
    };
    let allowed: BTreeSet<_> = allowed.iter().map(|value| normalize(value)).collect();
    before.is_subset(&after)
        && after
            .difference(&before)
            .all(|value| allowed.contains(value))
}

pub(crate) fn business_state(snapshot: &Value) -> Result<Value, String> {
    let services = snapshot["services"]
        .as_object()
        .ok_or("state evidence must contain a services object")?;
    let mut result = Map::new();
    for (service, collections) in services {
        let collections = collections
            .as_object()
            .ok_or_else(|| format!("state evidence for {service} must contain collections"))?;
        let mut populated = Map::new();
        for (collection, records) in collections {
            let records = records.as_object().ok_or_else(|| {
                format!("state evidence for {service}/{collection} must contain records")
            })?;
            if !records.is_empty() {
                populated.insert(collection.clone(), Value::Object(records.clone()));
            }
        }
        if !populated.is_empty() {
            result.insert(service.clone(), Value::Object(populated));
        }
    }
    Ok(serde_json::json!({"services": result}))
}

pub(crate) fn changes(initial: &Value, final_state: &Value) -> Vec<StateChange> {
    let mut changes = Vec::new();
    diff(Some(initial), Some(final_state), "", &mut changes);
    changes
}

fn diff(
    before: Option<&Value>,
    after: Option<&Value>,
    pointer: &str,
    changes: &mut Vec<StateChange>,
) {
    if before == after {
        return;
    }
    match (before, after) {
        (Some(Value::Object(previous)), Some(Value::Object(current))) => {
            let keys: BTreeSet<_> = previous.keys().chain(current.keys()).collect();
            for key in keys {
                diff(
                    previous.get(key),
                    current.get(key),
                    &format!("{pointer}/{}", escape(key)),
                    changes,
                );
            }
        }
        (None, Some(Value::Object(current))) if !current.is_empty() => {
            for (key, value) in current {
                diff(
                    None,
                    Some(value),
                    &format!("{pointer}/{}", escape(key)),
                    changes,
                );
            }
        }
        (Some(Value::Object(previous)), None) if !previous.is_empty() => {
            for (key, value) in previous {
                diff(
                    Some(value),
                    None,
                    &format!("{pointer}/{}", escape(key)),
                    changes,
                );
            }
        }
        (Some(Value::Array(previous)), Some(Value::Array(current)))
            if previous.len() == current.len() =>
        {
            for (index, (previous, current)) in previous.iter().zip(current).enumerate() {
                diff(
                    Some(previous),
                    Some(current),
                    &format!("{pointer}/{index}"),
                    changes,
                );
            }
        }
        _ => changes.push(StateChange {
            pointer: pointer.into(),
            before: before.cloned(),
            after: after.cloned(),
        }),
    }
}

fn prefix_matches(pattern: &[String], actual: &[String]) -> bool {
    pattern.len() <= actual.len()
        && pattern
            .iter()
            .zip(actual)
            .all(|(expected, found)| expected == "*" || expected == found)
}
