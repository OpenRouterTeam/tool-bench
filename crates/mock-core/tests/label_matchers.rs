use std::collections::BTreeMap;

use mock_core::{
    label_matchers::{LabelMatchersGoal, RegexSupportError, compile_label_regex},
    relations::RecordCheckError,
};
use serde_json::{Value, json};

fn goal() -> LabelMatchersGoal {
    LabelMatchersGoal {
        field: "/matchers".into(),
        equals: BTreeMap::from([
            ("alertname".into(), "PostgresReplicationLag".into()),
            ("instance".into(), "pg-replica-2".into()),
        ]),
    }
}

fn base() -> Vec<Value> {
    vec![
        json!({"name":"alertname","value":"PostgresReplicationLag"}),
        json!({"name":"instance","value":"pg-replica-2"}),
    ]
}

#[test]
fn exact_regexes_and_redundant_conjuncts_are_equivalent_not_fixed_array_spellings() {
    let mut variants = vec![base()];
    for pattern in [
        "^pg-replica-2$",
        "^(?:pg-replica-2)$",
        "pg-replica-[2]",
        "\\Qpg-replica-2\\E",
    ] {
        let mut matchers = base();
        matchers[1] = json!({"name":"instance","value":pattern,"isRegex":true});
        variants.push(matchers);
    }
    let mut redundant = base();
    redundant.push(json!({"name":"instance","value":"pg-replica-.*","isRegex":true}));
    redundant.push(json!({"name":"instance","value":"pg-replica-1","isEqual":false}));
    variants.push(redundant);
    let mut tautology = base();
    tautology.push(json!({"name":"other_label","value":"(?s:.*)","isRegex":true}));
    variants.push(tautology);
    for matchers in variants {
        goal().check(&json!({"matchers":matchers})).unwrap();
    }
}

#[test]
fn contradictory_narrower_and_broader_scopes_are_proved_wrong_over_all_values() {
    let mut contradictory = base();
    contradictory.push(json!({"name":"instance","value":"pg-replica-1"}));
    let mut narrower = base();
    narrower.push(json!({"name":"environment","value":"prod"}));
    let mut broad = base();
    broad[1] = json!({"name":"instance","value":"pg-replica-[23]","isRegex":true});
    for matchers in [contradictory, narrower, broad, base()[..1].to_vec()] {
        assert!(matches!(
            goal().check(&json!({"matchers":matchers})),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn full_string_anchoring_and_go_ascii_classes_are_not_rust_unicode_class_guesses() {
    let expression = compile_label_regex("pg-replica-\\d").unwrap();
    assert!(expression.is_match("pg-replica-2"));
    assert!(!expression.is_match("pg-replica-٢"));
    assert!(!expression.is_match("prefix-pg-replica-2"));
    assert!(!expression.is_match("pg-replica-2-suffix"));
    let literal = compile_label_regex("\\Qpg-replica-[2]\\E").unwrap();
    assert!(literal.is_match("pg-replica-[2]"));
    assert!(!literal.is_match("pg-replica-2"));
}

#[test]
fn unsupported_regex_languages_require_adjudication_not_a_fake_false_verdict() {
    let mut matchers = base();
    matchers[1] = json!({"name":"instance","value":"\\p{L}+","isRegex":true});
    assert!(matches!(
        goal().check(&json!({"matchers":matchers})),
        Err(RecordCheckError::NeedsAdjudication(_))
    ));
}

#[test]
fn predicate_equivalence_respects_missing_labels_empty_strings_and_negation() {
    let expected = LabelMatchersGoal {
        field: "/matchers".into(),
        equals: BTreeMap::from([("instance".into(), "".into())]),
    };
    expected
        .check(&json!({"matchers":[{"name":"instance","value":"^$","isRegex":true}]}))
        .unwrap();
    assert!(matches!(
        expected.check(&json!({"matchers":[{"name":"instance","value":"","isEqual":false}]})),
        Err(RecordCheckError::Failed(_))
    ));
}

#[test]
fn malformed_flags_and_empty_predicates_are_not_silently_defaulted() {
    for matchers in [
        json!([]),
        json!([{"name":"instance","value":"pg-replica-2","isRegex":"true"}]),
    ] {
        assert!(matches!(
            goal().check(&json!({"matchers":matchers})),
            Err(RecordCheckError::Failed(_))
        ));
    }
}

#[test]
fn noncanonical_go_braces_cannot_become_a_false_singleton_proof() {
    for pattern in [
        "pg-replica-2{01}",
        "pg-replica-2{1,01}",
        "pg-replica-2{ 1 }",
    ] {
        let mut matchers = base();
        matchers[1] = json!({"name":"instance","value":pattern,"isRegex":true});
        assert!(
            matches!(
                goal().check(&json!({"matchers":matchers})),
                Err(RecordCheckError::NeedsAdjudication(_))
            ),
            "{pattern}"
        );
    }
}

#[test]
fn leading_bracket_and_shorthand_range_ambiguities_are_not_forwarded_as_rust_set_operations() {
    for pattern in ["pg-replica-[]2&&2]", "[\\w-a]", "[\\s-z]", "[0-\\d]"] {
        assert!(
            matches!(
                compile_label_regex(pattern),
                Err(RegexSupportError::Unsupported(_))
            ),
            "{pattern}"
        );
    }
}

#[test]
fn casefolding_and_counted_repetition_require_their_own_qualified_grammar() {
    for pattern in [
        "(?i:k)",
        "(?i:\\x{1C89})",
        "a{1000}",
        "a{1001}",
        "(?:a{100}){11}",
        "pg-replica-2(?:a{1001}){0}",
    ] {
        assert!(
            matches!(
                compile_label_regex(pattern),
                Err(RegexSupportError::Unsupported(_))
            ),
            "{pattern}"
        );
    }
}

#[test]
fn go_escaped_punctuation_remains_literal_rather_than_rust_word_assertions() {
    let expression = compile_label_regex("\\<pg-replica-2\\>").unwrap();
    assert!(expression.is_match("<pg-replica-2>"));
    assert!(!expression.is_match("pg-replica-2"));
    let expression = compile_label_regex("[\\<]").unwrap();
    assert!(expression.is_match("<"));
}

#[test]
fn quotes_groups_and_repetition_cannot_escape_the_full_string_wrapper() {
    assert!(matches!(
        compile_label_regex("\\Qpg-replica-2"),
        Err(RegexSupportError::Unsupported(_))
    ));
    for pattern in [
        "pg-replica-2)|(?:pg-replica-2",
        "a**",
        "a++",
        "a?+",
        "a*?*",
        "a*\\Q\\E*",
    ] {
        assert!(
            matches!(
                compile_label_regex(pattern),
                Err(RegexSupportError::Invalid(_))
            ),
            "{pattern}"
        );
    }
}

#[test]
fn supported_multiline_ascii_classes_and_control_escapes_keep_exact_meaning() {
    let expression = compile_label_regex("(?m:^pg-replica-2$)").unwrap();
    assert!(expression.is_match("pg-replica-2"));
    assert!(!expression.is_match("pg-replica-2\n"));
    assert!(!expression.is_match("x\npg-replica-2"));
    let expression = compile_label_regex("(?m:a$\\n^b)").unwrap();
    assert!(expression.is_match("a\nb"));
    assert!(!expression.is_match("a\r\nb"));
    for pattern in ["[\\d]", "[^\\D]"] {
        let expression = compile_label_regex(pattern).unwrap();
        assert!(expression.is_match("2"));
        assert!(!expression.is_match("٢"));
    }
    let expression = compile_label_regex("[\\D]").unwrap();
    assert!(expression.is_match("٢"));
    assert!(!expression.is_match("2"));
    assert!(compile_label_regex("\\v").unwrap().is_match("\u{b}"));
    assert!(!compile_label_regex("\\s").unwrap().is_match("\u{b}"));
    assert!(compile_label_regex("\\S").unwrap().is_match("\u{b}"));
}
