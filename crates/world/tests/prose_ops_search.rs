//! Ordinary wording of the free text in operations and research write tasks gets a pass/fail
//! verdict. Every wording travels through the mock services into state and through `grade`.

mod support;

use mock_core::grading::Grade;
use serde_json::{Value, json};
use support::{ContractTrial, prose::Proof};

const PAGERDUTY_TOKEN: (&str, &str) = ("authorization", "Token token=local-fixture-credential");
const CHECKOUT_INCIDENT: &str = "Q7CHKAP15PCT1D";

async fn pagerduty_note(trial: &ContractTrial, incident: &str, content: &str) {
    trial
        .call_from(
            "pagerduty",
            "POST",
            &format!("/incidents/{incident}/notes"),
            json!({"note": {"content": content}}),
        )
        .await;
}

async fn pagerduty_update(trial: &ContractTrial, incident: &str, attributes: Value) {
    let mut input = json!({"id": incident, "type": "incident_reference"});
    for (field, value) in attributes.as_object().unwrap() {
        input[field] = value.clone();
    }
    trial
        .call_from(
            "pagerduty",
            "PUT",
            "/incidents",
            json!({"incidents": [input]}),
        )
        .await;
}

// ops/multi/checkout-saturation-scale-and-resolve

async fn checkout_saturation(note: String) -> Grade {
    let trial = ContractTrial::load_many_with_dataset(
        "ops/multi/checkout-saturation-scale-and-resolve",
        &[
            (&mock_kubernetes::Kubernetes, &[]),
            (
                &mock_pagerduty::Pagerduty,
                &[PAGERDUTY_TOKEN, ("from", "lena.weber@tidewater.io")],
            ),
        ],
        |_| {},
    );
    trial
        .call_from(
            "kubernetes",
            "PATCH",
            "/apis/apps/v1/namespaces/shop-prod/deployments/checkout/scale",
            json!({"spec": {"replicas": 9}}),
        )
        .await;
    pagerduty_note(&trial, CHECKOUT_INCIDENT, &note).await;
    pagerduty_update(&trial, CHECKOUT_INCIDENT, json!({"status": "resolved"})).await;
    trial.assess()
}

#[tokio::test]
async fn checkout_saturation_remediation_note() {
    Proof {
        scenario: "ops/multi/checkout-saturation-scale-and-resolve",
        field: "/content",
        also: &[],
        dictated: false,
        passes: &[
            "Scaled checkout to 9 replicas in shop-prod per RB-CHK-07.",
            "Applied the runbook remediation for this incident (RB-CHK-07, checkout saturation): scaled the checkout Deployment in the shop-prod namespace from 3 to 9 replicas. All 9 pods are Running and Ready, so I'm resolving the incident. checkout-worker and the staging namespace were not touched.",
            "**Remediation applied (RB-CHK-07)**\n\n- Deployment: `shop-prod/checkout`\n- Change: `spec.replicas` 3 → 9\n- Status: 9/9 pods Ready\n\nResolving this incident.",
            "Hi team, per the runbook step on this incident I increased the production checkout deployment (namespace shop-prod) to nine replicas (it was at 3) to relieve the CPU saturation on POST /v2/checkout. The new pods are up, so I am resolving the page.\n\nThanks,\nLena",
            "kubectl scale deployment/checkout -n shop-prod --replicas=9 (was 3). p95 should recover as the new pods take traffic.",
            "Bumped checkout from 3 to 9 in shop-prod, as the runbook says.",
        ],
        fails: &[
            "Applied the runbook remediation for checkout and confirmed the workload is healthy. Resolving.",
            "Scaled the checkout Deployment in shop-prod from 3 to 6 replicas per RB-CHK-07.",
            "Scaled checkout in shop-prod to 19 replicas per the runbook.",
        ],
    }
    .run(checkout_saturation)
    .await;
}

// ops/multi/pagerduty-sentry-github-issue

const ESCALATION_TITLE: &str =
    "CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s";
const ESCALATION_BODY: &str = "PagerDuty incident #4810 (Checkout error rate above 5%, production).\n\nSentry: CHECKOUT-API-3F\nCulprit: checkout.views.orders in create_order\nError: OperationalError: could not obtain connection from pool within 5.0s\nEndpoint: POST /api/orders/ (1,842 events, 611 users)";
const ESCALATION_NOTE: &str =
    "Tracking the fix in GitHub issue lumen-labs/checkout#{ISSUE} (Sentry CHECKOUT-API-3F).";

/// Opens the issue, then notes it on the incident. `{ISSUE}` in the note stands for the number
/// the GitHub service emitted for the new issue.
async fn escalation(title: &str, body: &str, note: &str) -> Grade {
    let trial = ContractTrial::load_many_with_dataset(
        "ops/multi/pagerduty-sentry-github-issue",
        &[
            (
                &mock_pagerduty::Pagerduty,
                &[PAGERDUTY_TOKEN, ("from", "lena.weber@lumenlabs.dev")],
            ),
            (
                &mock_sentry::Sentry,
                &[("authorization", "Bearer sntrys_toolbench")],
            ),
            (&mock_github::GitHub, &[]),
        ],
        |_| {},
    );
    let number = trial
        .call_from(
            "github",
            "POST",
            "/api/v3/repos/lumen-labs/checkout/issues",
            json!({"title": title, "body": body}),
        )
        .await["number"]
        .as_u64()
        .unwrap();
    pagerduty_note(
        &trial,
        CHECKOUT_INCIDENT,
        &note.replace("{ISSUE}", &number.to_string()),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn escalation_issue_title() {
    Proof {
        scenario: "ops/multi/pagerduty-sentry-github-issue",
        field: "/title",
        // The issue is selected, and its creation permitted, by a title that starts with the
        // short ID, so a title without it leaves no issue for the text check to read.
        also: &[
            "/services/github/issues selected 0 records",
            "/services/github/issues/",
        ],
        dictated: false,
        passes: &[
            "CHECKOUT-API-3F: DB connection pool timeout",
            "CHECKOUT-API-3F: OperationalError: could not obtain connection from pool within 5.0s (checkout.views.orders in create_order)",
            "CHECKOUT-API-3F: `create_order` cannot get a database connection, production checkout failing",
            "CHECKOUT-API-3F: Checkout error rate above 5% in production (PagerDuty #4810)",
            "CHECKOUT-API-3F: OperationalError on POST /api/orders/",
            "CHECKOUT-API-3F: Production checkout failures when creating orders",
        ],
        fails: &[
            "OperationalError: could not obtain connection from pool within 5.0s",
            "Checkout API: could not obtain connection from pool (Sentry CHECKOUT-API-3F)",
            "CHECKOUT-API-3C: OperationalError: could not obtain connection from pool within 5.0s",
            "PAYMENTS-WORKER-1K: OperationalError: could not obtain connection from pool within 5.0s",
        ],
    }
    .run(|title| async move { escalation(&title, ESCALATION_BODY, ESCALATION_NOTE).await })
    .await;
}

#[tokio::test]
async fn escalation_issue_description() {
    Proof {
        scenario: "ops/multi/pagerduty-sentry-github-issue",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "PagerDuty incident #4810. Sentry culprit: checkout.views.orders in create_order. Error message: OperationalError: could not obtain connection from pool within 5.0s",
            "PagerDuty paged us for incident #4810 (\"Checkout error rate above 5%\") on the production checkout-api service. The error behind it in Sentry is CHECKOUT-API-3F: an OperationalError raised in checkout.views.orders in create_order with the message \"could not obtain connection from pool within 5.0s\". It has been seen 1,842 times and affects 611 users on POST /api/orders/. Could the payments team take a look?",
            "## Summary\nProduction checkout is failing with database connection pool timeouts.\n\n## Details\n- **PagerDuty incident:** #4810 (Checkout error rate above 5%)\n- **Sentry issue:** CHECKOUT-API-3F\n- **Culprit:** `checkout.views.orders in create_order`\n- **Error message:** `OperationalError: could not obtain connection from pool within 5.0s`\n- **Transaction:** `POST /api/orders/`\n- **Events / users:** 1,842 / 611",
            "Hi payments team,\n\nWe were paged (PagerDuty incident number 4810) about the checkout error rate in production. Sentry points at checkout.views.orders in create_order, which is failing with:\n\n    could not obtain connection from pool within 5.0s\n\nPlease investigate.\n\nThanks!",
            "Incident 4810 (production, not the staging load-test alert #4811).\nCulprit: checkout.views.orders in create_order\nError: could not obtain connection from pool within 5.0s\nNote: PAYMENTS-WORKER-1K shows the same message in another project and is tracked separately.",
            "### Context\nPagerDuty **#4810** fired for the production checkout error rate.\n\n### Sentry\n- Culprit: `checkout.views.orders` in `create_order`\n- Message: *could not obtain connection from pool within 5.0s*",
        ],
        fails: &[
            "Sentry issue CHECKOUT-API-3F: checkout.views.orders in create_order raises OperationalError: could not obtain connection from pool within 5.0s. Seen in production checkout.",
            "Tracking PagerDuty incident #4811. Culprit: checkout.views.orders in create_order. Error message: could not obtain connection from pool within 5.0s.",
            "PagerDuty incident #4810. Culprit: payments_worker.tasks.capture in capture_payment. Error message: could not obtain connection from pool within 5.0s.",
            "PagerDuty incident #4810; the Sentry culprit is checkout.views.orders in create_order. See Sentry for the stack trace.",
        ],
    }
    .run(|body| async move { escalation(ESCALATION_TITLE, &body, ESCALATION_NOTE).await })
    .await;
}

#[tokio::test]
async fn escalation_incident_note() {
    Proof {
        scenario: "ops/multi/pagerduty-sentry-github-issue",
        field: "/content",
        also: &[],
        dictated: false,
        passes: &[
            "Opened GitHub issue #{ISSUE}.",
            "I opened GitHub issue {ISSUE} in lumen-labs/checkout so the payments team can fix the Sentry error behind this page (CHECKOUT-API-3F, OperationalError: could not obtain connection from pool within 5.0s).",
            "**GitHub issue:** lumen-labs/checkout#{ISSUE}\n- Sentry: CHECKOUT-API-3F\n- Culprit: `checkout.views.orders in create_order`",
            "Hi all, the fix is being tracked on GitHub: https://github.octo-corp.com/lumen-labs/checkout/issues/{ISSUE}. Thanks, Lena",
            "GitHub issue number: {ISSUE}",
        ],
        fails: &[
            "Opened a GitHub issue in lumen-labs/checkout for incident 4810 (Sentry CHECKOUT-API-3F).",
            "Opened GitHub issue #207 in lumen-labs/checkout for this incident.",
            "The fix is tracked in GitHub issue lumen-labs/checkout#{ISSUE}0.",
        ],
    }
    .run(|note| async move { escalation(ESCALATION_TITLE, ESCALATION_BODY, &note).await })
    .await;
}

// code/multi/terraform-failure-grafana-annotate

async fn annotation(text: String) -> Grade {
    let grafana = format!("Bearer {}", mock_grafana::TOKEN);
    let trial = ContractTrial::load_many_with_dataset(
        "code/multi/terraform-failure-grafana-annotate",
        &[
            (
                &mock_terraform::Terraform,
                &[("authorization", "Bearer local-fixture-credential")],
            ),
            (&mock_grafana::Grafana, &[("authorization", &grafana)]),
        ],
        |_| {},
    );
    trial
        .call_from(
            "grafana",
            "POST",
            "/api/annotations",
            json!({
                "dashboardUID": "payments-db-prod",
                "time": 1_793_193_129_000_i64,
                "text": text,
                "tags": ["terraform", "payments"]
            }),
        )
        .await;
    trial.assess()
}

/// The contract selects the annotation by a pattern that needs the run ID and the error code as
/// substrings, so text that lacks one never reaches the text check: it fails the selection
/// (see `annotation_rejects_wrong_run_error_code_and_missing_required_facts`). The wrong wordings
/// here are therefore other identifiers that merely contain the right ones.
#[tokio::test]
async fn terraform_failure_annotation_text() {
    Proof {
        scenario: "code/multi/terraform-failure-grafana-annotate",
        field: "/text",
        also: &[],
        dictated: false,
        passes: &[
            "run-Gy4tBHGggKx49nbH: InvalidParameterCombination",
            "Postgres 16 upgrade of the production payments database failed: HCP Terraform run run-Gy4tBHGggKx49nbH on payments-api-prod errored during apply with AWS error InvalidParameterCombination.",
            "Terraform apply errored\nRun: run-Gy4tBHGggKx49nbH\nWorkspace: payments-api-prod\nAWS error code: InvalidParameterCombination",
            "Failed Terraform run run-Gy4tBHGggKx49nbH (\"Upgrade payments DB to Postgres 16\"): api error InvalidParameterCombination: Cannot upgrade postgres from 14.12 to 16.4. You must set AllowMajorVersionUpgrade to true when upgrading to a new major version.",
            "AWS `InvalidParameterCombination` on aws_db_instance.payments (ModifyDBInstance), Terraform run `run-Gy4tBHGggKx49nbH`",
        ],
        fails: &[
            "Terraform run run-Gy4tBHGggKx49nbH2 failed on payments-api-prod with InvalidParameterCombination",
            "Terraform run run-Gy4tBHGggKx49nbH-retry failed on payments-api-prod with InvalidParameterCombination",
            "Terraform run run-Gy4tBHGggKx49nbH failed on payments-api-prod with InvalidParameterCombinationException",
        ],
    }
    .run(annotation)
    .await;
}

// ops/pagerduty/reassign-replication-lag and ops/pagerduty/resolve-prod-checkout

fn pagerduty(id: &str, actor: &str) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_pagerduty::Pagerduty,
        &[PAGERDUTY_TOKEN, ("from", actor)],
        |_| {},
    )
}

async fn replication_lag_handoff(note: String) -> Grade {
    let trial = pagerduty(
        "ops/pagerduty/reassign-replication-lag",
        "alex.kim@tidewater.io",
    );
    pagerduty_update(
        &trial,
        "Q3K8ORDBLAG2RX",
        json!({"assignments": [{"assignee": {"id": "PDNGYN2", "type": "user_reference"}}]}),
    )
    .await;
    pagerduty_note(&trial, "Q3K8ORDBLAG2RX", &note).await;
    trial.assess()
}

#[tokio::test]
async fn replication_lag_handoff_note() {
    Proof {
        scenario: "ops/pagerduty/reassign-replication-lag",
        field: "/content",
        also: &[],
        dictated: true,
        passes: &[
            "Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize.",
            "**Handing off to DBRE:** replica-2 disk is at 100%, needs a volume resize.",
            "Handing off to DBRE:\n`replica-2` disk is at 100%, needs a volume resize.",
            "_Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize._",
            "handing off to DBRE: replica-2 disk is at 100%, needs a volume resize",
        ],
        fails: &[
            "Handing off to DBRE: replica-1 disk is at 100%, needs a volume resize.",
            "Handing off to DBRE: replica-2 disk is at 90%, needs a volume resize.",
            "Handing off to DBRE.",
            "Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize. The volume has since been resized.",
        ],
    }
    .run(replication_lag_handoff)
    .await;
}

async fn checkout_resolution(note: String) -> Grade {
    let trial = pagerduty(
        "ops/pagerduty/resolve-prod-checkout",
        "lena.weber@tidewater.io",
    );
    pagerduty_note(&trial, CHECKOUT_INCIDENT, &note).await;
    pagerduty_update(&trial, CHECKOUT_INCIDENT, json!({"status": "resolved"})).await;
    trial.assess()
}

#[tokio::test]
async fn checkout_resolution_note() {
    Proof {
        scenario: "ops/pagerduty/resolve-prod-checkout",
        field: "/content",
        also: &[],
        dictated: false,
        passes: &[
            "Resolved by rollback of release 2026.10.29.3.",
            "This incident was resolved by rolling back release 2026.10.29.3. The checkout error rate is back to baseline, so I am marking it resolved.",
            "**Resolution**\n- Fix: release `2026.10.29.3` was rolled back\n- Status: error rate back to baseline, resolving",
            "Hi all, closing this one out: the team rolled back release 2026.10.29.3 and checkout errors have returned to baseline. The staging incident stays open while the load test runs.\n\nThanks,\nLena",
            "Fixed: release 2026.10.29.3 was reverted and errors are back to baseline.",
        ],
        fails: &[
            "Resolved; errors are back to baseline after the rollback.",
            "Resolved by rolling back release 2026.10.29.",
            "Resolved by rolling back release 2026.10.29.30.",
            "Resolved by rolling back release 2026.10.28.3.",
            "Release 2026.10.29.3 is live and errors are back to baseline.",
        ],
    }
    .run(checkout_resolution)
    .await;
}

// search/multi/competitor-research-github-issue

const GA_TITLE: &str = "Lumenrail Scale plan is generally available";
const GA_BODY: &str = "Lumenrail Scale plan (generally available): $349 per month billed annually, 50M events per month included.\n\nSource: https://lumenrail.io/blog/scale-plan-generally-available";

async fn competitor_issue(title: &str, body: &str) -> Grade {
    let trial = ContractTrial::load_many_with_dataset(
        "search/multi/competitor-research-github-issue",
        &[
            (&mock_exa::Exa, &[("x-api-key", "research-contract")]),
            (&mock_github::GitHub, &[]),
        ],
        |_| {},
    );
    trial
        .call_from(
            "exa",
            "POST",
            "/search",
            json!({
                "query": "Lumenrail Scale plan generally available pricing events per month",
                "contents": {"text": true}
            }),
        )
        .await;
    trial
        .call_from(
            "exa",
            "POST",
            "/contents",
            json!({
                "urls": ["https://lumenrail.io/blog/scale-plan-generally-available"],
                "text": true
            }),
        )
        .await;
    trial
        .call_from(
            "github",
            "POST",
            "/api/v3/repos/octo-org/competitive-intel/issues",
            json!({"title": title, "body": body, "labels": ["pricing", "lumenrail"]}),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn competitor_issue_title() {
    Proof {
        scenario: "search/multi/competitor-research-github-issue",
        field: "/title",
        also: &[],
        dictated: false,
        passes: &[
            "Lumenrail Scale plan",
            "Lumenrail Scale plan now GA: $349/mo (annual), 50M events/mo",
            "Competitive intel: Lumenrail takes its Scale plan out of beta",
            "[Lumenrail] Scale plan GA pricing (Oct 2026)",
            "Lumenrail's Scale plan leaves beta: new price and included events",
        ],
        fails: &[
            "Competitor pricing update: Scale plan is now GA",
            "Lumenwise Scale plan GA pricing",
            "Lumenrail GA pricing update",
        ],
    }
    .run(|title| async move { competitor_issue(&title, GA_BODY).await })
    .await;
}

#[tokio::test]
async fn competitor_issue_description() {
    Proof {
        scenario: "search/multi/competitor-research-github-issue",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "Scale plan (GA): $349/mo billed annually, 50M events/mo included. Source: https://lumenrail.io/pricing",
            "Lumenrail moved its Scale plan from beta to general availability on October 27, 2026. Scale now costs $349 per month when billed annually ($419 if billed monthly) and includes 50 million events per month; additional events are $6 per million. Source: https://lumenrail.io/blog/scale-plan-generally-available",
            "## Lumenrail Scale plan: GA pricing\n\n| | GA (current) | Beta (2025) |\n|---|---|---|\n| Price | **$349/month** billed annually | $299/month |\n| Included events | **50M events/month** | 25M events/month |\n\n### Sources\n- [GA announcement](https://lumenrail.io/blog/scale-plan-generally-available)\n- [Pricing page](https://lumenrail.io/pricing)",
            "Hi team,\n\nLogging this week's change for the tracker. Lumenrail's Scale plan is out of beta. The GA price is $349 a month on annual billing and it comes with 50,000,000 events every month, plus 13-month retention and SAML SSO. Beta customers keep their old price until renewal.\n\nSource: https://lumenrail.io/blog/scale-plan-generally-available\n\nThanks!",
            "- Price: $349 per month (annual billing)\n- Included: 50M events per month\n- Source: lumenrail.io/pricing",
        ],
        fails: &[
            "Lumenrail's Scale plan is now generally available and includes 50M events per month. Source: https://lumenrail.io/blog/scale-plan-generally-available",
            "Scale costs $299/month and includes 25M events/month (https://lumenrail.io/blog/introducing-scale-beta).",
            "Scale is generally available at $349/month billed annually with 50M events per month included.",
            "Scale plan (GA): $419/month, 50M events/month. Source: https://lumenrail.io/pricing",
            "Scale plan (GA): $349/month billed annually, 25M events/month. Source: https://lumenrail.io/pricing",
        ],
    }
    .run(|body| async move { competitor_issue(GA_TITLE, &body).await })
    .await;
}

// search/multi/vendor-rate-limit-zendesk-reply

/// Replies with `reply` through the tool the reference solution uses. The service renders the
/// Markdown to `html_body` and derives `body` from that rendering, so both fields always carry
/// the same reply, and a wrong reply fails both checks.
async fn vendor_reply(reply: String) -> Grade {
    let trial = ContractTrial::load_many_with_dataset(
        "search/multi/vendor-rate-limit-zendesk-reply",
        &[
            (
                &mock_tavily::Tavily,
                &[("authorization", "Bearer tvly-research-contract")],
            ),
            (&mock_zendesk::Zendesk, &[]),
        ],
        |_| {},
    );
    trial
        .call_from(
            "tavily",
            "POST",
            "/search",
            json!({"query": "Quillstream API rate limit Business plan requests per minute"}),
        )
        .await;
    trial
        .call_from(
            "tavily",
            "POST",
            "/extract",
            json!({"urls": ["https://docs.quillstream.io/api/rate-limits"]}),
        )
        .await;
    for (name, arguments) in [
        (
            "create_ticket_comment",
            json!({"ticket_id": 5302, "public": true, "comment": reply}),
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
    trial.assess()
}

const RATE_LIMIT_REPLIES: [&str; 5] = [
    "Quillstream's Business plan is limited to 750 requests per minute.",
    "Hi Priya,\n\nThanks for your patience. I checked Quillstream's current API reference (v3, last updated September 14, 2026): the Business plan is limited to 750 requests/minute, with a burst allowance of 120. The limit applies per workspace, not per API key, so every integration in your workspace shares it. If you keep your sync under that rate the 429 errors should stop.\n\nLet us know how it goes.\n\nBest regards,\nRita",
    "Hello Priya,\n\nHere is what Quillstream's up-to-date (v3) rate-limit documentation says for the **Business** plan:\n\n- **Limit:** **750 requests per minute**\n- **Burst:** 120\n- **Scope:** per workspace (shared by all API keys and integrations)\n- When exceeded, the API returns `429 Too Many Requests` with a `Retry-After` header\n\nYou may have seen 300 or 500 quoted elsewhere; those come from the deprecated v2 docs and an old forum thread and no longer apply.\n\nKind regards,\nRita",
    "Hi Priya, short answer: Business is capped at 750 RPM per workspace on Quillstream's current v3 API. Tuning the sync to stay below that should clear the 429s.",
    "Priya, the current limit for Business is 750 req/min (source: https://docs.quillstream.io/api/rate-limits).\n\nRita",
];

const WRONG_RATE_LIMIT_REPLIES: [&str; 4] = [
    "Hi Priya,\n\nQuillstream's Business plan allows 300 requests per minute, so please keep your sync below that.\n\nRita",
    "Hi Priya,\n\nI looked into this: Quillstream rate limits are applied per workspace and the API answers 429 with a Retry-After header once you go over. Please slow the sync down and let us know if it keeps failing.\n\nRita",
    "Hi Priya,\n\nThe Business plan allows 7500 requests per minute on Quillstream's current API.\n\nRita",
    "Hello Priya,\n\n- **Plan:** Business\n- **Limit:** 500 requests per minute (per the Quillstream community forum)\n\nRita",
];

#[tokio::test]
async fn vendor_rate_limit_reply_text() {
    Proof {
        scenario: "search/multi/vendor-rate-limit-zendesk-reply",
        field: "/body",
        also: &["field /html_body:"],
        dictated: false,
        passes: &RATE_LIMIT_REPLIES,
        fails: &WRONG_RATE_LIMIT_REPLIES,
    }
    .run(vendor_reply)
    .await;
}

#[tokio::test]
async fn vendor_rate_limit_reply_rendering() {
    Proof {
        scenario: "search/multi/vendor-rate-limit-zendesk-reply",
        field: "/html_body",
        also: &["field /body:"],
        dictated: false,
        passes: &RATE_LIMIT_REPLIES,
        fails: &WRONG_RATE_LIMIT_REPLIES,
    }
    .run(vendor_reply)
    .await;
}

// search/tavily/competitor-pricing-research

async fn pricing_research(input: String) -> Grade {
    let trial = ContractTrial::load_with_dataset(
        "search/tavily/competitor-pricing-research",
        &mock_tavily::Tavily,
        &[("authorization", "Bearer tvly-toolbench")],
        |_| {},
    );
    trial
        .call(
            "POST",
            "/extract",
            json!({"urls": ["https://www.ledgerlyhq.com/compare"]}),
        )
        .await;
    let identity = trial
        .call("POST", "/research", json!({"input": input, "model": "pro"}))
        .await["request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    trial
        .call("GET", &format!("/research/{identity}"), Value::Null)
        .await;
    trial.assess()
}

#[tokio::test]
async fn pricing_research_brief() {
    Proof {
        scenario: "search/tavily/competitor-pricing-research",
        field: "/input",
        // The contract also selects the research by each competitor's name, and the service
        // writes the authored report only for a brief that names all three, so a brief that
        // drops one leaves that selection empty and gets a different report.
        also: &[
            "/services/tavily/research selected 0 records",
            "Equals relation between /services/tavily/research/",
        ],
        dictated: false,
        passes: &[
            "Tallybook, Billfold and Sumwise: current pricing plans",
            "Produce a comprehensive report on the current pricing plans of three invoicing software vendors: Tallybook, Billfold, and Sumwise. For each vendor, list every plan, its monthly and annual price, what is included, usage limits, and any free tier or trial. Cite each vendor's own pricing page where possible.",
            "# Research brief\n\nCompare current pricing for:\n- **Tallybook**\n- **Billfold**\n- **Sumwise**\n\nFor each: plan names, price per month, billing options, key limits.",
            "We position Ledgerly (invoicing software) against Tallybook, Billfold and Sumwise on our compare page. Research what each of these three competitors currently charges: plan tiers, prices, and what each tier includes. Only cover these three companies.",
            "How much do Sumwise, Billfold and Tallybook cost today? Give every plan and its cost per month.",
        ],
        fails: &[
            "Current pricing plans of Tallybook and Sumwise.",
            "Compare the current pricing plans of Tallybook, Billfold, Sumwise and Zervant.",
            "Compare the feature sets of Tallybook, Billfold and Sumwise.",
            "Research the current pricing plans of the top invoicing tools: Billfold, InvoiceNinja, Tallybook, Zervant, FreshBooks and Sumwise.",
        ],
    }
    .run(pricing_research)
    .await;
}

// search/firecrawl/monitor-competitor-pricing

async fn pricing_monitor(goal: String) -> Grade {
    let trial = ContractTrial::load_with_dataset(
        "search/firecrawl/monitor-competitor-pricing",
        &mock_firecrawl::Firecrawl,
        &[("authorization", "Bearer local-firecrawl")],
        |_| {},
    );
    trial
        .call(
            "POST",
            "/v2/monitor",
            json!({
                "name": "Fabrikam pricing",
                "schedule": {"text": "daily", "timezone": "UTC"},
                "goal": goal,
                "targets": [{"type": "scrape", "urls": ["https://www.fabrikam.io/pricing"]}],
                "notification": {"email": {
                    "enabled": true,
                    "recipients": ["pricing-intel@contoso.example"],
                    "includeDiffs": true
                }}
            }),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn pricing_monitor_goal() {
    Proof {
        scenario: "search/firecrawl/monitor-competitor-pricing",
        field: "judged_goal",
        also: &[],
        dictated: false,
        passes: &[
            "Prices or plan limits change",
            "Notify the pricing team whenever any plan price or plan limit on Fabrikam's pricing page changes, for example a new monthly price, a changed seat or usage limit, or a plan being added or removed.",
            "Detect changes to:\n- plan prices (monthly or annual)\n- plan limits (seats, usage quotas, included features)",
            "Fabrikam just changed their prices. Email us as soon as the pricing or the limits of any plan change again.",
            "Report every change to the pricing page.",
            "Track pricing changes and changes to plan quotas.",
            "Let us know about any updates to Fabrikam's pricing page.",
        ],
        fails: &[
            "Alert when Fabrikam changes the price of any plan.",
            "Alert when a plan's usage limits change.",
            "Notify when Fabrikam posts new job openings.",
        ],
    }
    .run(pricing_monitor)
    .await;
}
