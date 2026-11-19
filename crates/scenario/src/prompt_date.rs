//! A prompt whose outcome depends on today says what today is.
//!
//! The agent's own clock reads the day the trial runs. The world's clock reads the scenario's
//! `now`. A request such as "the current cycle" or "who is on call now" has its one answer on
//! the scenario's date only: an agent that compares the listed dates with its own date, or
//! passes its own date to a tool, answers for a different day. `[scenario] prompt_date`
//! declares that dependence, and the loader then requires the instruction to state the date.
//!
//! The statement has the form the runner's surface audit reads (`DATE_PHRASE` in
//! `deploy/runner/surface_audit.py`): "Today is 2026-10-29", "Today's date is ..." or "The
//! current date is ...", with a colon in place of "is" if preferred. A declared scenario is
//! therefore `coherent` in that audit and not `not_supplied`. A statement of that form that
//! names another day is refused in every scenario, declared or not.

use serde::Deserialize;

/// What the instruction must state about the scenario's `now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptDate {
    /// The calendar date: the outcome is the same at any time on that day.
    Date,
    /// The calendar date and the hour and minute, as `HH:MM UTC`.
    DateTime,
}

const LEADS: [&str; 6] = [
    "today's date is",
    "today's date:",
    "today is",
    "today:",
    "current date is",
    "current date:",
];

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

/// Every date the instruction states as today, in the surface audit's form.
pub fn stated(instruction: &str) -> Vec<String> {
    let lower = instruction.to_ascii_lowercase();
    let mut dates = Vec::new();
    for lead in LEADS {
        let mut from = 0;
        while let Some(found) = lower[from..].find(lead) {
            let start = from + found;
            from = start + lead.len();
            let starts_a_word = lower[..start]
                .chars()
                .next_back()
                .is_none_or(|before| !before.is_alphanumeric() && before != '_');
            if let Some(date) = iso_date(lower[from..].trim_start())
                && starts_a_word
            {
                dates.push(date.to_owned());
            }
        }
    }
    dates
}

/// Holds the instruction to the scenario's `now`.
pub fn check(now: &str, instruction: &str, declared: Option<PromptDate>) -> Result<(), String> {
    let today = iso_date(now).ok_or("`now` must start with a calendar date")?;
    let stated = stated(instruction);
    if let Some(other) = stated.iter().find(|date| *date != today) {
        return Err(format!(
            "the instruction says today is {other}, and the scenario's `now` is {today}"
        ));
    }
    let Some(declared) = declared else {
        return Ok(());
    };
    if !now.ends_with('Z') {
        return Err("`prompt_date` needs `now` in UTC, ending in `Z`".into());
    }
    if stated.is_empty() {
        return Err(format!(
            "`prompt_date` is declared, so the instruction must say \"Today is {today}\""
        ));
    }
    let time = format!("{} UTC", now.get(11..16).unwrap_or_default());
    if declared == PromptDate::DateTime && !instruction.contains(&time) {
        return Err(format!(
            "`prompt_date = \"date_time\"` is declared, so the instruction must also say \"{time}\""
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-10-29T15:00:00Z";

    #[test]
    fn reads_the_statements_the_surface_audit_reads() {
        for (instruction, dates) in [
            (
                "Today is 2026-10-29. Please pull the issue in.",
                vec!["2026-10-29"],
            ),
            ("today's date is 2026-10-29", vec!["2026-10-29"]),
            ("Today's date: 2026-10-29.", vec!["2026-10-29"]),
            ("Today:2026-10-29", vec!["2026-10-29"]),
            (
                "The current date is  2026-10-29, so act on it.",
                vec!["2026-10-29"],
            ),
            ("Current date: 2026-10-29", vec!["2026-10-29"]),
            // Not a statement of today in that form.
            ("We ship today (October 29, 2026).", vec![]),
            ("It is due on 2026-10-29 and today is busy.", vec![]),
            ("Yesterday's holiday is 2026-10-28.", vec![]),
            ("The report dated 2026-10-29 says today is fine.", vec![]),
        ] {
            assert_eq!(stated(instruction), dates, "{instruction}");
        }
    }

    #[test]
    fn a_declared_scenario_must_state_the_date_of_now() {
        assert!(
            check(
                NOW,
                "Today is 2026-10-29. Pull it in.",
                Some(PromptDate::Date)
            )
            .is_ok()
        );
        let missing = check(
            NOW,
            "Pull it into the current cycle.",
            Some(PromptDate::Date),
        );
        assert!(missing.unwrap_err().contains("Today is 2026-10-29"));
        // Words about today that carry no date do not state it.
        assert!(check(NOW, "We ship today.", Some(PromptDate::Date)).is_err());
    }

    #[test]
    fn a_declared_time_must_be_stated_in_utc() {
        let declared = Some(PromptDate::DateTime);
        assert!(check(NOW, "Today is 2026-10-29 and it is 15:00 UTC.", declared).is_ok());
        assert!(check(NOW, "Today is 2026-10-29.", declared).is_err());
        assert!(check(NOW, "Today is 2026-10-29 and it is 16:00 UTC.", declared).is_err());
        assert!(
            check(
                "2026-10-29T15:00:00+02:00",
                "Today is 2026-10-29, 15:00 UTC.",
                declared
            )
            .is_err()
        );
    }

    #[test]
    fn a_stated_date_that_is_not_now_is_refused_in_every_scenario() {
        for declared in [None, Some(PromptDate::Date)] {
            let wrong = check(NOW, "Today is 2026-10-30. Pull it in.", declared);
            assert!(wrong.unwrap_err().contains("2026-10-30"));
        }
        // An undeclared scenario may state nothing, or state the date in other words.
        assert!(
            check(
                NOW,
                "Log the escalation reported today (October 29, 2026).",
                None
            )
            .is_ok()
        );
    }
}
