use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

fn check() -> RecordCheck {
    serde_json::from_value(json!({
        "selector": {"collection": "/services/crm/notes", "created": true},
        "equals": 1,
        "html_text": {"/body": "Review passed; send the SOW by Nov 14."}
    }))
    .unwrap()
}

fn initial() -> Value {
    json!({"services": {}})
}

fn state(body: Value) -> Value {
    json!({"services": {"crm": {"notes": {"actual": {"body": body}}}}})
}

#[test]
fn passive_formatting_entities_and_declared_whitespace_equivalence_do_not_change_content() {
    for body in [
        "Review passed; send the SOW by Nov 14.",
        "<p>Review <strong>passed</strong>; send the SOW by Nov 14.</p>",
        "<div>Review passed;<br/>send the SOW by Nov&nbsp;14.</div>",
        "<p>Review passed;</p><p>send the SOW by Nov 14.</p>",
        "<DIV><span>Review passed;</span> <i>send the SOW</i> by Nov 14.</DIV>",
    ] {
        check()
            .check(Some(&initial()), &state(json!(body)))
            .unwrap_or_else(|error| panic!("{body}: {error}"));
    }
}

#[test]
fn different_text_duplicate_text_comment_only_and_nontext_fields_are_proven_failures() {
    for body in [
        json!("Review did not pass; send the SOW by Nov 14."),
        json!("Review passed; send the SOW by Nov 15."),
        json!("<p>Review passed; send the SOW by Nov 14.</p><p>But this is wrong.</p>"),
        json!("<!--Review passed; send the SOW by Nov 14.-->"),
        json!(42),
        Value::Null,
    ] {
        assert!(
            matches!(
                check().check(Some(&initial()), &state(body.clone())),
                Err(RecordCheckError::Failed(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn hidden_active_foreign_unknown_or_repaired_markup_never_establishes_a_pass() {
    for body in [
        "<div hidden>Review passed; send the SOW by Nov 14.</div>",
        "<div style='display:none'>Review passed; send the SOW by Nov 14.</div>",
        "<script>Review passed; send the SOW by Nov 14.</script>",
        "<style>Review passed; send the SOW by Nov 14.</style>",
        "<template>Review passed; send the SOW by Nov 14.</template>",
        "<head>Review passed; send the SOW by Nov 14.</head>",
        "<svg><text>Review passed; send the SOW by Nov 14.</text></svg>",
        "<p>Review passed; send the SOW by Nov 14.</p style='display:none'>",
        "<p>Review passed; send the SOW by Nov 14.</p unexpected>",
        "<p>Review passed; send the SOW by Nov 14.</p><head></head>",
        "<p>Review passed; send the SOW by Nov 14.",
        "<p><div>Review passed; send the SOW by Nov 14.</div></p>",
        "<p><strong>Review passed; send the SOW by Nov 14.</p></strong>",
        "<p/>Review passed; send the SOW by Nov 14.",
        "<del>Review passed; send the SOW by Nov 14.</del>",
        "<pre>Review passed; send the SOW by Nov 14.</pre>",
    ] {
        assert!(
            matches!(
                check().check(Some(&initial()), &state(json!(body))),
                Err(RecordCheckError::NeedsAdjudication(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn uncertain_rendering_cannot_hide_a_known_mismatch_in_later_records_or_totals() {
    let mut definition = serde_json::to_value(check()).unwrap();
    definition["equals"] = json!(2);
    let check: RecordCheck = serde_json::from_value(definition).unwrap();
    let state = json!({"services": {"crm": {"notes": {
        "first": {"body": "<p style='display:none'>Review passed; send the SOW by Nov 14.</p>"},
        "second": {"body": "Review did not pass; send the SOW by Nov 15."}
    }}}});
    assert!(matches!(
        check.check(Some(&initial()), &state),
        Err(RecordCheckError::Failed(_))
    ));
    let mut definition = serde_json::to_value(check).unwrap();
    definition["equals"] = json!(1);
    definition["totals"] = json!({"/amount": 20});
    let check: RecordCheck = serde_json::from_value(definition).unwrap();
    let state = json!({"services": {"crm": {"notes": {
        "only": {"body": "<p style='display:none'>Review passed; send the SOW by Nov 14.</p>", "amount": 10}
    }}}});
    assert!(matches!(
        check.check(Some(&initial()), &state),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn rich_text_fields_need_valid_pointers_and_nonzero_record_capacity() {
    for extra in [
        json!({"equals": 0}),
        json!({"html_text": {"invalid": "text"}}),
    ] {
        let mut definition = serde_json::to_value(check()).unwrap();
        definition
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let check: RecordCheck = serde_json::from_value(definition).unwrap();
        assert!(check.validate().is_err());
    }
}

#[test]
fn excessive_input_or_depth_requires_adjudication_instead_of_unbounded_rendering() {
    for body in [
        " ".repeat(2 * 1024 * 1024),
        format!(
            "{}Review passed; send the SOW by Nov 14.{}",
            "<div>".repeat(4096),
            "</div>".repeat(4096)
        ),
    ] {
        assert!(matches!(
            check().check(Some(&initial()), &state(json!(body))),
            Err(RecordCheckError::NeedsAdjudication(_))
        ));
    }
}

#[test]
fn directional_and_invisible_controls_are_not_guessed_into_textual_truth() {
    for body in [
        "Re\u{200b}view passed; send the SOW by Nov 14.",
        "\u{202e}Review passed; send the SOW by Nov 14.",
        "\u{feff}Review passed; send the SOW by Nov 14.",
    ] {
        assert!(matches!(
            check().check(Some(&initial()), &state(json!(body))),
            Err(RecordCheckError::NeedsAdjudication(_))
        ));
    }
}
