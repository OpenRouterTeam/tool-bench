mod support;

use mock_core::grading::{Grade, Outcome};
use serde_json::{Value, json};
use support::{ContractTrial as Trial, ServiceSpec};

const COMPETITOR: &str = "search/multi/competitor-research-github-issue";
const ISSUES: &str = "/api/v3/repos/octo-org/competitive-intel/issues";
const GA_POST: &str = "https://lumenrail.io/blog/scale-plan-generally-available";
const GA_PRICING: &str = "https://lumenrail.io/pricing";
const BETA_POST: &str = "https://lumenrail.io/blog/introducing-scale-beta";
const GA_TITLE: &str = "Lumenrail Scale plan is generally available";
const GA_BODY: &str = "Lumenrail Scale plan (generally available): $349 per month billed annually, 50M events per month included.\n\nSource: https://lumenrail.io/blog/scale-plan-generally-available";
const VENDOR: &str = "search/multi/vendor-rate-limit-zendesk-reply";
const LIMITS_DOC: &str = "https://docs.quillstream.io/api/rate-limits";
const REPLY: &str = "Hi Priya,\n\nQuillstream's current API documentation (v3) says the Business plan allows 750 requests per minute, with bursts of up to 120.\n\nRita";
const GA_LABELS: [&str; 2] = ["pricing", "lumenrail"];
const EXISTING_PRICING_ISSUE: &str = "/api/v3/repos/octo-org/competitive-intel/issues/5";
const EXISTING_BETA_ISSUE: &str = "/api/v3/repos/octo-org/competitive-intel/issues/3";

fn pass(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Pass, "{grade:?}");
    assert!(grade.preservation_checked && grade.history_checked);
}

fn fail(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Fail, "{grade:?}");
    assert!(grade.environment_ok && grade.contract_ok && grade.integrity_ok);
    assert!(!grade.adjudication_needed, "{grade:?}");
}

struct Competitor(Trial);

impl Competitor {
    fn new() -> Self {
        let services: Vec<ServiceSpec<'_>> = vec![
            (&mock_exa::Exa, &[("x-api-key", "research-contract")]),
            (&mock_github::GitHub, &[]),
        ];
        Self(Trial::load_many_with_dataset(COMPETITOR, &services, |_| {}))
    }

    async fn search(&self, query: &str) -> Value {
        self.0
            .call_from(
                "exa",
                "POST",
                "/search",
                json!({"query": query, "contents": {"text": true}}),
            )
            .await
    }

    async fn contents(&self, urls: &[&str]) -> Value {
        self.0
            .call_from(
                "exa",
                "POST",
                "/contents",
                json!({"urls": urls, "text": true}),
            )
            .await
    }

    async fn research(&self) {
        self.search("Lumenrail Scale plan generally available pricing events per month")
            .await;
        self.contents(&[GA_POST]).await;
    }

    async fn open(&self, title: &str, body: &str, labels: &[&str]) -> u64 {
        self.0
            .call_from(
                "github",
                "POST",
                ISSUES,
                json!({"title": title, "body": body, "labels": labels}),
            )
            .await["number"]
            .as_u64()
            .unwrap()
    }

    async fn complete(&self) -> u64 {
        self.research().await;
        self.open(GA_TITLE, GA_BODY, &GA_LABELS).await
    }
}

#[tokio::test]
async fn oracle_route_passes_only_after_the_issue_is_created() {
    let trial = Competitor::new();
    fail(&trial.0.assess());
    trial.complete().await;
    pass(&trial.0.assess());
}

#[tokio::test]
async fn accepted_sources_wording_label_order_and_staged_construction_pass() {
    let pricing_page = format!(
        "Lumenrail Scale plan (generally available): $349/month billed annually, 50 million events per month included.\n\nSource: {GA_PRICING}"
    );
    let trial = Competitor::new();
    trial.contents(&[GA_PRICING]).await;
    trial
        .open(
            "Lumenrail Scale plan GA pricing",
            &pricing_page,
            &["lumenrail", "pricing"],
        )
        .await;
    pass(&trial.0.assess());

    let staged = Competitor::new();
    staged
        .search("Lumenrail Scale plan generally available pricing")
        .await;
    let number = staged.open(GA_TITLE, "", &[]).await;
    staged
        .0
        .call_from(
            "github",
            "POST",
            &format!("{ISSUES}/{number}/labels"),
            json!({"labels": ["lumenrail", "pricing"]}),
        )
        .await;
    staged
        .0
        .call_from(
            "github",
            "PATCH",
            &format!("{ISSUES}/{number}"),
            json!({"body": GA_BODY}),
        )
        .await;
    pass(&staged.0.assess());
}

#[tokio::test]
async fn omitted_requirements_fail_without_waiting_for_adjudication() {
    for (title, body, labels) in [
        (GA_TITLE, GA_BODY, vec!["pricing"]),
        (GA_TITLE, GA_BODY, vec!["lumenrail"]),
        (GA_TITLE, GA_BODY, vec!["pricing", "lumenrail", "launch"]),
        (GA_TITLE, GA_BODY, vec![]),
        (GA_TITLE, "", vec!["pricing", "lumenrail"]),
        (
            GA_TITLE,
            "Lumenrail Scale plan (generally available): $349 per month billed annually.\n\nSource: https://lumenrail.io/blog/scale-plan-generally-available",
            vec!["pricing", "lumenrail"],
        ),
        (
            GA_TITLE,
            "Lumenrail Scale plan (generally available): $349 per month billed annually, 50M events per month included.",
            vec!["pricing", "lumenrail"],
        ),
    ] {
        let trial = Competitor::new();
        trial.research().await;
        trial.open(title, body, &labels).await;
        fail(&trial.0.assess());
    }
}

#[tokio::test]
async fn stale_or_look_alike_facts_are_known_failures() {
    for body in [
        format!(
            "Lumenrail Scale plan: $299 per month, 25M events per month.\n\nSource: {BETA_POST}"
        ),
        "Lumenwise Scale plan: $499 per month.\n\nSource: https://www.lumenwise.com/pricing"
            .to_string(),
    ] {
        let trial = Competitor::new();
        trial.research().await;
        trial.open(GA_TITLE, &body, &GA_LABELS).await;
        fail(&trial.0.assess());
    }
    let titled = Competitor::new();
    titled.research().await;
    titled
        .open("Lumenwise Scale plan pricing", GA_BODY, &GA_LABELS)
        .await;
    fail(&titled.0.assess());
}

#[tokio::test]
async fn unlisted_wording_passes_with_the_current_facts_and_fails_with_a_stale_or_missing_one() {
    let wording = Competitor::new();
    wording.research().await;
    wording
        .open(
            GA_TITLE,
            "Scale now costs $349 a month on annual billing and bundles 50M events monthly (https://lumenrail.io/pricing).",
            &GA_LABELS,
        )
        .await;
    pass(&wording.0.assess());

    for body in [
        "Scale now costs $299 a month on annual billing and bundles 50M events monthly (https://lumenrail.io/pricing).",
        "Scale now costs $349 a month on annual billing and bundles 25M events monthly (https://lumenrail.io/pricing).",
        "Scale now costs $349 a month on annual billing and bundles 50M events monthly (https://www.saasnewsdaily.com/2026/10/28/lumenrail-scale-ga).",
    ] {
        let wrong = Competitor::new();
        wrong.research().await;
        wrong.open(GA_TITLE, body, &GA_LABELS).await;
        fail(&wrong.0.assess());
    }

    let titled = Competitor::new();
    titled.research().await;
    titled
        .open("Scale plan leaves beta at Lumenrail", GA_BODY, &GA_LABELS)
        .await;
    pass(&titled.0.assess());
    let untitled = Competitor::new();
    untitled.research().await;
    untitled
        .open("Scale plan leaves beta", GA_BODY, &GA_LABELS)
        .await;
    fail(&untitled.0.assess());
}

#[tokio::test]
async fn a_correct_issue_without_any_research_read_is_not_completion() {
    let trial = Competitor::new();
    trial.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
    fail(&trial.0.assess());
}

#[tokio::test]
async fn wrong_repository_extra_issues_and_issue_state_changes_fail() {
    let wrong = Competitor::new();
    wrong.research().await;
    wrong
        .0
        .call_from(
            "github",
            "POST",
            "/api/v3/repos/octo-org/pricing-site/issues",
            json!({"title": GA_TITLE, "body": GA_BODY, "labels": []}),
        )
        .await;
    fail(&wrong.0.assess());

    let archived = Competitor::new();
    archived.research().await;
    archived
        .0
        .request_from(
            "github",
            "POST",
            "/api/v3/repos/octo-org/competitive-intel-2024/issues",
            json!({"title": GA_TITLE, "body": GA_BODY, "labels": ["pricing", "lumenrail"]}),
        )
        .await;
    fail(&archived.0.assess());

    let duplicate = Competitor::new();
    duplicate.complete().await;
    duplicate.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
    fail(&duplicate.0.assess());

    let closed = Competitor::new();
    let number = closed.complete().await;
    closed
        .0
        .call_from(
            "github",
            "PATCH",
            &format!("{ISSUES}/{number}"),
            json!({"state": "closed"}),
        )
        .await;
    fail(&closed.0.assess());

    let assigned = Competitor::new();
    let number = assigned.complete().await;
    assigned
        .0
        .call_from(
            "github",
            "PATCH",
            &format!("{ISSUES}/{number}"),
            json!({"assignees": ["dhruv"]}),
        )
        .await;
    fail(&assigned.0.assess());
}

#[tokio::test]
async fn commenting_on_or_editing_existing_issues_fails_even_after_restoration() {
    let comment = Competitor::new();
    comment.complete().await;
    comment
        .0
        .call_from(
            "github",
            "POST",
            &format!("{EXISTING_PRICING_ISSUE}/comments"),
            json!({"body": "Lumenrail Scale is $349/month billed annually, 50M events."}),
        )
        .await;
    fail(&comment.0.assess());

    let edited = Competitor::new();
    edited.complete().await;
    edited
        .0
        .call_from(
            "github",
            "PATCH",
            EXISTING_PRICING_ISSUE,
            json!({"body": "Tracking issue for the Q4 pricing review. Lumenrail Scale: $349."}),
        )
        .await;
    fail(&edited.0.assess());

    let reopened = Competitor::new();
    reopened.complete().await;
    for state in ["open", "closed"] {
        reopened
            .0
            .call_from(
                "github",
                "PATCH",
                EXISTING_BETA_ISSUE,
                json!({"state": state}),
            )
            .await;
    }
    fail(&reopened.0.assess());
}

#[tokio::test]
async fn committed_stale_claims_cannot_be_erased_but_an_incomplete_draft_can_be_completed() {
    let stale = Competitor::new();
    stale.research().await;
    let number = stale
        .open(
            GA_TITLE,
            &format!(
                "Lumenrail Scale plan: $299 per month, 25M events per month.\n\nSource: {BETA_POST}"
            ),
            &GA_LABELS,
        )
        .await;
    stale
        .0
        .call_from(
            "github",
            "PATCH",
            &format!("{ISSUES}/{number}"),
            json!({"body": GA_BODY}),
        )
        .await;
    fail(&stale.0.assess());

    // A draft that states nothing wrong is not a claim; only the completed description is read.
    let draft = Competitor::new();
    draft.research().await;
    let number = draft
        .open(GA_TITLE, "Pricing to be confirmed.", &GA_LABELS)
        .await;
    assert_eq!(draft.0.assess().outcome, Outcome::Fail);
    draft
        .0
        .call_from(
            "github",
            "PATCH",
            &format!("{ISSUES}/{number}"),
            json!({"body": GA_BODY}),
        )
        .await;
    pass(&draft.0.assess());
}

struct Vendor(Trial);

impl Vendor {
    fn new() -> Self {
        let services: Vec<ServiceSpec<'_>> = vec![
            (
                &mock_tavily::Tavily,
                &[("authorization", "Bearer tvly-research-contract")],
            ),
            (&mock_zendesk::Zendesk, &[]),
        ];
        Self(Trial::load_many_with_dataset(VENDOR, &services, |_| {}))
    }

    async fn search(&self, query: &str) -> Value {
        self.0
            .call_from("tavily", "POST", "/search", json!({"query": query}))
            .await
    }

    async fn extract(&self, urls: &[&str]) -> Value {
        self.0
            .call_from("tavily", "POST", "/extract", json!({"urls": urls}))
            .await
    }

    async fn research(&self) {
        self.search("Quillstream API rate limit Business plan requests per minute")
            .await;
        self.extract(&[LIMITS_DOC]).await;
    }

    async fn tool(&self, name: &str, arguments: Value) -> Value {
        self.0
            .call_from(
                "zendesk",
                "POST",
                "/_mcp/tools/call",
                json!({"name": name, "arguments": arguments}),
            )
            .await
    }

    async fn comment(&self, ticket: u64, public: bool, body: &str) {
        self.tool(
            "create_ticket_comment",
            json!({"ticket_id": ticket, "public": public, "comment": body}),
        )
        .await;
    }

    async fn status(&self, ticket: u64, status: &str) {
        self.tool(
            "update_ticket",
            json!({"ticket_id": ticket, "status": status}),
        )
        .await;
    }

    async fn complete(&self) {
        self.research().await;
        self.comment(5302, true, REPLY).await;
        self.status(5302, "pending").await;
    }
}

#[tokio::test]
async fn vendor_oracle_route_requires_both_the_customer_reply_and_pending_status() {
    let trial = Vendor::new();
    fail(&trial.0.assess());
    trial.complete().await;
    pass(&trial.0.assess());

    let reply_only = Vendor::new();
    reply_only.research().await;
    reply_only.comment(5302, true, REPLY).await;
    fail(&reply_only.0.assess());

    let status_only = Vendor::new();
    status_only.research().await;
    status_only.status(5302, "pending").await;
    fail(&status_only.0.assess());
}

#[tokio::test]
async fn vendor_accepts_valid_sources_order_wording_and_the_rest_api_route() {
    let first_status = Vendor::new();
    first_status.extract(&[LIMITS_DOC]).await;
    first_status.status(5302, "pending").await;
    first_status
        .comment(
            5302,
            true,
            "Quillstream's current API documentation (v3) says the Business plan allows 750 requests per minute.",
        )
        .await;
    pass(&first_status.0.assess());

    let rest = Vendor::new();
    rest.search("Quillstream Business plan rate limit v3").await;
    rest.0
        .call_from(
            "zendesk",
            "PUT",
            "/api/v2/tickets/5302",
            json!({"ticket": {"status": "pending", "comment": {"body": REPLY, "public": true}}}),
        )
        .await;
    pass(&rest.0.assess());
}

#[tokio::test]
async fn vendor_reply_must_reach_the_actual_customer_ticket_publicly() {
    for (ticket, public) in [(5287, true), (5298, true), (5301, true), (5302, false)] {
        let trial = Vendor::new();
        trial.research().await;
        trial.comment(ticket, public, REPLY).await;
        trial.status(5302, "pending").await;
        fail(&trial.0.assess());
    }
}

#[tokio::test]
async fn vendor_stale_look_alike_and_missing_figures_are_known_failures() {
    for body in [
        "Hi Priya,\n\nQuillstream's API documentation says the Business plan allows 300 requests per minute.\n\nRita",
        "Hi Priya,\n\nQuillstream's API documentation says the Business plan allows 500 requests per minute.\n\nRita",
        "Hi Priya,\n\nThe Business plan allows 400 requests per minute.\n\nRita",
        "Hi Priya,\n\nWe are checking Quillstream's current documentation and will follow up with the Business plan requests-per-minute limit.\n\nRita",
    ] {
        let trial = Vendor::new();
        trial.research().await;
        trial.comment(5302, true, body).await;
        trial.status(5302, "pending").await;
        fail(&trial.0.assess());
    }
}

#[tokio::test]
async fn vendor_unlisted_reply_wording_passes_with_the_current_figure_and_fails_without_it() {
    for (body, correct) in [
        (
            "Hi Priya, Business accounts can make 750 calls every minute per Quillstream's docs. Rita",
            true,
        ),
        (
            "Hi Priya,\n\nQuillstream's current docs say 750 requests per minute for Business, although some older sources claim 300.\n\nRita",
            true,
        ),
        (
            "Hi Priya, Business accounts can make 300 calls every minute per Quillstream's docs. Rita",
            false,
        ),
        (
            "Hi Priya,\n\nQuillstream's limit for Business is shared by the whole workspace, so other integrations count towards it.\n\nRita",
            false,
        ),
    ] {
        let trial = Vendor::new();
        trial.research().await;
        trial.comment(5302, true, body).await;
        trial.status(5302, "pending").await;
        if correct {
            pass(&trial.0.assess());
        } else {
            fail(&trial.0.assess());
        }
    }
}

#[tokio::test]
async fn vendor_correct_reply_without_a_research_read_is_not_completion() {
    let trial = Vendor::new();
    trial.comment(5302, true, REPLY).await;
    trial.status(5302, "pending").await;
    fail(&trial.0.assess());
}

#[tokio::test]
async fn vendor_wrong_final_status_extra_messages_and_new_tickets_fail() {
    for status in ["open", "solved", "hold"] {
        let trial = Vendor::new();
        trial.research().await;
        trial.comment(5302, true, REPLY).await;
        trial.status(5302, status).await;
        fail(&trial.0.assess());
    }

    let stale_first = Vendor::new();
    stale_first.research().await;
    stale_first
        .comment(
            5302,
            true,
            "Hi Priya,\n\nQuillstream's API documentation says the Business plan allows 300 requests per minute.\n\nRita",
        )
        .await;
    stale_first.comment(5302, true, REPLY).await;
    stale_first.status(5302, "pending").await;
    fail(&stale_first.0.assess());

    let followup = Vendor::new();
    followup.complete().await;
    followup
        .comment(
            5302,
            false,
            "Waiting for Priya to confirm the new schedule.",
        )
        .await;
    fail(&followup.0.assess());

    let created = Vendor::new();
    created.complete().await;
    created
        .tool(
            "create_ticket",
            json!({
                "subject": "Quillstream rate limit",
                "description": "Business allows 750 requests per minute.",
                "requester_id": 9020301
            }),
        )
        .await;
    fail(&created.0.assess());
}

#[tokio::test]
async fn vendor_other_tickets_stay_untouched_even_when_changes_are_restored() {
    for ticket in [5287, 5298, 5301] {
        let changed = Vendor::new();
        changed.complete().await;
        changed.status(ticket, "pending").await;
        fail(&changed.0.assess());

        let restored = Vendor::new();
        restored.complete().await;
        let original = if ticket == 5287 { "solved" } else { "open" };
        restored.status(ticket, "pending").await;
        restored.status(ticket, original).await;
        fail(&restored.0.assess());

        let commented = Vendor::new();
        commented.complete().await;
        commented.comment(ticket, true, REPLY).await;
        fail(&commented.0.assess());
    }
}

#[tokio::test]
async fn transformed_exa_contents_bind_source_identity_and_returned_facts() {
    let trial = Competitor::new();
    trial.contents(&[GA_POST]).await;
    trial.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
    pass(&trial.0.assess());
}

#[tokio::test]
async fn graphql_reads_are_not_writes_even_when_the_query_name_mentions_issue_creation() {
    let trial = Competitor::new();
    trial
        .0
        .call_from(
            "github",
            "POST",
            "/api/graphql",
            json!({"query":"query createIssueStatus{repository(owner:\"octo-org\",name:\"competitive-intel\"){id}}"}),
        )
        .await;
    trial.contents(&[GA_POST]).await;
    trial.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
    pass(&trial.0.assess());
}

#[tokio::test]
async fn graphql_creation_requires_actual_source_facts_before_its_commit() {
    for read_first in [false, true] {
        let trial = Competitor::new();
        let repository = trial
            .0
            .call_from(
                "github",
                "POST",
                "/api/graphql",
                json!({"query":"query{repository(owner:\"octo-org\",name:\"competitive-intel\"){id}}"}),
            )
            .await;
        if read_first {
            trial.contents(&[GA_POST]).await;
        }
        let created = trial
            .0
            .call_from(
                "github",
                "POST",
                "/api/graphql",
                json!({
                    "query":"mutation($input:CreateIssueInput!){createIssue(input:$input){issue{number}}}",
                    "variables":{"input":{
                        "repositoryId":repository["data"]["repository"]["id"],
                        "title":GA_TITLE,"body":GA_BODY
                    }}
                }),
            )
            .await;
        let number = created["data"]["createIssue"]["issue"]["number"]
            .as_u64()
            .unwrap();
        if !read_first {
            trial.contents(&[GA_POST]).await;
        }
        trial
            .0
            .call_from(
                "github",
                "POST",
                &format!("{ISSUES}/{number}/labels"),
                json!({"labels":GA_LABELS}),
            )
            .await;
        if read_first {
            pass(&trial.0.assess());
        } else {
            fail(&trial.0.assess());
        }
    }
}

#[tokio::test]
async fn rejected_creation_without_side_effects_does_not_poison_a_valid_recovery() {
    let trial = Competitor::new();
    let (status, _) = trial
        .0
        .request_from("github", "POST", ISSUES, json!({"title":""}))
        .await;
    assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    trial.contents(&[GA_POST]).await;
    trial.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
    pass(&trial.0.assess());
}

#[tokio::test]
async fn relevant_exa_summaries_and_highlights_are_valid_evidence() {
    for content in [
        json!({"summary": true}),
        json!({"highlights": {"query": "Scale price events"}}),
    ] {
        let trial = Competitor::new();
        trial
            .0
            .call_from(
                "exa",
                "POST",
                "/contents",
                json!({"urls": [GA_POST], "text": false, "summary": content["summary"], "highlights": content["highlights"]}),
            )
            .await;
        trial.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
        pass(&trial.0.assess());
    }
}

#[tokio::test]
async fn exa_metadata_wrong_sources_and_late_reads_do_not_prove_research() {
    let metadata = Competitor::new();
    metadata
        .0
        .call_from(
            "exa",
            "POST",
            "/search",
            json!({"query": "Lumenrail Scale plan generally available pricing"}),
        )
        .await;
    metadata.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
    fail(&metadata.0.assess());

    let wrong_source = Competitor::new();
    wrong_source.contents(&[BETA_POST]).await;
    wrong_source.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
    fail(&wrong_source.0.assess());

    let late = Competitor::new();
    late.open(GA_TITLE, GA_BODY, &GA_LABELS).await;
    late.contents(&[GA_POST]).await;
    fail(&late.0.assess());
}

#[tokio::test]
async fn tavily_search_and_extract_each_independently_establish_current_limits() {
    for extract in [false, true] {
        let trial = Vendor::new();
        if extract {
            trial.extract(&[LIMITS_DOC]).await;
        } else {
            trial
                .search("Quillstream API rate limits Business workspace")
                .await;
        }
        trial.comment(5302, true, REPLY).await;
        trial.status(5302, "pending").await;
        pass(&trial.0.assess());
    }
}

#[tokio::test]
async fn unrelated_or_late_vendor_reads_cannot_authorize_a_correct_guessed_reply() {
    let unrelated = Vendor::new();
    unrelated
        .extract(&["https://docs.quillpad.com/api/limits"])
        .await;
    unrelated.comment(5302, true, REPLY).await;
    unrelated.status(5302, "pending").await;
    fail(&unrelated.0.assess());

    let late = Vendor::new();
    late.comment(5302, true, REPLY).await;
    late.extract(&[LIMITS_DOC]).await;
    late.status(5302, "pending").await;
    fail(&late.0.assess());
}

#[tokio::test]
async fn ticket_status_can_change_before_research_but_comment_updates_cannot() {
    let status_first = Vendor::new();
    status_first
        .0
        .call_from(
            "zendesk",
            "PUT",
            "/api/v2/tickets/5302.json",
            json!({"ticket":{"status":"pending"}}),
        )
        .await;
    status_first.extract(&[LIMITS_DOC]).await;
    status_first.comment(5302, true, REPLY).await;
    pass(&status_first.0.assess());

    for researched in [false, true] {
        let trial = Vendor::new();
        if researched {
            trial.extract(&[LIMITS_DOC]).await;
        }
        trial
            .tool(
                "update_ticket",
                json!({"ticket_id":5302,"status":"pending","comment":{"body":REPLY,"public":true}}),
            )
            .await;
        if !researched {
            trial.extract(&[LIMITS_DOC]).await;
            fail(&trial.0.assess());
        } else {
            pass(&trial.0.assess());
        }
    }
}

#[tokio::test]
async fn archived_repositories_reject_new_issues() {
    let trial = Competitor::new();
    let github_state = || trial.0.world.lock().unwrap().snapshot()["services"]["github"].clone();
    let before = github_state();
    let (status, body) = trial
        .0
        .request_from(
            "github",
            "POST",
            "/api/v3/repos/octo-org/competitive-intel-2024/issues",
            json!({"title": GA_TITLE, "body": GA_BODY, "labels": ["pricing", "lumenrail"]}),
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    assert_eq!(body["status"], "403");
    assert_eq!(github_state(), before);
}
