use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::RecordCheckError;
use crate::{clock::parse_rfc3339_nanos, json_pointer::tokens};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntervalCoverage {
    pub start_field: String,
    pub end_field: String,
    pub start: String,
    pub end: String,
}

impl IntervalCoverage {
    pub fn validate(&self) -> Result<(), String> {
        for field in [&self.start_field, &self.end_field] {
            let parts = tokens(field)?;
            if parts.is_empty()
                || parts
                    .iter()
                    .any(|part| part.is_empty() || part.contains('*'))
            {
                return Err("interval boundaries require literal record-field pointers".into());
            }
        }
        if self.start_field == self.end_field
            || self
                .start_field
                .starts_with(&format!("{}/", self.end_field))
            || self
                .end_field
                .starts_with(&format!("{}/", self.start_field))
        {
            return Err("interval boundary fields must be distinct and disjoint".into());
        }
        let Some((start, end)) = self.bounds() else {
            return Err("interval coverage requires exact supported RFC3339 instants".into());
        };
        if start >= end {
            return Err("interval coverage must have strictly increasing endpoints".into());
        }
        Ok(())
    }

    fn bounds(&self) -> Option<(i128, i128)> {
        Some((
            parse_rfc3339_nanos(&self.start)?,
            parse_rfc3339_nanos(&self.end)?,
        ))
    }

    fn record_bounds(&self, record: &Value) -> Result<(i128, i128), RecordCheckError> {
        let boundary = |field: &str| {
            record
                .pointer(field)
                .and_then(Value::as_str)
                .and_then(parse_rfc3339_nanos)
                .ok_or_else(|| {
                    RecordCheckError::Failed(format!(
                        "interval boundary {field} is not an exact supported instant"
                    ))
                })
        };
        Ok((boundary(&self.start_field)?, boundary(&self.end_field)?))
    }

    pub(super) fn check_record(&self, record: &Value) -> Result<(), RecordCheckError> {
        let (expected_start, expected_end) = self.bounds().ok_or_else(|| {
            RecordCheckError::InvalidContract("invalid interval coverage bounds".into())
        })?;
        let (start, end) = self.record_bounds(record)?;
        if start >= end || start < expected_start || end > expected_end {
            return Err(RecordCheckError::Failed(
                "committed interval is empty, reversed or extends outside the authorized window"
                    .into(),
            ));
        }
        Ok(())
    }

    pub(super) fn check(&self, records: &[(String, &Value)]) -> Result<(), RecordCheckError> {
        let (expected_start, expected_end) = self.bounds().ok_or_else(|| {
            RecordCheckError::InvalidContract("invalid interval coverage bounds".into())
        })?;
        let mut intervals = Vec::with_capacity(records.len());
        for (_, record) in records {
            self.check_record(record)?;
            intervals.push(self.record_bounds(record)?);
        }
        intervals.sort_unstable();
        let mut covered_until = expected_start;
        for (start, end) in intervals {
            if start > covered_until {
                return Err(RecordCheckError::Failed(
                    "interval coverage has a gap".into(),
                ));
            }
            covered_until = covered_until.max(end);
        }
        if covered_until != expected_end {
            return Err(RecordCheckError::Failed(
                "interval coverage does not reach the requested end".into(),
            ));
        }
        Ok(())
    }
}
