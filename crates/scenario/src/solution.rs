use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Solution, SolutionCall};

pub type Bindings = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentReplacement {
    pub pointer: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub name: String,
    pub pointer: String,
    #[serde(default)]
    pub json_pointer: Option<String>,
    #[serde(default)]
    pub select: Option<CaptureSelection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureSelection {
    pub matching: BTreeMap<String, String>,
    pub pointer: String,
}

impl Capture {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_name(&self.name) {
            return Err(format!("invalid oracle binding name {:?}", self.name));
        }
        mock_core::json_pointer::validate(&self.pointer)?;
        if let Some(pointer) = &self.json_pointer {
            mock_core::json_pointer::validate(pointer)?;
        }
        if let Some(selection) = &self.select {
            if selection.matching.is_empty() || selection.matching.keys().any(String::is_empty) {
                return Err("oracle selections need nonempty exact string-field criteria".into());
            }
            mock_core::json_pointer::validate(&selection.pointer)?;
        }
        Ok(())
    }

    fn read(&self, result: &Value) -> Result<Value, String> {
        self.validate()?;
        let value = result
            .pointer(&self.pointer)
            .ok_or_else(|| format!("capture {}: source pointer was not found", self.name))?;
        let value = match &self.json_pointer {
            None => Ok(value.clone()),
            Some(pointer) => {
                let text = value
                    .as_str()
                    .ok_or_else(|| format!("capture {}: JSON text was expected", self.name))?;
                let decoded: Value = serde_json::from_str(text).map_err(|error| {
                    format!("capture {}: invalid JSON text: {error}", self.name)
                })?;
                decoded
                    .pointer(pointer)
                    .cloned()
                    .ok_or_else(|| format!("capture {}: decoded pointer was not found", self.name))
            }
        }?;
        if let Some(selection) = &self.select {
            let matches: Vec<_> = value
                .as_array()
                .ok_or("oracle selection requires an array")?
                .iter()
                .filter(|row| {
                    selection.matching.iter().all(|(key, expected)| {
                        row.get(key).and_then(Value::as_str) == Some(expected.as_str())
                    })
                })
                .collect();
            if matches.len() != 1 {
                return Err(format!(
                    "capture {} selected {} rows; exactly one is required",
                    self.name,
                    matches.len()
                ));
            }
            matches[0]
                .pointer(&selection.pointer)
                .cloned()
                .ok_or_else(|| format!("capture {}: selected pointer was not found", self.name))
        } else {
            Ok(value)
        }
    }
}

impl SolutionCall {
    pub fn resolve_arguments(&self, bindings: &Bindings) -> Result<Value, String> {
        self.validate_replacements()?;
        let mut arguments = if self.template {
            render(&self.arguments, bindings)
        } else {
            Ok(self.arguments.clone())
        }?;
        for replacement in &self.replacements {
            let value = render(&replacement.value, bindings)?;
            let field = arguments.pointer_mut(&replacement.pointer).ok_or_else(|| {
                format!(
                    "argument replacement pointer {} does not exist",
                    replacement.pointer
                )
            })?;
            *field = value;
        }
        Ok(arguments)
    }

    fn validate_replacements(&self) -> Result<(), String> {
        if !self.replacements.is_empty() && !self.template {
            return Err("argument replacements require explicit templating".into());
        }
        let mut paths: Vec<Vec<String>> = Vec::new();
        for replacement in &self.replacements {
            let tokens = mock_core::json_pointer::tokens(&replacement.pointer)?;
            if tokens.is_empty()
                || paths
                    .iter()
                    .any(|path| path.starts_with(&tokens) || tokens.starts_with(path))
            {
                return Err(
                    "argument replacements require distinct nonoverlapping literal fields".into(),
                );
            }
            paths.push(tokens);
        }
        Ok(())
    }

    pub fn capture_result(&self, result: &Value, bindings: &mut Bindings) -> Result<(), String> {
        if self.capture.is_empty() {
            return Ok(());
        }
        if !result.is_object() {
            return Err("oracle captures require a tool-result object".into());
        }
        if result["isError"] == true {
            return Err("oracle bindings cannot be captured from a tool error".into());
        }
        let mut values = Bindings::new();
        for capture in &self.capture {
            if bindings.contains_key(&capture.name) || values.contains_key(&capture.name) {
                return Err(format!(
                    "oracle binding {} is already defined",
                    capture.name
                ));
            }
            values.insert(capture.name.clone(), capture.read(result)?);
        }
        bindings.extend(values);
        Ok(())
    }
}

impl Solution {
    pub fn resolve_answer(&self, bindings: &Bindings) -> Result<Option<String>, String> {
        let Some(answer) = &self.answer else {
            return if self.answer_template {
                Err("answer_template requires solution.answer".into())
            } else {
                Ok(None)
            };
        };
        if !self.answer_template {
            return Ok(Some(answer.clone()));
        }
        Ok(Some(match render_string(answer, bindings)? {
            Value::String(answer) => answer,
            answer => answer.to_string(),
        }))
    }

    pub fn validate_bindings(&self) -> Result<(), String> {
        let mut known = Bindings::new();
        for (index, call) in self.calls.iter().enumerate() {
            call.validate_replacements()
                .map_err(|error| format!("solution.call[{index}]: {error}"))?;
            if call.template {
                render(&call.arguments, &known)
                    .map_err(|error| format!("solution.call[{index}]: {error}"))?;
                for replacement in &call.replacements {
                    render(&replacement.value, &known)
                        .map_err(|error| format!("solution.call[{index}]: {error}"))?;
                }
            }
            let mut names = BTreeSet::new();
            for capture in &call.capture {
                capture.validate()?;
                if known.contains_key(&capture.name) || !names.insert(&capture.name) {
                    return Err(format!(
                        "oracle binding {} is already defined",
                        capture.name
                    ));
                }
            }
            for capture in &call.capture {
                known.insert(capture.name.clone(), Value::String("validation".into()));
            }
        }
        self.resolve_answer(&known)
            .map_err(|error| format!("solution.answer: {error}"))?;
        Ok(())
    }
}

fn render(value: &Value, bindings: &Bindings) -> Result<Value, String> {
    match value {
        Value::String(text) => render_string(text, bindings),
        Value::Array(values) => values
            .iter()
            .map(|value| render(value, bindings))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(name, value)| Ok((name.clone(), render(value, bindings)?)))
            .collect::<Result<_, String>>()
            .map(Value::Object),
        value => Ok(value.clone()),
    }
}

fn render_string(text: &str, bindings: &Bindings) -> Result<Value, String> {
    let mut output = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let variable = &remaining[start + 2..];
        let end = variable
            .find('}')
            .ok_or("an oracle template contains an unterminated binding")?;
        let name = &variable[..end];
        if !valid_name(name) {
            return Err(format!("invalid oracle template binding {name:?}"));
        }
        let value = bindings
            .get(name)
            .ok_or_else(|| format!("oracle binding {name} has not been captured"))?;
        if output.is_empty() && start == 0 && end + 1 == variable.len() && remaining == text {
            return Ok(value.clone());
        }
        match value {
            Value::String(value) => output.push_str(value),
            value => output.push_str(&value.to_string()),
        }
        remaining = &variable[end + 1..];
    }
    output.push_str(remaining);
    Ok(Value::String(output))
}

fn valid_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}
