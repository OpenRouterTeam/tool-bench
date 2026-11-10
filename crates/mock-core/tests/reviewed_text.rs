use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

fn contract() -> Value {
    json!({
        "selector": {"collection": "/services/crm/notes", "created": true},
        "equals": 1,
        "reviewed_text": {"/body": {
            "requirement": "Dana requested a three-year renewal quote by May 22, 2026.",
            "accepted": [
                "Dana requested a three-year renewal quote by May 22, 2026.",
                "Dana wants a 3-year renewal quote by May 22."
            ]
        }}
    })
}

fn check() -> RecordCheck {
    serde_json::from_value(contract()).unwrap()
}

fn initial() -> Value {
    json!({"services": {}})
}

fn state(body: Value) -> Value {
    json!({"services": {"crm": {"notes": {"actual": {"body": body}}}}})
}

#[test]
fn only_complete_author_declared_positive_examples_establish_a_pass() {
    for body in [
        "Dana requested a three-year renewal quote by May 22, 2026.",
        "Dana wants a 3-year renewal quote by May 22.",
        "  Dana wants a 3-year\nrenewal quote by May 22.  ",
    ] {
        check()
            .check(Some(&initial()), &state(json!(body)))
            .unwrap();
    }
}

#[test]
fn other_wording_contradictions_quotes_and_keyword_fragments_require_adjudication() {
    for body in [
        "A three-year pricing proposal is needed before the 22nd of May.",
        "quote",
        "Dana does not want a 3-year renewal quote by May 22.",
        "The incorrect note said: Dana wants a 3-year renewal quote by May 22.",
        "Dana wants a 3-year renewal quote by May 22. This is false.",
        "<span hidden>Dana wants a 3-year renewal quote by May 22.</span>",
        "Dana wants a 3-year renewal quote by May 22.\u{200b}",
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
fn missing_or_nontext_content_is_a_known_violation_of_a_required_note() {
    for body in [Value::Null, json!(42), json!(""), json!(" \n\t")] {
        assert!(matches!(
            check().check(Some(&initial()), &state(body)),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn optional_reviewed_text_does_not_require_narration_but_still_checks_any_supplied_claims() {
    let mut definition = contract();
    definition["reviewed_text"]["/body"]["optional"] = json!(true);
    let optional: RecordCheck = serde_json::from_value(definition).unwrap();
    optional.validate().unwrap();
    for body in [Value::Null, json!(""), json!(" \n\t")] {
        optional.check(Some(&initial()), &state(body)).unwrap();
    }
    optional
        .check(
            Some(&initial()),
            &json!({"services":{"crm":{"notes":{"actual":{}}}}}),
        )
        .unwrap();
    assert!(matches!(
        optional.check(Some(&initial()), &state(json!(42))),
        Err(RecordCheckError::Failed(_))
    ));
    assert!(matches!(
        optional.check(
            Some(&initial()),
            &state(json!(
                "Dana did NOT request a three-year renewal quote by May 22, 2026."
            ))
        ),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
    assert!(
        serde_json::to_value(check()).unwrap()["reviewed_text"]["/body"]
            .get("optional")
            .is_none()
    );
}

fn html_check() -> RecordCheck {
    let mut definition = contract();
    definition["reviewed_text"]["/body"]["format"] = json!("html");
    serde_json::from_value(definition).unwrap()
}

#[test]
fn reviewed_html_accepts_passive_formats_of_complete_positive_examples() {
    for body in [
        "<p>Dana requested a <strong>three-year</strong> renewal quote by May 22, 2026.</p>",
        "<div>Dana wants a 3-year<br>renewal quote by May&nbsp;22.</div>",
        "Dana wants a 3-year renewal quote by May 22.",
    ] {
        html_check()
            .check(Some(&initial()), &state(json!(body)))
            .unwrap_or_else(|error| panic!("{body}: {error}"));
    }
}

#[test]
fn reviewed_html_cannot_turn_hidden_markup_or_contradictory_content_into_success() {
    for body in [
        "<p hidden>Dana wants a 3-year renewal quote by May 22.</p>",
        "<script>Dana wants a 3-year renewal quote by May 22.</script>",
        "<p>Dana wants a 3-year renewal quote by May 22.</p><p>This is incorrect.</p>",
        "<p>Dana does not want a 3-year renewal quote by May 22.</p>",
        "<p>Dana wants a 3-year renewal quote by May 22.",
        "<p>A three-year offer is needed before the twenty-second of May.</p>",
    ] {
        assert!(
            matches!(
                html_check().check(Some(&initial()), &state(json!(body))),
                Err(RecordCheckError::NeedsAdjudication(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn reviewed_html_requires_nonempty_visible_content_but_does_not_guess_unsupported_rendering() {
    for body in [
        json!(""),
        json!("<p> </p>"),
        json!("<!--Only a comment-->"),
        Value::Null,
    ] {
        assert!(matches!(
            html_check().check(Some(&initial()), &state(body)),
            Err(RecordCheckError::Failed(_))
        ));
    }
    assert!(matches!(
        html_check().check(Some(&initial()), &state(json!("<img alt='text'>"))),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn reviewed_plain_text_does_not_implicitly_parse_html_and_unknown_formats_are_rejected() {
    assert!(matches!(
        check().check(
            Some(&initial()),
            &state(json!("<p>Dana wants a 3-year renewal quote by May 22.</p>"))
        ),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
    let mut definition = contract();
    definition["reviewed_text"]["/body"]["format"] = json!("guess");
    assert!(serde_json::from_value::<RecordCheck>(definition).is_err());
}

#[test]
fn zendesk_html_explicitly_accepts_only_the_known_envelope_and_passive_inner_content() {
    let mut definition = contract();
    definition["reviewed_text"]["/body"]["format"] = json!("zendesk_html");
    let check: RecordCheck = serde_json::from_value(definition).unwrap();
    for body in [
        "<div class=\"zd-comment\" dir=\"auto\"><p>Dana wants a 3-year renewal quote by May 22.</p></div>",
        "<p>Dana wants a 3-year renewal quote by May 22.</p>",
    ] {
        check.check(Some(&initial()), &state(json!(body))).unwrap();
    }
    for body in [
        "<div class=\"zd-comment\" dir=\"auto\" hidden><p>Dana wants a 3-year renewal quote by May 22.</p></div>",
        "<div class=\"zd-comment\" dir=\"auto\"><p hidden>Dana wants a 3-year renewal quote by May 22.</p></div>",
        "<div class=\"zd-comment\" dir=\"auto\"><p>Dana wants a 3-year renewal quote by May 22.</p></div><div></div>",
        "<div class=\"zd-comment\" dir=\"auto\"><script>Dana wants a 3-year renewal quote by May 22.</script></div>",
    ] {
        assert!(
            matches!(
                check.check(Some(&initial()), &state(json!(body))),
                Err(RecordCheckError::NeedsAdjudication(_))
            ),
            "{body}"
        );
    }
    assert!(matches!(
        html_check().check(Some(&initial()), &state(json!(
            "<div class=\"zd-comment\" dir=\"auto\"><p>Dana wants a 3-year renewal quote by May 22.</p></div>"
        ))),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn empty_criteria_examples_duplicate_normalized_examples_and_invalid_pointers_are_invalid() {
    for (pointer, value) in [
        ("/reviewed_text/~1body/requirement", json!(" ")),
        ("/reviewed_text/~1body/accepted", json!([])),
        ("/reviewed_text/~1body/accepted", json!([" "])),
        (
            "/reviewed_text/~1body/accepted",
            json!(["same text", " same\ntext "]),
        ),
        (
            "/reviewed_text",
            json!({"": {"requirement": "meaning", "accepted": ["text"]}}),
        ),
    ] {
        let mut definition = contract();
        *definition.pointer_mut(pointer).unwrap() = value;
        let check: RecordCheck = serde_json::from_value(definition).unwrap();
        assert!(check.validate().is_err(), "{pointer}");
    }
}

#[test]
fn known_field_and_count_violations_are_not_concealed_by_an_unreviewed_paraphrase() {
    let mut definition = contract();
    definition["fields"] = json!({"/owner": "Dana"});
    let check: RecordCheck = serde_json::from_value(definition).unwrap();
    assert!(matches!(
        check.check(Some(&initial()), &state(json!("unreviewed wording"))),
        Err(RecordCheckError::Failed(_))
    ));
    let mut definition = contract();
    definition["equals"] = json!(2);
    let check: RecordCheck = serde_json::from_value(definition).unwrap();
    assert!(matches!(
        check.check(Some(&initial()), &state(json!("unreviewed wording"))),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn nonempty_field_alternatives_accept_any_complete_branch_without_vacuous_success() {
    let definition = json!({
        "selector": {"collection": "/services/crm/notes", "created": true},
        "equals": 1,
        "any_fields": [{"/type": "Call"}, {"/subtype": "Call", "/type": null}]
    });
    let check: RecordCheck = serde_json::from_value(definition.clone()).unwrap();
    for record in [
        json!({"type": "Call"}),
        json!({"subtype": "Call", "type": null}),
    ] {
        check
            .check(
                Some(&initial()),
                &json!({"services": {"crm": {"notes": {"actual": record}}}}),
            )
            .unwrap();
    }
    assert!(matches!(
        check.check(Some(&initial()), &state(json!("text"))),
        Err(RecordCheckError::Failed(_))
    ));
    let mut invalid = definition;
    invalid["any_fields"] = json!([{}]);
    let check: RecordCheck = serde_json::from_value(invalid).unwrap();
    assert!(check.validate().is_err());
}

#[test]
fn optional_fields_allow_absence_and_null_but_reject_inconsistent_nonnull_values() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector": {"collection": "/services/crm/notes", "created": true},
        "equals": 1,
        "optional_fields": {"/account": "Northgate"}
    }))
    .unwrap();
    for record in [
        json!({}),
        json!({"account": null}),
        json!({"account": "Northgate"}),
    ] {
        check
            .check(
                Some(&initial()),
                &json!({"services": {"crm": {"notes": {"actual": record}}}}),
            )
            .unwrap();
    }
    assert!(matches!(
        check.check(
            Some(&initial()),
            &json!({"services": {"crm": {"notes": {"actual": {"account": "Other"}}}}})
        ),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn null_fields_distinguish_explicit_null_from_absence_and_invented_values() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector": {"collection": "/services/crm/notes", "created": true},
        "equals": 1,
        "null_fields": ["/duration"]
    }))
    .unwrap();
    for (record, passes) in [
        (json!({"duration": null}), true),
        (json!({}), false),
        (json!({"duration": 60}), false),
    ] {
        assert_eq!(
            check
                .check(
                    Some(&initial()),
                    &json!({"services": {"crm": {"notes": {"actual": record}}}})
                )
                .is_ok(),
            passes
        );
    }
}

#[test]
fn reviewed_wording_cannot_be_forced_into_one_literal_by_a_second_predicate() {
    for (field, value) in [
        ("fields", json!({"/body": "one spelling"})),
        ("optional_fields", json!({"/body": "one spelling"})),
        ("optional_choices", json!({"/body": ["one spelling"]})),
        ("null_fields", json!(["/body"])),
        ("any_fields", json!([{"/body": "one spelling"}])),
        ("html_text", json!({"/body": "one spelling"})),
    ] {
        let mut definition = contract();
        definition[field] = value;
        let check: RecordCheck = serde_json::from_value(definition).unwrap();
        assert!(check.validate().is_err(), "{field}");
    }
}

#[test]
fn nullable_choices_are_explicit_and_do_not_make_required_classification_optional() {
    let definition = json!({
        "selector": {"collection": "/services/crm/notes", "created": true},
        "equals": 1,
        "optional_choices": {"/subtype": ["Task", "Call", ""], "/type": ["Call", ""]},
        "any_fields": [{"/subtype": "Call"}, {"/type": "Call"}]
    });
    let check: RecordCheck = serde_json::from_value(definition.clone()).unwrap();
    for (record, passes) in [
        (json!({"subtype": "Call", "type": ""}), true),
        (json!({"subtype": null, "type": "Call"}), true),
        (json!({"subtype": "Task", "type": "Call"}), true),
        (json!({"subtype": "Email", "type": "Call"}), false),
        (json!({"subtype": null, "type": null}), false),
    ] {
        assert_eq!(
            check
                .check(
                    Some(&initial()),
                    &json!({"services": {"crm": {"notes": {"actual": record}}}})
                )
                .is_ok(),
            passes
        );
    }
    for choices in [json!([]), json!(["Call", "Call"])] {
        let mut invalid = definition.clone();
        invalid["optional_choices"]["/type"] = choices;
        let check: RecordCheck = serde_json::from_value(invalid).unwrap();
        assert!(check.validate().is_err());
    }
}
