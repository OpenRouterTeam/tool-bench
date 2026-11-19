//! The eleven search tasks whose instruction names no vendor
//! (`scenarios/search/*/*-any-search`): the pages are reachable through each of the four
//! connected web-search services, a read through any of them is accepted, and a reply
//! built from a look-alike or superseded page still fails.

mod support;

use mock_core::grading::{CheckKind, Grade, Outcome};
use serde_json::{Value, json};
use support::{ContractTrial, ServiceSpec};

const BRAVE_KEY: [(&str, &str); 1] = [("x-subscription-token", "BSA-contract")];
const EXA_KEY: [(&str, &str); 1] = [("x-api-key", "exa-toolbench")];
const FIRECRAWL_KEY: [(&str, &str); 1] = [("authorization", "Bearer local-firecrawl")];
const TAVILY_KEY: [(&str, &str); 1] = [("authorization", "Bearer tvly-toolbench")];
const SERVICES: [&str; 4] = ["brave", "exa", "firecrawl", "tavily"];

/// One thing to look up: what an agent would type, the page that holds the fact and a
/// passage of that page that carries it.
struct Read {
    /// Brave request path and query, since Brave has one endpoint per kind of result.
    brave: &'static str,
    query: &'static str,
    page: &'static str,
    passage: &'static str,
}

struct Lookup {
    id: &'static str,
    reads: &'static [Read],
    right: &'static str,
    /// The same reply with a look-alike or superseded page's values.
    stale: &'static str,
}

const HOSTING: Lookup = Lookup {
    id: "search/brave/hosting-sla-any-search",
    reads: &[Read {
        brave: "/res/v1/llm/context?q=Ironbark+Hosting+SLA+Business+plan+uptime+service+credit",
        query: "Ironbark Hosting SLA Business plan uptime service credit",
        page: "https://www.ironbarkhosting.com/legal/sla",
        passage: "99.95%",
    }],
    right: "Uptime: 99.95% Service credit: 10% of the monthly fee",
    stale: "Uptime: 99.9% Service credit: 5% of the monthly fee",
};

const FUNDING: Lookup = Lookup {
    id: "search/brave/funding-news-any-search",
    reads: &[Read {
        brave: "/res/v1/llm/context?q=Norvik+Analytics+funding+round",
        query: "Norvik Analytics funding round",
        page: "https://www.techbeat.io/2026/10/27/norvik-analytics-series-b",
        passage: "Halcyon Ventures",
    }],
    right: "Amount: $45M Round: Series B Lead investor: Halcyon Ventures",
    stale: "Amount: $12M Round: Series A Lead investor: Fjord Capital",
};

const KEYNOTE: Lookup = Lookup {
    id: "search/brave/keynote-any-search",
    reads: &[Read {
        brave: "/res/v1/videos/search?q=Scaling+Postgres+to+1M+writes%2Fs+keynote+PGConf+Nordic+2026",
        query: "Scaling Postgres to 1M writes/s keynote PGConf Nordic 2026 full recording",
        page: "https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        passage: "47:12",
    }],
    right: "Speaker: Marta Lindqvist. Duration: 47:12. Recording: https://www.youtube.com/watch?v=Zt4mQeL1x9c",
    stale: "Speaker: Marta Lindqvist. Duration: 28:40. Recording: https://www.youtube.com/watch?v=Hq7wB3nVt2s",
};

const TIMEOUT: Lookup = Lookup {
    id: "search/exa/sdk-timeout-any-search",
    reads: &[Read {
        brave: "/res/v1/llm/context?q=Driftwood+Python+SDK+v4+client+configuration+timeout+option+default",
        query: "Driftwood Python SDK v4 client configuration timeout option default",
        page: "https://docs.driftwood.dev/python/v4/configuration",
        passage: "timeout_seconds",
    }],
    right: "Option: timeout_seconds Default: 30 seconds",
    stale: "Option: request_timeout Default: 60 seconds",
};

const PROFILE: Lookup = Lookup {
    id: "search/exa/vp-profile-any-search",
    reads: &[Read {
        brave: "/res/v1/llm/context?q=Priya+Raman+VP+of+Engineering+Tessellate+Bio+Boston",
        query: "Priya Raman VP of Engineering Tessellate Bio Boston",
        page: "https://www.linkedin.com/in/priya-raman-tessellate",
        passage: "Tessellate Bio, VP of Engineering (2024-present)",
    }],
    right: "https://www.linkedin.com/in/priya-raman-tessellate",
    stale: "https://www.linkedin.com/in/priya-raman-4b21a9",
};

const PRICING: Lookup = Lookup {
    id: "search/firecrawl/team-pricing-any-search",
    reads: &[Read {
        brave: "/res/v1/llm/context?q=Northwind+Analytics+pricing+Team+plan",
        query: "Northwind Analytics pricing Team plan",
        page: "https://www.northwind-analytics.com/pricing",
        passage: "$79/month",
    }],
    right: "Team plan: $79/month billed annually, 12 seats included.",
    stale: "Team plan: $69/month billed annually, 10 seats included.",
};

const RATE_LIMIT: Lookup = Lookup {
    id: "search/tavily/vendor-rate-limit-any-search",
    reads: &[Read {
        brave: "/res/v1/llm/context?q=Quillstream+API+rate+limit+Business+plan+requests+per+minute",
        query: "Quillstream API rate limit Business plan requests per minute",
        page: "https://docs.quillstream.io/api/rate-limits",
        passage: "1,200",
    }],
    right: "1,200 requests per minute (https://docs.quillstream.io/api/rate-limits)",
    stale: "600 requests per minute (https://docs.quillstream.io/v2/api/rate-limits)",
};

const SUNSET: Lookup = Lookup {
    id: "search/tavily/webhook-sunset-any-search",
    reads: &[Read {
        brave: "/res/v1/llm/context?q=Ferrowave+verifying+webhook+signatures+v1+stop+sending",
        query: "Ferrowave verifying webhook signatures v1 stop sending",
        page: "https://docs.ferrowave.dev/guides/webhooks/verify-signatures",
        passage: "January 15, 2027",
    }],
    right: "Date: 2027-01-15 Header: Ferrowave-Signature-V2",
    stale: "Date: 2026-12-01 Header: Ferrowave-Signature-V2",
};

const BRIEF: Lookup = Lookup {
    id: "search/multi/funding-and-pricing-any-search",
    reads: &[
        Read {
            brave: "/res/v1/llm/context?q=Norvik+Analytics+funding+round",
            query: "Norvik Analytics funding round",
            page: "https://www.techbeat.io/2026/10/27/norvik-analytics-series-b",
            passage: "Halcyon Ventures",
        },
        Read {
            brave: "/res/v1/llm/context?q=Norvik+Analytics+pricing+Growth+plan",
            query: "Norvik Analytics pricing Growth plan",
            page: "https://www.norvikanalytics.com/pricing",
            passage: "$890",
        },
    ],
    right: "Funding: $45M Series B led by Halcyon Ventures. Pricing: Growth is $890 per month, billed annually.",
    stale: "Funding: $45M Series B led by Halcyon Ventures. Pricing: Growth is $690 per month, billed annually.",
};

fn search_services() -> Vec<ServiceSpec<'static>> {
    vec![
        (&mock_brave::Brave, &BRAVE_KEY),
        (&mock_exa::Exa, &EXA_KEY),
        (&mock_firecrawl::Firecrawl, &FIRECRAWL_KEY),
        (&mock_tavily::Tavily, &TAVILY_KEY),
    ]
}

fn trial(id: &str) -> ContractTrial {
    ContractTrial::load_many_with_dataset(id, &search_services(), |_| {})
}

/// One search on `service`, the way its MCP server issues it, with the page text asked for.
async fn search(trial: &ContractTrial, service: &str, read: &Read) -> Value {
    match service {
        "brave" => {
            trial
                .call_from("brave", "GET", read.brave, Value::Null)
                .await
        }
        "exa" => {
            let body = json!({"query": read.query, "contents": {"text": true}});
            trial.call_from("exa", "POST", "/search", body).await
        }
        "firecrawl" => {
            let body = json!({"query": read.query, "scrapeOptions": {"formats": ["markdown"]}});
            trial
                .call_from("firecrawl", "POST", "/v2/search", body)
                .await
        }
        "tavily" => {
            let body = json!({"query": read.query, "include_raw_content": true});
            trial.call_from("tavily", "POST", "/search", body).await
        }
        other => panic!("not a web-search service: {other}"),
    }
}

/// Each of the four services returns every page the task needs with the passage that
/// carries the fact, and the reply built from it passes whichever service was asked.
async fn passes_through_each_service(task: &Lookup) {
    for service in SERVICES {
        let trial = trial(task.id);
        for read in task.reads {
            let found = search(&trial, service, read).await.to_string();
            assert!(
                found.contains(read.page),
                "{}: {service} did not return {}",
                task.id,
                read.page
            );
            assert!(
                found.contains(read.passage),
                "{}: {service} did not return {:?}",
                task.id,
                read.passage
            );
        }
        let grade = trial.assess_with_answer(Some(task.right));
        assert_eq!(grade.outcome, Outcome::Pass, "{} via {service}", task.id);
        assert!(grade.history_ok && grade.preservation_ok, "{grade:?}");
    }
}

/// The superseded or look-alike values fail on every route, on the answer check alone, and
/// so does no reply at all.
async fn look_alike_values_fail_on_every_route(task: &Lookup) {
    for service in SERVICES {
        let trial = trial(task.id);
        for read in task.reads {
            search(&trial, service, read).await;
        }
        let grade = trial.assess_with_answer(Some(task.stale));
        assert_eq!(grade.outcome, Outcome::Fail, "{} via {service}", task.id);
        assert!(grade.integrity_ok && grade.environment_ok, "{grade:?}");
        assert!(
            grade
                .failures
                .iter()
                .all(|failure| failure.check == CheckKind::Answer),
            "{grade:?}"
        );
        assert_eq!(trial.assess().outcome, Outcome::Fail, "{}", task.id);
    }
    assert_eq!(trial(task.id).assess().outcome, Outcome::Fail);
}

#[tokio::test]
async fn hosting_sla_passes_through_each_of_the_four_search_services() {
    passes_through_each_service(&HOSTING).await;
}

#[tokio::test]
async fn hosting_sla_archived_and_look_alike_values_fail_on_every_route() {
    look_alike_values_fail_on_every_route(&HOSTING).await;
}

#[tokio::test]
async fn funding_news_passes_through_each_of_the_four_search_services() {
    passes_through_each_service(&FUNDING).await;
}

#[tokio::test]
async fn funding_news_last_years_round_fails_on_every_route() {
    look_alike_values_fail_on_every_route(&FUNDING).await;
}

#[tokio::test]
async fn keynote_passes_through_each_of_the_four_search_services() {
    passes_through_each_service(&KEYNOTE).await;
}

#[tokio::test]
async fn keynote_meetup_version_fails_on_every_route() {
    look_alike_values_fail_on_every_route(&KEYNOTE).await;
}

#[tokio::test]
async fn sdk_timeout_passes_through_each_of_the_four_search_services() {
    passes_through_each_service(&TIMEOUT).await;
}

#[tokio::test]
async fn sdk_timeout_legacy_option_fails_on_every_route() {
    look_alike_values_fail_on_every_route(&TIMEOUT).await;
}

#[tokio::test]
async fn vp_profile_passes_through_each_of_the_four_search_services() {
    passes_through_each_service(&PROFILE).await;
}

#[tokio::test]
async fn vp_profile_former_director_fails_on_every_route() {
    look_alike_values_fail_on_every_route(&PROFILE).await;
}

#[tokio::test]
async fn team_pricing_passes_through_each_of_the_four_search_services() {
    passes_through_each_service(&PRICING).await;
}

#[tokio::test]
async fn team_pricing_last_years_price_fails_on_every_route() {
    look_alike_values_fail_on_every_route(&PRICING).await;
}

#[tokio::test]
async fn vendor_rate_limit_passes_through_each_of_the_four_search_services() {
    passes_through_each_service(&RATE_LIMIT).await;
}

#[tokio::test]
async fn vendor_rate_limit_deprecated_figure_fails_on_every_route() {
    look_alike_values_fail_on_every_route(&RATE_LIMIT).await;
}

#[tokio::test]
async fn webhook_sunset_passes_through_each_of_the_four_search_services() {
    passes_through_each_service(&SUNSET).await;
}

#[tokio::test]
async fn webhook_sunset_api_key_deadline_fails_on_every_route() {
    look_alike_values_fail_on_every_route(&SUNSET).await;
}

#[tokio::test]
async fn funding_and_pricing_passes_with_both_halves_from_any_one_search_service() {
    passes_through_each_service(&BRIEF).await;
}

#[tokio::test]
async fn funding_and_pricing_last_years_price_fails_on_every_route() {
    look_alike_values_fail_on_every_route(&BRIEF).await;
}

// ── The two tasks that write after the lookup ───────────────────────────────

const ZENDESK: &str = "search/multi/zendesk-reply-any-search";
const LIMITS_DOC: &str = "https://docs.quillstream.io/api/rate-limits";
const LIMITS_QUERY: &str = "Quillstream API rate limit Business plan requests per minute";
const LIMITS_BRAVE: &str = "q=Quillstream+API+rate+limit+Business+plan+requests+per+minute";
const REPLY: &str = "Hi Priya,\n\nQuillstream's current API documentation (v3) says the Business plan allows 750 requests per minute, with bursts of up to 120.\n\nRita";
const STALE_REPLY: &str = "Hi Priya,\n\nQuillstream's API documentation says the Business plan allows 300 requests per minute.\n\nRita";

const GITHUB: &str = "search/multi/github-issue-any-search";
const ISSUES: &str = "/api/v3/repos/octo-org/competitive-intel/issues";
const GA_QUERY: &str = "Lumenrail Scale plan generally available pricing events per month";
const GA_BRAVE: &str = "q=Lumenrail+Scale+plan+generally+available+pricing+events+per+month";
const GA_TITLE: &str = "Lumenrail Scale plan is generally available";
const GA_BODY: &str = "Lumenrail Scale plan (generally available): $349 per month billed annually, 50M events per month included.\n\nSource: https://lumenrail.io/blog/scale-plan-generally-available";
const BETA_BODY: &str = "Lumenrail Scale plan: $299 per month, 25M events per month.\n\nSource: https://lumenrail.io/blog/introducing-scale-beta";
const GA_LABELS: [&str; 2] = ["pricing", "lumenrail"];

/// Every read a connected search server can make that carries the page text.
const ROUTES: [&str; 5] = ["tavily", "exa", "brave-context", "brave-web", "firecrawl"];

fn writing(id: &str, second: ServiceSpec<'static>) -> ContractTrial {
    let mut services = search_services();
    services.push(second);
    ContractTrial::load_many_with_dataset(id, &services, |_| {})
}

async fn read(trial: &ContractTrial, route: &str, query: &str, brave: &str) -> String {
    let found = match route {
        "tavily" => {
            trial
                .call_from("tavily", "POST", "/search", json!({"query": query}))
                .await
        }
        "exa" => {
            let body = json!({"query": query, "contents": {"text": true}});
            trial.call_from("exa", "POST", "/search", body).await
        }
        "brave-context" => {
            let path = format!("/res/v1/llm/context?{brave}");
            trial.call_from("brave", "GET", &path, Value::Null).await
        }
        "brave-web" => {
            let path = format!("/res/v1/web/search?{brave}");
            trial.call_from("brave", "GET", &path, Value::Null).await
        }
        "firecrawl" => {
            trial
                .call_from("firecrawl", "POST", "/v2/search", json!({"query": query}))
                .await
        }
        other => panic!("unknown route {other}"),
    };
    found.to_string()
}

async fn reply(trial: &ContractTrial, body: &str) {
    for (name, arguments) in [
        (
            "create_ticket_comment",
            json!({"ticket_id": 5302, "public": true, "comment": body}),
        ),
        (
            "update_ticket",
            json!({"ticket_id": 5302, "status": "pending"}),
        ),
    ] {
        trial
            .call_from(
                "zendesk",
                "POST",
                "/_mcp/tools/call",
                json!({"name": name, "arguments": arguments}),
            )
            .await;
    }
}

async fn open_issue(trial: &ContractTrial, body: &str) {
    trial
        .call_from(
            "github",
            "POST",
            ISSUES,
            json!({"title": GA_TITLE, "body": body, "labels": GA_LABELS}),
        )
        .await;
}

#[track_caller]
fn passed(grade: &Grade, route: &str) {
    assert_eq!(grade.outcome, Outcome::Pass, "{route}: {grade:?}");
    assert!(grade.preservation_checked && grade.history_checked);
}

#[track_caller]
fn failed(grade: &Grade, route: &str) {
    assert_eq!(grade.outcome, Outcome::Fail, "{route}: {grade:?}");
    assert!(grade.environment_ok && grade.contract_ok && grade.integrity_ok);
}

/// The current rate-limit page read through any connected search server authorizes the
/// reply; the Zendesk half is graded as before.
#[tokio::test]
async fn zendesk_reply_accepts_the_page_read_through_each_search_service() {
    for route in ROUTES {
        let trial = writing(ZENDESK, (&mock_zendesk::Zendesk, &[]));
        let found = read(&trial, route, LIMITS_QUERY, LIMITS_BRAVE).await;
        assert!(
            found.contains(LIMITS_DOC) && found.contains("750"),
            "{route}"
        );
        reply(&trial, REPLY).await;
        passed(&trial.assess(), route);
    }
}

/// A read that returned only the look-alike vendor's page is not the evidence, whichever
/// service made it, and the superseded figure fails after a valid read.
#[tokio::test]
async fn zendesk_reply_rejects_a_look_alike_read_and_a_superseded_figure_on_every_route() {
    for route in ROUTES {
        let unread = writing(ZENDESK, (&mock_zendesk::Zendesk, &[]));
        let found = read(&unread, route, "Quillpad", "q=Quillpad").await;
        assert!(!found.contains(LIMITS_DOC), "{route}: {found}");
        reply(&unread, REPLY).await;
        let grade = unread.assess();
        failed(&grade, route);
        assert!(
            grade
                .failures
                .iter()
                .any(|failure| failure.check == CheckKind::Ordering),
            "{route}: {grade:?}"
        );

        let stale = writing(ZENDESK, (&mock_zendesk::Zendesk, &[]));
        read(&stale, route, LIMITS_QUERY, LIMITS_BRAVE).await;
        reply(&stale, STALE_REPLY).await;
        failed(&stale.assess(), route);
    }
}

/// The launch post or the live pricing page read through any connected search server
/// authorizes the issue; the GitHub half is graded as before.
#[tokio::test]
async fn github_issue_accepts_the_page_read_through_each_search_service() {
    for route in ROUTES {
        let trial = writing(GITHUB, (&mock_github::GitHub, &[]));
        let found = read(&trial, route, GA_QUERY, GA_BRAVE).await;
        assert!(
            found.contains("$349") && found.contains("50M events"),
            "{route}"
        );
        open_issue(&trial, GA_BODY).await;
        passed(&trial.assess(), route);
    }
}

/// A read that returned only the look-alike company's page is not the evidence, whichever
/// service made it, and the beta figures fail after a valid read.
#[tokio::test]
async fn github_issue_rejects_a_look_alike_read_and_the_beta_figures_on_every_route() {
    for route in ROUTES {
        let unread = writing(GITHUB, (&mock_github::GitHub, &[]));
        let found = read(&unread, route, "Lumenwise", "q=Lumenwise").await;
        assert!(!found.contains("lumenrail.io"), "{route}: {found}");
        open_issue(&unread, GA_BODY).await;
        let grade = unread.assess();
        failed(&grade, route);
        assert!(
            grade
                .failures
                .iter()
                .any(|failure| failure.check == CheckKind::Ordering),
            "{route}: {grade:?}"
        );

        let stale = writing(GITHUB, (&mock_github::GitHub, &[]));
        read(&stale, route, GA_QUERY, GA_BRAVE).await;
        open_issue(&stale, BETA_BODY).await;
        failed(&stale.assess(), route);
    }
}

/// The known limit of the read-before-write rule on Brave and Firecrawl: a search whose
/// response carries no web result list cannot be read as evidence either way, so a write
/// after it leaves the trial unassessed. It is never a pass.
#[tokio::test]
async fn a_search_without_a_web_result_list_before_the_write_is_not_assessed() {
    let news_only = writing(ZENDESK, (&mock_zendesk::Zendesk, &[]));
    news_only
        .call_from(
            "firecrawl",
            "POST",
            "/v2/search",
            json!({"query": LIMITS_QUERY, "sources": ["news"]}),
        )
        .await;
    reply(&news_only, REPLY).await;
    assert_eq!(news_only.assess().outcome, Outcome::EnvironmentError);

    let filtered = writing(GITHUB, (&mock_github::GitHub, &[]));
    let path = format!("/res/v1/web/search?{GA_BRAVE}&result_filter=news");
    filtered.call_from("brave", "GET", &path, Value::Null).await;
    open_issue(&filtered, GA_BODY).await;
    assert_eq!(filtered.assess().outcome, Outcome::EnvironmentError);
}
