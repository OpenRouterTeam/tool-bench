//! Ordinary wording of the free text in business and cloud write tasks gets a pass/fail verdict.
//! Every wording travels through the mock service into state and through `grade`.

mod support;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_core::{grading::Grade, serve::service_app};
use serde_json::{Value, json};
use support::{ContractTrial, prose::Proof};
use tower::ServiceExt;

/// Calls a tool of a service that serves its MCP tools in-process.
async fn tool(trial: &ContractTrial, service: &str, name: &str, arguments: Value) -> Value {
    let result = trial
        .call_from(
            service,
            "POST",
            "/_mcp/tools/call",
            json!({"name": name, "arguments": arguments}),
        )
        .await;
    // The Zendesk server reports failures as text, not as `isError`.
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(!text.starts_with("Error:"), "{name}: {text}");
    result
}

/// Adds a ticket comment the way the scenarios' solutions do: Markdown through
/// `create_ticket_comment`, which the service renders to `html_body` and reads back as `body`.
/// A reply that is already HTML goes through `update_ticket`, the tool's other way to comment.
async fn zendesk_comment(trial: &ContractTrial, ticket: u64, public: bool, text: &str) {
    if text.starts_with('<') {
        tool(
            trial,
            "zendesk",
            "update_ticket",
            json!({"ticket_id": ticket, "comment": {"html_body": text, "public": public}}),
        )
        .await;
    } else {
        tool(
            trial,
            "zendesk",
            "create_ticket_comment",
            json!({"ticket_id": ticket, "public": public, "comment": text}),
        )
        .await;
    }
}

fn zendesk(scenario: &str) -> ContractTrial {
    ContractTrial::load_with_dataset(scenario, &mock_zendesk::Zendesk, &[], |_| {})
}

/// The shared wordings of a ticket comment plus one reply authored directly as HTML.
fn with_html<'a>(wordings: &[&'a str], html: &'a str) -> Vec<&'a str> {
    wordings.iter().copied().chain([html]).collect()
}

// Zendesk derives a comment's `body` from its rendered `html_body`, so a reply written through
// the tools reads the same in both fields and a wrong reply fails both checks. Each proof names
// the other field in `also`; the `/html_body` proofs add replies that render lists and bold.

// ───────────────────────── business/multi/hubspot-won-to-salesforce-opportunity ─────────────────────────

async fn hubspot_sync_note(body: String) -> Grade {
    let trial = ContractTrial::load_many_with_dataset(
        "business/multi/hubspot-won-to-salesforce-opportunity",
        &[
            (
                &mock_salesforce::Salesforce,
                &[("authorization", "Bearer prose-proof")],
            ),
            (
                &mock_hubspot::Hubspot,
                &[("authorization", "Bearer prose-proof")],
            ),
        ],
        |_| {},
    );
    trial
        .call_from(
            "salesforce",
            "PATCH",
            "/services/data/v60.0/sobjects/Opportunity/0065g00000IDWm1AAH",
            json!({"StageName": "Closed Won", "Amount": 176500, "CloseDate": "2026-10-28"}),
        )
        .await;
    trial
        .call_from(
            "hubspot",
            "POST",
            "/engagements/v1/engagements",
            json!({
                "engagement": {"type": "NOTE", "timestamp": 1793282400000_i64},
                "metadata": {"body": body},
                "associations": {"dealIds": ["9100104"]}
            }),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn hubspot_won_to_salesforce_sync_note() {
    Proof {
        scenario: "business/multi/hubspot-won-to-salesforce-opportunity",
        field: "/properties/hs_note_body",
        also: &[],
        dictated: false,
        passes: &[
            "Synced to Salesforce. Opportunity ID: 0065g00000IDWm1AAH",
            "This deal has been synced to Salesforce. The matching opportunity, Initech - Data Warehouse Migration (ID 0065g00000IDWm1AAH), is now Closed Won with the final amount of $176,500 and a close date of 2026-10-28.",
            "<p><strong>Salesforce sync complete</strong></p><ul><li>Opportunity: <a href=\"https://octo-corp.my.salesforce.com/0065g00000IDWm1AAH\">Initech - Data Warehouse Migration</a></li><li>Opportunity ID: 0065g00000IDWm1AAH</li><li>Stage: Closed Won</li><li>Amount: $176,500</li><li>Close date: October 28, 2026</li></ul>",
            "Hi team, this deal is now synchronized with Salesforce so finance can report on it. You can find it there as opportunity 0065g00000IDWm1 (Closed Won, $176,500.00, close date 10/28/2026). Thanks!",
            "Salesforce opportunity 0065g00000IDWm1AAH updated to Closed Won to match this deal (amount 176,500; close date 2026-10-28).",
        ],
        fails: &[
            "This deal has been synced to Salesforce; the matching opportunity is now Closed Won at $176,500 with a close date of 2026-10-28.",
            "Synced to Salesforce opportunity 0065g00000IRdw1AAD (Closed Won, $176,500, closed 2026-10-28).",
            "Synced to Salesforce. Opportunity ID: 0065g00000IDWp0AAH",
            "0065g00000IDWm1AAH",
        ],
    }
    .run(hubspot_sync_note)
    .await;
}

// ───────────────────────── business/multi/zendesk-refund-via-stripe ─────────────────────────

/// `{REFUND}` in a wording stands for the id of the refund this trial creates in Stripe.
async fn refund_via_stripe(reply: String) -> Grade {
    let trial = ContractTrial::load_many_with_dataset(
        "business/multi/zendesk-refund-via-stripe",
        &[(&mock_zendesk::Zendesk, &[]), (&mock_stripe::Stripe, &[])],
        |_| {},
    );
    let created = tool(
        &trial,
        "stripe",
        "stripe_api_write",
        json!({
            "stripe_api_operation_id": "PostRefunds",
            "parameters": {"charge": "ch_r1lxhPeTsZKQxIfuhPANytDa", "reason": "duplicate"},
            "stripe_context": "acct_1FwY8bQk2LmN4pRs",
            "livemode": false
        }),
    )
    .await;
    let refund: Value =
        serde_json::from_str(created["content"][0]["text"].as_str().unwrap()).unwrap();
    let identity = refund["id"].as_str().unwrap();
    assert!(identity.starts_with("re_"), "{refund}");
    zendesk_comment(&trial, 5102, true, &reply.replace("{REFUND}", identity)).await;
    tool(
        &trial,
        "zendesk",
        "update_ticket",
        json!({"ticket_id": 5102, "status": "solved"}),
    )
    .await;
    trial.assess()
}

const STRIPE_REFUND_REPLIES: &[&str] = &[
    "Refunded. Stripe refund ID: {REFUND}",
    "Hi Hannah,\n\nThanks for flagging this. I checked the payments for wholesale order #1077 and you were indeed charged twice on March 10. I've refunded the duplicate $480.00 charge in full; your original payment for the order stays in place.\n\nYour Stripe refund ID is {REFUND} in case you need to reference it.\n\nBest regards,\nSupport",
    "Hi Hannah,\n\nGood news: the duplicate charge has been refunded.\n\n- **Order:** #1077\n- **Amount refunded:** $480.00\n- **Stripe refund ID:** {REFUND}\n\nThe original charge for the order is unchanged.",
    "Hello Hannah \u{2014} we've issued a full refund of $480.00 for the second (duplicate) charge on your Visa ending 1881 for order #1077. Reference: `{REFUND}`. Sorry for the trouble!",
    "Hannah, the duplicate payment for order #1077 has been reversed and the $480.00 is on its way back to your card. For your records, the Stripe reference is {REFUND}.",
];

#[tokio::test]
async fn zendesk_refund_via_stripe_reply_body() {
    Proof {
        scenario: "business/multi/zendesk-refund-via-stripe",
        field: "/body",
        also: &["field /html_body:"],
        dictated: false,
        passes: STRIPE_REFUND_REPLIES,
        fails: &[
            "Hi Hannah,\n\nThe duplicate $480.00 charge for order #1077 has been refunded in full. The original charge remains in place.",
            "Hi Hannah,\n\nWe refunded the duplicate $480.00 charge for order #1077. Your Stripe refund ID is re_3MtwBwLkdIwHu7ix0k9qZx1Q.",
            "Hi Hannah, the duplicate charge ch_r1lxhPeTsZKQxIfuhPANytDa for order #1077 has been refunded in full.",
            "{REFUND}",
        ],
    }
    .run(refund_via_stripe)
    .await;
}

#[tokio::test]
async fn zendesk_refund_via_stripe_reply_html_body() {
    Proof {
        scenario: "business/multi/zendesk-refund-via-stripe",
        field: "/html_body",
        also: &["field /body:"],
        dictated: false,
        passes: &with_html(
            STRIPE_REFUND_REPLIES,
            "<p>Hi Hannah,</p><p>The duplicate charge for order #1077 has been <strong>refunded in full</strong>.</p><ul><li>Amount: $480.00</li><li>Stripe refund ID: {REFUND}</li></ul><p>The original charge for the order is unchanged.</p>",
        ),
        fails: &[
            "Hi Hannah,\n\nGood news: the duplicate charge has been refunded.\n\n- **Order:** #1077\n- **Amount refunded:** $480.00\n\nThe original charge for the order is unchanged.",
            "<p>Hi Hannah,</p><ul><li>Refunded: $480.00 (duplicate charge, order #1077)</li><li>Stripe refund ID: <strong>re_3MtwBwLkdIwHu7ix0k9qZx1Q</strong></li></ul>",
        ],
    }
    .run(refund_via_stripe)
    .await;
}

// ───────────────────────── business/salesforce/log-call-on-contact ─────────────────────────

async fn log_call(note: String) -> Grade {
    let trial = ContractTrial::load_with_dataset(
        "business/salesforce/log-call-on-contact",
        &mock_salesforce::Salesforce,
        &[("authorization", "Bearer prose-proof")],
        |_| {},
    );
    let (status, created) = trial
        .request(
            "POST",
            "/services/data/v60.0/sobjects/Task",
            json!({
                "WhoId": "0035g00000p3z7WAAQ", "WhatId": "0015g000008HFvFAAW",
                "Subject": "Renewal pricing call", "Status": "Completed",
                "ActivityDate": "2026-05-12", "TaskSubtype": "Call", "Type": "Call",
                "Description": note,
            }),
        )
        .await;
    assert!(
        status.is_success() && created["success"] == true,
        "{created}"
    );
    trial.assess()
}

#[tokio::test]
async fn salesforce_log_call_note() {
    Proof {
        scenario: "business/salesforce/log-call-on-contact",
        field: "/Description",
        also: &[],
        dictated: false,
        passes: &[
            "Wants a 3-year quote by May 22.",
            "Spoke with Dana Whitfield (Northgate Analytics) about renewal pricing. She would like a three-year quote, and she needs it by May 22, 2026.",
            "Call summary:\n- Topic: renewal pricing\n- Dana asked for a 3-year quote\n- Due: 5/22/2026",
            "Phone call with Dana re: renewal pricing. Action item: send her a 3 year quote no later than Friday, May 22nd.",
            "Renewal pricing discussion with Dana Whitfield. Next step: prepare a 3-yr renewal quote for Northgate Analytics; she wants it by 22 May 2026.",
            "Dana is asking for a quote covering three years, to be delivered by 2026-05-22.",
        ],
        fails: &[
            "Discussed renewal pricing with Dana. She wants a quote by May 22.",
            "Discussed renewal pricing; Dana wants a 3-year quote.",
            "Dana wants a 3-year quote by May 29.",
            "Dana wants a 2-year quote by May 22.",
        ],
    }
    .run(log_call)
    .await;
}

// ───────────────────────── business/zendesk/create-ticket-for-caller ─────────────────────────

async fn create_ticket_for_caller(description: String) -> Grade {
    let trial = zendesk("business/zendesk/create-ticket-for-caller");
    tool(
        &trial,
        "zendesk",
        "create_ticket",
        json!({
            "subject": "Barcode scanners not syncing after update",
            "requester_id": 9010301,
            "priority": "high",
            "type": "incident",
            "description": description,
        }),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn zendesk_create_ticket_description() {
    Proof {
        scenario: "business/zendesk/create-ticket-for-caller",
        field: "/description",
        also: &[],
        dictated: false,
        passes: &[
            "Since this morning's app update, the barcode scanners in the Bergen warehouse have stopped syncing inventory counts.",
            "Elena Petrova from Fjord Outfitters called our support line to report that, following this morning's app update, the barcode scanners at their Bergen warehouse are no longer syncing inventory counts. Ticket opened on her behalf.",
            "**Reported by phone:** Elena Petrova (Fjord Outfitters)\n\n**Issue:** Barcode scanners are not syncing inventory counts\n**Location:** Bergen warehouse\n**Started:** this morning, right after the app update",
            "Customer called in. After the app was updated this morning, the barcode scanners in Fjord Outfitters' Bergen warehouse stopped synchronizing inventory counts.",
            "Phone report from Elena Petrova:\n- Site: Bergen warehouse\n- Problem: barcode scanners stopped syncing inventory counts\n- Since: this morning's app update\n\nFiled on the caller's behalf.",
        ],
        fails: &[
            "Since this morning's app update, the barcode scanners in their warehouse have stopped syncing inventory counts.",
            "Since this morning's app update, the barcode scanners in the Stavanger warehouse have stopped syncing inventory counts.",
            "Elena Petrova called about a problem at the Bergen warehouse since this morning's app update.",
            "Bergen",
        ],
    }
    .run(create_ticket_for_caller)
    .await;
}

// ───────────────────────── business/zendesk/escalate-outage-ticket ─────────────────────────

async fn escalate_outage(note: String) -> Grade {
    let trial = zendesk("business/zendesk/escalate-outage-ticket");
    tool(
        &trial,
        "zendesk",
        "update_ticket",
        json!({"ticket_id": 6230, "priority": "urgent", "tags": ["api", "patient_portal", "escalated"]}),
    )
    .await;
    zendesk_comment(&trial, 6230, false, &note).await;
    trial.assess()
}

const ESCALATION_NOTES: &[&str] = &[
    "Escalated to on-call engineering under incident INC-4471.",
    "This ticket has been escalated to on-call engineering under incident INC-4471. Northwind Health's patient portal API calls are failing with 502 errors, and Dr. Osei reports that it is blocking patient check-ins.",
    "**Escalation**\n\n- Escalated to: on-call engineering\n- Incident: **INC-4471**\n- Impact: patient portal API returning 502s, front desks can't check patients in",
    "Internal note: escalating to the on-call engineering team; tracked as incident INC-4471. Priority raised to urgent and tagged \"escalated\".",
    "Heads up team, this one has been escalated to the engineer on call (incident INC-4471). The outage started around 12:30 UTC per Dr. Osei. Thanks!",
];

#[tokio::test]
async fn zendesk_escalation_note_body() {
    Proof {
        scenario: "business/zendesk/escalate-outage-ticket",
        field: "/body",
        also: &["field /html_body:"],
        dictated: false,
        passes: ESCALATION_NOTES,
        fails: &[
            "Escalated to on-call engineering. The 502s are blocking patient check-ins at Northwind Health.",
            "Escalated to on-call engineering under incident INC-4417.",
            "Escalated to on-call engineering under incident INC-44710.",
            "INC-4471",
        ],
    }
    .run(escalate_outage)
    .await;
}

#[tokio::test]
async fn zendesk_escalation_note_html_body() {
    Proof {
        scenario: "business/zendesk/escalate-outage-ticket",
        field: "/html_body",
        also: &["field /body:"],
        dictated: false,
        passes: &with_html(
            ESCALATION_NOTES,
            "<p><strong>Escalated</strong> to on-call engineering under incident INC-4471.</p><ul><li>Symptom: patient portal API calls return 502</li><li>Impact: front desks cannot check patients in</li></ul>",
        ),
        fails: &[
            "**Escalated** to on-call engineering.\n\n- Symptom: patient portal API calls return 502\n- Impact: front desks cannot check patients in",
            "<p><strong>Escalated</strong> to on-call engineering under incident <strong>INC-4417</strong>.</p><ul><li>Symptom: patient portal API calls return 502</li></ul>",
        ],
    }
    .run(escalate_outage)
    .await;
}

// ───────────────────────── business/zendesk/reply-refund-and-solve ─────────────────────────

async fn reply_refund(reply: String) -> Grade {
    let trial = zendesk("business/zendesk/reply-refund-and-solve");
    zendesk_comment(&trial, 4217, true, &reply).await;
    tool(
        &trial,
        "zendesk",
        "update_ticket",
        json!({"ticket_id": 4217, "status": "solved"}),
    )
    .await;
    trial.assess()
}

const REFUND_REPLIES: &[&str] = &[
    "Your refund has been issued. Reference: RF-88213. It should show up on your card within 5\u{2013}7 business days.",
    "Hi Marcus,\n\nBilling has confirmed that the refund for your duplicate October charge has been issued. Your refund reference is RF-88213, and the money should show up on your card within 5\u{2013}7 business days.\n\nThanks for your patience, and sorry again for the mix-up.\n\nBest regards,\nJordan",
    "Hi Marcus,\n\nGood news on the duplicate October charge:\n\n- **Status:** refund issued\n- **Refund reference:** RF-88213\n- **Expected on your card:** within 5 to 7 business days\n\nLet us know if it hasn't arrived by then.",
    "Hello Marcus, the duplicate charge on invoice INV-55120 ($1,240.00) has now been refunded by our billing team (refund reference **RF-88213**). Please allow five to seven business days for it to appear on your card statement.",
    "Marcus \u{2014} refund issued for the duplicate October charge. Ref RF-88213; allow 5-7 business days for it to reach your card.",
];

#[tokio::test]
async fn zendesk_refund_reply_body() {
    Proof {
        scenario: "business/zendesk/reply-refund-and-solve",
        field: "/body",
        also: &["field /html_body:"],
        dictated: false,
        passes: REFUND_REPLIES,
        fails: &[
            "Hi Marcus,\n\nBilling has confirmed that the refund for your duplicate October charge has been issued. Your refund reference is RF-88213.\n\nBest regards,\nJordan",
            "Hi Marcus,\n\nBilling has issued the refund for your duplicate October charge. It should show up on your card within 5\u{2013}7 business days.",
            "Hi Marcus, your refund has been issued. The reference is RF-87002 and it should show up on your card within 5\u{2013}7 business days.",
            "Hi Marcus, your refund (reference RF-88213) has been issued and should show up on your card within 7\u{2013}10 business days.",
        ],
    }
    .run(reply_refund)
    .await;
}

#[tokio::test]
async fn zendesk_refund_reply_html_body() {
    Proof {
        scenario: "business/zendesk/reply-refund-and-solve",
        field: "/html_body",
        also: &["field /body:"],
        dictated: false,
        passes: &with_html(
            REFUND_REPLIES,
            "<h3>Refund reference</h3><p>RF-88213</p><h3>When to expect it</h3><ul><li>On your card within 5\u{2013}7 business days</li></ul><p>Sorry again for the trouble, Marcus.</p>",
        ),
        fails: &[
            "Hi Marcus,\n\nBilling has issued the refund for your duplicate October charge.\n\n- **Refund reference:** RF-88213\n- **Expected on your card:** in about a week",
            "<p>Hi Marcus,</p><ul><li>Refund reference: <strong>RF-87002</strong></li><li>Expected on your card: within 5\u{2013}7 business days</li></ul>",
        ],
    }
    .run(reply_refund)
    .await;
}

// ───────────────────────── AWS requests of the cloud scenarios ─────────────────────────

async fn aws(app: &Router, request: axum::http::request::Builder, body: String) -> String {
    let response = app
        .clone()
        .oneshot(
            request
                .method("POST")
                .uri("/")
                .header("host", "world:8116")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(!text.contains("<Error>"), "{text}");
    text
}

async fn ec2(app: &Router, action: &str, fields: &[(&str, &str)]) {
    let body = std::iter::once(&("Action", action))
        .chain(fields)
        .map(|(name, value)| {
            format!(
                "{}={}",
                mock_aws::util::percent_encode(name),
                mock_aws::util::percent_encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    let request = Request::builder()
        .header(
            "authorization",
            "AWS4-HMAC-SHA256 Credential=AKIATEST/20261029/us-east-1/ec2/aws4_request, SignedHeaders=host;x-amz-date, Signature=00",
        )
        .header(
            "content-type",
            "application/x-www-form-urlencoded; charset=utf-8",
        );
    aws(app, request, body).await;
}

async fn cloudwatch(app: &Router, operation: &str, input: Value) {
    let request = Request::builder()
        .header(
            "authorization",
            "AWS4-HMAC-SHA256 Credential=AKIATEST/20261030/us-east-1/monitoring/aws4_request, SignedHeaders=host;x-amz-date, Signature=00",
        )
        .header("content-type", "application/x-amz-json-1.0")
        .header(
            "x-amz-target",
            format!("GraniteServiceVersion20100801.{operation}"),
        );
    aws(app, request, input.to_string()).await;
}

// ───────────────────────── cloud/multi/aws-jira-decommission-worker ─────────────────────────

async fn decommission_worker(comment: String) -> Grade {
    const WORKER: &str = "i-07b9d1f3a5c8e2046";
    let trial = ContractTrial::load_with_dataset(
        "cloud/multi/aws-jira-decommission-worker",
        &mock_jira::Jira,
        &[("authorization", "Bearer prose-proof")],
        |_| {},
    );
    let aws = service_app(trial.world.clone(), &mock_aws::Aws);
    ec2(&aws, "StopInstances", &[("InstanceId.1", WORKER)]).await;
    ec2(
        &aws,
        "CreateTags",
        &[
            ("ResourceId.1", WORKER),
            ("Tag.1.Key", "DecommissionTicket"),
            ("Tag.1.Value", "OPS-34"),
        ],
    )
    .await;
    trial
        .call(
            "POST",
            "/rest/api/2/issue/OPS-34/comment",
            json!({"body": comment}),
        )
        .await;
    trial
        .call(
            "POST",
            "/rest/api/2/issue/OPS-34/transitions",
            json!({"transition": {"id": "31"}, "fields": {"resolution": {"name": "Done"}}}),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn jira_decommission_comment() {
    Proof {
        scenario: "cloud/multi/aws-jira-decommission-worker",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "Stopped i-07b9d1f3a5c8e2046 (not terminated) and tagged it DecommissionTicket=OPS-34.",
            "I stopped the production EC2 instance reporting-worker-legacy (i-07b9d1f3a5c8e2046) in us-east-1. It was stopped, not terminated, so its EBS volume is retained for the 30-day window. I also added the tag DecommissionTicket=OPS-34 to the instance. The staging copy with the same name (i-0e4a6c8b2d1f35790) was left running for QA.",
            "## Decommission summary\n\n- **Stopped** the prod instance `reporting-worker-legacy` (`i-07b9d1f3a5c8e2046`) \u{2014} it was *not* terminated\n- **Tagged** it with `DecommissionTicket` = `OPS-34`\n- Root volume `vol-07c9e1a3b5d7f2046` is still attached and kept\n- Staging instance `i-0e4a6c8b2d1f35790` untouched (still running)",
            "h3. Done\n* Stopped production instance *reporting-worker-legacy* ({{i-07b9d1f3a5c8e2046}}); its state is now {{stopped}} (not terminated).\n* Added tag {{DecommissionTicket=OPS-34}}.\n* Staging copy left running.",
            "Hi Marcus, this is done: the prod reporting-worker-legacy instance is now stopped (I did not terminate it, so the disk is kept for the 30 days you asked for) and it carries the tag DecommissionTicket: OPS-34. I left the staging copy alone. Closing this out.",
            "Ran `aws ec2 stop-instances --instance-ids i-07b9d1f3a5c8e2046` and `aws ec2 create-tags --resources i-07b9d1f3a5c8e2046 --tags Key=DecommissionTicket,Value=OPS-34`. The instance is in the stopped state; nothing was terminated.",
        ],
        fails: &[
            "Stopped the production instance reporting-worker-legacy (i-07b9d1f3a5c8e2046); it was not terminated and its EBS volume is kept. The staging copy is still running.",
            "Tagged the production instance reporting-worker-legacy (i-07b9d1f3a5c8e2046) with DecommissionTicket=OPS-34. The staging copy is untouched.",
            "Terminated the prod reporting-worker-legacy instance i-07b9d1f3a5c8e2046 and tagged it DecommissionTicket=OPS-34.",
            "Done as requested.",
        ],
    }
    .run(decommission_worker)
    .await;
}

// ───────────────────────── cloud/multi/aws-replica-lag-pagerduty-notion ─────────────────────────

const ALARM: &str = "orders-db-replica-2-replica-lag";

/// The four free texts of the replica-lag paperwork; each proof varies one of them.
struct Paperwork {
    note: String,
    title: String,
    incident: String,
    alarm: String,
}

impl Paperwork {
    fn reference() -> Self {
        Self {
            note: "CloudWatch alarm orders-db-replica-2-replica-lag went into ALARM at 2026-10-30T13:11:00Z (ReplicaLag max 412s vs 300s threshold).".into(),
            title: "orders-db-replica-2 replication lag".into(),
            incident: "PagerDuty #4812".into(),
            alarm: ALARM.into(),
        }
    }
}

/// Notion rich text for a property value written with Markdown emphasis: `**bold**`, backticked
/// code and `[label](https://...)` become annotated or linked segments, the way an assistant
/// sends formatted text to the Notion API.
fn notion_rich_text(markdown: &str) -> Value {
    let segment = |content: &str, annotation: Option<&str>, link: Option<&str>| {
        let mut segment = json!({"type": "text", "text": {"content": content}});
        if let Some(name) = annotation {
            segment["annotations"] = json!({name: true});
        }
        if let Some(url) = link {
            segment["text"]["link"] = json!({"url": url});
        }
        segment
    };
    let mut segments = Vec::new();
    let mut plain = String::new();
    let mut rest = markdown;
    while let Some(character) = rest.chars().next() {
        let styled = match character {
            '*' => rest
                .strip_prefix("**")
                .and_then(|tail| tail.split_once("**"))
                .map(|(inner, tail)| (segment(inner, Some("bold"), None), tail)),
            '`' => rest[1..]
                .split_once('`')
                .map(|(inner, tail)| (segment(inner, Some("code"), None), tail)),
            '[' => rest[1..].split_once("](").and_then(|(label, tail)| {
                tail.split_once(')')
                    .map(|(url, tail)| (segment(label, None, Some(url)), tail))
            }),
            _ => None,
        };
        match styled {
            Some((styled, tail)) => {
                if !plain.is_empty() {
                    segments.push(segment(&plain, None, None));
                    plain.clear();
                }
                segments.push(styled);
                rest = tail;
            }
            None => {
                plain.push(character);
                rest = &rest[character.len_utf8()..];
            }
        }
    }
    if !plain.is_empty() {
        segments.push(segment(&plain, None, None));
    }
    Value::Array(segments)
}

async fn replica_paperwork(paperwork: Paperwork) -> Grade {
    let trial = ContractTrial::load_many_with_dataset(
        "cloud/multi/aws-replica-lag-pagerduty-notion",
        &[
            (
                &mock_pagerduty::Pagerduty,
                &[
                    ("authorization", "Token token=prose-proof"),
                    ("from", "lena.weber@tidewater.io"),
                ],
            ),
            (
                &mock_notion::Notion,
                &[
                    ("authorization", "Bearer prose-proof"),
                    ("notion-version", "2026-03-11"),
                ],
            ),
        ],
        |_| {},
    );
    let aws = service_app(trial.world.clone(), &mock_aws::Aws);
    cloudwatch(&aws, "DescribeAlarms", json!({"StateValue": "ALARM"})).await;
    cloudwatch(
        &aws,
        "DescribeAlarmHistory",
        json!({"AlarmName": ALARM, "HistoryItemType": "StateUpdate"}),
    )
    .await;
    trial
        .call_from(
            "pagerduty",
            "POST",
            "/incidents/Q3K8ORDBLAG2RX/notes",
            json!({"note": {"content": paperwork.note}}),
        )
        .await;
    trial
        .call_from(
            "notion",
            "POST",
            "/v1/pages",
            json!({
                "parent": {"type": "data_source_id", "data_source_id": "7d2e3f4a-5b6c-4d7e-9f80-a1b2c3d4e5f6"},
                "properties": {
                    "Title": {"title": notion_rich_text(&paperwork.title)},
                    "Incident": {"rich_text": notion_rich_text(&paperwork.incident)},
                    "Severity": {"select": {"name": "SEV2"}},
                    "Status": {"status": {"name": "Draft"}},
                    "Started": {"date": {"start": "2026-10-30"}},
                    "Alarm": {"rich_text": notion_rich_text(&paperwork.alarm)}
                }
            }),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn pagerduty_replica_lag_alarm_note() {
    Proof {
        scenario: "cloud/multi/aws-replica-lag-pagerduty-notion",
        field: "/content",
        also: &[],
        dictated: false,
        passes: &[
            "orders-db-replica-2-replica-lag \u{2014} ALARM since 2026-10-30T13:11:00Z",
            "The CloudWatch alarm that fired for this incident is orders-db-replica-2-replica-lag. It went into ALARM on October 30, 2026 at 13:11 UTC.",
            "CloudWatch alarm details:\n- Alarm name: orders-db-replica-2-replica-lag\n- Entered ALARM: 2026-10-30 13:11:00 UTC\n- Metric: AWS/RDS ReplicaLag (Maximum) above 300s for 3 datapoints",
            "Taking this one. Alarm `orders-db-replica-2-replica-lag` transitioned OK -> ALARM at 13:11 UTC on Oct 30 (ReplicaLag peaked at 412s against a 300s threshold).",
            "Alarm: orders-db-replica-2-replica-lag. State changed to ALARM at 1:11 PM UTC, 30 Oct 2026.",
        ],
        fails: &[
            "CloudWatch alarm orders-db-replica-2-replica-lag is in ALARM (ReplicaLag above the 300s threshold).",
            "The replica lag alarm for orders-db-replica-2 went into ALARM at 2026-10-30T13:11:00Z.",
            "CloudWatch alarm orders-db-replica-2-replica-lag went into ALARM at 2026-10-30T13:19:00Z.",
            "CloudWatch alarm orders-db-staging-replica-lag went into ALARM at 2026-10-30T13:11:00Z.",
            "Alarm orders-db-replica-2-replica-lag went into ALARM at 13:11 UTC on 2026-10-23.",
        ],
    }
    .run(|note| {
        replica_paperwork(Paperwork {
            note,
            ..Paperwork::reference()
        })
    })
    .await;
}

#[tokio::test]
async fn notion_postmortem_title() {
    Proof {
        scenario: "cloud/multi/aws-replica-lag-pagerduty-notion",
        field: "/properties/Title/title",
        also: &[],
        dictated: true,
        passes: &[
            "orders-db-replica-2 replication lag",
            "**orders-db-replica-2 replication lag**",
            "`orders-db-replica-2` replication lag",
            "Orders-db-replica-2 replication lag",
            "[orders-db-replica-2](https://console.aws.amazon.com/rds/home?region=us-east-1#database:id=orders-db-replica-2) replication lag",
        ],
        fails: &[
            "orders-db-replica-2 replication lag (draft)",
            "Postmortem: orders-db-replica-2 replication lag",
            "orders-db-replica-3 replication lag",
            "orders-db-replica-2-replica-lag",
        ],
    }
    .run(|title| {
        replica_paperwork(Paperwork {
            title,
            ..Paperwork::reference()
        })
    })
    .await;
}

#[tokio::test]
async fn notion_postmortem_incident() {
    Proof {
        scenario: "cloud/multi/aws-replica-lag-pagerduty-notion",
        field: "/properties/Incident/rich_text",
        also: &[],
        dictated: true,
        passes: &[
            "PagerDuty #4812",
            "**PagerDuty #4812**",
            "PagerDuty [#4812](https://tidewater.pagerduty.com/incidents/Q3K8ORDBLAG2RX)",
            "`PagerDuty #4812`",
            "pagerduty #4812",
        ],
        fails: &[
            "4812",
            "PagerDuty #4821",
            "PagerDuty #4812 and #4813",
            "PagerDuty incident Q3K8ORDBLAG2RX",
        ],
    }
    .run(|incident| {
        replica_paperwork(Paperwork {
            incident,
            ..Paperwork::reference()
        })
    })
    .await;
}

#[tokio::test]
async fn notion_postmortem_alarm() {
    Proof {
        scenario: "cloud/multi/aws-replica-lag-pagerduty-notion",
        field: "/properties/Alarm/rich_text",
        also: &[],
        dictated: false,
        passes: &[
            "`orders-db-replica-2-replica-lag`",
            "CloudWatch alarm orders-db-replica-2-replica-lag",
            "orders-db-replica-2-replica-lag (CloudWatch, us-east-1)",
            "**orders-db-replica-2-replica-lag**",
            "[orders-db-replica-2-replica-lag](https://console.aws.amazon.com/cloudwatch/home?region=us-east-1#alarmsV2:alarm/orders-db-replica-2-replica-lag)",
        ],
        fails: &[
            "orders-db-replica-2",
            "orders-db-replica-2 replica lag",
            "orders-db-replica-2-replica-lag-old",
            "orders-db-replica-3-replica-lag",
        ],
    }
    .run(|alarm| {
        replica_paperwork(Paperwork {
            alarm,
            ..Paperwork::reference()
        })
    })
    .await;
}

// ───────────────────────── cloud/multi/okta-offboard-linear-slack ─────────────────────────

async fn offboard(notice: String) -> Grade {
    const LINEAR_USER: &str = "3dbdd473-1a04-5dda-ad95-4c583313bccd";
    let trial = ContractTrial::load_many_with_dataset(
        "cloud/multi/okta-offboard-linear-slack",
        &[
            (&mock_okta::Okta, &[("authorization", "Bearer prose-proof")]),
            (&mock_linear::Linear, &[]),
            (&mock_slack::Slack, &[]),
        ],
        |_| {},
    );
    trial
        .call(
            "POST",
            "/api/v1/users/00u1lnfschr3Ab4Cd5x7/lifecycle/deactivate",
            Value::Null,
        )
        .await;
    for team in [
        "5854633b-d1e9-52b6-b3c7-d309bfb881cb",
        "6da75b4c-0e7c-5ac7-a415-f09a129582b1",
    ] {
        tool(
            &trial,
            "linear",
            "linear_removeUserFromTeam",
            json!({"teamId": team, "userId": LINEAR_USER}),
        )
        .await;
    }
    tool(
        &trial,
        "slack",
        "slack_send_message",
        json!({"channel_id": "C0ITOPS5LF2", "message": notice}),
    )
    .await;
    trial.assess()
}

#[tokio::test]
async fn slack_offboarding_notice() {
    Proof {
        scenario: "cloud/multi/okta-offboard-linear-slack",
        field: "/text",
        also: &[],
        dictated: false,
        passes: &[
            "Lena Fischer offboarded \u{2014} Okta deactivated, removed from Linear.",
            "Heads up: Lena Fischer (Product) has been offboarded. Her Okta account has been deactivated and she has been removed from all of her Linear teams.",
            ":white_check_mark: *Offboarding complete: Lena Fischer (Product)*\n\u{2022} Okta: account deactivated (not deleted, kept for the audit trail)\n\u{2022} Linear: removed from Payments and Web App",
            "Hi all, quick notice that Lena Fischer (Product), who left the company yesterday, has now been offboarded. Her Okta account is deactivated (kept, not deleted) and she's been removed from her Linear teams (Payments and Web App). Thanks!",
            "FYI: offboarding for Lena Fischer is done. Okta: deactivated. Linear: removed from all teams (Payments, Web App). Nobody else's access was changed.",
            "Lena Fischer (Product) has been offboarded: her Okta account is deactivated and she's been removed from her Linear teams. Note this is not Lena Fisher in Sales, whose access is unchanged.",
        ],
        fails: &[
            "Lena Fischer (Product) has been offboarded and removed from Linear.",
            "Lena Fischer has been offboarded. Her Okta account has been deactivated.",
            "Lena Fisher (Sales) has been offboarded: Okta deactivated, removed from Linear.",
            "Lena Fischer has been offboarded: Okta account deleted, removed from Linear.",
            "Offboarding is complete.",
        ],
    }
    .run(offboard)
    .await;
}
