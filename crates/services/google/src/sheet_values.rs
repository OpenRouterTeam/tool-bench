use serde_json::{Value, json};

use crate::tz;

fn date(source: &str, locale: &str) -> Option<String> {
    if mock_core::sheet_date::serial(source).is_some() {
        return Some(source.into());
    }
    let parts: Vec<_> = source.split('/').collect();
    if parts.len() == 3 && parts[2].len() == 4 {
        let first = parts[0].parse::<u32>().ok()?;
        let second = parts[1].parse::<u32>().ok()?;
        let year = parts[2].parse::<u32>().ok()?;
        let (month, day) = if locale == "en_US" {
            (first, second)
        } else if matches!(locale, "en_GB" | "en_AU") {
            (second, first)
        } else {
            return None;
        };
        let canonical = format!("{year:04}-{month:02}-{day:02}");
        return mock_core::sheet_date::serial(&canonical).map(|_| canonical);
    }
    None
}

fn formula(source: &str, locale: &str) -> Option<Value> {
    let expression = source.strip_prefix('=')?.trim();
    if expression
        .get(..10)
        .is_some_and(|name| name.eq_ignore_ascii_case("DATEVALUE("))
        && let Some(argument) = expression.get(10..)?.strip_suffix(')')
        && let Ok(value) = serde_json::from_str::<String>(argument.trim())
    {
        return Some(date(&value, locale).map_or(json!("#VALUE!"), Value::String));
    }
    if !expression
        .get(..5)
        .is_some_and(|name| name.eq_ignore_ascii_case("DATE("))
    {
        return None;
    }
    let arguments: Vec<_> = expression.get(5..)?.strip_suffix(')')?.split(',').collect();
    if arguments.len() != 3 {
        return None;
    }
    let values: Vec<i64> = arguments
        .iter()
        .map(|argument| {
            argument
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && value.abs() < 9_000_000_000_000_000.0)
                .map(|value| value.trunc() as i64)
        })
        .collect::<Option<_>>()?;
    let year = match values[0] {
        0..=1899 => values[0] + 1900,
        1900..=9999 => values[0],
        _ => return Some(json!("#NUM!")),
    };
    let total_months = year
        .checked_mul(12)?
        .checked_add(values[1].checked_sub(1)?)?;
    let year = total_months.div_euclid(12);
    let month = total_months.rem_euclid(12) + 1;
    if !(1..=9999).contains(&year) || values[2].unsigned_abs() > 4_000_000 {
        return Some(json!("#NUM!"));
    }
    let days = tz::days_from_civil(year, month, 1).checked_add(values[2].checked_sub(1)?)?;
    let serial = days.checked_add(25_569)?;
    Some(mock_core::sheet_date::iso(serial).map_or(json!("#NUM!"), Value::String))
}

pub(super) fn normalize(value: &Value, raw: bool, locale: &str) -> Result<Option<Value>, String> {
    if value.is_null() {
        return Ok(None);
    }
    if value.is_object() || value.is_array() {
        return Err("Sheet values must be strings, numbers, booleans or skipped nulls.".into());
    }
    if raw || !value.is_string() {
        return Ok(Some(value.clone()));
    }
    let source = value.as_str().unwrap();
    if let Some(literal) = source.strip_prefix('\'') {
        return Ok(Some(json!(literal)));
    }
    if source.starts_with('=') {
        return formula(source, locale).map(Some).ok_or_else(|| {
            "The requested spreadsheet formula is outside the qualified expression set".into()
        });
    }
    if let Some(canonical) = date(source, locale) {
        return Ok(Some(json!(canonical)));
    }
    if source.eq_ignore_ascii_case("true") || source.eq_ignore_ascii_case("false") {
        return Ok(Some(json!(source.eq_ignore_ascii_case("true"))));
    }
    if let Ok(number) = serde_json::from_str::<serde_json::Number>(source) {
        return Ok(Some(Value::Number(number)));
    }
    if let Ok(number) = source.parse::<f64>()
        && let Some(number) = serde_json::Number::from_f64(number)
    {
        return Ok(Some(Value::Number(number)));
    }
    Ok(Some(value.clone()))
}

pub(super) fn unavailable(
    arguments: &Value,
    state: Option<&mock_core::world::ServiceState>,
) -> Option<&'static str> {
    if arguments["clear_values"] == true
        || arguments["value_input_option"]
            .as_str()
            .is_some_and(|option| option != "USER_ENTERED")
    {
        return None;
    }
    let values = crate::util::arg_json(arguments, "values")?;
    let file = state?
        .store
        .collection("drive_files")?
        .get(arguments["spreadsheet_id"].as_str()?)?;
    let locale = file["locale"].as_str().unwrap_or("en_US");
    values
        .as_array()?
        .iter()
        .filter_map(Value::as_array)
        .flatten()
        .any(|value| {
            value.as_str().is_some_and(|source| source.starts_with('='))
                && normalize(value, false, locale).is_err()
        })
        .then_some("The requested spreadsheet expressions have no qualified local evaluation")
}
