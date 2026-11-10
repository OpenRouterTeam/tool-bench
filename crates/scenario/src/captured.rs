//! Validation of oracle arguments that are captured from an earlier tool result.
//!
//! A capture may declare a `constraint`: a JSON Schema in the small keyword subset below. The
//! same subset is implemented by `assets/value-schema.jq`, so the generated oracle can enforce
//! both the declared constraint (when a value is captured) and the consuming tool's locked
//! input schema (on the fully resolved arguments, before the call is sent).
//!
//! Static authoring checks compare the declared constraint with the locked schemas wherever a
//! schema position can be resolved. A capture that is not declared, or a consuming schema that
//! uses keywords outside the subset, keeps the `captured_argument_constraints_deferred` gap.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::{Solution, SolutionCall, solution::Capture};

/// Validation keywords implemented identically by the Rust validator and `value-schema.jq`.
const VALIDATION_KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "const",
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "pattern",
    "minimum",
    "maximum",
];
/// Keywords that never affect validity here (formats are annotations in both validators).
const ANNOTATION_KEYWORDS: &[&str] = &[
    "$schema",
    "description",
    "title",
    "default",
    "examples",
    "format",
];
const TYPES: &[&str] = &[
    "string", "integer", "number", "boolean", "object", "array", "null",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingMode {
    /// The whole argument is the captured value, with its JSON type.
    Whole,
    /// The captured scalar is interpolated into surrounding text.
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticCompatibility {
    /// The declared constraint's type is admitted by the resolved locked schema position.
    Compatible,
    /// The locked schema does not constrain this position; only the runtime check applies.
    ConsumerUnconstrained,
    /// The capture declares no constraint, so nothing can be compared.
    Undeclared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BindingUse {
    pub binding: String,
    pub argument: String,
    pub mode: BindingMode,
    pub static_check: StaticCompatibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapturedArguments {
    pub uses: Vec<BindingUse>,
    /// Whether the generated oracle validates the resolved arguments against the locked schema.
    pub runtime_locked_schema: bool,
    /// Why the captured-argument gap remains open; empty when it is closed.
    pub open: Vec<String>,
}

impl CapturedArguments {
    pub fn closed(&self) -> bool {
        self.open.is_empty()
    }
}

/// The first keyword outside the shared subset, or a malformed use of a supported one.
pub fn unsupported(schema: &Value) -> Option<String> {
    let object = match schema {
        Value::Bool(_) => return None,
        Value::Object(object) => object,
        _ => return Some("schemas must be objects or booleans".into()),
    };
    for (keyword, value) in object {
        if ANNOTATION_KEYWORDS.contains(&keyword.as_str()) {
            continue;
        }
        if !VALIDATION_KEYWORDS.contains(&keyword.as_str()) {
            return Some(format!("keyword {keyword}"));
        }
        let malformed = || Some(format!("malformed {keyword}"));
        match keyword.as_str() {
            "type" => {
                let names: Vec<&Value> = match value {
                    Value::Array(names) if !names.is_empty() => names.iter().collect(),
                    Value::String(_) => vec![value],
                    _ => return malformed(),
                };
                if names
                    .iter()
                    .any(|name| !name.as_str().is_some_and(|name| TYPES.contains(&name)))
                {
                    return malformed();
                }
            }
            "properties" => {
                let Some(properties) = value.as_object() else {
                    return malformed();
                };
                if let Some(reason) = properties.values().find_map(unsupported) {
                    return Some(reason);
                }
            }
            "required" => {
                if !value
                    .as_array()
                    .is_some_and(|names| names.iter().all(Value::is_string))
                {
                    return malformed();
                }
            }
            "additionalProperties" | "items" => {
                if let Some(reason) = unsupported(value) {
                    return Some(reason);
                }
            }
            "enum" => {
                if !value.as_array().is_some_and(|values| !values.is_empty()) {
                    return malformed();
                }
            }
            "const" => {}
            "minLength" | "maxLength" | "minItems" | "maxItems" => {
                if value.as_u64().is_none() {
                    return malformed();
                }
            }
            "minimum" | "maximum" => {
                if !value.is_number() {
                    return malformed();
                }
            }
            "pattern" => {
                if !value.as_str().is_some_and(portable_pattern) {
                    return Some("pattern outside the portable regular-expression subset".into());
                }
            }
            _ => unreachable!("every validation keyword is handled"),
        }
    }
    None
}

/// Patterns whose meaning is the same in the Rust validator and jq (Oniguruma): ASCII only, no
/// groups with flags or look-around, and only class or punctuation escapes.
fn portable_pattern(pattern: &str) -> bool {
    if !pattern.is_ascii() || pattern.contains("(?") {
        return false;
    }
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        if character == '\\'
            && !characters
                .next()
                .is_some_and(|escaped| "dws.\\/-+*?()[]{}|^$".contains(escaped))
        {
            return false;
        }
    }
    true
}

/// A declared capture constraint: a supported schema naming exactly one JSON type.
pub fn validate_constraint(constraint: &Value) -> Result<(), String> {
    if let Some(reason) = unsupported(constraint) {
        return Err(format!("capture constraints do not support {reason}"));
    }
    if declared_type(constraint).is_none() {
        return Err("capture constraints must declare a single JSON type".into());
    }
    compile(constraint).map(|_| ())
}

fn declared_type(constraint: &Value) -> Option<&str> {
    constraint.get("type")?.as_str()
}

fn compile(schema: &Value) -> Result<jsonschema::Validator, String> {
    jsonschema::options()
        .offline()
        .should_validate_formats(false)
        .build(schema)
        .map_err(|error| format!("invalid schema: {error}"))
}

/// Every violation of `schema` by `value`, as `<pointer>: <message>`.
pub fn violations(schema: &Value, value: &Value) -> Result<Vec<String>, String> {
    let validator = compile(schema)?;
    Ok(validator
        .iter_errors(value)
        .map(|error| {
            let path = error.instance_path().to_string();
            format!(
                "{}: violates {}",
                if path.is_empty() { "/" } else { &path },
                error.schema_path()
            )
        })
        .collect())
}

impl Capture {
    /// Fails closed when a captured value does not satisfy its declared constraint.
    pub(crate) fn check_constraint(&self, value: &Value) -> Result<(), String> {
        let Some(constraint) = &self.constraint else {
            return Ok(());
        };
        let found = violations(constraint, value)?;
        if found.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "capture {} violates its declared constraint ({})",
                self.name,
                found.join("; ")
            ))
        }
    }
}

struct Use {
    binding: String,
    tokens: Vec<String>,
    mode: BindingMode,
}

fn pointer(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| format!("/{}", token.replace('~', "~0").replace('/', "~1")))
        .collect()
}

fn collect(value: &Value, tokens: &mut Vec<String>, uses: &mut Vec<Use>) {
    match value {
        Value::String(text) => {
            let mut remaining = text.as_str();
            while let Some(start) = remaining.find("${") {
                let Some(end) = remaining[start + 2..].find('}') else {
                    return;
                };
                let name = &remaining[start + 2..start + 2 + end];
                uses.push(Use {
                    binding: name.into(),
                    tokens: tokens.clone(),
                    mode: if start == 0 && end + 3 == text.len() && remaining.len() == text.len() {
                        BindingMode::Whole
                    } else {
                        BindingMode::Embedded
                    },
                });
                remaining = &remaining[start + 3 + end..];
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                tokens.push(index.to_string());
                collect(value, tokens, uses);
                tokens.pop();
            }
        }
        Value::Object(values) => {
            for (name, value) in values {
                tokens.push(name.clone());
                collect(value, tokens, uses);
                tokens.pop();
            }
        }
        _ => {}
    }
}

fn uses(call: &SolutionCall) -> Result<Vec<Use>, String> {
    let mut found = Vec::new();
    if !call.template {
        return Ok(found);
    }
    collect(&call.arguments, &mut Vec::new(), &mut found);
    for replacement in &call.replacements {
        let mut tokens = mock_core::json_pointer::tokens(&replacement.pointer)?;
        collect(&replacement.value, &mut tokens, &mut found);
    }
    Ok(found)
}

/// Whether a call's arguments are rendered from values captured at oracle runtime.
pub fn uses_captured_values(call: &SolutionCall) -> bool {
    uses(call).is_ok_and(|found| !found.is_empty())
}

/// The schema governing `tokens` below `schema`, when the supported keywords determine one.
fn subschema<'a>(mut schema: &'a Value, tokens: &[String]) -> Option<&'a Value> {
    for token in tokens {
        if unsupported_here(schema) {
            return None;
        }
        let property = schema
            .get("properties")
            .and_then(|properties| properties.get(token));
        schema = match property {
            Some(property) => property,
            None if token.parse::<usize>().is_ok() && schema.get("items").is_some() => {
                &schema["items"]
            }
            None => schema
                .get("additionalProperties")
                .filter(|schema| schema.is_object())?,
        };
    }
    (!unsupported_here(schema)).then_some(schema)
}

fn unsupported_here(schema: &Value) -> bool {
    schema.as_object().is_none_or(|object| {
        object.keys().any(|keyword| {
            !VALIDATION_KEYWORDS.contains(&keyword.as_str())
                && !ANNOTATION_KEYWORDS.contains(&keyword.as_str())
        })
    })
}

fn admitted(schema: &Value) -> Option<Vec<&str>> {
    match schema.get("type")? {
        Value::String(name) => Some(vec![name.as_str()]),
        Value::Array(names) => Some(names.iter().filter_map(Value::as_str).collect()),
        _ => None,
    }
}

fn admits(schema: &Value, produced: &str) -> bool {
    admitted(schema).is_none_or(|types| {
        types.contains(&produced) || (produced == "integer" && types.contains(&"number"))
    })
}

fn overlaps(schema: &Value, declared: &str) -> bool {
    admits(schema, declared)
        || (declared == "number"
            && admitted(schema).is_some_and(|types| types.contains(&"integer")))
}

/// Where a capture reads from, as far as a locked output schema can describe it.
fn source_schema<'a>(capture: &Capture, output: Option<&'a Value>) -> Option<&'a Value> {
    if capture.json_pointer.is_some() {
        return None;
    }
    let tokens = mock_core::json_pointer::tokens(&capture.pointer).ok()?;
    let (first, rest) = tokens.split_first()?;
    if first != "structuredContent" {
        return None;
    }
    let mut schema = subschema(output?, rest)?;
    if let Some(selection) = &capture.select {
        let tokens = mock_core::json_pointer::tokens(&selection.pointer).ok()?;
        schema = subschema(schema.get("items")?, &tokens)?;
    }
    Some(schema)
}

pub struct LockedTool<'a> {
    pub input: &'a Value,
    pub output: Option<&'a Value>,
}

/// Checks every capture declaration and binding use in `solution` against the locked schemas.
///
/// Returns one entry per call: `None` when its arguments use no captured value.
pub fn analyze<'a>(
    solution: &Solution,
    locked: impl Fn(&SolutionCall) -> anyhow::Result<LockedTool<'a>>,
) -> anyhow::Result<Vec<Option<CapturedArguments>>> {
    let mut captures: BTreeMap<&str, &Capture> = BTreeMap::new();
    let mut result = Vec::new();
    for (index, call) in solution.calls.iter().enumerate() {
        let fail = |message: String| anyhow::anyhow!("solution.call[{index}]: {message}");
        let tool = locked(call)?;
        let found = uses(call).map_err(fail)?;
        let mut checked = Vec::new();
        let mut open = Vec::new();
        for used in &found {
            let argument = pointer(&used.tokens);
            let capture = captures.get(used.binding.as_str()).ok_or_else(|| {
                fail(format!(
                    "oracle binding {} has not been captured",
                    used.binding
                ))
            })?;
            let Some(constraint) = &capture.constraint else {
                let reason = format!("undeclared_capture_constraint:{}", used.binding);
                if !open.contains(&reason) {
                    open.push(reason);
                }
                checked.push(BindingUse {
                    binding: used.binding.clone(),
                    argument,
                    mode: used.mode.clone(),
                    static_check: StaticCompatibility::Undeclared,
                });
                continue;
            };
            let declared = declared_type(constraint).unwrap_or_default();
            let produced = match used.mode {
                BindingMode::Whole => declared,
                BindingMode::Embedded => {
                    if matches!(declared, "object" | "array" | "null") {
                        return Err(fail(format!(
                            "binding {} is declared as {declared} and cannot be interpolated into text at {argument}",
                            used.binding
                        )));
                    }
                    "string"
                }
            };
            let consumer = subschema(tool.input, &used.tokens);
            if let Some(consumer) = consumer {
                if !admits(consumer, produced) {
                    return Err(fail(format!(
                        "binding {} produces {produced} but {}.{} argument {argument} requires {}",
                        used.binding,
                        call.endpoint,
                        call.tool,
                        admitted(consumer).unwrap_or_default().join(" or ")
                    )));
                }
                if used.mode == BindingMode::Whole
                    && let (Some(allowed), Some(declared)) = (
                        consumer.get("enum").and_then(Value::as_array),
                        constraint.get("enum").and_then(Value::as_array),
                    )
                    && let Some(value) = declared.iter().find(|value| !allowed.contains(value))
                {
                    return Err(fail(format!(
                        "binding {} may be {value}, which {}.{} argument {argument} does not accept",
                        used.binding, call.endpoint, call.tool
                    )));
                }
            }
            checked.push(BindingUse {
                binding: used.binding.clone(),
                argument,
                mode: used.mode.clone(),
                static_check: if consumer.is_some_and(|schema| admitted(schema).is_some()) {
                    StaticCompatibility::Compatible
                } else {
                    StaticCompatibility::ConsumerUnconstrained
                },
            });
        }
        let runtime_locked_schema = match unsupported(tool.input) {
            None => true,
            Some(reason) => {
                if !found.is_empty() {
                    open.push(format!(
                        "locked_input_schema_not_runtime_checkable:{}",
                        reason.replace(' ', "_")
                    ));
                }
                false
            }
        };
        for capture in &call.capture {
            if let Some(constraint) = &capture.constraint {
                validate_constraint(constraint)
                    .map_err(|error| fail(format!("capture {}: {error}", capture.name)))?;
                let declared = declared_type(constraint).unwrap_or_default();
                if let Some(source) = source_schema(capture, tool.output)
                    && !overlaps(source, declared)
                {
                    return Err(fail(format!(
                        "capture {} is declared as {declared} but {}.{} returns {} at {}",
                        capture.name,
                        call.endpoint,
                        call.tool,
                        admitted(source).unwrap_or_default().join(" or "),
                        capture.pointer
                    )));
                }
            }
            captures.insert(&capture.name, capture);
        }
        result.push((!found.is_empty()).then_some(CapturedArguments {
            uses: checked,
            runtime_locked_schema,
            open,
        }));
    }
    Ok(result)
}
