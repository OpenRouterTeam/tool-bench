//! The refined search lookups (`scenarios/diagnostic/multi/*-any-search`): the fact is
//! reachable through each of the four connected web-search services, any of them is
//! accepted, and an answer that does not state the fact still fails.

mod support;

use mock_core::grading::{CheckKind, Grade, Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const BRAVE_KEY: [(&str, &str); 1] = [("x-subscription-token", "BSA-contract")];
const EXA_KEY: [(&str, &str); 1] = [("x-api-key", "exa-toolbench")];
const FIRECRAWL_KEY: [(&str, &str); 1] = [("authorization", "Bearer local-firecrawl")];
const TAVILY_KEY: [(&str, &str); 1] = [("authorization", "Bearer tvly-toolbench")];

struct Refined {
    id: &'static str,
    /// What an agent would type into a search tool, as a URL query value and as text.
    brave_query: &'static str,
    query: &'static str,
    /// The page that holds the fact.
    page: &'static str,
    /// A passage of that page that carries the fact.
    passage: &'static str,
    right: &'static str,
    /// The same reply with a look-alike or superseded page's values.
    stale: &'static str,
}

const HOSTING: Refined = Refined {
    id: "diagnostic/multi/hosting-sla-any-search",
    brave_query: "Ironbark+Hosting+SLA+Business+plan+uptime+service+credit",
    query: "Ironbark Hosting SLA Business plan uptime service credit",
    page: "https://www.ironbarkhosting.com/legal/sla",
    passage: "99.95%",
    right: "Uptime: 99.95% Service credit: 10% of the monthly fee",
    stale: "Uptime: 99.9% Service credit: 5% of the monthly fee",
};

const FUNDING: Refined = Refined {
    id: "diagnostic/multi/funding-news-any-search",
    brave_query: "Norvik+Analytics+funding+round",
    query: "Norvik Analytics funding round",
    page: "https://www.techbeat.io/2026/10/27/norvik-analytics-series-b",
    passage: "Halcyon Ventures",
    right: "Amount: $45M Round: Series B Lead investor: Halcyon Ventures",
    stale: "Amount: $12M Round: Series A Lead investor: Fjord Capital",
};

const RATE_LIMIT: Refined = Refined {
    id: "diagnostic/multi/vendor-rate-limit-any-search",
    brave_query: "Quillstream+API+rate+limit+Business+plan+requests+per+minute",
    query: "Quillstream API rate limit Business plan requests per minute",
    page: "https://docs.quillstream.io/api/rate-limits",
    passage: "1,200",
    right: "1,200 requests per minute (https://docs.quillstream.io/api/rate-limits)",
    stale: "600 requests per minute (https://docs.quillstream.io/v2/api/rate-limits)",
};

const TIMEOUT: Refined = Refined {
    id: "diagnostic/multi/sdk-timeout-any-search",
    brave_query: "Driftwood+Python+SDK+v4+client+configuration+timeout+option+default",
    query: "Driftwood Python SDK v4 client configuration timeout option default",
    page: "https://docs.driftwood.dev/python/v4/configuration",
    passage: "timeout_seconds",
    right: "Option: timeout_seconds Default: 30 seconds",
    stale: "Option: request_timeout Default: 60 seconds",
};

const PRICING: Refined = Refined {
    id: "diagnostic/multi/team-pricing-any-search",
    brave_query: "Northwind+Analytics+pricing+Team+plan",
    query: "Northwind Analytics pricing Team plan",
    page: "https://www.northwind-analytics.com/pricing",
    passage: "$79/month",
    right: "Team plan: $79/month billed annually, 12 seats included.",
    stale: "Team plan: $69/month billed annually, 10 seats included.",
};

const ALL: [&Refined; 5] = [&HOSTING, &FUNDING, &RATE_LIMIT, &TIMEOUT, &PRICING];
const SERVICES: [&str; 4] = ["brave", "exa", "firecrawl", "tavily"];

fn trial(task: &Refined) -> ContractTrial {
    ContractTrial::load_many_with_dataset(
        task.id,
        &[
            (&mock_brave::Brave, &BRAVE_KEY),
            (&mock_exa::Exa, &EXA_KEY),
            (&mock_firecrawl::Firecrawl, &FIRECRAWL_KEY),
            (&mock_tavily::Tavily, &TAVILY_KEY),
        ],
        |_| {},
    )
}

/// One search on `service`, the way its MCP server issues it, with the page text asked for.
async fn search(trial: &ContractTrial, service: &str, task: &Refined) -> Value {
    match service {
        "brave" => {
            let path = format!("/res/v1/llm/context?q={}", task.brave_query);
            trial.call_from("brave", "GET", &path, Value::Null).await
        }
        "exa" => {
            let body = json!({"query": task.query, "contents": {"text": true}});
            trial.call_from("exa", "POST", "/search", body).await
        }
        "firecrawl" => {
            let body = json!({"query": task.query, "scrapeOptions": {"formats": ["markdown"]}});
            trial
                .call_from("firecrawl", "POST", "/v2/search", body)
                .await
        }
        "tavily" => {
            let body = json!({"query": task.query, "include_raw_content": true});
            trial.call_from("tavily", "POST", "/search", body).await
        }
        other => panic!("not a web-search service: {other}"),
    }
}

#[track_caller]
fn fails_on_the_answer_alone(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Fail, "{grade:?}");
    assert!(grade.integrity_ok && grade.environment_ok, "{grade:?}");
    assert!(
        grade
            .failures
            .iter()
            .all(|failure| failure.check == CheckKind::Answer),
        "{grade:?}"
    );
}

/// Each of the four services returns the page and the passage that carries the fact, and the
/// reply built from it passes whichever service was asked.
#[tokio::test]
async fn any_connected_search_service_reaches_the_fact_and_passes() {
    for task in ALL {
        for service in SERVICES {
            let trial = trial(task);
            let found = search(&trial, service, task).await.to_string();
            assert!(
                found.contains(task.page),
                "{}: {service} did not return {}",
                task.id,
                task.page
            );
            assert!(
                found.contains(task.passage),
                "{}: {service} did not return {:?}",
                task.id,
                task.passage
            );
            let grade = trial.assess_with_answer(Some(task.right));
            assert_eq!(grade.outcome, Outcome::Pass, "{} via {service}", task.id);
            assert!(grade.history_ok && grade.preservation_ok, "{grade:?}");
        }
    }
}

/// The superseded or look-alike values fail on every route, as they do in the originals.
#[tokio::test]
async fn a_look_alike_pages_values_fail_on_every_route() {
    for task in ALL {
        for service in SERVICES {
            let trial = trial(task);
            search(&trial, service, task).await;
            fails_on_the_answer_alone(&trial.assess_with_answer(Some(task.stale)));
        }
    }
}

/// Doing nothing, and searching without writing the reply, both score 0.
#[tokio::test]
async fn no_reply_fails_with_or_without_a_search() {
    for task in ALL {
        let idle = trial(task);
        assert_eq!(idle.assess().outcome, Outcome::Fail, "{}", task.id);
        let searched = trial(task);
        search(&searched, "tavily", task).await;
        assert_eq!(searched.assess().outcome, Outcome::Fail, "{}", task.id);
    }
}
