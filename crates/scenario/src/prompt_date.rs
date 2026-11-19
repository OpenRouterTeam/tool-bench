//! A prompt whose outcome depends on today says what today is.
//!
//! The agent's own clock reads the day the trial runs. The world's clock reads the scenario's
//! `now`. A request such as "the current cycle" or "who is on call now" has its one answer on
//! the scenario's date only: an agent that compares the listed dates with its own date, or
//! passes its own date to a tool, answers for a different day. `[scenario] prompt_date`
//! declares that dependence, and the loader then requires the instruction to state `now`.
//!
//! Two statements are read:
//!
//! - the form the dated prompts use: "It is 14:00 UTC on Friday 30 October 2026";
//! - the form the runner's surface audit reads (`DATE_PHRASE` in
//!   `deploy/runner/surface_audit.py`): "Today is 2026-10-30", "Today's date is ..." or "The
//!   current date is ...", with a colon in place of "is" if preferred.
//!
//! A statement in either form that disagrees with `now` (the time, the weekday or the date) is
//! refused in every scenario, declared or not.

use serde::Deserialize;

/// What the instruction must state about the scenario's `now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptDate {
    /// The calendar date: the outcome is the same at any time on that day.
    Date,
    /// The calendar date and the hour and minute in UTC.
    DateTime,
}

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const TODAY_LEADS: [&str; 6] = [
    "today's date is",
    "today's date:",
    "today is",
    "today:",
    "current date is",
    "current date:",
];
const CLOCK_LEAD: &str = "it is ";
const CLOCK_TAIL: &str = " utc on ";

fn iso_date(text: &str) -> Option<&str> {
    let date = text.get(..10)?;
    date.bytes()
        .enumerate()
        .all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        })
        .then_some(date)
}

fn clock(text: &str) -> Option<&str> {
    let time = text.get(..5)?;
    time.bytes()
        .enumerate()
        .all(|(index, byte)| match index {
            2 => byte == b':',
            _ => byte.is_ascii_digit(),
        })
        .then_some(time)
}

fn starts_a_word(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|before| !before.is_alphanumeric() && before != '_')
}

/// The weekday of a proleptic Gregorian date, Monday first.
fn weekday(year: i64, month: i64, day: i64) -> usize {
    // Days since 1970-01-01, a Thursday.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    (days + 3).rem_euclid(7) as usize
}

/// `now` as the dated prompts spell it: `("14:00", "Friday 30 October 2026")`.
fn spelled(now: &str) -> Option<(&str, String)> {
    let date = iso_date(now)?;
    let time = clock(now.get(11..)?)?;
    let year: i64 = date[..4].parse().ok()?;
    let month: i64 = date[5..7].parse().ok()?;
    let day: i64 = date[8..].parse().ok()?;
    let name = MONTHS.get(usize::try_from(month).ok()?.checked_sub(1)?)?;
    let weekday = WEEKDAYS[weekday(year, month, day)];
    Some((time, format!("{weekday} {day} {name} {year}")))
}

/// Every date the instruction states as today in the surface audit's form.
pub fn stated_today(instruction: &str) -> Vec<String> {
    let lower = instruction.to_ascii_lowercase();
    let mut dates = Vec::new();
    for lead in TODAY_LEADS {
        let mut from = 0;
        while let Some(found) = lower[from..].find(lead) {
            let start = from + found;
            from = start + lead.len();
            if let Some(date) = iso_date(lower[from..].trim_start())
                && starts_a_word(&lower, start)
            {
                dates.push(date.to_owned());
            }
        }
    }
    dates
}

/// Every "It is HH:MM UTC on ..." statement: its time and the words after "on", lowercased.
pub fn stated_clock(instruction: &str) -> Vec<(String, String)> {
    let lower = instruction.to_ascii_lowercase();
    let mut statements = Vec::new();
    let mut from = 0;
    while let Some(found) = lower[from..].find(CLOCK_LEAD) {
        let start = from + found;
        from = start + CLOCK_LEAD.len();
        let Some(time) = clock(&lower[from..]) else {
            continue;
        };
        let Some(rest) = lower[from + time.len()..].strip_prefix(CLOCK_TAIL) else {
            continue;
        };
        if starts_a_word(&lower, start) {
            let sentence = rest.split(['.', '\n']).next().unwrap_or_default();
            statements.push((time.to_owned(), sentence.trim().to_owned()));
        }
    }
    statements
}

/// Holds the instruction to the scenario's `now`.
pub fn check(now: &str, instruction: &str, declared: Option<PromptDate>) -> Result<(), String> {
    let today = iso_date(now).ok_or("`now` must start with a calendar date")?;
    let (time, date_words) = spelled(now).ok_or("`now` must be a calendar date and a time")?;
    let sentence = format!("It is {time} UTC on {date_words}");

    let today_statements = stated_today(instruction);
    if let Some(other) = today_statements.iter().find(|date| *date != today) {
        return Err(format!(
            "the instruction says today is {other}, and the scenario's `now` is {today}"
        ));
    }
    let clock_statements = stated_clock(instruction);
    if let Some((other_time, other_date)) = clock_statements.iter().find(|(stated, words)| {
        stated != time || !words.starts_with(&date_words.to_ascii_lowercase())
    }) {
        return Err(format!(
            "the instruction says it is {other_time} UTC on {other_date}, and the scenario's `now` reads \"{sentence}\""
        ));
    }
    let Some(declared) = declared else {
        return Ok(());
    };
    if !now.ends_with('Z') {
        return Err("`prompt_date` needs `now` in UTC, ending in `Z`".into());
    }
    if today_statements.is_empty() && clock_statements.is_empty() {
        return Err(format!(
            "`prompt_date` is declared, so the instruction must say \"{sentence}\" (or \"Today is {today}\")"
        ));
    }
    let utc = format!("{time} UTC");
    if declared == PromptDate::DateTime
        && clock_statements.is_empty()
        && !instruction.contains(&utc)
    {
        return Err(format!(
            "`prompt_date = \"date_time\"` is declared, so the instruction must also say \"{utc}\""
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-10-30T14:00:00Z";
    const SENTENCE: &str = "It is 14:00 UTC on Friday 30 October 2026.";

    #[test]
    fn spells_now_as_the_dated_prompts_do() {
        for (now, time, words) in [
            (NOW, "14:00", "Friday 30 October 2026"),
            ("2026-10-29T15:00:00Z", "15:00", "Thursday 29 October 2026"),
            ("2026-11-12T15:30:00Z", "15:30", "Thursday 12 November 2026"),
            ("2026-05-12T09:05:00Z", "09:05", "Tuesday 12 May 2026"),
            ("2024-02-29T00:00:00Z", "00:00", "Thursday 29 February 2024"),
            ("2000-01-01T12:00:00Z", "12:00", "Saturday 1 January 2000"),
            ("1970-01-01T00:00:00Z", "00:00", "Thursday 1 January 1970"),
            ("2027-01-03T23:59:00Z", "23:59", "Sunday 3 January 2027"),
        ] {
            assert_eq!(spelled(now), Some((time, words.to_owned())), "{now}");
        }
        assert_eq!(spelled("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn reads_the_statements_the_surface_audit_reads() {
        for (instruction, dates) in [
            (
                "Today is 2026-10-30. Please pull the issue in.",
                vec!["2026-10-30"],
            ),
            ("today's date is 2026-10-30", vec!["2026-10-30"]),
            ("Today's date: 2026-10-30.", vec!["2026-10-30"]),
            ("Today:2026-10-30", vec!["2026-10-30"]),
            (
                "The current date is  2026-10-30, so act on it.",
                vec!["2026-10-30"],
            ),
            // Not a statement of today in that form.
            ("We ship today (October 30, 2026).", vec![]),
            ("It is due on 2026-10-30 and today is busy.", vec![]),
            ("Yesterday's holiday is 2026-10-29.", vec![]),
        ] {
            assert_eq!(stated_today(instruction), dates, "{instruction}");
        }
    }

    #[test]
    fn a_declared_scenario_must_state_now() {
        let dated = format!("{SENTENCE} Marcus pinged me.");
        for declared in [PromptDate::Date, PromptDate::DateTime] {
            assert!(check(NOW, &dated, Some(declared)).is_ok());
            let missing = check(
                NOW,
                "Marcus pinged me about who is on call now.",
                Some(declared),
            );
            assert!(
                missing
                    .unwrap_err()
                    .contains(SENTENCE.trim_end_matches('.'))
            );
            // Words about today that carry no date do not state it.
            assert!(check(NOW, "We ship today.", Some(declared)).is_err());
        }
        // The audit's form states the date; the time then has to be given too.
        assert!(check(NOW, "Today is 2026-10-30.", Some(PromptDate::Date)).is_ok());
        assert!(check(NOW, "Today is 2026-10-30.", Some(PromptDate::DateTime)).is_err());
        let timed = "Today is 2026-10-30, and it's 14:00 UTC right now.";
        assert!(check(NOW, timed, Some(PromptDate::DateTime)).is_ok());
        let offset = check("2026-10-30T14:00:00+02:00", &dated, Some(PromptDate::Date));
        assert!(offset.is_err());
    }

    #[test]
    fn a_statement_that_is_not_now_is_refused_in_every_scenario() {
        for wrong in [
            // The agent's own date in round `utl1`.
            "It is 14:00 UTC on Wednesday 18 November 2026. Who is on call?",
            // The right date under the wrong weekday, and the wrong time.
            "It is 14:00 UTC on Thursday 30 October 2026. Who is on call?",
            "It is 15:00 UTC on Friday 30 October 2026. Who is on call?",
            "Today is 2026-11-18. Who is on call?",
        ] {
            for declared in [None, Some(PromptDate::Date), Some(PromptDate::DateTime)] {
                assert!(check(NOW, wrong, declared).is_err(), "{wrong}");
            }
        }
        // An undeclared scenario may state nothing, or give a date in other words.
        for free in [
            "Log the escalation reported today (October 30, 2026).",
            "It is urgent: the window is 02:00 to 04:00 UTC on 2026-10-31.",
            SENTENCE,
        ] {
            assert!(check(NOW, free, None).is_ok(), "{free}");
        }
    }
}
