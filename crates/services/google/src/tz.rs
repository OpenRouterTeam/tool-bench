use chrono::Offset;

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// UTC offset in seconds and abbreviation of `zone_name` at unix time `t`.
pub fn offset_at(zone_name: &str, t: i64) -> (i64, String) {
    let timezone = zone_name.parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::UTC);
    let Some(moment) = chrono::DateTime::from_timestamp(t, 0) else {
        return (0, "UTC".into());
    };
    let local = moment.with_timezone(&timezone);
    (
        i64::from(local.offset().fix().local_minus_utc()),
        local.format("%Z").to_string(),
    )
}

/// Parses `YYYY-MM-DD` (or `YYYY/MM/DD`) to (y, m, d).
pub fn parse_date(s: &str) -> Option<(i64, i64, i64)> {
    let parts: Vec<&str> = s.trim().split(['-', '/']).collect();
    if parts.len() != 3 {
        return None;
    }
    let y = parts[0].parse().ok()?;
    let m = parts[1].parse().ok()?;
    let d = parts[2].parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some((y, m, d))
}

/// RFC 3339 UTC `2026-10-26T17:10:43Z`.
pub fn rfc3339(t: i64) -> String {
    let (y, m, d) = civil_from_days(t.div_euclid(86_400));
    let s = t.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chicago_dst_boundaries() {
        // 2026-10-26 17:10:43Z is CDT.
        let t = days_from_civil(2026, 10, 26) * 86_400 + 17 * 3600 + 10 * 60 + 43;
        assert_eq!(offset_at("America/Chicago", t), (-5 * 3600, "CDT".into()));
        // DST ends 2026-11-01 07:00Z.
        let end = days_from_civil(2026, 11, 1) * 86_400 + 7 * 3600;
        assert_eq!(offset_at("America/Chicago", end - 1).1, "CDT");
        assert_eq!(offset_at("America/Chicago", end).1, "CST");
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn civil_round_trip() {
        for d in [-1000, 0, 19_000, 20_757] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
    }
}
