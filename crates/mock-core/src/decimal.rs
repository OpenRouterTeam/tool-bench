use serde_json::Value;

#[derive(PartialEq, Eq)]
struct Decimal {
    negative: bool,
    digits: String,
    exponent: i64,
}

fn parse(value: &Value) -> Option<Decimal> {
    let text = match value {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.trim().to_owned(),
        _ => return None,
    };
    if text.len() > 4096 {
        return None;
    }
    let (negative, unsigned) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text.as_str()),
    };
    let mut parts = unsigned.split(['e', 'E']);
    let mantissa = parts.next()?;
    let exponent = match parts.next() {
        Some(exponent) => exponent.parse::<i64>().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole.bytes().all(|digit| digit.is_ascii_digit())
        || !fraction.bytes().all(|digit| digit.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Some(Decimal {
            negative: false,
            digits: "0".into(),
            exponent: 0,
        });
    }
    let trimmed = significant.trim_end_matches('0');
    let exponent = exponent
        .checked_sub(fraction.len().try_into().ok()?)?
        .checked_add((significant.len() - trimmed.len()).try_into().ok()?)?;
    Some(Decimal {
        negative,
        digits: trimmed.to_owned(),
        exponent,
    })
}

pub(crate) fn equivalent(left: &Value, right: &Value) -> bool {
    match (parse(left), parse(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

pub fn exact_integer(value: &Value) -> Option<i64> {
    let value = parse(value)?;
    if value.exponent < 0
        || value
            .digits
            .len()
            .checked_add(value.exponent.try_into().ok()?)?
            > 19
    {
        return None;
    }
    let sign = if value.negative { "-" } else { "" };
    format!(
        "{sign}{}{}",
        value.digits,
        "0".repeat(value.exponent.try_into().ok()?)
    )
    .parse()
    .ok()
}

fn bson_number(value: &Value) -> Option<Decimal> {
    if value.is_number() {
        return parse(value);
    }
    let wrapper = value.as_object().filter(|wrapper| wrapper.len() == 1)?;
    let (kind, value) = wrapper.iter().next()?;
    let text = value.as_str()?;
    if text.trim() != text {
        return None;
    }
    match kind.as_str() {
        "$numberInt" => parse(&Value::from(text.parse::<i32>().ok()?)),
        "$numberLong" => parse(&Value::from(text.parse::<i64>().ok()?)),
        "$numberDouble" => {
            let value = serde_json::Number::from_f64(text.parse::<f64>().ok()?)?;
            parse(&Value::Number(value))
        }
        "$numberDecimal" => {
            let number = parse(value)?;
            (number.digits.len() <= 34
                && number.exponent >= -6176
                && number
                    .exponent
                    .checked_add(number.digits.len() as i64 - 1)?
                    <= 6144)
                .then_some(number)
        }
        _ => None,
    }
}

pub(crate) fn bson_equivalent(left: &Value, right: &Value) -> bool {
    match (bson_number(left), bson_number(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}
