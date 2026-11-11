//! Production has no adjudication step, so free text an agent writes in a scored task must get a
//! pass/fail verdict whatever its wording.

use std::path::Path;

use catalog::Catalog;
use scenario::{Scenario, coverage, discover, oracle_tools::OracleTools};

const GAP: &str = "text_requires_adjudication";

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

#[test]
fn every_free_text_check_of_a_scored_scenario_has_a_deterministic_verdict() {
    let scenarios = discover(&root().join("scenarios")).unwrap();
    let mut checks = 0;
    let mut undecided = Vec::new();
    for scenario in scenarios.iter().filter(|scenario| scenario.scored()) {
        for record in &scenario.expect.records {
            record
                .validate()
                .unwrap_or_else(|error| panic!("{}: {error}", scenario.meta.id));
            let texts = record
                .reviewed_text
                .iter()
                .map(|(field, text)| (field.as_str(), text))
                .chain(
                    record
                        .firecrawl_monitor
                        .iter()
                        .map(|goal| ("firecrawl_monitor.judged_goal", &goal.judged_goal)),
                );
            for (field, text) in texts {
                checks += 1;
                if !text.deterministic() {
                    undecided.push(format!("{} {field}", scenario.meta.id));
                }
            }
            if let Some(fields) = &record.reviewed_fields {
                checks += 1;
                if !fields.deterministic() {
                    undecided.push(format!("{} reviewed_fields", scenario.meta.id));
                }
            }
        }
    }
    assert!(
        undecided.is_empty(),
        "required text without mentions can only be recorded as a failure: {undecided:#?}"
    );
    // 44 reviewed_text checks, one reviewed_fields projection and one monitor goal.
    assert!(checks >= 46, "only {checks} free-text checks were found");
}

#[test]
fn coverage_reports_required_text_without_mentions_as_a_source_gap() {
    let catalog = Catalog::load(&root().join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(root(), &catalog).unwrap();
    let gaps = |scenario: &Scenario| {
        let calls = scenario
            .validate(&root().join("scenarios"), &catalog, &tools)
            .unwrap();
        let inventory =
            coverage::inventory(std::slice::from_ref(scenario), vec![calls], &tools).unwrap();
        (
            inventory.scenarios[0].source_gaps.contains(&GAP),
            inventory
                .summary
                .scored_source_gaps
                .get(GAP)
                .copied()
                .unwrap_or(0),
        )
    };
    let mut bug = Scenario::load(&root().join("scenarios/code/github/file-login-bug")).unwrap();
    assert_eq!(gaps(&bug), (false, 0));
    for record in &mut bug.expect.records {
        for text in record.reviewed_text.values_mut() {
            text.mentions.clear();
        }
    }
    assert_eq!(gaps(&bug), (true, 1));

    // Optional narration needs no mentions; a multi-field projection and a monitor goal do.
    let review =
        Scenario::load(&root().join("scenarios/code/github/approve-merge-dependency-bump"))
            .unwrap();
    assert_eq!(gaps(&review), (false, 0));
    let mut apple =
        Scenario::load(&root().join("scenarios/collab/jira/file-apple-pay-bug")).unwrap();
    assert_eq!(gaps(&apple), (false, 0));
    for record in &mut apple.expect.records {
        if let Some(fields) = &mut record.reviewed_fields {
            fields.mentions.clear();
        }
    }
    assert_eq!(gaps(&apple), (true, 1));
    let mut monitor =
        Scenario::load(&root().join("scenarios/search/firecrawl/monitor-competitor-pricing"))
            .unwrap();
    assert_eq!(gaps(&monitor), (false, 0));
    for record in &mut monitor.expect.records {
        if let Some(goal) = &mut record.firecrawl_monitor {
            goal.judged_goal.mentions.clear();
        }
    }
    assert_eq!(gaps(&monitor), (true, 1));
}
