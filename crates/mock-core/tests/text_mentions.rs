//! `mentions` and `forbids` give wording outside the reviewed examples a pass/fail verdict.

use std::path::Path;

use mock_core::{
    dataset::Dataset,
    events::{self, OperationKind},
    grading::{Expect, Grade, GradeInput, Outcome, grade},
    relations::{RecordCheck, RecordCheckError},
    store::Store,
    world::{SharedWorld, WorldState, shared},
};
use serde_json::{Value, json};

fn text_check(reviewed: Value) -> RecordCheck {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector": {"collection": "/services/desk/comments", "created": true},
        "equals": 1,
        "reviewed_text": {"/body": reviewed}
    }))
    .unwrap();
    check.validate().unwrap();
    check
}

fn refund_reply() -> Value {
    json!({
        "requirement": "Tell Marcus the refund RF-88213 was issued and arrives within 5-7 business days.",
        "accepted": ["Refund RF-88213 issued; expect it within 5-7 business days."],
        "rejected": ["Refund RF-88213 was declined; nothing arrives within 5-7 business days."],
        "mentions": [
            "RF-88213",
            ["5-7 business days", "5 to 7 business days", "five to seven business days"]
        ],
        "forbids": ["RF-88214"]
    })
}

fn initial() -> Value {
    json!({"services": {}})
}

fn state(body: Value) -> Value {
    json!({"services": {"desk": {"comments": {"actual": {"body": body}}}}})
}

fn verdict(check: &RecordCheck, body: &str) -> Result<(), RecordCheckError> {
    check.check(Some(&initial()), &state(json!(body)))
}

fn failure(check: &RecordCheck, body: &str) -> String {
    match verdict(check, body) {
        Err(RecordCheckError::Failed(reason)) => reason,
        other => panic!("{body}: expected a proven failure, got {other:?}"),
    }
}

#[test]
fn ordinary_wording_that_carries_every_mention_passes() {
    let check = text_check(refund_reply());
    for body in [
        "RF-88213, 5-7 business days.",
        "Hi Marcus,\n\nBilling has issued your refund. The reference is RF-88213 and it should reach your card within 5\u{2013}7 business days.\n\nBest,\nJordan",
        "## Refund issued\n\n- **Reference:** `RF-88213`\n- **Arrival:** five to seven business days",
        "Your refund (rf-88213) is on its way\u{2014}allow 5 to 7 business days.",
    ] {
        verdict(&check, body).unwrap_or_else(|error| panic!("{body}: {error}"));
    }
}

#[test]
fn a_missing_or_wrong_fact_fails_and_the_reason_names_it() {
    let check = text_check(refund_reply());
    let reason = failure(
        &check,
        "Hi Marcus, your refund was issued. Reference RF-88213.",
    );
    assert!(reason.contains("5-7 business days"), "{reason}");
    assert!(!reason.contains("RF-88213"), "{reason}");
    let reason = failure(
        &check,
        "Refund RF-88231 issued; it arrives within 5-7 business days.",
    );
    assert!(reason.contains("\"RF-88213\""), "{reason}");
    for body in [
        "Refund RF-882130 issued; 5-7 business days.",
        "Refund XRF-88213 issued; 5-7 business days.",
        "Refund RF-88213-B issued; 5-7 business days.",
        "Refund RF-88213 issued; 15-7 business days.",
        "Refund issued.",
    ] {
        failure(&check, body);
    }
}

#[test]
fn examples_pass_first_and_known_incorrect_text_fails_even_with_every_mention() {
    let check = text_check(refund_reply());
    verdict(
        &check,
        "Refund RF-88213 issued; expect it within 5-7 business days.",
    )
    .unwrap();
    failure(
        &check,
        "Refund RF-88213 was declined; nothing arrives within 5-7 business days.",
    );
    let reason = failure(
        &check,
        "Refund RF-88213 (not RF-88214) issued; 5-7 business days.",
    );
    assert!(reason.contains("RF-88214"), "{reason}");
}

#[test]
fn missing_empty_and_nontext_values_keep_their_verdicts() {
    let check = text_check(refund_reply());
    for body in [Value::Null, json!(""), json!(" \n"), json!(7)] {
        assert!(matches!(
            check.check(Some(&initial()), &state(body)),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn optional_narration_passes_unless_it_is_known_incorrect() {
    let mut reviewed = json!({
        "requirement": "Optional approval narration.",
        "optional": true,
        "accepted": ["LGTM."],
        "rejected": ["Rejecting this."],
        "forbids": ["do not merge"]
    });
    let check = text_check(reviewed.clone());
    for body in ["", "Looks good, CI is green.", "Approving the lodash bump."] {
        verdict(&check, body).unwrap();
    }
    failure(&check, "Rejecting this.");
    failure(&check, "Approved, but DO NOT MERGE yet.");
    reviewed["mentions"] = json!(["lodash"]);
    reviewed["accepted"] = json!(["LGTM, lodash bump."]);
    let check = text_check(reviewed);
    verdict(&check, "").unwrap();
    verdict(&check, "Approving the lodash bump.").unwrap();
    failure(&check, "Looks good, CI is green.");
}

#[test]
fn required_text_without_mentions_still_needs_adjudication() {
    let check = text_check(json!({
        "requirement": "Say the refund was issued.",
        "accepted": ["The refund was issued."]
    }));
    assert!(matches!(
        verdict(&check, "We issued your refund."),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
    assert!(!check.reviewed_text["/body"].deterministic());
    assert!(text_check(refund_reply()).reviewed_text["/body"].deterministic());
}

#[test]
fn anchored_mentions_keep_dictated_text_exact() {
    let check = text_check(json!({
        "requirement": "The note is dictated.",
        "accepted": ["Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize."],
        "mentions": ["^Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize.$"]
    }));
    verdict(
        &check,
        "**Handing off to DBRE:** replica-2 disk is at 100%,\nneeds a volume resize.",
    )
    .unwrap();
    for body in [
        "Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize. Done.",
        "FYI: Handing off to DBRE: replica-2 disk is at 100%, needs a volume resize.",
        "Handing off to DBRE: replica-1 disk is at 100%, needs a volume resize.",
    ] {
        failure(&check, body);
    }
}

fn bound_check(mentions: Value) -> RecordCheck {
    text_check(json!({
        "requirement": "Report the refund and cite every actual refund id.",
        "mentions": mentions,
        "bound": {
            "identities": {"refunds": {
                "selector": {"collection": "/services/pay/refunds", "created": true},
                "field": "/id",
                "separator": ", "
            }},
            "examples": ["Refunded. References: ${refunds}."]
        }
    }))
}

fn bound_state(identities: &[&str], body: &str) -> Value {
    let refunds: serde_json::Map<_, _> = identities
        .iter()
        .map(|identity| ((*identity).to_owned(), json!({"id": identity})))
        .collect();
    json!({"services": {
        "pay": {"refunds": refunds},
        "desk": {"comments": {"actual": {"body": body}}}
    }})
}

#[test]
fn identity_slots_read_the_actual_values_from_state() {
    let check = bound_check(json!(["${refunds}", ["refund", "refunded"]]));
    let initial = json!({"services": {"pay": {"refunds": {}}}});
    for (identities, body) in [
        (
            &["re_a1"][..],
            "Hi Hannah, we refunded the duplicate charge. Stripe refund ID: re_a1.",
        ),
        (&["re_a1"][..], "Refunded. References: re_a1."),
        (
            &["re_a1", "re_b2"][..],
            "Refund issued in two parts: `re_b2` and `re_a1`.",
        ),
    ] {
        check
            .check(Some(&initial), &bound_state(identities, body))
            .unwrap_or_else(|error| panic!("{body}: {error}"));
    }
    for (identities, body) in [
        (
            &["re_a1"][..],
            "Hi Hannah, we refunded the duplicate charge. Stripe refund ID: re_a2.",
        ),
        (
            &["re_a1"][..],
            "Hi Hannah, we refunded the duplicate charge.",
        ),
        (&["re_a1"][..], "Stripe reference re_a1."),
        (&["re_a1", "re_b2"][..], "Refund issued: re_a1."),
        (&[][..], "Refund issued: re_a1."),
    ] {
        assert!(
            matches!(
                check.check(Some(&initial), &bound_state(identities, body)),
                Err(RecordCheckError::Failed(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn several_identities_without_a_list_separator_are_read_by_the_mentions() {
    let check = text_check(json!({
        "requirement": "Cite the issue that was opened.",
        "mentions": [["#${issue}", "issue ${issue}"]],
        "bound": {
            "identities": {"issue": {
                "selector": {"collection": "/services/pay/refunds", "created": true},
                "field": "/id"
            }},
            "examples": ["Opened #${issue}."]
        }
    }));
    let initial = json!({"services": {"pay": {"refunds": {}}}});
    let state = |body: &str| {
        json!({"services": {
            "pay": {"refunds": {"a": {"id": 7}, "b": {"id": 8}}},
            "desk": {"comments": {"actual": {"body": body}}}
        }})
    };
    check
        .check(Some(&initial), &state("Opened #7 and issue 8."))
        .unwrap();
    assert!(matches!(
        check.check(Some(&initial), &state("Opened #7.")),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn authored_mentions_must_be_usable_and_no_stricter_than_the_examples() {
    for (pointer, value) in [
        ("/mentions", json!([""])),
        ("/mentions", json!([[]])),
        ("/mentions", json!(["${refund}"])),
        ("/mentions", json!(["RF-88213", "next week"])),
        ("/forbids", json!(["--"])),
        ("/forbids", json!(["${refund}"])),
    ] {
        let mut reviewed = refund_reply();
        *reviewed.pointer_mut(pointer).unwrap() = value;
        let check: RecordCheck = serde_json::from_value(json!({
            "selector": {"collection": "/services/desk/comments", "created": true},
            "equals": 1,
            "reviewed_text": {"/body": reviewed}
        }))
        .unwrap();
        assert!(check.validate().is_err(), "{pointer}");
    }
    for mentions in [json!(["refund"]), json!(["${refunds}", "next week"])] {
        let check: RecordCheck = serde_json::from_value(json!({
            "selector": {"collection": "/services/desk/comments", "created": true},
            "equals": 1,
            "reviewed_text": {"/body": {
                "requirement": "Report the refund.",
                "mentions": mentions,
                "bound": {
                    "identities": {"refunds": {
                        "selector": {"collection": "/services/pay/refunds", "created": true},
                        "field": "/id"
                    }},
                    "examples": ["Refunded. Reference: ${refunds}."]
                }
            }}
        }))
        .unwrap();
        assert!(check.validate().is_err(), "{mentions}");
    }
    text_check(json!({"requirement": "Mention deploy 412.", "mentions": ["412"]}));
}

#[test]
fn mentions_are_authored_in_toml_as_phrases_or_lists_of_spellings() {
    let check: RecordCheck = toml::from_str(
        r#"
selector = { collection = "/services/desk/comments", created = true }
equals = 1
reviewed_text = { "/body" = { requirement = "Cite the refund.", accepted = ["Refund RF-88213, 5-7 business days."], mentions = ["RF-88213", ["5-7 business days", "5 to 7 business days"]], forbids = ["RF-88214"] } }
"#,
    )
    .unwrap();
    check.validate().unwrap();
    verdict(&check, "RF-88213 in 5 to 7 business days").unwrap();
    let serialized = serde_json::to_value(&check).unwrap();
    assert_eq!(
        serialized["reviewed_text"]["/body"]["mentions"],
        json!(["RF-88213", ["5-7 business days", "5 to 7 business days"]])
    );
    let plain: RecordCheck = serde_json::from_value(json!({
        "selector": {"collection": "/services/desk/comments", "created": true},
        "equals": 1,
        "reviewed_text": {"/body": {"requirement": "Cite the refund.", "accepted": ["Refunded."]}}
    }))
    .unwrap();
    let serialized = serde_json::to_value(&plain).unwrap();
    assert!(
        serialized["reviewed_text"]["/body"]
            .get("mentions")
            .is_none()
    );
    assert!(
        serialized["reviewed_text"]["/body"]
            .get("forbids")
            .is_none()
    );
}

#[test]
fn rendered_lists_headings_and_web_links_are_read_as_their_visible_text() {
    let mut reviewed = refund_reply();
    reviewed["format"] = json!("zendesk_html");
    let check = text_check(reviewed);
    for body in [
        "<p>Hi Marcus,</p>\n<ul>\n<li>Reference: <strong>RF-88213</strong></li>\n<li>Arrival: 5-7 business days</li>\n</ul>\n",
        "<div class=\"zd-comment\" dir=\"auto\"><h2>Refund</h2><ol><li>RF-88213</li><li>5 to 7 business days</li></ol></div>",
        "<p>See <a href=\"https://billing.example/refunds/RF-88213\" target=\"_blank\" rel=\"noopener\">RF-88213</a>: 5-7 business days.</p><hr><blockquote><code>ok</code></blockquote>",
    ] {
        verdict(&check, body).unwrap_or_else(|error| panic!("{body}: {error}"));
    }
    failure(
        &check,
        "<ul><li>Reference: RF-88213</li><li>Arrival: next week</li></ul>",
    );
    for body in [
        "<p>RF-88213, 5-7 business days</p><p hidden>Actually declined.</p>",
        "<p>RF-88213, 5-7 business days <a href=\"javascript:void(0)\">details</a></p>",
        "<p>RF-88213, 5-7 business days <a href=\"mailto:billing@example.com\">billing</a></p>",
        "<p>RF-88213, 5-7 business days <a href=\"https://example.com\" style=\"display:none\">x</a></p>",
        "<ul><li>RF-88213, 5-7 business days</ul>",
        "<h2><p>RF-88213, 5-7 business days</p></h2>",
    ] {
        assert!(
            matches!(
                verdict(&check, body),
                Err(RecordCheckError::NeedsAdjudication(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn notion_web_link_labels_are_text_but_contact_links_are_not() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector": {"collection": "/services/desk/comments", "created": true},
        "equals": 1,
        "reviewed_text": {"/body": {
            "requirement": "Record PagerDuty #4812.",
            "format": "notion_rich_text",
            "accepted": ["PagerDuty #4812"],
            "mentions": ["^PagerDuty #4812$"]
        }}
    }))
    .unwrap();
    let segment = |content: &str, link: Value| {
        let href = link.get("url").cloned().unwrap_or(Value::Null);
        json!({"type": "text", "text": {"content": content, "link": link}, "plain_text": content, "href": href})
    };
    let assess = |segments: Value| check.check(Some(&initial()), &state(segments));
    assess(json!([
        segment("PagerDuty ", Value::Null),
        segment(
            "#4812",
            json!({"url": "https://acme.pagerduty.com/incidents/Q1"})
        )
    ]))
    .unwrap();
    assert!(matches!(
        assess(json!([segment("PagerDuty #4813", Value::Null)])),
        Err(RecordCheckError::Failed(_))
    ));
    for link in [
        json!({"url": "tel:+15550100"}),
        json!({"url": "mailto:oncall@example.com"}),
        json!({"url": "https://example.com", "other": true}),
    ] {
        assert!(
            matches!(
                assess(json!([segment("PagerDuty #4812", link.clone())])),
                Err(RecordCheckError::NeedsAdjudication(_))
            ),
            "{link}"
        );
    }
}

fn fields_check() -> RecordCheck {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector": {"collection": "/services/desk/comments", "created": true},
        "equals": 1,
        "reviewed_fields": {
            "requirement": "Report that Apple Pay is missing at checkout on iOS 18 Safari.",
            "accepted": [{"/summary": "Apple Pay missing on iOS 18 Safari checkout", "/description": ""}],
            "mentions": ["Apple Pay", "checkout", "iOS 18", "Safari"],
            "forbids": ["Google Pay"]
        }
    }))
    .unwrap();
    check.validate().unwrap();
    check
}

#[test]
fn reviewed_fields_read_mentions_across_the_whole_projection() {
    let check = fields_check();
    let assess = |record: Value| {
        check.check(
            Some(&initial()),
            &json!({"services": {"desk": {"comments": {"actual": record}}}}),
        )
    };
    for record in [
        json!({"summary": "Apple Pay button not shown", "description": "Seen at checkout in Safari on iOS 18."}),
        json!({"summary": "[iOS 18][Safari] Apple Pay button missing at checkout"}),
        json!({"summary": "Checkout: Apple Pay missing", "description": "## Environment\n- iOS 18\n- Safari"}),
    ] {
        assess(record.clone()).unwrap_or_else(|error| panic!("{record}: {error}"));
    }
    for record in [
        json!({"summary": "Apple Pay button not shown", "description": "Seen at checkout in Safari on iOS 17."}),
        json!({"summary": "Payment button missing at checkout", "description": "iOS 18 Safari."}),
        json!({"summary": "Apple Pay and Google Pay missing at checkout on iOS 18 Safari"}),
        json!({"summary": "", "description": " "}),
    ] {
        assert!(
            matches!(assess(record.clone()), Err(RecordCheckError::Failed(_))),
            "{record}"
        );
    }
}

struct Trial {
    world: SharedWorld,
    expect: Expect,
}

impl Trial {
    fn new(reviewed: Value) -> Self {
        let expect: Expect = serde_json::from_value(json!({
            "version": 2,
            "audit_history": true,
            "records": [{
                "selector": {"collection": "/services/desk/comments", "created": true},
                "equals": 1,
                "reviewed_text": {"/body": reviewed}
            }],
            "preservation": {"allow": [{
                "pointer": "/services/desk/comments/*",
                "kind": "create",
                "matching": {"author": "agent"}
            }]}
        }))
        .unwrap();
        let directory =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
        let mut dataset = Dataset::load(&directory).unwrap();
        dataset.contract = expect.clone();
        dataset.services.insert("desk".into(), Store::default());
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        Self { world, expect }
    }

    fn write(&self, body: &str) {
        let operation = self
            .world
            .begin_operation(OperationKind::Http, "desk", "tools", "POST", "/comments")
            .unwrap();
        events::sync_scope(Some(operation.context().clone()), || {
            self.world
                .lock()
                .unwrap()
                .service_mut("desk")
                .store
                .collection_mut("comments")
                .insert(
                    "actual",
                    json!({"id": "actual", "author": "agent", "body": body}),
                );
        });
        operation.finish(200, true);
    }

    fn assess(&self) -> Grade {
        let world = self.world.lock().unwrap();
        grade(&GradeInput {
            expect: &self.expect,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer: None,
            tools_ips: vec!["tools".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&world.initial_snapshot()),
            events: Some(world.events().entries()),
        })
    }
}

const FINAL: &str = "Hi Marcus, refund RF-88213 was issued and arrives in 5-7 business days.";

#[test]
fn an_earlier_draft_that_lacks_a_fact_is_not_a_failure() {
    let trial = Trial::new(refund_reply());
    for body in ["Hi Marcus, your refund was issued.", "", FINAL] {
        trial.write(body);
    }
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    assert!(assessment.history_checked && !assessment.adjudication_needed);
}

#[test]
fn an_earlier_known_incorrect_version_fails_even_when_replaced() {
    for earlier in [
        "Refund RF-88213 was declined; nothing arrives within 5-7 business days.",
        "Hi Marcus, refund RF-88214 was issued.",
    ] {
        let trial = Trial::new(refund_reply());
        trial.write(earlier);
        trial.write(FINAL);
        let assessment = trial.assess();
        assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
        assert!(
            assessment
                .failures
                .iter()
                .any(|failure| !failure.event_evidence.is_empty()),
            "{assessment:?}"
        );
    }
}

#[test]
fn a_final_text_that_lacks_a_fact_fails_whatever_was_written_before() {
    let trial = Trial::new(refund_reply());
    trial.write(FINAL);
    trial.write("Hi Marcus, your refund was issued.");
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(!assessment.adjudication_needed);
}

#[test]
fn earlier_wording_without_a_deterministic_reading_still_needs_adjudication() {
    let trial = Trial::new(json!({
        "requirement": "Say the refund was issued.",
        "accepted": ["The refund was issued."]
    }));
    trial.write("We issued your refund.");
    trial.write("The refund was issued.");
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Unresolved, "{assessment:?}");
}
