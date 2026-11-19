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
            "It is 15:00 UTC on Thursday 29 October 2026.",
        ),
        (
            "collab/google/schedule-q1-planning",
            PromptDate::Date,
            "It is 15:00 UTC on Thursday 29 October 2026.",
        ),
        (
            "collab/multi/slack-pagerduty-dbre-oncall",
            PromptDate::DateTime,
            "It is 14:00 UTC on Friday 30 October 2026.",
        ),
    ] {
        let scenario = Scenario::load(&source(id)).unwrap();
        assert_eq!(scenario.meta.prompt_date, Some(declared), "{id}");
        assert!(scenario.instruction.starts_with(statement), "{id}");
    }
}

#[test]
fn a_declared_scenario_does_not_load_without_its_date() {
    let id = "collab/linear/pull-issue-into-cycle";
    let undated = with_instruction(id, |text| {
        text.replace("It is 15:00 UTC on Thursday 29 October 2026. ", "")
    });
    let error = Scenario::load(undated.path()).unwrap_err().to_string();
    assert!(
        error.contains(id) && error.contains("It is 15:00 UTC on Thursday 29 October 2026"),
        "{error}"
    );

    // The agent's own date in round `utl1`, which made cycle 43 the "current" one.
    let misdated = with_instruction(id, |text| {
        text.replace("Thursday 29 October 2026", "Thursday 19 November 2026")
    });
    let error = Scenario::load(misdated.path()).unwrap_err().to_string();
    assert!(error.contains("19 november 2026"), "{error}");
}

/// The dated prompts of the other groups carry no declaration. Their statement is still held
/// to `now`: every scenario loads, and a copy with the wrong weekday does not.
#[test]
fn a_dated_prompt_with_no_declaration_is_still_held_to_now() {
    let id = "ops/pagerduty/payments-oncall-now";
    let scenario = Scenario::load(&source(id)).unwrap();
    assert_eq!(scenario.meta.prompt_date, None);
    assert!(
        scenario
            .instruction
            .starts_with("It is 14:00 UTC on Friday 30 October 2026.")
    );
    let wrong_weekday = with_instruction(id, |text| text.replace("on Friday 30", "on Saturday 30"));
    let error = Scenario::load(wrong_weekday.path())
        .unwrap_err()
        .to_string();
    assert!(error.contains("saturday 30 october 2026"), "{error}");
}
