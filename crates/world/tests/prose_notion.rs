//! Ordinary forms of the text in Notion write tasks get a pass/fail verdict. Every wording is
//! stored as rich-text segments through the Notion REST API and read back through `grade`.

mod support;

use mock_core::grading::Grade;
use serde_json::{Value, json};
use support::{ContractTrial, prose::Proof};

const ESCALATIONS: &str = "f937b556-2452-4808-a340-f4c80ce63d68";
const ALEX: &str = "6a5aa3fe-744c-41b8-ab6d-2e0cc2ef706a";
const CONTACT: &str = "d18edb31-c7d9-4fa2-9759-783c0b2d0b89";

fn notion(slug: &str) -> ContractTrial {
    ContractTrial::load_with_dataset(
        &format!("collab/notion/{slug}"),
        &mock_notion::Notion,
        &[
            ("authorization", "Bearer local-notion-fixture"),
            ("notion-version", "2026-03-11"),
        ],
        |_| {},
    )
}

/// A wording as Notion stores it: `|` separates rich-text segments, and a segment written as
/// `**bold**`, `_italic_` or `` `code` `` carries that annotation instead of the markers.
fn rich_text(wording: &str) -> Value {
    wording
        .split('|')
        .map(|segment| {
            for (marker, annotation) in [("**", "bold"), ("_", "italic"), ("`", "code")] {
                let styled = segment
                    .strip_prefix(marker)
                    .and_then(|rest| rest.strip_suffix(marker))
                    .filter(|content| !content.is_empty());
                if let Some(content) = styled {
                    return json!({
                        "type": "text",
                        "text": {"content": content},
                        "annotations": {annotation: true}
                    });
                }
            }
            json!({"type": "text", "text": {"content": segment}})
        })
        .collect()
}

async fn escalation(summary: &str, customer: &str) -> Grade {
    let trial = notion("log-customer-escalation");
    trial
        .call(
            "POST",
            "/v1/pages",
            json!({
                "parent": {"type": "data_source_id", "data_source_id": ESCALATIONS},
                "properties": {
                    "Summary": {"title": rich_text(summary)},
                    "Customer": {"rich_text": rich_text(customer)},
                    "Severity": {"select": {"name": "Sev 2"}},
                    "State": {"status": {"name": "Open"}},
                    "Owner": {"people": [{"id": ALEX}]},
                    "Reported": {"date": {"start": "2026-10-29"}}
                }
            }),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn escalation_summary() {
    Proof {
        scenario: "collab/notion/log-customer-escalation",
        field: "/properties/Summary/title",
        also: &[],
        dictated: true,
        passes: &[
            "Checkout button unresponsive on Safari",
            "Checkout button |unresponsive on Safari",
            "**Checkout button unresponsive on Safari**",
            "Checkout button unresponsive on |`Safari`",
            "Checkout Button Unresponsive on Safari",
        ],
        fails: &[
            "Checkout button unresponsive on Chrome",
            "Checkout button unresponsive",
            "Checkout page slow on Safari",
            "Checkout button unresponsive on Safari| (could not reproduce)",
        ],
    }
    .run(|summary| async move { escalation(&summary, "Acme Retail").await })
    .await;
}

#[tokio::test]
async fn escalation_customer() {
    Proof {
        scenario: "collab/notion/log-customer-escalation",
        field: "/properties/Customer/rich_text",
        also: &[],
        dictated: true,
        passes: &[
            "Acme Retail",
            "Acme |Retail",
            "**Acme Retail**",
            "ACME Retail",
            "_Acme_| Retail",
        ],
        fails: &[
            "Acme Corp",
            "Acme",
            "Apex Retail",
            "Acme Retail Group",
        ],
    }
    .run(|customer| async move {
        escalation("Checkout button unresponsive on Safari", &customer).await
    })
    .await;
}

async fn runbook_contact(paragraph: String) -> Grade {
    let trial = notion("update-current-runbook");
    trial
        .call(
            "PATCH",
            &format!("/v1/blocks/{CONTACT}"),
            json!({"paragraph": {"rich_text": rich_text(&paragraph)}}),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn runbook_escalation_contact() {
    Proof {
        scenario: "collab/notion/update-current-runbook",
        field: "/paragraph/rich_text",
        also: &[],
        dictated: true,
        passes: &[
            "Escalation contact: Sam Ortiz, +1 555-0142 (Head of Infrastructure)",
            "Escalation contact: |**Sam Ortiz**|, +1 555-0142 (Head of Infrastructure)",
            "Escalation contact: |Sam Ortiz, +1 555-0142| (Head of Infrastructure)",
            "Escalation contact: Sam Ortiz, |`+1 555-0142`| (Head of Infrastructure)",
            "_Escalation contact: Sam Ortiz, +1 555-0142 (Head of Infrastructure)_",
        ],
        fails: &[
            "Escalation contact: Sam Ortiz, +1 555-0100 (Head of Infrastructure)",
            "Escalation contact: Marcus Webb, +1 555-0142 (Head of Infrastructure)",
            "Escalation contact: Sam Ortiz, +1 555-0124 (Head of Infrastructure)",
            "Escalation contact: Sam Ortiz (Head of Infrastructure)",
            "Escalation contact: Marcus Webb, +1 555-0100 or Sam Ortiz, +1 555-0142 (Head of Infrastructure)",
        ],
    }
    .run(runbook_contact)
    .await;
}
