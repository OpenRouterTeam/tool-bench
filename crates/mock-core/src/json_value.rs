use std::fmt;

use serde::{
    Deserialize, Deserializer,
    de::{Error, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};

struct UniqueValue(Value);

impl<'input> Deserialize<'input> for UniqueValue {
    fn deserialize<Decoder: Deserializer<'input>>(
        decoder: Decoder,
    ) -> Result<Self, Decoder::Error> {
        decoder.deserialize_any(UniqueVisitor)
    }
}

struct UniqueVisitor;

impl<'input> Visitor<'input> for UniqueVisitor {
    type Value = UniqueValue;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON value with distinct object members")
    }

    fn visit_bool<Failure: Error>(self, value: bool) -> Result<Self::Value, Failure> {
        Ok(UniqueValue(Value::Bool(value)))
    }

    fn visit_i64<Failure: Error>(self, value: i64) -> Result<Self::Value, Failure> {
        Ok(UniqueValue(Value::Number(value.into())))
    }

    fn visit_u64<Failure: Error>(self, value: u64) -> Result<Self::Value, Failure> {
        Ok(UniqueValue(Value::Number(value.into())))
    }

    fn visit_f64<Failure: Error>(self, value: f64) -> Result<Self::Value, Failure> {
        Number::from_f64(value)
            .map(|number| UniqueValue(Value::Number(number)))
            .ok_or_else(|| Failure::custom("JSON numbers must be finite"))
    }

    fn visit_str<Failure: Error>(self, value: &str) -> Result<Self::Value, Failure> {
        Ok(UniqueValue(Value::String(value.into())))
    }

    fn visit_string<Failure: Error>(self, value: String) -> Result<Self::Value, Failure> {
        Ok(UniqueValue(Value::String(value)))
    }

    fn visit_unit<Failure: Error>(self) -> Result<Self::Value, Failure> {
        Ok(UniqueValue(Value::Null))
    }

    fn visit_seq<Sequence: SeqAccess<'input>>(
        self,
        mut sequence: Sequence,
    ) -> Result<Self::Value, Sequence::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<UniqueValue>()? {
            values.push(value.0);
        }
        Ok(UniqueValue(Value::Array(values)))
    }

    fn visit_map<Access: MapAccess<'input>>(
        self,
        mut access: Access,
    ) -> Result<Self::Value, Access::Error> {
        let mut values = Map::new();
        while let Some((name, value)) = access.next_entry::<String, UniqueValue>()? {
            if values.insert(name, value.0).is_some() {
                return Err(Access::Error::custom("duplicate JSON object member"));
            }
        }
        Ok(UniqueValue(Value::Object(values)))
    }
}

pub(crate) fn parse(source: &str) -> Result<Value, serde_json::Error> {
    let value = serde_json::from_str::<UniqueValue>(source)?.0;
    let mut numbers = NumberTokens { source, offset: 0 };
    if !exact_numbers(&value, &mut numbers) || numbers.next().is_some() {
        return Err(serde_json::Error::custom(
            "JSON numeric literals must retain their exact decimal value",
        ));
    }
    Ok(value)
}

struct NumberTokens<'source> {
    source: &'source str,
    offset: usize,
}

impl<'source> Iterator for NumberTokens<'source> {
    type Item = &'source str;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.source.as_bytes();
        while self.offset < bytes.len() {
            let current = bytes[self.offset];
            self.offset += 1;
            if current == b'"' {
                while self.offset < bytes.len() {
                    let current = bytes[self.offset];
                    self.offset += 1;
                    if current == b'\\' {
                        self.offset += 1;
                    } else if current == b'"' {
                        break;
                    }
                }
            } else if current == b'-' || current.is_ascii_digit() {
                let start = self.offset - 1;
                while bytes.get(self.offset).is_some_and(|byte| {
                    byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-')
                }) {
                    self.offset += 1;
                }
                return Some(&self.source[start..self.offset]);
            }
        }
        None
    }
}

fn exact_numbers(value: &Value, numbers: &mut NumberTokens<'_>) -> bool {
    match value {
        Value::Number(_) => numbers
            .next()
            .is_some_and(|source| crate::decimal::equivalent(&Value::String(source.into()), value)),
        Value::Array(values) => values.iter().all(|value| exact_numbers(value, numbers)),
        Value::Object(values) => values.values().all(|value| exact_numbers(value, numbers)),
        _ => true,
    }
}

pub(crate) fn normalize_numbers(before: &Value, after: &mut Value) {
    if before.is_number() && after.is_number() && crate::decimal::equivalent(before, after) {
        *after = before.clone();
        return;
    }
    match (before, after) {
        (Value::Object(previous), Value::Object(current)) => {
            for (name, value) in previous {
                if let Some(current) = current.get_mut(name) {
                    normalize_numbers(value, current);
                }
            }
        }
        (Value::Array(previous), Value::Array(current)) => {
            for (previous, current) in previous.iter().zip(current) {
                normalize_numbers(previous, current);
            }
        }
        _ => {}
    }
}

pub(crate) fn equivalent(expected: &Value, actual: &Value) -> bool {
    let mut actual = actual.clone();
    normalize_numbers(expected, &mut actual);
    expected == &actual
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn numeric_precision_checks_ignore_numbers_in_strings_and_preserve_nested_wire_order() {
        for source in [
            r#""25.000000000000000001""#,
            r#"{"z":"quoted \" 99","b":[{"unicode":"\u0032","number":25.0}],"a":-4.5e2}"#,
            r#"[null,true,false,"3",-1,0,25.000,1e3]"#,
            "18446744073709551615",
        ] {
            parse(source).unwrap();
        }
        for source in [
            r#"{"z":"quoted \" 99","b":[{"number":25.000000000000000001}],"a":-4.5e2}"#,
            "18446744073709551616",
            "0.1000000000000000000001",
        ] {
            assert!(parse(source).is_err());
        }
    }
}
