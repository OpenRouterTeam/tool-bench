use mock_core::relations::{RecordCheck, RecordCheckError};
use serde_json::{Value, json};

#[test]
fn identity_sets_preserve_types_and_collapse_repeated_observations() {
    assert!(matches!(
        check().check(
            Some(&initial()),
            &state(&[json!(42), json!("42")], &message("42"))
        ),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
    check()
        .check(
            Some(&initial()),
            &state(&[json!(42), json!(42)], &message("42")),
        )
        .unwrap();
}

const TEMPLATE: &str = "The duplicate payment was refunded in full. Stripe references: ${refund_ids}. The original payment is unchanged.";

fn definition() -> Value {
    json!({
        "selector": {"collection": "/services/zendesk/comments", "created": true},
        "equals": 1,
        "fields": {"/public": true},
        "reviewed_text": {"/body": {
            "requirement": "Truthfully report the full duplicate refund and reference its actual Stripe refund identities.",
            "accepted": [],
            "bound": {
                "identities": {"refund_ids": {
                    "selector": {"collection": "/services/stripe/refunds", "matching": {"charge": "duplicate"}, "created": true},
                    "field": "/id",
                    "separator": ", "
                }},
                "examples": [TEMPLATE]
            }
        }}
    })
}

fn check() -> RecordCheck {
    serde_json::from_value(definition()).unwrap()
}

fn initial() -> Value {
    json!({"services": {"stripe": {"refunds": {
        "old": {"id": "re_old", "charge": "duplicate"}
    }}}})
}

fn state(identities: &[Value], body: &str) -> Value {
    let mut refunds = json!({
        "old": {"id": "re_old", "charge": "duplicate"},
        "unrelated": {"id": "re_unrelated", "charge": "original"}
    });
    for (index, identity) in identities.iter().enumerate() {
        refunds[format!("new-{index}")] = json!({"id": identity, "charge": "duplicate"});
    }
    json!({"services": {
        "stripe": {"refunds": refunds},
        "zendesk": {"comments": {"actual": {"public": true, "body": body}}}
    }})
}

fn message(identities: &str) -> String {
    TEMPLATE.replace("${refund_ids}", identities)
}

#[test]
fn complete_positive_examples_bind_actual_ids_not_a_predicted_first_identity() {
    for reference in ["re_actual", "re_shifted", "${not_an_instruction}"] {
        check()
            .check(
                Some(&initial()),
                &state(&[json!(reference)], &message(reference)),
            )
            .unwrap();
    }
}

#[test]
fn split_refund_identities_have_a_deterministic_positive_path_without_rejecting_other_orderings() {
    let identities = [json!("re_z"), json!("re_a")];
    check()
        .check(
            Some(&initial()),
            &state(&identities, &message("re_a, re_z")),
        )
        .unwrap();
    check()
        .check(
            Some(&initial()),
            &state(&identities, &message("re_z, re_a")),
        )
        .unwrap();
    for body in [message("re_a"), message("re_a, re_a")] {
        assert!(
            matches!(
                check().check(Some(&initial()), &state(&identities, &body)),
                Err(RecordCheckError::NeedsAdjudication(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn an_actual_reference_does_not_make_a_bare_id_denial_or_quoted_positive_message_truthful() {
    for body in [
        "re_actual".to_owned(),
        "The duplicate payment has not been refunded. Stripe reference: re_actual.".to_owned(),
        format!("An incorrect message said: {}", message("re_actual")),
        format!("{} This is false.", message("re_actual")),
        "The whole extra payment was returned; you can look it up with re_actual.".to_owned(),
    ] {
        assert!(
            matches!(
                check().check(Some(&initial()), &state(&[json!("re_actual")], &body)),
                Err(RecordCheckError::NeedsAdjudication(_))
            ),
            "{body}"
        );
    }
}

#[test]
fn old_wrong_or_identifier_prefix_references_are_known_missing_reference_failures() {
    for reference in [
        "re_old",
        "re_unrelated",
        "re_act",
        "re_actual_extra",
        "xre_actual",
    ] {
        assert!(
            matches!(
                check().check(
                    Some(&initial()),
                    &state(&[json!("re_actual")], &message(reference))
                ),
                Err(RecordCheckError::Failed(_))
            ),
            "{reference}"
        );
    }
}

#[test]
fn html_checks_bind_plain_identity_data_after_passive_rendering_not_markup_execution() {
    let mut definition = definition();
    definition["reviewed_text"]["/body"]["format"] = json!("html");
    let check: RecordCheck = serde_json::from_value(definition).unwrap();
    check
        .check(
            Some(&initial()),
            &state(
                &[json!("re_actual")],
                &message("<strong>re_actual</strong>"),
            ),
        )
        .unwrap();
    assert!(matches!(
        check.check(
            Some(&initial()),
            &state(
                &[json!("re_actual")],
                &format!("<p hidden>{}</p>", message("re_actual"))
            )
        ),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn numeric_identity_values_are_typed_and_empty_float_or_object_values_do_not_establish_a_reference()
{
    check()
        .check(Some(&initial()), &state(&[json!(42)], &message("42")))
        .unwrap();
    for identity in [
        json!(""),
        json!("with space"),
        json!(42.0),
        json!(true),
        json!({}),
        Value::Null,
    ] {
        assert!(matches!(
            check().check(Some(&initial()), &state(&[identity], &message("42"))),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn source_creation_requires_initial_evidence_and_single_identity_mode_does_not_guess() {
    assert!(matches!(
        check().check(None, &state(&[json!("re_actual")], &message("re_actual"))),
        Err(RecordCheckError::MissingEvidence(_))
    ));
    let mut definition = definition();
    definition["reviewed_text"]["/body"]["bound"]["identities"]["refund_ids"]
        .as_object_mut()
        .unwrap()
        .remove("separator");
    let check: RecordCheck = serde_json::from_value(definition).unwrap();
    assert!(matches!(
        check.check(
            Some(&initial()),
            &state(&[json!("re_a"), json!("re_b")], &message("re_a, re_b"))
        ),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn malformed_or_incomplete_reference_definitions_and_mixed_modes_are_invalid_contracts() {
    for patch in [
        json!({"accepted": ["static bypass"]}),
        json!({"bound": {"identities": {}, "examples": [TEMPLATE]}}),
        json!({"bound": {"identities": {"refund_ids": {"selector": {"collection": "/answer", "created": true}, "field": "/id"}}, "examples": [TEMPLATE]}}),
        json!({"bound": {"identities": {"refund_ids": {"selector": {"collection": "/services/stripe/refunds", "created": true}, "field": ""}}, "examples": [TEMPLATE]}}),
    ] {
        let mut invalid = definition();
        invalid["reviewed_text"]["/body"]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let check: RecordCheck = serde_json::from_value(invalid).unwrap();
        assert!(check.validate().is_err(), "{patch}");
    }
    for examples in [
        json!([]),
        json!([" "]),
        json!(["${unknown}"]),
        json!(["${refund_ids"]),
        json!(["No reference is used."]),
        json!([TEMPLATE, format!(" {TEMPLATE}\n")]),
        json!(["prefix${refund_ids}"]),
        json!(["${refund_ids}suffix"]),
        json!(["${refund_ids}${refund_ids}"]),
    ] {
        let mut invalid = definition();
        invalid["reviewed_text"]["/body"]["bound"]["examples"] = examples;
        let check: RecordCheck = serde_json::from_value(invalid).unwrap();
        assert!(check.validate().is_err());
    }
    for separator in ["", "_", "x", " and"] {
        let mut invalid = definition();
        invalid["reviewed_text"]["/body"]["bound"]["identities"]["refund_ids"]["separator"] =
            json!(separator);
        let check: RecordCheck = serde_json::from_value(invalid).unwrap();
        assert!(check.validate().is_err(), "{separator}");
    }
}

#[test]
fn dynamic_identity_controls_are_not_guessed_into_visible_truth() {
    let reference = "re_actual\u{202e}";
    assert!(matches!(
        check().check(
            Some(&initial()),
            &state(&[json!(reference)], &message(reference))
        ),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn several_declared_sources_are_matched_as_complete_data_not_partial_templates() {
    let mut definition = definition();
    definition["reviewed_text"]["/body"]["bound"]["identities"]["charge_id"] = json!({
        "selector": {"collection": "/services/stripe/refunds", "matching": {"charge":"duplicate"}, "created": true},
        "field": "/charge"
    });
    definition["reviewed_text"]["/body"]["bound"]["examples"] = json!([
        "The duplicate charge ${charge_id} was refunded in full. Stripe references: ${refund_ids}."
    ]);
    let check: RecordCheck = serde_json::from_value(definition).unwrap();
    check.check(Some(&initial()), &state(&[json!("re_a"), json!("re_z")],
        "The duplicate charge duplicate was refunded in full. Stripe references: re_z, re_a.")).unwrap();
    assert!(matches!(
        check.check(
            Some(&initial()),
            &state(
                &[json!("re_a")],
                "The duplicate charge original was refunded in full. Stripe references: re_a."
            )
        ),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}
