use serde_json::{Value, json};

const DAY_NANOS: i128 = 86_400_000_000_000;
const UNIX_SERIAL: i64 = 25_569;

pub fn serial(source: &str) -> Option<i64> {
    if source.len() != 10 || !source.is_ascii() {
        return None;
    }
    let nanos = crate::clock::parse_rfc3339_nanos(&format!("{source}T00:00:00Z"))?;
    i64::try_from(nanos / DAY_NANOS)
        .ok()?
        .checked_add(UNIX_SERIAL)
}

pub fn iso(serial: i64) -> Option<String> {
    let timestamp = crate::clock::format_rfc3339_nanos(
        (i128::from(serial) - i128::from(UNIX_SERIAL)) * DAY_NANOS,
    )?;
    let date = timestamp.split_once('T')?.0;
    (date.len() == 10).then(|| date.to_owned())
}

pub fn matches(value: &Value, expected: &str) -> Result<bool, String> {
    let required = serial(expected).ok_or("invalid required spreadsheet date")?;
    match value {
        Value::Number(_) => Ok(crate::decimal::equivalent(value, &json!(required))),
        Value::String(actual)
            if actual.len() == 10
                && actual.as_bytes()[..4].iter().all(u8::is_ascii_digit)
                && actual.as_bytes().get(4) == Some(&b'-')
                && actual.as_bytes().get(7) == Some(&b'-') =>
        {
            Ok(serial(actual) == Some(required))
        }
        Value::String(actual) if actual.contains('/') => {
            let parts: Vec<_> = actual.split('/').collect();
            if parts.len() == 3
                && let (Ok(first), Ok(second), Ok(year)) = (
                    parts[0].parse::<u32>(),
                    parts[1].parse::<u32>(),
                    parts[2].parse::<u32>(),
                )
                && parts[2].len() == 4
            {
                let (month, day) = match (first, second) {
                    (1..=12, 13..=31) => (first, second),
                    (13..=31, 1..=12) => (second, first),
                    (1..=12, 1..=12) if first == second => (first, second),
                    _ => return Err("ambiguous or unqualified spreadsheet date spelling".into()),
                };
                return Ok(serial(&format!("{year:04}-{month:02}-{day:02}")) == Some(required));
            }
            Ok(false)
        }
        Value::String(actual) if actual.starts_with('=') || actual.parse::<f64>().is_ok() => {
            Ok(false)
        }
        Value::String(actual) if actual.trim().is_empty() || actual.starts_with('#') => Ok(false),
        Value::String(_) => Err("unreviewed spreadsheet date representation".into()),
        _ => Ok(false),
    }
}
