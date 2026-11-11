use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockMode {
    #[default]
    Fixed,
    LegacyTransport,
}

pub fn parse_rfc3339_nanos(source: &str) -> Option<i128> {
    let bytes = source.as_bytes();
    if !source.is_ascii()
        || bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
        || [(0, 4), (5, 7), (8, 10), (11, 13), (14, 16), (17, 19)]
            .iter()
            .any(|(start, end)| !bytes[*start..*end].iter().all(u8::is_ascii_digit))
        || source[17..19].parse::<u8>().ok()? > 59
    {
        return None;
    }
    let mut rest = &source[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let length = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=9).contains(&length) {
            return None;
        }
        rest = &fraction[length..];
    }
    if !matches!(rest, "Z" | "z")
        && !(rest.len() == 6
            && matches!(rest.as_bytes()[0], b'+' | b'-')
            && rest.as_bytes()[3] == b':'
            && rest.as_bytes()[1..3].iter().all(u8::is_ascii_digit)
            && rest.as_bytes()[4..6].iter().all(u8::is_ascii_digit))
    {
        return None;
    }
    OffsetDateTime::parse(source, &Rfc3339)
        .ok()
        .map(|instant| instant.unix_timestamp_nanos())
}

pub fn format_rfc3339_nanos(nanos: i128) -> Option<String> {
    OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .ok()?
        .format(&Rfc3339)
        .ok()
}

pub fn validate_evidence(
    initial: &serde_json::Value,
    final_state: &serde_json::Value,
) -> std::result::Result<(), String> {
    if initial["clock"] != final_state["clock"] {
        return Err("business-clock policy changed during the trial".into());
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Descriptor {
        mode: ClockMode,
        reference: String,
        step_secs: i64,
    }
    let descriptor: Descriptor = serde_json::from_value(initial["clock"].clone())
        .map_err(|_| "business-clock metadata is missing or invalid")?;
    let reference = LogicalClock::new(&descriptor.reference, descriptor.step_secs)
        .map_err(|_| "business reference time is invalid")?
        .now_unix();
    let starting = initial["now"]
        .as_str()
        .ok_or("initial business time is missing")?;
    let ending = final_state["now"]
        .as_str()
        .ok_or("final business time is missing")?;
    let starting = LogicalClock::new(starting, 0)
        .map_err(|_| "initial business time is invalid")?
        .now_unix();
    let ending = LogicalClock::new(ending, 0)
        .map_err(|_| "final business time is invalid")?
        .now_unix();
    if starting != reference
        || ending < starting
        || (descriptor.mode == ClockMode::Fixed && ending != reference)
        || (descriptor.mode == ClockMode::LegacyTransport
            && (descriptor.step_secs == 0 && ending != starting
                || descriptor.step_secs > 0 && (ending - starting) % descriptor.step_secs != 0))
    {
        return Err("business time does not match the declared clock policy".into());
    }
    Ok(())
}

/// Scenario business time, independent of the performance clock.
#[derive(Debug, Clone)]
pub struct LogicalClock {
    current: i64,
    step_secs: i64,
}

impl LogicalClock {
    pub const DEFAULT_STEP_SECS: i64 = 7;

    pub fn new(start_rfc3339: &str, step_secs: i64) -> Result<Self> {
        if step_secs < 0 {
            return Err(Error::Clock {
                message: "clock_step_secs must not be negative".into(),
            });
        }
        let start = OffsetDateTime::parse(start_rfc3339, &Rfc3339)
            .map_err(|source| Error::Timestamp {
                value: start_rfc3339.to_owned(),
                source,
            })?
            .unix_timestamp();
        Ok(Self {
            current: start,
            step_secs,
        })
    }

    pub fn now_unix(&self) -> i64 {
        self.current
    }

    pub fn now_rfc3339(&self) -> String {
        format_unix(self.now_unix())
    }

    pub fn transport_step(&mut self, mode: ClockMode) -> Result<()> {
        if mode == ClockMode::LegacyTransport {
            self.advance(self.step_secs)?;
        }
        Ok(())
    }

    /// Advances one step and returns the new time.
    pub fn tick(&mut self) -> i64 {
        self.advance(self.step_secs)
            .expect("configured clock step is in range")
    }

    pub fn advance(&mut self, seconds: i64) -> Result<i64> {
        let next = self
            .current
            .checked_add(seconds)
            .filter(|next| seconds >= 0 && OffsetDateTime::from_unix_timestamp(*next).is_ok())
            .ok_or_else(|| Error::Clock {
                message: "time advance is negative or outside the supported range".into(),
            })?;
        self.current = next;
        Ok(self.now_unix())
    }
}

/// Formats a unix timestamp as second-precision RFC 3339 UTC, e.g. `2026-03-14T09:00:00Z`.
pub fn format_unix(secs: i64) -> String {
    OffsetDateTime::from_unix_timestamp(secs)
        .expect("timestamp within supported range")
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .expect("RFC 3339 formatting of a UTC timestamp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_scenario_now() {
        let clock = LogicalClock::new("2026-03-14T09:00:00Z", 7).unwrap();
        assert_eq!(clock.now_rfc3339(), "2026-03-14T09:00:00Z");
    }

    #[test]
    fn tick_advances_by_step() {
        let mut clock = LogicalClock::new("2026-03-14T09:00:00Z", 7).unwrap();
        let after = clock.tick();
        assert_eq!(after, clock.now_unix());
        assert_eq!(clock.now_rfc3339(), "2026-03-14T09:00:07Z");
        clock.tick();
        assert_eq!(clock.now_rfc3339(), "2026-03-14T09:00:14Z");
    }

    #[test]
    fn normalizes_offsets_to_utc() {
        let clock = LogicalClock::new("2026-03-14T10:00:00+01:00", 7).unwrap();
        assert_eq!(clock.now_rfc3339(), "2026-03-14T09:00:00Z");
    }

    #[test]
    fn rejects_bad_timestamp() {
        assert!(LogicalClock::new("yesterday", 7).is_err());
    }
}
