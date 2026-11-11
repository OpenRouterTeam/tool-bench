use std::fmt;

use serde::{
    Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;

/// The message of the error for an object that repeats a key.
pub(super) const DUPLICATE_KEY: &str = "duplicate answer key";

struct UniqueVisitor {
    depth: usize,
}

impl<'input> Visitor<'input> for UniqueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON without duplicate object keys")
    }

    fn visit_bool<Error: de::Error>(self, value: bool) -> Result<Self::Value, Error> {
        Ok(Value::Bool(value))
    }

    fn visit_str<Error: de::Error>(self, value: &str) -> Result<Self::Value, Error> {
        Ok(Value::String(value.into()))
    }

    fn visit_unit<Error: de::Error>(self) -> Result<Self::Value, Error> {
        Ok(Value::Null)
    }

    fn visit_seq<Sequence: SeqAccess<'input>>(
        self,
        mut sequence: Sequence,
    ) -> Result<Self::Value, Sequence::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<Box<serde_json::value::RawValue>>()? {
            values.push(json_at_depth(value.get(), self.depth + 1).map_err(de::Error::custom)?);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<Object: MapAccess<'input>>(
        self,
        mut object: Object,
    ) -> Result<Self::Value, Object::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom(DUPLICATE_KEY));
            }
            let value: Box<serde_json::value::RawValue> = object.next_value()?;
            values.insert(
                key,
                json_at_depth(value.get(), self.depth + 1).map_err(de::Error::custom)?,
            );
        }
        Ok(Value::Object(values))
    }
}

pub(super) fn json(text: &str) -> Result<Value, serde_json::Error> {
    json_at_depth(text, 0)
}

fn json_at_depth(text: &str, depth: usize) -> Result<Value, serde_json::Error> {
    if depth > 8 {
        return Err(de::Error::custom("unsupported structured answer nesting"));
    }
    let trimmed = text.trim();
    if trimmed.starts_with(|character: char| character.is_ascii_digit() || character == '-') {
        serde_json::from_str::<Box<serde_json::value::RawValue>>(trimmed)?;
        let exact = if !trimmed.contains(['e', 'E'])
            && let Some((integer, fraction)) = trimmed.split_once('.')
        {
            let fraction = fraction.trim_end_matches('0');
            match fraction.len() {
                0 => integer.to_owned(),
                3 => format!("{integer}.{fraction}0"),
                _ => format!("{integer}.{fraction}"),
            }
        } else {
            trimmed.to_owned()
        };
        return Ok(Value::String(exact));
    }
    let mut decoder = serde_json::Deserializer::from_str(text);
    let value = (&mut decoder).deserialize_any(UniqueVisitor { depth })?;
    decoder.end()?;
    Ok(value)
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Grid {
    pub(super) headers: Vec<String>,
    pub(super) rows: Vec<Vec<String>>,
}

fn cells(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return None;
    }
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    Some(
        inner
            .split('|')
            .map(|cell| cell.trim().to_owned())
            .collect(),
    )
}

fn separator(line: &str, width: usize) -> bool {
    cells(line).is_some_and(|parts| {
        parts.len() == width
            && parts.iter().all(|part| {
                let dashes = part.trim_start_matches(':').trim_end_matches(':');
                !dashes.is_empty() && dashes.bytes().all(|byte| byte == b'-')
            })
    })
}

/// The Markdown table that starts at `lines[index]`, with the index of the
/// first line after it.
pub(super) fn table_at(lines: &[&str], index: usize) -> Option<(Grid, usize)> {
    let headers = cells(lines.get(index)?)?;
    if !lines
        .get(index + 1)
        .is_some_and(|line| separator(line, headers.len()))
    {
        return None;
    }
    let mut rows = Vec::new();
    let mut next = index + 2;
    while let Some(row) = lines.get(next).and_then(|line| cells(line)) {
        rows.push(row);
        next += 1;
    }
    Some((Grid { headers, rows }, next))
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// `marketLeader` and `market_leader` as the words `market leader`.
fn words(key: &str) -> String {
    let mut text = String::with_capacity(key.len() + 4);
    let mut previous = None::<char>;
    for character in key.chars() {
        if matches!(character, '_' | '-') {
            text.push(' ');
        } else {
            if character.is_uppercase()
                && previous.is_some_and(|before| before.is_lowercase() || before.is_ascii_digit())
            {
                text.push(' ');
            }
            text.push(character);
        }
        previous = Some(character);
    }
    text
}

/// Writes a structured answer as table-like lines: one line per object, each
/// scalar field a `label: value` cell.  The label carries the keys of every
/// enclosing object, so a wrapper such as `vendorsOtherThanTheLeader` still
/// qualifies the values inside it.
pub(super) fn flatten(value: &Value, prefix: &str, lines: &mut Vec<String>) {
    let labelled = |label: &str, text: &str| match (prefix.is_empty(), label.is_empty()) {
        (true, true) => text.to_owned(),
        (true, false) => format!("{label}: {text}"),
        (false, true) => format!("{prefix}: {text}"),
        (false, false) => format!("{prefix} {label}: {text}"),
    };
    let (cells, nested): (Vec<String>, Vec<(String, &Value)>) = match value {
        Value::Object(fields) => (
            fields
                .iter()
                .filter_map(|(key, field)| Some(labelled(&words(key), &scalar(field)?)))
                .collect(),
            fields
                .iter()
                .filter(|(_, field)| field.is_array() || field.is_object())
                .map(|(key, field)| (format!("{prefix} {}", words(key)), field))
                .collect(),
        ),
        Value::Array(items) => (
            items
                .iter()
                .filter_map(|item| Some(labelled("", &scalar(item)?)))
                .collect(),
            items
                .iter()
                .filter(|item| item.is_array() || item.is_object())
                .map(|item| (prefix.to_owned(), item))
                .collect(),
        ),
        other => (
            scalar(other)
                .map(|text| labelled("", &text))
                .into_iter()
                .collect(),
            Vec::new(),
        ),
    };
    if !cells.is_empty() {
        lines.push(cells.join(" | "));
    }
    for (prefix, field) in nested {
        flatten(field, prefix.trim(), lines);
    }
}

pub(super) fn transposed(grid: &Grid) -> Grid {
    let width = grid.headers.len();
    let headers = std::iter::once(grid.headers[0].clone())
        .chain(
            grid.rows
                .iter()
                .map(|row| row.first().cloned().unwrap_or_default()),
        )
        .collect();
    let rows = (1..width)
        .map(|column| {
            std::iter::once(grid.headers[column].clone())
                .chain(
                    grid.rows
                        .iter()
                        .map(|row| row.get(column).cloned().unwrap_or_default()),
                )
                .collect()
        })
        .collect();
    Grid { headers, rows }
}
