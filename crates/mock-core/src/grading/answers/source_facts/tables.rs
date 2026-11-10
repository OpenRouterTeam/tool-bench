use std::fmt;

use serde::{
    Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;

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
                return Err(de::Error::custom("duplicate answer key"));
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

pub(super) struct Markdown {
    pub(super) tables: Vec<Grid>,
    pub(super) prose: bool,
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

pub(super) fn markdown(text: &str) -> Markdown {
    let lines: Vec<&str> = text.lines().collect();
    let mut tables = Vec::new();
    let mut prose = false;
    let mut index = 0;
    while index < lines.len() {
        let header = cells(lines[index]);
        if let Some(headers) = header
            && lines
                .get(index + 1)
                .is_some_and(|line| separator(line, headers.len()))
        {
            let mut rows = Vec::new();
            index += 2;
            while let Some(row) = lines.get(index).and_then(|line| cells(line)) {
                rows.push(row);
                index += 1;
            }
            tables.push(Grid { headers, rows });
            continue;
        }
        prose |= !lines[index].trim().is_empty();
        index += 1;
    }
    Markdown { tables, prose }
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

pub(super) fn structured(value: &Value) -> Option<Grid> {
    let records: Vec<&serde_json::Map<String, Value>> = match value {
        Value::Array(rows) => rows.iter().map(Value::as_object).collect::<Option<_>>()?,
        Value::Object(object)
            if object
                .values()
                .all(|field| !field.is_array() && !field.is_object()) =>
        {
            vec![object]
        }
        Value::Object(object) => {
            let [(key, nested)] = object.iter().collect::<Vec<_>>()[..] else {
                return None;
            };
            if !matches!(
                super::header_key(key).as_str(),
                "companies" | "vendors" | "results" | "rows" | "leads"
            ) {
                return None;
            }
            nested
                .as_array()?
                .iter()
                .map(Value::as_object)
                .collect::<Option<_>>()?
        }
        _ => return None,
    };
    let mut headers: Vec<String> = Vec::new();
    for record in &records {
        for field in record.keys() {
            if !headers.contains(field) {
                headers.push(field.clone());
            }
        }
    }
    let rows = records
        .iter()
        .map(|record| {
            headers
                .iter()
                .map(|field| record.get(field).map_or(Some(String::new()), scalar))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Grid { headers, rows })
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
