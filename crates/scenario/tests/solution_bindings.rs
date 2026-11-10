use scenario::{
    Solution, SolutionCall,
    solution::{Bindings, Capture},
};
use serde_json::{Value, json};

fn call(arguments: Value, template: bool) -> SolutionCall {
    serde_json::from_value(json!({
        "endpoint": "stripe",
        "tool": "example",
        "arguments": arguments,
        "template": template
    }))
    .unwrap()
}

#[test]
fn captures_actual_refund_ids_from_json_text_tool_results() {
    let mut refund = call(json!({}), false);
    refund.capture = vec![Capture {
        name: "refund_id".into(),
        pointer: "/content/0/text".into(),
        json_pointer: Some("/id".into()),
        select: None,
        constraint: None,
    }];
    let mut bindings = Bindings::new();
    refund
        .capture_result(
            &json!({"content": [{"type": "text", "text": "{\"id\":\"re_generated_later\"}"}]}),
            &mut bindings,
        )
        .unwrap();
    let reply = call(json!({"comment": "Your refund is ${refund_id}."}), true);
    assert_eq!(
        reply.resolve_arguments(&bindings).unwrap(),
        json!({"comment": "Your refund is re_generated_later."})
    );
}

#[test]
fn shared_rendering_fixtures_preserve_types_and_do_not_reinterpret_captured_text() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/oracle-bindings.json")).unwrap();
    for fixture in fixtures["render"].as_array().unwrap() {
        let bindings: Bindings = serde_json::from_value(fixture["bindings"].clone()).unwrap();
        let arguments = call(fixture["arguments"].clone(), true)
            .resolve_arguments(&bindings)
            .unwrap();
        assert_eq!(arguments, fixture["expected"], "{}", fixture["name"]);
    }
}

#[test]
fn shared_argument_replacements_preserve_complete_captures_and_change_only_existing_fields() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/oracle-bindings.json")).unwrap();
    for fixture in fixtures["replacement_render"].as_array().unwrap() {
        let bindings: Bindings = serde_json::from_value(fixture["bindings"].clone()).unwrap();
        let mut request = call(fixture["arguments"].clone(), true);
        request.replacements = serde_json::from_value(fixture["replacements"].clone()).unwrap();
        assert_eq!(
            request.resolve_arguments(&bindings).unwrap(),
            fixture["expected"],
            "{}",
            fixture["name"]
        );
    }
}

#[test]
fn shared_invalid_replacements_cannot_create_fields_overlap_or_expand_unknown_bindings() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/oracle-bindings.json")).unwrap();
    for fixture in fixtures["invalid_replacements"].as_array().unwrap() {
        let bindings: Bindings = serde_json::from_value(fixture["bindings"].clone()).unwrap();
        let mut request = call(fixture["arguments"].clone(), true);
        request.replacements = serde_json::from_value(fixture["replacements"].clone()).unwrap();
        assert!(
            request.resolve_arguments(&bindings).is_err(),
            "{}",
            fixture["name"]
        );
    }
    let mut request = call(json!({"field":"literal"}), false);
    request.replacements =
        serde_json::from_value(json!([{"pointer":"/field","value":"changed"}])).unwrap();
    assert!(request.resolve_arguments(&Bindings::new()).is_err());
}

#[test]
fn shared_final_answer_fixtures_preserve_actual_returned_bytes() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/oracle-bindings.json")).unwrap();
    for fixture in fixtures["answers"].as_array().unwrap() {
        let bindings: Bindings = serde_json::from_value(fixture["bindings"].clone()).unwrap();
        let solution = Solution {
            answer: Some(fixture["template"].as_str().unwrap().to_owned()),
            answer_template: true,
            ..Solution::default()
        };
        assert_eq!(
            solution.resolve_answer(&bindings).unwrap().unwrap(),
            fixture["expected"].as_str().unwrap(),
            "{}",
            fixture["name"]
        );
    }
}

#[test]
fn shared_invalid_templates_cannot_silently_remain_unrendered() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/oracle-bindings.json")).unwrap();
    for fixture in fixtures["invalid_render"].as_array().unwrap() {
        let bindings: Bindings = serde_json::from_value(fixture["bindings"].clone()).unwrap();
        assert!(
            call(fixture["arguments"].clone(), true)
                .resolve_arguments(&bindings)
                .is_err(),
            "{}",
            fixture["name"]
        );
    }
}

#[test]
fn shared_capture_fixtures_use_rfc6901_and_preserve_null_values() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/oracle-bindings.json")).unwrap();
    for fixture in fixtures["capture"].as_array().unwrap() {
        let mut oracle_call = call(json!({}), false);
        oracle_call.capture = serde_json::from_value(fixture["captures"].clone()).unwrap();
        let mut bindings: Bindings = serde_json::from_value(fixture["bindings"].clone()).unwrap();
        oracle_call
            .capture_result(&fixture["result"], &mut bindings)
            .unwrap();
        assert_eq!(
            serde_json::to_value(bindings).unwrap(),
            fixture["expected"],
            "{}",
            fixture["name"]
        );
    }
}

#[test]
fn templates_are_opt_in_so_existing_javascript_is_not_modified() {
    let original = json!({"code": "async () => `/zones/${zone.id}/dns_records`"});
    assert_eq!(
        call(original.clone(), false)
            .resolve_arguments(&Bindings::new())
            .unwrap(),
        original
    );
}

#[test]
fn captures_are_atomic_and_cannot_overwrite_bindings() {
    let mut oracle_call = call(json!({}), false);
    oracle_call.capture = vec![
        Capture {
            name: "one".into(),
            pointer: "/one".into(),
            json_pointer: None,
            select: None,
            constraint: None,
        },
        Capture {
            name: "two".into(),
            pointer: "/missing".into(),
            json_pointer: None,
            select: None,
            constraint: None,
        },
    ];
    let mut bindings = Bindings::new();
    assert!(
        oracle_call
            .capture_result(&json!({"one": 1}), &mut bindings)
            .is_err()
    );
    assert!(bindings.is_empty());
    oracle_call.capture.pop();
    oracle_call
        .capture_result(&json!({"one": 1}), &mut bindings)
        .unwrap();
    assert!(
        oracle_call
            .capture_result(&json!({"one": 2}), &mut bindings)
            .is_err()
    );
    assert_eq!(bindings["one"], json!(1));
}

#[test]
fn unsafe_names_forward_references_and_invalid_pointers_fail_authoring() {
    for name in ["", "one.two", "id;command", "1name", "bad-name"] {
        assert!(
            Capture {
                name: name.into(),
                pointer: "".into(),
                json_pointer: None,
                select: None,
                constraint: None,
            }
            .validate()
            .is_err()
        );
    }
    for pointer in ["not/a/pointer", "/bad~2escape"] {
        assert!(
            Capture {
                name: "valid".into(),
                pointer: pointer.into(),
                json_pointer: None,
                select: None,
                constraint: None,
            }
            .validate()
            .is_err()
        );
    }
    let solution = Solution {
        calls: vec![call(json!({"id": "${not_captured}"}), true)],
        answer: None,
        answer_template: false,
    };
    assert!(solution.validate_bindings().is_err());
}

#[test]
fn final_answers_render_actual_captures_once_and_preserve_literal_compatibility() {
    let bindings = Bindings::from([
        (
            "dsn".into(),
            json!("https://actual@ingest.example.net/73023"),
        ),
        (
            "untrusted".into(),
            json!("${private}; $(not-a-command) \"quoted\""),
        ),
        ("private".into(), json!("never_expand")),
        ("amount".into(), json!(73023)),
    ]);
    for (template, expected) in [
        ("${dsn}", "https://actual@ingest.example.net/73023"),
        (
            "SENTRY_DSN=${dsn}\n",
            "SENTRY_DSN=https://actual@ingest.example.net/73023\n",
        ),
        ("${untrusted}", "${private}; $(not-a-command) \"quoted\""),
        ("${amount}", "73023"),
    ] {
        let solution = Solution {
            answer: Some(template.into()),
            answer_template: true,
            ..Solution::default()
        };
        assert_eq!(
            solution.resolve_answer(&bindings).unwrap().unwrap(),
            expected
        );
    }
    let literal = Solution {
        answer: Some("${dsn} is literal unless opted in".into()),
        ..Solution::default()
    };
    assert_eq!(
        literal.resolve_answer(&bindings).unwrap().unwrap(),
        "${dsn} is literal unless opted in"
    );
}

#[test]
fn final_answer_templates_require_captured_bindings_and_a_real_template() {
    for template in ["${missing}", "${invalid.name}", "${unclosed"] {
        let solution = Solution {
            answer: Some(template.into()),
            answer_template: true,
            ..Solution::default()
        };
        assert!(solution.validate_bindings().is_err(), "{template}");
        assert!(
            solution.resolve_answer(&Bindings::new()).is_err(),
            "{template}"
        );
    }
    let missing = Solution {
        answer_template: true,
        ..Solution::default()
    };
    assert!(missing.validate_bindings().is_err());
    let mut capture = call(json!({}), false);
    capture.capture = vec![Capture {
        name: "dsn".into(),
        pointer: "/structuredContent/dsn".into(),
        json_pointer: None,
        select: None,
        constraint: None,
    }];
    let valid = Solution {
        calls: vec![capture],
        answer: Some("${dsn}".into()),
        answer_template: true,
    };
    valid.validate_bindings().unwrap();
}

#[test]
fn failed_tools_and_missing_capture_fields_do_not_supply_values() {
    let mut oracle_call = call(json!({}), false);
    oracle_call.capture = vec![Capture {
        name: "refund_id".into(),
        pointer: "/content/0/text".into(),
        json_pointer: Some("/id".into()),
        select: None,
        constraint: None,
    }];
    for result in [
        json!({"isError": true, "content": [{"text": "{\"id\":\"fake\"}"}]}),
        json!({"content": [{"text": "{\"not_id\":1}"}]}),
        json!({"content": [{"text": "not JSON"}]}),
    ] {
        assert!(
            oracle_call
                .capture_result(&result, &mut Bindings::new())
                .is_err()
        );
    }
}

#[test]
fn selections_require_one_exact_string_match_and_commit_no_partial_bindings_on_error() {
    let mut oracle_call = call(json!({}), false);
    oracle_call.capture = serde_json::from_value(json!([{
        "name": "publication", "pointer": "/rows",
        "select": {"matching": {"name": "Online Store"}, "pointer": "/id"}
    }]))
    .unwrap();
    for rows in [
        json!([]),
        json!([{"name": "Not Online Store", "id": "wrong"}]),
        json!([{"name": "Online Store", "id": "one"}, {"name": "Online Store", "id": "two"}]),
        json!([{"name": "Online Store"}]),
    ] {
        let mut bindings = Bindings::new();
        assert!(
            oracle_call
                .capture_result(&json!({"rows": rows}), &mut bindings)
                .is_err()
        );
        assert!(bindings.is_empty());
    }
    let mut invalid = oracle_call.capture[0].clone();
    invalid.select.as_mut().unwrap().matching.clear();
    assert!(invalid.validate().is_err());
}
