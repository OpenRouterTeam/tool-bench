use std::sync::OnceLock;

use chrono::{DateTime, Datelike, Duration, NaiveDate, SecondsFormat, Timelike, Utc};
use chrono_tz::Tz;
use croner::Cron;
use regex::Regex;
use serde_json::{Value, json};
use thiserror::Error;

const SEARCH_MINUTES: i64 = 366 * 24 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unsupported(String),
}

fn invalid(message: impl Into<String>) -> ScheduleError {
    ScheduleError::Invalid(message.into())
}

fn unsupported(message: impl Into<String>) -> ScheduleError {
    ScheduleError::Unsupported(message.into())
}

pub struct MonitorSchedule {
    expression: String,
    timezone: Tz,
    parsed: Cron,
}

fn text_expression(source: &str) -> Result<String, ScheduleError> {
    static MINUTES: OnceLock<Regex> = OnceLock::new();
    static HOURLY: OnceLock<Regex> = OnceLock::new();
    static HOURS: OnceLock<Regex> = OnceLock::new();
    static DAILY: OnceLock<Regex> = OnceLock::new();
    if !source.is_ascii() {
        return Err(unsupported(
            "Non-ASCII schedule text is outside the qualified normalization",
        ));
    }
    let text = source
        .to_lowercase()
        .replace(['.', ','], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let parse = |source: &str| {
        source
            .parse::<u32>()
            .map_err(|_| invalid("Schedule time is out of range"))
    };
    if let Some(captures) = MINUTES
        .get_or_init(|| {
            Regex::new(
                r"^every ([0-9]+) (?:minutes?|mins?)(?: (?:starting|start)(?: at| on)? :?([0-9]{1,2}))?$",
            )
            .unwrap()
        })
        .captures(&text)
    {
        let interval = parse(&captures[1])?;
        let start = captures.get(2).map_or(Ok(0), |value| parse(value.as_str()))?;
        if interval == 0 || interval > 60 || 60 % interval != 0 || start > 59 {
            return Err(invalid("Minute schedules require a divisor of 60 and a valid start minute"));
        }
        return Ok(if start == 0 {
            format!("*/{interval} * * * *")
        } else {
            format!("{start}-59/{interval} * * * *")
        });
    }
    if let Some(captures) = HOURLY
        .get_or_init(|| {
            Regex::new(r"^(?:hourly|every hour)(?: (?:at|starting at) :?([0-9]{1,2}))?$").unwrap()
        })
        .captures(&text)
    {
        let minute = captures
            .get(1)
            .map_or(Ok(0), |value| parse(value.as_str()))?;
        if minute > 59 {
            return Err(invalid("Hourly schedule minute is out of range"));
        }
        return Ok(format!("{minute} * * * *"));
    }
    if let Some(captures) = HOURS
        .get_or_init(|| Regex::new(r"^every ([0-9]+) (?:hours?|hrs?)$").unwrap())
        .captures(&text)
    {
        let interval = parse(&captures[1])?;
        if interval == 0 || interval > 24 || 24 % interval != 0 {
            return Err(invalid("Hour schedules require a positive divisor of 24"));
        }
        return Ok(format!("0 */{interval} * * *"));
    }
    if let Some(captures) = DAILY
        .get_or_init(|| {
            Regex::new(r"^(?:daily|every day)(?: at ([0-9]{1,2})(?::([0-9]{2}))?\s*(am|pm)?)?$")
                .unwrap()
        })
        .captures(&text)
    {
        let mut hour = captures
            .get(1)
            .map_or(Ok(0), |value| parse(value.as_str()))?;
        let minute = captures
            .get(2)
            .map_or(Ok(0), |value| parse(value.as_str()))?;
        if let Some(meridiem) = captures.get(3) {
            if !(1..=12).contains(&hour) {
                return Err(invalid("Daily meridiem hour is out of range"));
            }
            hour %= 12;
            if meridiem.as_str() == "pm" {
                hour += 12;
            }
        }
        if hour > 23 || minute > 59 {
            return Err(invalid("Daily schedule time is out of range"));
        }
        return Ok(format!("{minute} {hour} * * *"));
    }
    if matches!(text.as_str(), "weekly" | "every week") {
        return Ok("0 0 * * 0".into());
    }
    Err(invalid("Unsupported schedule text"))
}

fn compatible_expression(source: &str) -> Result<String, ScheduleError> {
    if !source.is_ascii() {
        return Err(unsupported(
            "Non-ASCII cron spelling is outside the qualified normalization",
        ));
    }
    let fields: Vec<_> = source.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(invalid("Schedules require five cron fields"));
    }
    let domains = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
    let mut normalized = Vec::new();
    for (field, (minimum, maximum)) in fields.into_iter().zip(domains) {
        if !field
            .bytes()
            .all(|byte| byte.is_ascii_digit() || b"*/,-".contains(&byte))
        {
            return Err(unsupported(
                "Selected cron spelling is outside the qualified numeric syntax",
            ));
        }
        let mut arms = Vec::new();
        for arm in field.split(',') {
            let parts: Vec<_> = arm.split('/').collect();
            if arm.is_empty() || parts.len() > 2 || parts.iter().any(|part| part.is_empty()) {
                return Err(unsupported(
                    "Selected cron separators have unqualified publisher semantics",
                ));
            }
            let base = parts[0];
            if base != "*"
                && (base.split('-').count() > 2
                    || base.split('-').any(|part| {
                        part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit())
                    }))
            {
                return Err(unsupported(
                    "Selected cron range spelling is outside the qualified syntax",
                ));
            }
            let step = if parts.len() == 2 {
                let value = parts[1]
                    .parse::<u64>()
                    .map_err(|_| unsupported("Cron step is outside the qualified numeric range"))?;
                if value == 0 {
                    return Err(invalid("Cron steps must be positive"));
                }
                if value > 60 {
                    return Err(unsupported(
                        "Large cron steps are not qualified for this connection",
                    ));
                }
                Some(value)
            } else {
                None
            };
            let base = if base == "*" {
                format!("{minimum}-{maximum}")
            } else if step.is_some() && base.bytes().all(|byte| byte.is_ascii_digit()) {
                format!("{base}-{base}")
            } else {
                base.to_owned()
            };
            arms.push(step.map_or_else(|| base.clone(), |step| format!("{base}/{step}")));
        }
        normalized.push(arms.join(","));
    }
    Ok(normalized.join(" "))
}

impl MonitorSchedule {
    pub fn from_fixture(value: &Value, next: Option<&str>) -> Result<Self, ScheduleError> {
        let mut source = value.clone();
        let legacy = source
            .as_object_mut()
            .ok_or_else(|| invalid("Fixture schedule must be an object"))?
            .remove("intervalMinutes")
            .is_some();
        let parsed = Self::parse(&source)?;
        if legacy
            && source
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| {
                    matches!(
                        text.trim().to_ascii_lowercase().as_str(),
                        "daily" | "every day"
                    )
                })
            && let Some(next) = next
        {
            let next = DateTime::parse_from_rfc3339(next)
                .map_err(|_| invalid("Fixture next-run timestamp is invalid"))?
                .with_timezone(&parsed.timezone);
            return Self::parse(&json!({
                "cron":format!("{} {} * * *", next.minute(), next.hour()),
                "timezone":parsed.timezone.name()
            }));
        }
        Ok(parsed)
    }

    pub fn parse(value: &Value) -> Result<Self, ScheduleError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("Schedule must be an object"))?;
        if object
            .keys()
            .any(|key| !["cron", "text", "timezone"].contains(&key.as_str()))
        {
            return Err(invalid("Schedule contains unknown fields"));
        }
        let expression = match (object.get("cron"), object.get("text")) {
            (Some(Value::String(expression)), None) => expression.clone(),
            (None, Some(Value::String(text))) => {
                if text.is_empty() || text.len() > 128 {
                    return Err(invalid("Schedule text length is out of range"));
                }
                text_expression(text)?
            }
            _ => return Err(invalid("Schedule requires exactly one cron or text string")),
        };
        if expression.is_empty() || expression.len() > 128 {
            return Err(invalid("Cron length is out of range"));
        }
        let timezone = match object.get("timezone") {
            None => "UTC",
            Some(Value::String(timezone)) if !timezone.is_empty() && timezone.len() <= 128 => {
                timezone
            }
            _ => return Err(invalid("Schedule timezone must be a nonempty string")),
        };
        let timezone = timezone
            .parse::<Tz>()
            .ok()
            .or_else(|| {
                chrono_tz::TZ_VARIANTS
                    .iter()
                    .find(|candidate| candidate.name().eq_ignore_ascii_case(timezone))
                    .copied()
            })
            .ok_or_else(|| invalid("Unknown schedule timezone"))?;
        let compatible = compatible_expression(&expression)?;
        let parsed = Cron::new(&compatible)
            .with_dom_and_dow()
            .parse()
            .map_err(|_| invalid("Invalid numeric cron expression"))?;
        Ok(Self {
            expression: expression.split_whitespace().collect::<Vec<_>>().join(" "),
            timezone,
            parsed,
        })
    }

    pub fn normalized(&self) -> Value {
        json!({"cron":self.expression,"timezone":self.timezone.name()})
    }

    fn matches(&self, instant: &DateTime<Utc>) -> Result<bool, ScheduleError> {
        let local = instant.with_timezone(&self.timezone);
        let pattern = &self.parsed.pattern;
        let matching = || -> Result<bool, croner::errors::CronError> {
            Ok(pattern.minute_match(local.minute())?
                && pattern.hour_match(local.hour())?
                && pattern.month_match(local.month())?
                && pattern.day_match(local.year(), local.month(), local.day())?)
        };
        matching()
            .map_err(|_| unsupported("The selected cron matcher could not evaluate this instant"))
    }

    pub fn next_after(&self, from: i64) -> Result<i64, ScheduleError> {
        let minute = from
            .div_euclid(60)
            .checked_add(1)
            .and_then(|minute| minute.checked_mul(60))
            .ok_or_else(|| invalid("Schedule reference is out of range"))?;
        let mut candidate = DateTime::<Utc>::from_timestamp(minute, 0)
            .ok_or_else(|| invalid("Schedule reference is out of range"))?;
        for _ in 0..SEARCH_MINUTES {
            if self.matches(&candidate)? {
                return Ok(candidate.timestamp());
            }
            candidate = candidate
                .checked_add_signed(Duration::minutes(1))
                .ok_or_else(|| invalid("Schedule search exceeds the supported date range"))?;
        }
        Err(invalid(
            "No schedule occurrence within the bounded search horizon",
        ))
    }

    pub fn validate_at(&self, from: i64) -> Result<i64, ScheduleError> {
        let first = self.next_after(from)?;
        let second = self.next_after(first)?;
        if second - first < 300 {
            return Err(invalid("Monitors can run at most every five minutes"));
        }
        Ok(first)
    }

    pub fn next_rfc3339(&self, from: i64) -> Result<String, ScheduleError> {
        let next = self.validate_at(from)?;
        Ok(DateTime::<Utc>::from_timestamp(next, 0)
            .ok_or_else(|| invalid("Schedule result is out of range"))?
            .to_rfc3339_opts(SecondsFormat::Millis, true))
    }

    pub fn is_daily(&self) -> Result<bool, ScheduleError> {
        let pattern = &self.parsed.pattern;
        let count = |limit, predicate: &dyn Fn(u32) -> Result<bool, croner::errors::CronError>| {
            (0..limit).try_fold(0, |count, value| {
                predicate(value).map(|matches| count + usize::from(matches))
            })
        };
        if count(60, &|minute| pattern.minute_match(minute))
            .map_err(|_| invalid("Invalid minute matcher"))?
            != 1
            || count(24, &|hour| pattern.hour_match(hour))
                .map_err(|_| invalid("Invalid hour matcher"))?
                != 1
        {
            return Ok(false);
        }
        for month in 1..=12 {
            if !pattern
                .month_match(month)
                .map_err(|_| invalid("Invalid month matcher"))?
            {
                return Ok(false);
            }
        }
        let mut date = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2400, 1, 1).unwrap();
        while date < end {
            if !pattern
                .day_match(date.year(), date.month(), date.day())
                .map_err(|_| invalid("Invalid day matcher"))?
            {
                return Ok(false);
            }
            date = date
                .succ_opt()
                .ok_or_else(|| invalid("Calendar cycle exceeds the supported range"))?;
        }
        Ok(true)
    }
}
