use chrono::{DateTime, LocalResult, NaiveDateTime, SecondsFormat, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};

#[derive(Debug)]
pub(super) enum TimeError {
    Invalid,
    Unavailable,
}

fn fraction(source: &str, digits: usize) -> String {
    let Some(tail) = source.get(19..).and_then(|tail| tail.strip_prefix('.')) else {
        return source.into();
    };
    let length = tail.bytes().take_while(u8::is_ascii_digit).count();
    if length <= digits {
        return source.into();
    }
    format!("{}{}", &source[..20 + digits], &source[20 + length..])
}

fn expanded(source: &str) -> String {
    if source.len() >= 16
        && source.is_ascii()
        && (source.len() == 16 || matches!(source.as_bytes()[16], b'Z' | b'z' | b'+' | b'-'))
    {
        format!("{}:00{}", &source[..16], &source[16..])
    } else {
        source.into()
    }
}

pub(super) fn instant(
    source: &str,
    zone: Option<&str>,
) -> Result<(DateTime<Utc>, bool), TimeError> {
    resolved_instant(source, zone, true)
}

pub(super) fn observed_instant(
    source: &str,
    zone: Option<&str>,
) -> Result<(DateTime<Utc>, bool), TimeError> {
    resolved_instant(source, zone, false)
}

fn resolved_instant(
    source: &str,
    zone: Option<&str>,
    convert_submitted: bool,
) -> Result<(DateTime<Utc>, bool), TimeError> {
    if !source.is_ascii() || source.starts_with("0000") {
        return Err(TimeError::Invalid);
    }
    if !source.contains('T') {
        let days = mock_core::sheet_date::serial(source).ok_or(TimeError::Invalid)? - 25_569;
        let local = DateTime::from_timestamp(days * 86_400, 0)
            .ok_or(TimeError::Invalid)?
            .naive_utc();
        let zone = zone
            .unwrap_or("UTC")
            .parse::<Tz>()
            .map_err(|_| TimeError::Invalid)?;
        return match zone.from_local_datetime(&local) {
            LocalResult::Single(value) => Ok((value.with_timezone(&Utc), false)),
            _ => Err(TimeError::Unavailable),
        };
    }
    let source = expanded(source);
    let explicit = source
        .get(19..)
        .is_some_and(|tail| tail.contains(['Z', 'z', '+', '-']));
    let source = if explicit && zone.is_some() && convert_submitted {
        fraction(&source, 6)
    } else {
        source
    };
    if let Some(tail) = source.get(19..).and_then(|tail| tail.strip_prefix('.'))
        && tail.bytes().take_while(u8::is_ascii_digit).count() > 9
    {
        return Err(TimeError::Unavailable);
    }
    if explicit {
        mock_core::clock::parse_rfc3339_nanos(&source).ok_or(TimeError::Invalid)?;
        let moment = DateTime::parse_from_rfc3339(&source).map_err(|_| TimeError::Invalid)?;
        return Ok((moment.with_timezone(&Utc), true));
    }
    mock_core::clock::parse_rfc3339_nanos(&format!("{source}Z")).ok_or(TimeError::Invalid)?;
    let zone = zone
        .ok_or(TimeError::Invalid)?
        .parse::<Tz>()
        .map_err(|_| TimeError::Invalid)?;
    let local = NaiveDateTime::parse_from_str(&source, "%Y-%m-%dT%H:%M:%S%.f")
        .map_err(|_| TimeError::Invalid)?;
    match zone.from_local_datetime(&local) {
        LocalResult::Single(value) => Ok((value.with_timezone(&Utc), false)),
        _ => Err(TimeError::Unavailable),
    }
}

pub(super) fn updated_boundary(
    source: &str,
    submitted_zone: Option<&str>,
    previous: &Value,
    calendar_zone: &str,
) -> Result<Value, TimeError> {
    if !source.contains('T') {
        return boundary(source, submitted_zone, calendar_zone);
    }
    let effective_zone = submitted_zone.or_else(|| previous["timeZone"].as_str());
    let name = effective_zone.unwrap_or(calendar_zone);
    let (moment, _) = resolved_instant(source, effective_zone, submitted_zone.is_some())?;
    let timezone = name.parse::<Tz>().map_err(|_| TimeError::Invalid)?;
    Ok(json!({
        "dateTime":moment.with_timezone(&timezone).to_rfc3339_opts(SecondsFormat::AutoSi,true),
        "timeZone":name
    }))
}

pub(super) fn boundary(
    source: &str,
    zone: Option<&str>,
    calendar_zone: &str,
) -> Result<Value, TimeError> {
    if !source.contains('T') {
        mock_core::sheet_date::serial(source).ok_or(TimeError::Invalid)?;
        return Ok(json!({"date":source}));
    }
    let (instant, _) = instant(source, zone)?;
    let name = zone.unwrap_or(calendar_zone);
    let timezone = name.parse::<Tz>().map_err(|_| TimeError::Invalid)?;
    Ok(json!({
        "dateTime":instant.with_timezone(&timezone).to_rfc3339_opts(SecondsFormat::AutoSi,true),
        "timeZone":name
    }))
}

pub(super) fn rendered(source: &str, zone: &str) -> Option<String> {
    let source = fraction(source, 6);
    let moment = DateTime::parse_from_rfc3339(&source).ok()?;
    let timezone = zone.parse::<Tz>().ok()?;
    let local = moment.with_timezone(&timezone);
    let precision = if local.nanosecond() == 0 {
        SecondsFormat::Secs
    } else {
        SecondsFormat::Micros
    };
    Some(local.to_rfc3339_opts(precision, false))
}

pub(super) fn unavailable(
    arguments: &Value,
    state: Option<&mock_core::world::ServiceState>,
) -> Option<&'static str> {
    for (field, zone_field) in [
        ("start_time", "start_timezone"),
        ("end_time", "end_timezone"),
    ] {
        let Some(value) = arguments[field].as_str() else {
            continue;
        };
        let submitted = arguments[zone_field]
            .as_str()
            .or_else(|| arguments["timezone"].as_str());
        let previous = state
            .and_then(|state| state.store.collection("calendar_events"))
            .and_then(|events| {
                arguments["event_id"]
                    .as_str()
                    .and_then(|identity| events.get(identity))
            });
        let prior_field = if field == "start_time" {
            "start"
        } else {
            "end"
        };
        let effective = submitted.or_else(|| {
            (arguments["action"]
                .as_str()
                .is_some_and(|action| action.trim().eq_ignore_ascii_case("update")))
            .then(|| previous.and_then(|event| event[prior_field]["timeZone"].as_str()))
            .flatten()
        });
        if matches!(
            resolved_instant(value, effective, submitted.is_some()),
            Err(TimeError::Unavailable)
        ) {
            return Some(
                "The requested civil-time ambiguity or precision has no qualified local resolution",
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_and_explicit_offsets_preserve_actual_instants() {
        let local = boundary("2026-11-03T14:00:00", Some("US/Eastern"), "UTC").unwrap();
        assert_eq!(local["dateTime"], "2026-11-03T14:00:00-05:00");
        let wrong_offset =
            boundary("2026-11-03T14:00:00-04:00", Some("America/New_York"), "UTC").unwrap();
        assert_eq!(wrong_offset["dateTime"], "2026-11-03T13:00:00-05:00");
    }

    #[test]
    fn native_offset_conversion_and_backend_precision_are_distinct() {
        let converted = boundary(
            "2026-11-03T14:00:00.000000999-05:00",
            Some("America/New_York"),
            "UTC",
        )
        .unwrap();
        assert_eq!(converted["dateTime"], "2026-11-03T14:00:00-05:00");
        let forwarded = boundary(
            "2026-11-03T14:00:00.000000999",
            Some("America/New_York"),
            "UTC",
        )
        .unwrap();
        assert_eq!(forwarded["dateTime"], "2026-11-03T14:00:00.000000999-05:00");
        let updated = updated_boundary(
            "2026-11-03T14:00:00.000000999-05:00",
            None,
            &json!({"timeZone":"America/New_York"}),
            "UTC",
        )
        .unwrap();
        assert_eq!(updated["dateTime"], "2026-11-03T14:00:00.000000999-05:00");
        let observed = observed_instant(
            "2026-11-03T14:00:00.000000999-05:00",
            Some("America/New_York"),
        )
        .unwrap();
        assert_eq!(observed.0.timestamp_subsec_nanos(), 999);
    }

    #[test]
    fn civil_time_ambiguity_is_not_silently_resolved() {
        for value in ["2026-11-01T01:30:00", "2026-03-08T02:30:00"] {
            assert!(matches!(
                instant(value, Some("America/New_York")),
                Err(TimeError::Unavailable)
            ));
        }
        assert!(matches!(
            instant("2026-02-30T14:00:00", Some("America/New_York")),
            Err(TimeError::Invalid)
        ));
        assert!(matches!(
            instant("2026-11-03T14:00:00", Some("Invalid/Zone")),
            Err(TimeError::Invalid)
        ));
        assert!(matches!(
            instant("😅😅😅2026", None),
            Err(TimeError::Invalid)
        ));
    }
}
