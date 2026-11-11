use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

fn check() -> RecordCheck {
    serde_json::from_value(json!({
        "selector": {"collection": "/services/notion/blocks", "created": true},
        "equals": 1,
        "reviewed_text": {"/paragraph/rich_text": {
            "requirement": "The escalation contact is Sam Ortiz, +1 555-0142.",
            "format": "notion_rich_text",
            "accepted": ["Escalation contact: Sam Ortiz, +1 555-0142"]
        }}
    }))
    .unwrap()
}

fn segment(content: &str) -> Value {
    json!({
        "type": "text",
        "text": {"content": content, "link": null},
        "plain_text": content,
        "href": null,
        "annotations": {
            "bold": false, "italic": false, "strikethrough": false,
            "underline": false, "code": false, "color": "default"
        }
    })
}

fn assess(content: Value) -> Result<(), RecordCheckError> {
    check().check(
        Some(&json!({"services": {}})),
        &json!({"services":{"notion":{"blocks":{"actual":{"paragraph":{"rich_text":content}}}}}}),
    )
}

#[test]
fn structured_text_joins_all_segments_without_inventing_separators() {
    for segments in [
        json!([segment("Escalation contact: Sam Ortiz, +1 555-0142")]),
        json!([
            segment("Escalation contact: Sam "),
            segment("Ortiz, +1 555-0142")
        ]),
        json!([
            segment("  Escalation contact:\nSam Ortiz, "),
            segment("+1 555-0142  ")
        ]),
    ] {
        assess(segments).unwrap();
    }
    check().validate().unwrap();
}

#[test]
fn passive_annotations_do_not_change_the_complete_claim() {
    let mut formatted = segment("Escalation contact: Sam Ortiz, +1 555-0142");
    formatted["annotations"]["bold"] = json!(true);
    formatted["annotations"]["italic"] = json!(true);
    formatted["annotations"]["underline"] = json!(true);
    formatted["annotations"]["color"] = json!("blue_background");
    assess(json!([formatted])).unwrap();
}

#[test]
fn expected_words_in_one_segment_cannot_hide_another_segments_contradiction() {
    for segments in [
        json!([
            segment("Escalation contact: Sam Ortiz, +1 555-0142"),
            segment(". This is incorrect.")
        ]),
        json!([
            segment("Not the escalation contact: "),
            segment("Sam Ortiz, +1 555-0142")
        ]),
        json!([segment("Sam Ortiz, +1 555-0142")]),
        json!([segment(
            "Escalation contact: Sam Ortiz, +1 555-0142\u{200b}"
        )]),
    ] {
        assert!(matches!(
            assess(segments),
            Err(RecordCheckError::NeedsAdjudication(_))
        ));
    }
}

#[test]
fn response_plain_text_cannot_override_the_underlying_content() {
    let mut forged = segment("Marcus Webb is still the escalation contact.");
    forged["plain_text"] = json!("Escalation contact: Sam Ortiz, +1 555-0142");
    assert!(matches!(
        assess(json!([forged])),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn active_links_withdrawn_text_mentions_equations_and_unknown_metadata_require_review() {
    let original = segment("Escalation contact: Sam Ortiz, +1 555-0142");
    let mut withdrawn = original.clone();
    withdrawn["annotations"]["strikethrough"] = json!(true);
    let mut linked = original.clone();
    linked["text"]["link"] = json!({"url":"tel:+15550100"});
    linked["href"] = json!("tel:+15550100");
    let mut unknown = original.clone();
    unknown["annotations"]["hidden"] = json!(true);
    let mut malformed = original.clone();
    malformed["annotations"]["bold"] = json!("false");
    let mut other_color = original.clone();
    other_color["annotations"]["color"] = json!("transparent");
    for item in [
        withdrawn,
        linked,
        unknown,
        malformed,
        other_color,
        json!({"type":"mention","plain_text":"Escalation contact: Sam Ortiz, +1 555-0142","mention":{}}),
        json!({"type":"equation","equation":{"expression":"Escalation contact: Sam Ortiz, +1 555-0142"}}),
    ] {
        assert!(matches!(
            assess(json!([item])),
            Err(RecordCheckError::NeedsAdjudication(_))
        ));
    }
}

#[test]
fn empty_required_arrays_fail_and_oversized_content_cannot_be_partially_accepted() {
    assert!(matches!(
        assess(json!([])),
        Err(RecordCheckError::Failed(_))
    ));
    assert!(matches!(
        assess(json!([segment(&"x".repeat(1024 * 1024 + 1))])),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn array_cardinality_requires_the_exact_count_and_an_actual_array() {
    let definition: RecordCheck = serde_json::from_value(json!({
        "selector":{"collection":"/services/notion/pages","created":true},
        "equals":1,
        "array_lengths":{"/people":1}
    }))
    .unwrap();
    for people in [json!([]), json!([1, 2]), json!({"first":1}), Value::Null] {
        assert!(matches!(
            definition.check(
                Some(&json!({"services":{}})),
                &json!({"services":{"notion":{"pages":{"new":{"people":people}}}}})
            ),
            Err(RecordCheckError::Failed(_))
        ));
    }
    definition
        .check(
            Some(&json!({"services":{}})),
            &json!({"services":{"notion":{"pages":{"new":{"people":[1]}}}}}),
        )
        .unwrap();
    definition.validate().unwrap();
}

#[test]
fn reviewed_incorrect_examples_are_exact_disjoint_and_not_a_keyword_classifier() {
    let mut definition = serde_json::to_value(check()).unwrap();
    definition["reviewed_text"]["/paragraph/rich_text"]["rejected"] =
        json!(["Escalation contact: Marcus Webb, +1 555-0100"]);
    let rejected: RecordCheck = serde_json::from_value(definition.clone()).unwrap();
    rejected.validate().unwrap();
    let state = json!({"services":{"notion":{"blocks":{"actual":{"paragraph":{"rich_text":[segment("Escalation contact: Marcus Webb, +1 555-0100")]}}}}}});
    assert!(matches!(
        rejected.check(Some(&json!({"services":{}})), &state),
        Err(RecordCheckError::Failed(_))
    ));
    definition["reviewed_text"]["/paragraph/rich_text"]["rejected"] =
        json!([" Escalation contact: Sam Ortiz, +1 555-0142 "]);
    assert!(
        serde_json::from_value::<RecordCheck>(definition)
            .unwrap()
            .validate()
            .is_err()
    );
}
