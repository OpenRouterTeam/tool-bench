//! Rules F6 and F7 of `docs/fairness.md`, for every scenario the admission check finds
//! accepting a class of interchangeable services (`catalog/equivalence.toml`): a read on
//! each member, the way its MCP server issues it, returns the record that holds the facts
//! and every fact a reply must state; the reference reply then passes; no reply fails.
//!
//! The scenarios are discovered, not listed, so a new one that accepts a class is run
//! through each member without a change here.

mod support;

use std::path::Path;

use mock_core::{grading::Outcome, service::MockService};
use regex::Regex;
use scenario::{
    Scenario, discover,
    fairness::{Class, ClassRead, Equivalence},
};
use serde_json::Value;
use support::{ContractTrial, ServiceSpec};

/// The credential each mock asks for, as the tools container sends it.
const CREDENTIALS: [(&str, (&str, &str)); 4] = [
    ("brave", ("x-subscription-token", "BSA-contract")),
    ("exa", ("x-api-key", "exa-toolbench")),
    ("firecrawl", ("authorization", "Bearer local-firecrawl")),
    ("tavily", ("authorization", "Bearer tvly-toolbench")),
];

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Every scenario that passes the admission check by accepting a class, with that class.
fn accepting() -> Vec<(Scenario, Class)> {
    let equivalence = Equivalence::load(&root().join("catalog/equivalence.toml")).unwrap();
    let mut found = Vec::new();
    for scenario in discover(&root().join("scenarios")).unwrap() {
        let task = equivalence.assess(&scenario).unwrap();
        if task.flagged {
            continue;
        }
        for id in task.accepted_classes() {
            let class = equivalence
                .classes
                .iter()
                .find(|class| class.id == id)
                .unwrap();
            found.push((scenario.clone(), class.clone()));
        }
    }
    found
}

/// A trial of `scenario` with the mock of every member of `class`.
fn trial(scenario: &Scenario, class: &Class) -> ContractTrial {
    let mocks = world::services();
    let credentials: Vec<[(&str, &str); 1]> = class
        .members
        .iter()
        .map(|member| {
            let (_, header) = CREDENTIALS
                .iter()
                .find(|(name, _)| name == member)
                .unwrap_or_else(|| panic!("no credential listed for {member}"));
            [*header]
        })
        .collect();
    let specs: Vec<ServiceSpec<'_>> = class
        .members
        .iter()
        .zip(&credentials)
        .map(|(member, header)| {
            let mock: &dyn MockService = mocks
                .iter()
                .find(|mock| mock.name() == member)
                .unwrap_or_else(|| panic!("no mock for {member}"))
                .as_ref();
            (mock, header.as_slice())
        })
        .collect();
    ContractTrial::load_from_directory(&scenario.dir, &specs)
}

/// What the reference solution types into its first search.
fn query(scenario: &Scenario) -> &str {
    scenario
        .solution
        .calls
        .iter()
        .find_map(|call| call.arguments.get("query").and_then(Value::as_str))
        .unwrap_or_else(|| panic!("{}: no reference call has a query", scenario.meta.id))
}

fn percent_encoded(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The class's job asked of `member`, with the scenario's own query.
async fn read(trial: &ContractTrial, class: &Class, member: &str, query: &str) -> String {
    let ClassRead { method, path, body } = class
        .read
        .get(member)
        .unwrap_or_else(|| panic!("class {} has no read for {member}", class.id));
    let path = path.replace("{query_url}", &percent_encoded(query));
    let body = body.as_ref().map_or(Value::Null, |body| {
        let quoted = serde_json::to_string(query).unwrap();
        serde_json::from_str(&body.replace("{query}", &quoted[1..quoted.len() - 1])).unwrap()
    });
    trial
        .call_from(member, method, &path, body)
        .await
        .to_string()
}

/// The page that holds the facts and each fact a reply must state, read from the record
/// the answer contract selects.
fn facts(scenario: &Scenario, trial: &ContractTrial) -> Vec<(String, String)> {
    let id = &scenario.meta.id;
    let contract = scenario
        .expect
        .answer
        .as_ref()
        .and_then(|answer| answer.record_facts.as_ref())
        .unwrap_or_else(|| panic!("{id}: routes are compared on a record_facts answer"));
    let world = trial.world.lock().unwrap();
    let state = world.snapshot();
    let selected = contract
        .selector
        .selected_identities(Some(&world.initial_snapshot()), &state)
        .unwrap();
    let [identity] = selected.as_slice() else {
        panic!("{id}: the answer contract selects {selected:?}, not one record");
    };
    let record = &state.pointer(&contract.selector.collection).unwrap()[identity];
    let text = |pointer: &str| match record.pointer(pointer) {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => panic!("{id}: the selected record has no {pointer}"),
    };
    let page = record["url"].as_str().unwrap_or(identity).to_owned();
    let mut wanted = vec![("page".to_owned(), page)];
    for (name, fact) in &contract.facts {
        if fact.optional || fact.record.is_some() {
            continue;
        }
        let source = text(&fact.field);
        let value = match &fact.capture {
            Some(pattern) => Regex::new(pattern).unwrap().captures(&source).unwrap()[1].to_owned(),
            None => source,
        };
        wanted.push((name.clone(), value));
    }
    wanted
}

#[test]
fn the_refined_search_lookups_are_the_scenarios_that_accept_a_class() {
    let accepting = accepting();
    let ids: Vec<&str> = accepting
        .iter()
        .map(|(scenario, class)| {
            assert_eq!(class.id, "web_search");
            scenario.meta.id.as_str()
        })
        .collect();
    assert_eq!(
        ids,
        [
            "diagnostic/multi/funding-news-any-search",
            "diagnostic/multi/hosting-sla-any-search",
            "diagnostic/multi/sdk-timeout-any-search",
            "diagnostic/multi/team-pricing-any-search",
            "diagnostic/multi/vendor-rate-limit-any-search",
        ]
    );
}

/// Each member returns the page and the facts, and the reference reply passes after a
/// read on that member alone.
#[tokio::test]
async fn every_accepted_member_returns_the_facts_and_the_reference_reply_passes() {
    for (scenario, class) in accepting() {
        let id = &scenario.meta.id;
        for member in &class.members {
            let trial = trial(&scenario, &class);
            let returned = read(&trial, &class, member, query(&scenario)).await;
            for (name, value) in facts(&scenario, &trial) {
                assert!(
                    returned.contains(&value),
                    "{id}: {member} does not return {name} ({value:?})"
                );
            }
            let grade = trial.assess_with_answer(scenario.solution.answer.as_deref());
            assert_eq!(grade.outcome, Outcome::Pass, "{id} via {member}: {grade:?}");
            assert!(grade.history_ok && grade.preservation_ok, "{grade:?}");
        }
    }
}

/// Doing nothing scores 0, and so does a read on any member with no reply.
#[tokio::test]
async fn no_reply_fails_on_every_route() {
    for (scenario, class) in accepting() {
        let id = &scenario.meta.id;
        let idle = trial(&scenario, &class);
        assert_eq!(idle.assess().outcome, Outcome::Fail, "{id}: doing nothing");
        for member in &class.members {
            let trial = trial(&scenario, &class);
            read(&trial, &class, member, query(&scenario)).await;
            assert_eq!(trial.assess().outcome, Outcome::Fail, "{id} via {member}");
        }
    }
}
