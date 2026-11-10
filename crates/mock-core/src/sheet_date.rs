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
        // Any other text either spells one calendar date with a month name or is not a date.
        Value::String(actual) => Ok(named_month(actual) == Some(required)),
        _ => Ok(false),
    }
}

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

/// The serial of a date written with a month name: `November 30, 2027`, `30 Nov 2027`,
/// `2027 Nov 30`, optionally after a weekday and with an ordinal day. The month name removes the
/// day/month ambiguity of numeric spellings.
fn named_month(source: &str) -> Option<i64> {
    let lowered = source.to_ascii_lowercase();
    let weekday =
        |word: &str| word.len() >= 3 && WEEKDAYS.iter().any(|name| name.starts_with(word));
    let words: Vec<&str> = lowered
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty() && !weekday(word))
        .collect();
    let month = |word: &str| {
        (word.len() >= 3)
            .then(|| MONTHS.iter().position(|name| name.starts_with(word)))
            .flatten()
            .or_else(|| (word == "sept").then_some(8))
            .map(|index| index as u32 + 1)
    };
    let day = |word: &str| {
        let digits = ["st", "nd", "rd", "th"]
            .iter()
            .find_map(|suffix| word.strip_suffix(suffix))
            .unwrap_or(word);
        (digits.len() <= 2)
            .then(|| digits.parse::<u32>().ok())
            .flatten()
    };
    let year = |word: &str| {
        (word.len() == 4)
            .then(|| word.parse::<u32>().ok())
            .flatten()
    };
    let (year, month, day) = match words.as_slice() {
        [first, second, third] => month(first)
            .zip(day(second))
            .zip(year(third))
            .map(|((month, day), year)| (year, month, day))
            .or_else(|| {
                day(first)
                    .zip(month(second))
                    .zip(year(third))
                    .map(|((day, month), year)| (year, month, day))
            })
            .or_else(|| {
                year(first)
                    .zip(month(second))
                    .zip(day(third))
                    .map(|((year, month), day)| (year, month, day))
            })?,
        _ => return None,
    };
    serial(&format!("{year:04}-{month:02}-{day:02}"))
}
