use std::io::{self, Read};

use chrono::{DateTime, SecondsFormat, Utc};
use mock_core::monitor_schedule::{MonitorSchedule, ScheduleError};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Value> = serde_json::from_str(&input)?;
    let mut results = Vec::new();
    for case in cases {
        let reference =
            DateTime::parse_from_rfc3339(case["from"].as_str().ok_or("missing reference")?)?
                .timestamp();
        let result = MonitorSchedule::parse(&case["schedule"]).and_then(|parsed| {
            let first = parsed.validate_at(reference)?;
            let second = parsed.next_after(first)?;
            let format = |instant| {
                DateTime::<Utc>::from_timestamp(instant, 0)
                    .ok_or_else(|| ScheduleError::Invalid("result out of range".into()))
                    .map(|instant| instant.to_rfc3339_opts(SecondsFormat::Millis, true))
            };
            Ok(json!({
                "id":case["id"],"outcome":"ok","cron":parsed.normalized()["cron"],
                "first":format(first)?,"second":format(second)?,"daily":parsed.is_daily()?
            }))
        });
        results.push(match result {
            Ok(result) => result,
            Err(ScheduleError::Invalid(_)) => json!({"id":case["id"],"outcome":"invalid"}),
            Err(ScheduleError::Unsupported(_)) => json!({"id":case["id"],"outcome":"unsupported"}),
        });
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
