//! `[scenario] prompt_date` is enforced when a scenario is loaded, so no task whose outcome
//! depends on today can reach an agent without the date.

use std::{fs, path::Path};

use scenario::{Scenario, prompt_date::PromptDate};

fn source(id: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(id)
}

/// A copy of the scenario's two authored files with the instruction rewritten.
fn with_instruction(id: &str, rewrite: impl FnOnce(&str) -> String) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    fs::copy(
        source(id).join("scenario.toml"),
        copy.path().join("scenario.toml"),
    )
    .unwrap();
    let instruction = fs::read_to_string(source(id).join("instruction.md")).unwrap();
    fs::write(copy.path().join("instruction.md"), rewrite(&instruction)).unwrap();
    copy
}

#[test]
fn the_tasks_that_depend_on_today_declare_it_and_state_it() {
    for (id, declared, statement) in [
        (
            "collab/linear/pull-issue-into-cycle",
            PromptDate::Date,
            "Today is 2026-10-29",
        ),
        (
            "collab/google/schedule-q1-planning",
            PromptDate::Date,
            "Today is 2026-10-29",
        ),
        (
            "collab/multi/slack-pagerduty-dbre-oncall",
            PromptDate::DateTime,
            "Today is 2026-10-30",
        ),
    ] {
        let scenario = Scenario::load(&source(id)).unwrap();
        assert_eq!(scenario.meta.prompt_date, Some(declared), "{id}");
        assert!(scenario.instruction.contains(statement), "{id}");
        assert!(
            scenario
                .meta
                .now
                .starts_with(&statement["Today is ".len()..]),
            "{id}"
        );
    }
}

#[test]
fn a_declared_scenario_does_not_load_without_its_date() {
    let id = "collab/linear/pull-issue-into-cycle";
    let undated = with_instruction(id, |text| text.replace("Today is 2026-10-29. ", ""));
    let error = Scenario::load(undated.path()).unwrap_err().to_string();
    assert!(
        error.contains(id) && error.contains("Today is 2026-10-29"),
        "{error}"
    );

    // The agent's own date in round `utl1`, which made cycle 43 the "current" one.
    let misdated = with_instruction(id, |text| text.replace("2026-10-29", "2026-11-19"));
    let error = Scenario::load(misdated.path()).unwrap_err().to_string();
    assert!(error.contains("2026-11-19"), "{error}");
}

#[test]
fn a_declared_time_does_not_load_without_its_utc_time() {
    let id = "collab/multi/slack-pagerduty-dbre-oncall";
    let untimed = with_instruction(id, |text| text.replace("14:00 UTC", "two in the afternoon"));
    let error = Scenario::load(untimed.path()).unwrap_err().to_string();
    assert!(error.contains("14:00 UTC"), "{error}");
}
