//! Ordinary wording of the free text in code and collaboration write tasks gets a pass/fail
//! verdict. Every wording travels through the mock service into state and through `grade`.

mod support;

use mock_core::grading::Grade;
use serde_json::{Value, json};
use support::{ContractTrial, ServiceSpec, prose::Proof};

/// Calls an in-process MCP tool of one of the trial's services.
async fn tool(trial: &ContractTrial, service: &str, name: &str, arguments: Value) -> Value {
    trial
        .call_from(
            service,
            "POST",
            "/_mcp/tools/call",
            json!({"name": name, "arguments": arguments}),
        )
        .await
}

fn single(id: &str, service: &dyn mock_core::service::MockService) -> ContractTrial {
    ContractTrial::load_with_dataset(id, service, &[], |_| {})
}

fn many(id: &str, services: &[ServiceSpec<'_>]) -> ContractTrial {
    ContractTrial::load_many_with_dataset(id, services, |_| {})
}

fn jira(id: &str) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_jira::Jira,
        &[("authorization", "Bearer synthetic-jira-contract")],
        |_| {},
    )
}

async fn file_login_bug(body: String) -> Grade {
    let trial = single("code/github/file-login-bug", &mock_github::GitHub);
    trial
        .call(
            "POST",
            "/api/v3/repos/octo-org/app/issues",
            json!({"title": "Login page returns 500", "body": body}),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn github_file_login_bug_description() {
    Proof {
        scenario: "code/github/file-login-bug",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "Started after deploy 412.",
            "Since deploy 412 went out this morning, users hitting the login page of the web app get an HTTP 500. Before that deploy the page loaded normally, so this looks like a regression introduced by it.",
            "## Summary\nThe login page returns an HTTP 500 for all users.\n\n## Details\n- **First seen:** this morning, right after deploy #412\n- **Affected page:** `/login`\n- **Impact:** users cannot sign in\n\n## Next steps\nInvestigate what changed in the 412 deployment.",
            "Hi team,\n\nUsers have been getting an HTTP 500 on the login page since this morning. The errors began once Deploy 412 was rolled out; nothing else changed in that window as far as we know.\n\nThanks!",
            "Regression: /login responds with 500 Internal Server Error. Last deployed build: 412.",
        ],
        fails: &[
            "Users hitting the login page of our web app get an HTTP 500 since this morning's deploy.",
            "Since deploy 421 went out this morning, users hitting the login page get an HTTP 500.",
            "The login page has returned HTTP 500 since deploy 4120.",
        ],
    }
    .run(file_login_bug)
    .await;
}

async fn approve_and_merge_lodash_bump(body: String) -> Grade {
    const PULL: &str = "/api/v3/repos/octo-org/web/pulls/57";
    let trial = single(
        "code/github/approve-merge-dependency-bump",
        &mock_github::GitHub,
    );
    trial
        .call(
            "POST",
            &format!("{PULL}/reviews"),
            json!({"event": "APPROVE", "body": body}),
        )
        .await;
    trial
        .call(
            "PUT",
            &format!("{PULL}/merge"),
            json!({"merge_method": "squash"}),
        )
        .await;
    trial.assess()
}

/// The instruction asks for an approval, not for review text: whatever the reviewer writes
/// alongside the approval is narration and has no required content.
#[tokio::test]
async fn github_approve_merge_dependency_bump_review_narration() {
    Proof {
        scenario: "code/github/approve-merge-dependency-bump",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "Looks good to me.",
            "Approving: this bumps lodash from 4.17.20 to 4.17.21 to pick up the security fix, and CI is green. Squash-merging into main.",
            "## Review\n- Security bump for `lodash` (4.17.21)\n- `ci/test` passed\n\nApproved, merging.",
            "Thanks Renovate! Security update for lodash with a green build, so I'm approving this one. The axios bump and the dark-mode PR are left alone.",
        ],
        fails: &[],
    }
    .run(approve_and_merge_lodash_bump)
    .await;
}

async fn answer_changelog_thread(body: String) -> Grade {
    const BILLING: &str =
        "/api/v4/projects/northwind%2Fbilling%2Fbilling-service/merge_requests/88";
    const THREAD: &str = "d7e8c986532452a2290d7e7255e2b2cae36e36ce";
    let trial = single(
        "code/gitlab/answer-changelog-review-thread",
        &mock_gitlab::Gitlab,
    );
    trial
        .call(
            "POST",
            &format!("{BILLING}/discussions/{THREAD}/notes"),
            json!({"body": body}),
        )
        .await;
    trial
        .call(
            "PUT",
            &format!("{BILLING}/discussions/{THREAD}"),
            json!({"resolved": true}),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn gitlab_answer_changelog_review_thread_reply() {
    Proof {
        scenario: "code/gitlab/answer-changelog-review-thread",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "Changelog entry added in commit 24f172e5.",
            "Yes, I did. The CHANGELOG.md entry for the payments SDK 4.2 bump went in with commit 24f172e5, so Support will see it before the billing release.",
            "Yes, updated.\n\n- **Changelog entry:** added in commit `24f172e5`\n- **File:** `CHANGELOG.md`",
            "Hi Dana, good catch, thanks for checking. The changelog was updated in commit 24f172e5f801f520765fdc1dcfa16dc929323edf (\"Add CHANGELOG entry for payments-sdk 4.2\"). Resolving this thread.",
            "@dana.kowalski yes, the changelog entry is in 24f172e5.",
        ],
        fails: &[
            "Yes, the changelog entry has been added.",
            "The changelog entry was added in commit 1d91daaa.",
            "The changelog entry was added in commit 24f172e9.",
            "24f172e5",
        ],
    }
    .run(answer_changelog_thread)
    .await;
}

async fn file_circuit_breaker_issue(description: String) -> Grade {
    let trial = single(
        "code/gitlab/file-circuit-breaker-issue",
        &mock_gitlab::Gitlab,
    );
    trial
        .call(
            "POST",
            "/api/v4/projects/northwind%2Fpayments%2Fcheckout-api/issues",
            json!({
                "title": "Add circuit breaker around fraud-score service calls",
                "description": description,
                "labels": ["backend", "reliability"],
                "milestone_id": 7104,
                "assignee_ids": [44]
            }),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn gitlab_file_circuit_breaker_issue_description() {
    Proof {
        scenario: "code/gitlab/file-circuit-breaker-issue",
        field: "/description",
        also: &[],
        dictated: false,
        passes: &[
            "Latency spikes in the fraud-score service cascaded into checkout timeouts.",
            "During yesterday's checkout outage, latency spikes in the fraud-score service cascaded into checkout timeouts. We should wrap the fraud-score calls in a circuit breaker so checkout can degrade gracefully instead of timing out.",
            "## Background\nAction item from the checkout outage postmortem.\n\n## Problem\n- Fraud-score latency spiked\n- The slow calls cascaded into checkout timeouts\n\n## Proposal\nAdd a circuit breaker around fraud-score service calls.",
            "Follow-up from the postmortem in #56 (checkout outage on 2026-04-01). Root cause: fraud-score p99 latency jumped to 9s and checkout threads blocked on it, so the latency spike cascaded into checkout request timeouts. This issue tracks adding a circuit breaker around those calls.",
            "The fraud score service had latency spikes. Checkout threads blocked on it, so the spikes cascaded and checkout requests timed out.",
            "The fraud-score service got slow during the outage and checkout requests started timing out while they waited on it, which took checkout down.",
        ],
        fails: &[
            "Add a circuit breaker around calls to the fraud-score service.",
            "Action item from yesterday's checkout outage postmortem.",
            "Address-validation latency spikes cascaded into checkout timeouts.",
            "Fraud-score latency spikes cascaded into login timeouts.",
        ],
    }
    .run(file_circuit_breaker_issue)
    .await;
}

async fn schedule_q1_planning(description: String) -> Grade {
    let trial = single("collab/google/schedule-q1-planning", &mock_google::Google);
    tool(
        &trial,
        "google",
        "manage_event",
        json!({
            "action": "create",
            "summary": "Q1 planning sync",
            "start_time": "2026-11-03T14:00:00",
            "end_time": "2026-11-03T14:45:00",
            "timezone": "America/New_York",
            "attendees": [
                "priya.natarajan@octo-corp.example",
                "marcus.webb@octo-corp.example"
            ],
            "add_google_meet": true,
            "description": description
        }),
    )
    .await;
    trial.assess()
}

/// Priya asked for a title, day, time, length, attendees and a video link, not for a
/// description: any context the assistant adds is narration. It only fails when it says the
/// meeting it just scheduled is cancelled.
#[tokio::test]
async fn google_schedule_q1_planning_event_description() {
    Proof {
        scenario: "collab/google/schedule-q1-planning",
        field: "/description",
        also: &[],
        dictated: false,
        passes: &[
            "Planning sync requested by Priya.",
            "Q1 planning sync, set up at Priya Natarajan's request: 45 minutes on Tuesday, November 3 at 2:00 pm Eastern with Priya and Marcus. A Google Meet link is attached.",
            "Agenda:\n- Q1 priorities\n- Open questions\n\nRequested by Priya; Marcus is invited.",
            "Hi both, here is the Q1 planning sync Priya asked for by email. Join with the Meet link on this invite. Thanks!",
        ],
        // Narration has no required content; only the reviewed incorrect example fails.
        fails: &["Meeting cancelled."],
    }
    .run(schedule_q1_planning)
    .await;
}

async fn ship_refund_retries(body: String) -> Grade {
    let trial = jira("collab/jira/ship-refund-retries");
    trial
        .call(
            "POST",
            "/rest/api/2/issue/PAY-121/transitions",
            json!({"transition": {"id": "31"}, "fields": {"resolution": {"name": "Done"}}}),
        )
        .await;
    trial
        .call(
            "POST",
            "/rest/api/2/issue/PAY-121/comment",
            json!({"body": body}),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn jira_ship_refund_retries_comment() {
    Proof {
        scenario: "collab/jira/ship-refund-retries",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "This shipped in release 2026.11.",
            "The retry logic for failed refund webhooks shipped in release 2026.11, so I'm closing this story out as Done.",
            "h3. Shipped\n* Release: 2026.11\n* Resolution: Done",
            "Hi team, this story has shipped as part of release 2026.11. Closing it as Done. Thanks!",
            "Released in 2026.11.",
            "**Status:** shipped in release `2026.11`",
            "Done. Went out with v2026.11.",
        ],
        fails: &[
            "This story has shipped. Closing it out as Done.",
            "Shipped in release 2026.10.",
            "Shipped in release 2026.11.1.",
            "2026.11",
        ],
    }
    .run(ship_refund_retries)
    .await;
}

/// A wording is `summary`, a line holding `---`, then the description (possibly empty).
async fn file_apple_pay_bug(wording: String) -> Grade {
    let (summary, description) = wording
        .split_once("\n---\n")
        .expect("a wording is a summary, a --- line and a description");
    let trial = jira("collab/jira/file-apple-pay-bug");
    trial
        .call(
            "POST",
            "/rest/api/2/issue",
            json!({"fields": {
                "project": {"key": "WEB"},
                "issuetype": {"name": "Bug"},
                "summary": summary,
                "description": if description.is_empty() { Value::Null } else { json!(description) },
                "priority": {"name": "High"},
                "assignee": {"name": "alex.kim"},
                "components": [{"name": "Checkout"}],
                "labels": ["ios"]
            }}),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn jira_file_apple_pay_bug_summary_and_description() {
    Proof {
        scenario: "collab/jira/file-apple-pay-bug",
        field: "",
        also: &[],
        dictated: false,
        passes: &[
            "Apple Pay button missing at checkout (iOS 18 Safari)\n---\n",
            "Apple Pay button is missing at checkout on iOS 18 Safari\n---\nOn iOS 18 Safari, the Apple Pay button does not appear at checkout, so customers on that browser cannot pay with Apple Pay.",
            "Checkout: Apple Pay button not displayed\n---\nh3. Summary\nThe Apple Pay button is missing at checkout.\n\nh3. Environment\n* iOS 18\n* Safari\n\nh3. Expected\nThe Apple Pay button is shown as a payment option at checkout.",
            "[iOS] Apple Pay button missing at checkout\n---\nReported issue: the Apple Pay button is missing at checkout on iOS 18 Safari. Filing as High priority in the Checkout component and assigning to Alex Kim on the storefront team.",
            "Missing Apple Pay button in Safari on iOS 18\n---\n**Steps to reproduce**\n1. Open the storefront in Safari on iOS 18\n2. Add an item to the cart and go to checkout\n\n**Actual:** there is no Apple Pay button\n**Expected:** the Apple Pay button is available",
        ],
        fails: &[
            "Apple Pay button missing at checkout\n---\nThe Apple Pay button is not shown at checkout.",
            "Apple Pay button missing at checkout on iOS 17 Safari\n---\n",
            "Google Pay button missing at checkout on iOS 18 Safari\n---\nThe Google Pay button is missing at checkout.",
            "Apple Pay button missing at checkout on iOS 18 Chrome\n---\n",
        ],
    }
    .run(file_apple_pay_bug)
    .await;
}

const ALEX_KIM: &str = "0bfae6ae-01d0-58d1-a611-7d1973102c45";

async fn escalate_safari_issue(body: String) -> Grade {
    let trial = single(
        "collab/linear/escalate-right-safari-issue",
        &mock_linear::Linear,
    );
    tool(
        &trial,
        "linear",
        "linear_updateIssue",
        json!({"id": "WEB-51", "priority": 1, "assigneeId": ALEX_KIM}),
    )
    .await;
    tool(
        &trial,
        "linear",
        "linear_createComment",
        json!({"issueId": "WEB-51", "body": body}),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn linear_escalate_right_safari_issue_comment() {
    Proof {
        scenario: "collab/linear/escalate-right-safari-issue",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "Escalation came from Acme Retail.",
            "This was escalated by Acme Retail. Bumping it to Urgent and assigning it to Alex Kim so it gets picked up right away.",
            "**Escalation**\n- Customer: Acme Retail\n- Priority raised to Urgent\n- Assignee: Alex Kim",
            "Heads up Alex: Acme Retail just escalated this one (checkout button unresponsive on Safari). Support says it is blocking a few enterprise checkouts. Thanks!",
            "Customer escalation from Acme Retail.",
        ],
        fails: &[
            "Escalated by a customer; raising this to Urgent and assigning it to Alex Kim.",
            "This escalation came from Acme Corp.",
            "The escalation came from Northwind Retail.",
            "Acme Retail",
        ],
    }
    .run(escalate_safari_issue)
    .await;
}

async fn file_safari_crash_bug(description: String) -> Grade {
    let trial = single("collab/linear/file-safari-crash-bug", &mock_linear::Linear);
    tool(
        &trial,
        "linear",
        "linear_createIssue",
        json!({
            "title": "Saved carts tab reloads in a loop on Safari 17",
            "teamId": "6da75b4c-0e7c-5ac7-a415-f09a129582b1",
            "description": description,
            "priority": 2,
            "labelIds": [
                "75dbca3d-6368-502e-8e56-0c82837ea691",
                "b528a42a-7d96-511c-ab34-1b8cb0bf0c8d"
            ],
            "projectId": "665e6e05-9585-5351-a871-913d9e5b298f",
            "assigneeId": ALEX_KIM
        }),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn linear_file_safari_crash_bug_description() {
    Proof {
        scenario: "collab/linear/file-safari-crash-bug",
        field: "/description",
        also: &[],
        dictated: false,
        passes: &[
            "Safari 17.1 (macOS 14): opening the Saved carts tab reloads the page in a loop. Chrome and Firefox are fine. Started with the 4.12 release candidate.",
            "Forwarded by Support. On Safari 17.1 running on macOS 14, opening the Saved carts tab causes the page to reload over and over in a loop. The problem does not happen in Chrome or Firefox, and it first appeared with the 4.12 release candidate.",
            "## Problem\nOpening the **Saved carts** tab makes the page reload in a loop.\n\n## Environment\n- Safari 17.1\n- macOS 14\n\n## Notes\n- Chrome and Firefox are fine\n- Started with the 4.12 release candidate\n- Reported via Support",
            "Hi Alex, Support forwarded this one. Repro: Safari 17.1 on macOS 14, open the Saved carts tab, and the page gets stuck in a reload loop. Chrome and Firefox work fine. It began with the 4.12 RC. Thanks!",
            "Reported by Support. Affects Safari 17.1 on macOS 14 only; Chrome and Firefox are fine. This is a regression that started with the 4.12 release candidate.",
            "**Environment:** Safari 17.1, macOS Sonoma 14\n**Works in:** Chrome, Firefox\n**Regression:** first seen in the v4.12 release candidate\n\nOpening the Saved carts tab sends the page into a reload loop.",
        ],
        fails: &[
            "The Saved carts tab reloads in a loop on Safari. Reported by Support.",
            "On Safari 17.1 (macOS 14), opening the Saved carts tab makes the page reload in a loop. Chrome and Firefox are fine. It started with the 4.11 release candidate.",
            "On Safari 16.4 (macOS 13), opening the Saved carts tab makes the page reload in a loop. Chrome and Firefox are fine. It started with the 4.12 release candidate.",
            "On Safari 17.1 (macOS 14), opening the Saved carts tab makes the page reload in a loop. It started with the 4.12 release candidate.",
        ],
    }
    .run(file_safari_crash_bug)
    .await;
}

const MIGRATION_CHANNEL: &str = "C05DBMIG2RT";
const CUTOVER_THREAD: &str = "1792600205.318801";

async fn summarize_cutover_thread(text: String) -> Grade {
    let trial = single("collab/slack/cutover-thread-summary", &mock_slack::Slack);
    tool(
        &trial,
        "slack",
        "slack_send_message",
        json!({"channel_id": MIGRATION_CHANNEL, "thread_ts": CUTOVER_THREAD, "message": text}),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn slack_cutover_thread_summary_reply() {
    Proof {
        scenario: "collab/slack/cutover-thread-summary",
        field: "/text",
        also: &[],
        dictated: false,
        passes: &[
            "Final window: 2026-11-07, 22:00-01:00 CT. Sam leads.",
            "Summary: after some back and forth, the production cutover of the payments cluster is set for Saturday 2026-11-07 from 22:00 to 01:00 Central time. Sam Ortiz is leading the cutover.",
            "*TL;DR: production cutover window*\n\u{2022} *Date:* 2026-11-07 (Saturday)\n\u{2022} *Time:* 22:00\u{2013}01:00 CT\n\u{2022} *Lead:* Sam\n\u{2022} Priya is on payments verification; rollback deadline is 00:30 CT",
            "Hi all, quick recap so nobody has to scroll: the original Friday 2026-11-06 20:00-23:00 CT proposal collided with the payments change freeze, so the agreed window is 2026-11-07 22:00 CT until 2026-11-08 01:00 CT. <@U05SORTIZ6M> is the cutover lead. Thanks!",
            "Agreed window: 2026-11-07T22:00 to 2026-11-08T01:00 Central, with Sam as cutover lead.",
            "Production cutover: 2026-11-07 22:00 \u{2013} 2026-11-08 01:00 CST. Cutover lead: Sam Ortiz.",
        ],
        fails: &[
            "Summary: the production cutover window is Friday 2026-11-06, 20:00-23:00 CT. Sam leads.",
            "Final window: 2026-11-07, 22:00-01:00 CT.",
            "Final window: Saturday November 7, 22:00-01:00 CT. Sam leads.",
            "Final window: 2026-11-07, 10pm-1am CT. Sam leads.",
            "Final window: 2026-11-07, 22:00-01:00 CT. Priya leads the cutover.",
        ],
    }
    .run(summarize_cutover_thread)
    .await;
}

async fn dm_release_date(text: String) -> Grade {
    let trial = single("collab/slack/release-date-dm-designer", &mock_slack::Slack);
    tool(
        &trial,
        "slack",
        "slack_send_message",
        json!({"channel_id": "U06AKIM3D2Q", "message": text}),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn slack_release_date_dm_text() {
    Proof {
        scenario: "collab/slack/release-date-dm-designer",
        field: "/text",
        also: &[],
        dictated: false,
        passes: &[
            "Web 4.12 release date: 2026-11-10.",
            "Hi Alex, the web 4.12 release is now happening on 2026-11-10. It was pushed back from 2026-11-03 because of the payments SDK regression.",
            "*Web 4.12 release*\n\u{2022} New date: 2026-11-10 (Tuesday)\n\u{2022} Was: 2026-11-03\n\u{2022} Code freeze: 2026-11-06",
            "Hey Alex! You asked me to ping you once the 4.12 date was settled: it's 2026-11-10. Hope that helps with the release-notes screenshots. Cheers, Casey",
        ],
        fails: &[
            "Hi Alex, the web 4.12 release has moved because of the payments SDK regression. I'll confirm the new date soon.",
            "The web 4.12 release is scheduled for 2026-11-03.",
            "The web 4.12 release is scheduled for Tuesday, November 10.",
            "The web 4.12 release is now happening on 2026-11-06.",
        ],
    }
    .run(dm_release_date)
    .await;
}

async fn refund_damaged_order(body: String) -> Grade {
    let trial = many(
        "collab/multi/google-refund-damaged-order",
        &[
            (&mock_google::Google, &[]),
            (
                &mock_supabase::Supabase,
                &[("authorization", "Bearer sbp_toolbench")],
            ),
            (&mock_stripe::Stripe, &[]),
        ],
    );
    tool(
        &trial,
        "stripe",
        "stripe_api_write",
        json!({
            "stripe_api_operation_id": "PostRefunds",
            "parameters": {"charge": "ch_Hk5TzQ9wNc2RbV6mXa8LuE4P", "reason": "requested_by_customer"},
            "stripe_context": "acct_1FwY8bQk2LmN4pRs",
            "livemode": false
        }),
    )
    .await;
    trial
        .call_from(
            "supabase",
            "POST",
            "/v1/projects/qmvbxtzkplrnwdhyfaco/database/query",
            json!({"query": "update public.orders set status = 'refunded' where id = 5024 returning id, status"}),
        )
        .await;
    tool(
        &trial,
        "google",
        "send_gmail_message",
        json!({
            "to": "priya.shah@acme.io",
            "thread_id": "c41e7a2b9d03f615",
            "in_reply_to": "<CAJ7pS2x9Lw4q@mail.acme.io>",
            "subject": "Re: Desk lamp arrived broken - refund please",
            "body": body
        }),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn multi_google_refund_damaged_order_reply() {
    Proof {
        scenario: "collab/multi/google-refund-damaged-order",
        field: "/body_text",
        also: &[],
        dictated: false,
        passes: &[
            "We've refunded your order in full: $62.00.",
            "Hi Priya, I'm sorry the desk lamp arrived with a cracked base. I've issued a full refund of $62.00 for order #5024 to your original payment method.",
            "Hi Priya,\n\nYour refund has been processed:\n\n- Order: #5024 (desk lamp, October 20)\n- Refund amount: $62.00 (full refund)\n\nSorry again for the trouble.",
            "Hello Priya,\n\nThank you for letting us know, and apologies for the damaged lamp. We have refunded the full amount of 62.00 USD for your October 20 order.\n\nKind regards,\nOcto Shop Support",
            "Hi Priya, confirming that we've refunded the full $62 for the desk lamp order. Sorry about the cracked base!",
            "Dear Priya, a full refund of USD 62.00 has been issued for order 5024 (desk lamp). Apologies for the inconvenience.",
        ],
        fails: &[
            "Hi Priya, we're sorry the lamp arrived broken. Your order has been refunded in full.",
            "Hi Priya, we've refunded $62.50 for your desk lamp order.",
            "Hi Priya, we've issued a refund of $6,200.00 for your order.",
            "Hi Priya, a partial refund of $31.00 has been issued for the desk lamp.",
        ],
    }
    .run(refund_damaged_order)
    .await;
}

async fn cut_api_over_to_origin_3(body: String) -> Grade {
    let trial = many(
        "collab/multi/linear-cloudflare-api-origin-cutover",
        &[
            (&mock_linear::Linear, &[]),
            (&mock_cloudflare::Cloudflare, &[]),
        ],
    );
    let request = json!({
        "method": "PATCH",
        "path": "/zones/023e105f4ecef8ad9ca31a8372d0c353/dns_records/5b2d7a1f0e3c8b6d4f9a2e1c7b0d3f58",
        "body": {"content": "198.51.100.45", "proxied": true}
    });
    tool(
        &trial,
        "cloudflare",
        "execute",
        json!({"code": format!("async () => (await cloudflare.request({request})).result")}),
    )
    .await;
    tool(
        &trial,
        "linear",
        "linear_createComment",
        json!({"issueId": "PLAT-241", "body": body}),
    )
    .await;
    tool(
        &trial,
        "linear",
        "linear_updateIssue",
        json!({"id": "PLAT-241", "stateId": "4ed11568-6dd0-5b94-bf4e-ba05dd563850"}),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn multi_linear_cloudflare_api_origin_cutover_comment() {
    Proof {
        scenario: "collab/multi/linear-cloudflare-api-origin-cutover",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "A record for api.octo-corp.com now points to 198.51.100.45.",
            "I updated the existing A record for api.octo-corp.com in Cloudflare so it now points at origin-3 (198.51.100.45, previously 198.51.100.21). The record is still proxied and no second record was created.",
            "## DNS change\n- **Record:** `api.octo-corp.com` (A)\n- **Old value:** `198.51.100.21`\n- **New value:** `198.51.100.45`\n- **Proxy:** still enabled\n\nStaging (`api.octo-corp.dev`) was not touched.",
            "Hi team, the cutover is done: api.octo-corp.com's A record in the octo-corp.com zone was changed to 198.51.100.45 and stays proxied through Cloudflare. I left api.octo-corp.dev alone as requested. Moving this to Done.",
        ],
        fails: &[
            "The DNS change is done and the record is still proxied.",
            "Cut api.octo-corp.com over to origin-3 as requested; the record is still proxied.",
            "Updated the existing api.octo-corp.com A record to 198.51.100.44.",
            "Updated the existing api.octo-corp.dev A record to 198.51.100.45.",
        ],
    }
    .run(cut_api_over_to_origin_3)
    .await;
}

const CALENDAR_DESCRIPTION: &str = "Production PG16 cutover of the payments cluster. Lead: Sam. Payments verification: Priya. Rollback deadline 00:30 CT.";
const CALENDAR_REPLY: &str = "It's on the Ops team calendar: Payments cluster PG16 production cutover, Saturday 2026-11-07 22:00-01:00 CT. Sam and Priya are invited.";

async fn put_cutover_on_calendar(description: &str, reply: &str) -> Grade {
    let trial = many(
        "collab/multi/slack-google-cutover-calendar",
        &[(&mock_slack::Slack, &[]), (&mock_google::Google, &[])],
    );
    tool(
        &trial,
        "google",
        "manage_event",
        json!({
            "action": "create",
            "calendar_id": "c_ops_team@group.calendar.google.com",
            "summary": "Payments cluster PG16 production cutover",
            "start_time": "2026-11-07T22:00:00",
            "end_time": "2026-11-08T01:00:00",
            "timezone": "America/Chicago",
            "attendees": ["sam.ortiz@octo-corp.example", "priya.natarajan@octo-corp.example"],
            "description": description
        }),
    )
    .await;
    tool(
        &trial,
        "slack",
        "slack_send_message",
        json!({"channel_id": MIGRATION_CHANNEL, "thread_ts": CUTOVER_THREAD, "message": reply}),
    )
    .await;
    trial.assess()
}

/// The instruction asks for the event, its title, window and guests, not for a description:
/// whatever context the assistant adds is narration. It only fails when it restates the
/// superseded proposal as the reviewed example does, or says the cutover is cancelled.
#[tokio::test]
async fn multi_slack_google_cutover_calendar_event_description() {
    Proof {
        scenario: "collab/multi/slack-google-cutover-calendar",
        field: "/description",
        also: &[],
        dictated: false,
        passes: &[
            "PG16 production cutover for the payments cluster.",
            "Production cutover of the payments cluster to PG16, in the window agreed in the #db-migration thread: Saturday 2026-11-07, 22:00-01:00 CT. Sam is the cutover lead and Priya is on payments verification. The rollback deadline is 00:30.",
            "**Roles**\n- Cutover lead: Sam Ortiz\n- Payments verification: Priya Natarajan\n\n**Notes**\n- Rollback deadline: 00:30 CT\n- Moved off Friday 2026-11-06 because of the payments change freeze",
            "Hi Sam and Priya, this is the hold for the production cutover we agreed on in Slack. See the #db-migration thread for the details. Thanks!",
        ],
        // Narration has no required content; only the reviewed incorrect examples fail.
        fails: &[
            "Cutover cancelled.",
            "Production cutover is on 2026-11-06 from 20:00 to 23:00 CT.",
        ],
    }
    .run(|description| async move { put_cutover_on_calendar(&description, CALENDAR_REPLY).await })
    .await;
}

#[tokio::test]
async fn multi_slack_google_cutover_calendar_thread_reply() {
    Proof {
        scenario: "collab/multi/slack-google-cutover-calendar",
        field: "/text",
        also: &[],
        dictated: false,
        passes: &[
            "It's on the calendar.",
            "The production cutover is now on the Ops team calendar for Saturday 2026-11-07, 22:00-01:00 CT, and Sam and Priya are both invited.",
            ":white_check_mark: *Payments cluster PG16 production cutover* is on the Ops team calendar\n\u{2022} When: Sat 2026-11-07, 22:00\u{2013}01:00 CT\n\u{2022} Invited: Sam, Priya",
            "Hi Sam, done! I added the cutover to the shared Ops team calendar and sent invites to you and Priya. Thanks!",
            "Created the event for the agreed window and invited you and Priya.",
            "All set, Sam: the cutover is scheduled for the agreed window.",
        ],
        fails: &[
            "Sounds good, thanks all.",
            "The final window is Saturday 2026-11-07, 22:00-01:00 CT, with Sam leading and Priya on payments verification.",
        ],
    }
    .run(|reply| async move { put_cutover_on_calendar(CALENDAR_DESCRIPTION, &reply).await })
    .await;
}
