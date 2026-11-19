//! The fairness admission check over the repository's own scenarios
//! (`docs/fairness.md`, rules F3 to F5).

use std::{collections::BTreeSet, path::Path};

use catalog::Catalog;
use mock_core::grading::CallMatch;
use scenario::{
    Scenario, discover,
    fairness::{Equivalence, Route, TaskFairness},
};

/// The search lookups of release 1 that name no vendor and keep their pages on one.
const SINGLE_VENDOR_SEARCH: [&str; 11] = [
    "search/brave/competitor-funding-news",
    "search/brave/hosting-sla-lookup",
    "search/brave/keynote-recording",
    "search/exa/find-vp-engineering-profile",
    "search/exa/sdk-timeout-default",
    "search/firecrawl/northwind-team-pricing",
    "search/multi/competitor-funding-and-pricing",
    "search/multi/competitor-research-github-issue",
    "search/multi/vendor-rate-limit-zendesk-reply",
    "search/tavily/vendor-rate-limit",
    "search/tavily/webhook-signature-sunset",
];

/// Their refined versions, which serve the pages on all four search services.
const ANY_SEARCH: [&str; 5] = [
    "diagnostic/multi/funding-news-any-search",
    "diagnostic/multi/hosting-sla-any-search",
    "diagnostic/multi/sdk-timeout-any-search",
    "diagnostic/multi/team-pricing-any-search",
    "diagnostic/multi/vendor-rate-limit-any-search",
];

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn equivalence() -> Equivalence {
    Equivalence::load(&root().join("catalog/equivalence.toml")).unwrap()
}

fn load(id: &str) -> Scenario {
    Scenario::load(&root().join("scenarios").join(id)).unwrap()
}

fn assess(id: &str) -> TaskFairness {
    equivalence().assess(&load(id)).unwrap()
}

#[test]
fn the_class_file_has_a_row_for_every_catalog_service_and_known_members() {
    let catalog = Catalog::load(&root().join("catalog/services.toml")).unwrap();
    let equivalence = equivalence();
    equivalence.validate(&catalog).unwrap();
    assert_eq!(equivalence.services.len(), catalog.services.len());
}

#[test]
fn the_single_vendor_search_lookups_are_flagged_and_their_refined_versions_pass() {
    for id in SINGLE_VENDOR_SEARCH {
        let task = assess(id);
        assert!(task.flagged && task.known, "{id}: {task:?}");
        assert_eq!(task.route, Route::Unnamed, "{id}");
        assert!(task.refused_classes().contains("web_search"), "{id}");
    }
    for id in ANY_SEARCH {
        let task = assess(id);
        assert!(!task.flagged, "{id}: {task:?}");
        assert_eq!(task.route, Route::Unnamed, "{id}");
        assert_eq!(
            task.accepted_classes(),
            BTreeSet::from(["web_search"]),
            "{id}"
        );
    }
}

/// The list in `catalog/equivalence.toml` is what the check flags among scored tasks, no
/// more and no fewer. The number only goes down: a repaired task leaves the list.
#[test]
fn the_known_list_is_exactly_the_scored_tasks_the_check_flags() {
    let equivalence = equivalence();
    let scenarios = discover(&root().join("scenarios")).unwrap();
    let report = equivalence.report(&scenarios).unwrap();
    report.admit().unwrap();
    let known: BTreeSet<&str> = equivalence.known.tasks.iter().map(String::as_str).collect();
    let flagged: BTreeSet<&str> = report.flagged.iter().map(String::as_str).collect();
    assert_eq!(known, flagged);
    assert_eq!(known.len(), equivalence.known.tasks.len(), "listed twice");
    assert_eq!(flagged.len(), 19);
    assert_eq!(report.summary.scored_tasks, 150);
    assert_eq!(report.summary.scored_routes["route:named"], 120);
    assert_eq!(report.summary.scored_routes["route:kind"], 11);
    assert_eq!(report.summary.scored_routes["route:unnamed"], 19);
    assert_eq!(report.summary.scored_flagged_by_class["web_search"], 11);
    assert_eq!(report.summary.scored_flagged_by_class["support_tickets"], 7);
    assert_eq!(report.summary.scored_flagged_by_class["crm"], 2);
    assert_eq!(report.summary.scored_flagged_by_class["orders"], 1);
}

#[test]
fn a_flagged_scored_task_outside_the_known_list_is_refused() {
    let mut equivalence = equivalence();
    equivalence
        .known
        .tasks
        .retain(|id| id != "business/hubspot/largest-open-deal");
    let scenarios = discover(&root().join("scenarios")).unwrap();
    let error = equivalence
        .report(&scenarios)
        .unwrap()
        .admit()
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("1 scored task(s) fail"), "{error}");
    assert!(
        error.contains("business/hubspot/largest-open-deal: the instruction does not name hubspot, and crm (hubspot, salesforce) fit its wording: the scenario serves no records on salesforce"),
        "{error}"
    );
}

#[test]
fn naming_the_product_clears_the_flag_and_a_kind_word_alone_does_not() {
    let equivalence = equivalence();
    let mut scenario = load("business/hubspot/largest-open-deal");
    assert!(scenario.instruction.contains("In our CRM"));
    let unnamed = equivalence.assess(&scenario).unwrap();
    assert!(unnamed.flagged);
    assert_eq!(
        unnamed.services[0].classes[0].kind_words,
        ["crm", "pipeline"]
    );

    scenario.instruction = scenario.instruction.replace("In our CRM", "In HubSpot");
    let named = equivalence.assess(&scenario).unwrap();
    assert!(!named.flagged);
    assert_eq!(named.route, Route::Named);
    assert_eq!(named.services[0].words, ["HubSpot"]);
}

#[test]
fn a_kind_only_one_service_has_is_a_route_of_its_own() {
    let caterer = assess("search/brave/caterer-saturday-hours");
    assert_eq!(caterer.route, Route::Kind);
    assert_eq!(caterer.services[0].words, ["local business listing"]);
    assert!(!caterer.flagged);
    let pod = assess("ops/multi/crashloop-oncall-lookup");
    assert_eq!(
        pod.route,
        Route::Kind,
        "PagerDuty is named, the cluster is not"
    );
    assert!(!pod.flagged);
}

#[test]
fn a_contract_entry_that_takes_one_member_is_not_acceptance() {
    let equivalence = equivalence();
    let mut scenario = load("diagnostic/multi/hosting-sla-any-search");
    scenario.expect.required_calls.push(CallMatch {
        service: "brave".into(),
        ..CallMatch::default()
    });
    let task = equivalence.assess(&scenario).unwrap();
    assert!(task.flagged);
    assert_eq!(
        task.services[0].classes[0].gaps,
        ["required_calls[0] takes brave alone"]
    );

    let mut scenario = load("diagnostic/multi/hosting-sla-any-search");
    scenario.meta.services.retain(|service| service != "tavily");
    let task = equivalence.assess(&scenario).unwrap();
    assert!(task.flagged);
    assert_eq!(
        task.services[0].classes[0].gaps,
        ["the scenario serves no records on tavily"]
    );
}

#[test]
fn a_route_tag_must_agree_with_the_instruction() {
    let equivalence = equivalence();
    let mut scenario = load("diagnostic/multi/hosting-sla-any-search");
    assert!(scenario.meta.tags.iter().any(|tag| tag == "route:unnamed"));
    for tag in &mut scenario.meta.tags {
        if tag == "route:unnamed" {
            *tag = "route:named".into();
        }
    }
    let error = equivalence.assess(&scenario).unwrap_err().to_string();
    assert_eq!(
        error,
        "diagnostic/multi/hosting-sla-any-search: tagged route:named, but its instruction reads as route:unnamed"
    );
}

#[test]
fn the_label_file_holds_every_scenario_and_reads_back() {
    let scenarios = discover(&root().join("scenarios")).unwrap();
    let labels = equivalence().report(&scenarios).unwrap().route_labels();
    assert_eq!(labels.labels.len(), scenarios.len());
    let committed: scenario::fairness::RouteLabels = serde_json::from_slice(
        &std::fs::read(root().join("docs/coverage/route-labels.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(committed.labels, labels.labels);
}
