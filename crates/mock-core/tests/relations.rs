use mock_core::relations::{RecordCheck, RecordCheckError, RelationCheck};
use serde_json::{Value, json};

fn initial() -> Value {
    json!({"services": {
        "stripe": {"refunds": {
            "re_old": {"id": "re_old", "charge": "ch_other", "status": "succeeded"}
        }},
        "zendesk": {"comments": {
            "5102/1": {"public": true, "body": "Original request"},
            "5104/1": {"public": true, "body": "Refund re_other was issued"}
        }}
    }})
}

fn completed(refund_id: &str) -> Value {
    let mut state = initial();
    state["services"]["stripe"]["refunds"][refund_id] =
        json!({"id": refund_id, "charge": "ch_duplicate", "status": "succeeded", "amount": 48000});
    state["services"]["zendesk"]["comments"]["5102/2"] =
        json!({"public": true, "body": format!("Your refund {refund_id} was issued.")});
    state
}

fn reference_check() -> RelationCheck {
    serde_json::from_value(json!({
        "source": {
            "collection": "/services/stripe/refunds",
            "matching": {"charge": "ch_duplicate", "status": "succeeded", "amount": 48000},
            "created": true
        },
        "source_field": "/id",
        "target": {
            "collection": "/services/zendesk/comments",
            "matching": {"public": true},
            "key_prefix": "5102/",
            "created": true
        },
        "target_field": "/body",
        "kind": "contains_reference"
    }))
    .unwrap()
}

#[test]
fn optional_bijections_allow_only_a_deliberately_declared_empty_pair() {
    let definition = json!({
        "source":{"collection":"/services/kubernetes/secrets","matching":{"name":"target"}},
        "source_field":"/status",
        "target":{"collection":"/services/kubernetes/releases","matching":{"name":"target"}},
        "target_field":"/status","kind":"bijection","allow_empty":true
    });
    let check: RelationCheck = serde_json::from_value(definition.clone()).unwrap();
    let empty = json!({"services":{"kubernetes":{"secrets":{},"releases":{}}}});
    check.check(None, &empty).unwrap();
    let mut required = check.clone();
    required.allow_empty = false;
    assert!(matches!(
        required.check(None, &empty),
        Err(RecordCheckError::Failed(_))
    ));
    for (source, target, accepted) in [
        (
            json!({"name":"target","status":"uninstalled"}),
            Value::Null,
            false,
        ),
        (
            Value::Null,
            json!({"name":"target","status":"uninstalled"}),
            false,
        ),
        (
            json!({"name":"target","status":"deployed"}),
            json!({"name":"target","status":"uninstalled"}),
            false,
        ),
        (
            json!({"name":"target","status":"uninstalled"}),
            json!({"name":"target","status":"uninstalled"}),
            true,
        ),
    ] {
        let mut state = empty.clone();
        if !source.is_null() {
            state["services"]["kubernetes"]["secrets"]["actual"] = source;
        }
        if !target.is_null() {
            state["services"]["kubernetes"]["releases"]["actual"] = target;
        }
        assert_eq!(check.check(None, &state).is_ok(), accepted, "{state}");
    }
    for kind in [
        "foreign_key",
        "key_reference",
        "equals",
        "numeric_equals",
        "contains_reference",
    ] {
        let mut invalid = definition.clone();
        invalid["kind"] = json!(kind);
        if kind == "key_reference" {
            invalid["target_field"] = json!("");
        }
        let invalid: RelationCheck = serde_json::from_value(invalid).unwrap();
        assert!(invalid.validate().is_err(), "{kind}");
    }
}

#[test]
fn distinct_identity_counts_bind_actual_target_quantities_not_fixture_constants() {
    let check: RelationCheck = serde_json::from_value(json!({
        "source":{"collection":"/services/kubernetes/pods","matching":{"ready":true}},
        "source_field":"/uid",
        "target":{"collection":"/services/kubernetes/deployments","matching":{"name":"worker"}},
        "target_field":"/replicas","kind":"count_equals"
    }))
    .unwrap();
    for replicas in [json!(2), json!(2.0)] {
        let state = json!({"services":{"kubernetes":{
            "pods":{"one":{"uid":"actual-one","ready":true},"two":{"uid":"actual-two","ready":true},
                "unready":{"uid":"not-ready","ready":false}},
            "deployments":{"worker":{"name":"worker","replicas":replicas}}
        }}});
        check.check(None, &state).unwrap();
        for wrong in [
            json!(1),
            json!(3),
            json!(-1),
            json!(1.5),
            json!("2"),
            Value::Null,
        ] {
            let mut wrong_state = state.clone();
            wrong_state["services"]["kubernetes"]["deployments"]["worker"]["replicas"] = wrong;
            assert!(check.check(None, &wrong_state).is_err());
        }
        let mut duplicate = state.clone();
        duplicate["services"]["kubernetes"]["pods"]["two"]["uid"] = json!("actual-one");
        assert!(check.check(None, &duplicate).is_err());
    }
}

#[test]
fn zero_count_relations_still_require_one_real_target_and_reject_ambiguous_targets() {
    let check: RelationCheck = serde_json::from_value(json!({
        "source":{"collection":"/services/kubernetes/pods","matching":{"ready":true}},
        "source_field":"/uid",
        "target":{"collection":"/services/kubernetes/deployments","matching":{"name":"worker"}},
        "target_field":"/replicas","kind":"count_equals"
    }))
    .unwrap();
    let mut state = json!({"services":{"kubernetes":{
        "pods":{},"deployments":{"worker":{"name":"worker","replicas":0}}
    }}});
    check.check(None, &state).unwrap();
    state["services"]["kubernetes"]["deployments"]["duplicate"] =
        json!({"name":"worker","replicas":0});
    assert!(check.check(None, &state).is_err());
    state["services"]["kubernetes"]["deployments"] = json!({});
    assert!(check.check(None, &state).is_err());
}

#[test]
fn many_children_join_fields_to_their_actual_foreign_key_parent() {
    let check: RelationCheck = serde_json::from_value(json!({
        "source":{"collection":"/services/kubernetes/pods","matching":{"ready":true}},
        "source_field":"/controller",
        "target":{"collection":"/services/kubernetes/replicasets","matching":{"name":"worker"}},
        "target_field":"/uid","kind":"foreign_key",
        "fields":[{"source_field":"/containers","target_field":"/template/containers","kind":"equals"}]
    })).unwrap();
    let mut state = json!({"services":{"kubernetes":{
        "pods":{"one":{"uid":"one","controller":"actual-parent","ready":true,"containers":[{"image":"correct"}]},
            "two":{"uid":"two","controller":"actual-parent","ready":true,"containers":[{"image":"correct"}]}},
        "replicasets":{"parent":{"uid":"actual-parent","name":"worker","template":{"containers":[{"image":"correct"}]}}}
    }}});
    check.check(None, &state).unwrap();
    state["services"]["kubernetes"]["pods"]["two"]["containers"][0]["image"] = json!("wrong");
    assert!(check.check(None, &state).is_err());
    state["services"]["kubernetes"]["pods"]["two"]["containers"][0]["image"] = json!("correct");
    state["services"]["kubernetes"]["pods"]["two"]["controller"] = json!("unrelated-parent");
    assert!(check.check(None, &state).is_err());
}

#[test]
fn references_follow_the_actual_generated_record_instead_of_the_seed_sequence() {
    for refund_id in ["re_generated_first", "re_generated_after_other_operations"] {
        reference_check()
            .check(Some(&initial()), &completed(refund_id))
            .unwrap();
    }
}

#[test]
fn an_unrelated_refund_reference_does_not_satisfy_the_relation() {
    let mut state = completed("re_actual");
    state["services"]["zendesk"]["comments"]["5102/2"]["body"] =
        json!("Your refund re_old was issued.");
    assert!(reference_check().check(Some(&initial()), &state).is_err());
}

#[test]
fn identity_prefixes_suffixes_and_other_tickets_do_not_match() {
    for text in [
        "The refund is re_actual_other.",
        "The refund is prefixre_actual.",
        "The refund is re_actual-other.",
        "The refund is re_actual好.",
    ] {
        let mut state = completed("re_actual");
        state["services"]["zendesk"]["comments"]["5102/2"]["body"] = json!(text);
        assert!(
            reference_check().check(Some(&initial()), &state).is_err(),
            "{text}"
        );
    }
    let mut state = completed("re_actual");
    let comment = state["services"]["zendesk"]["comments"]
        .as_object_mut()
        .unwrap()
        .remove("5102/2")
        .unwrap();
    state["services"]["zendesk"]["comments"]["5104/2"] = comment;
    assert!(reference_check().check(Some(&initial()), &state).is_err());
}

#[test]
fn duplicate_matching_refunds_are_not_resolved_by_choosing_the_first() {
    let mut state = completed("re_actual");
    state["services"]["stripe"]["refunds"]["re_duplicate"] =
        state["services"]["stripe"]["refunds"]["re_actual"].clone();
    let error = reference_check()
        .check(Some(&initial()), &state)
        .unwrap_err();
    assert!(error.to_string().contains("2 records"));
}

#[test]
fn a_preexisting_comment_cannot_be_reused_as_a_new_customer_reply() {
    let mut original = initial();
    original["services"]["zendesk"]["comments"]["5102/1"]["body"] =
        json!("Your refund re_actual was issued.");
    let mut state = completed("re_actual");
    state["services"]["zendesk"]["comments"] = original["services"]["zendesk"]["comments"].clone();
    assert!(reference_check().check(Some(&original), &state).is_err());
}

#[test]
fn created_record_checks_require_initial_evidence() {
    assert!(matches!(
        reference_check().check(None, &completed("re_actual")),
        Err(RecordCheckError::MissingEvidence(_))
    ));
}

#[test]
fn exact_field_relations_preserve_type_and_reject_missing_fields() {
    let mut check = reference_check();
    check.kind = mock_core::relations::RelationKind::Equals;
    check.source_field = "/amount".into();
    check.target_field = "/amount".into();
    let mut state = completed("re_actual");
    assert!(check.check(Some(&initial()), &state).is_err());
    state["services"]["zendesk"]["comments"]["5102/2"]["amount"] = json!("48000");
    assert!(check.check(Some(&initial()), &state).is_err());
    state["services"]["zendesk"]["comments"]["5102/2"]["amount"] = json!(48000);
    check.check(Some(&initial()), &state).unwrap();
}

#[test]
fn cardinality_checks_can_require_one_created_record_or_no_new_records() {
    let mut check: RecordCheck = serde_json::from_value(json!({
        "selector": {
            "collection": "/services/stripe/refunds",
            "matching": {"charge": "ch_duplicate"},
            "created": true
        },
        "equals": 1
    }))
    .unwrap();
    check
        .check(Some(&initial()), &completed("re_actual"))
        .unwrap();
    assert!(check.check(Some(&initial()), &initial()).is_err());
    check.equals = Some(0);
    check.check(Some(&initial()), &initial()).unwrap();
    assert!(
        check
            .check(Some(&initial()), &completed("re_actual"))
            .is_err()
    );
}

#[test]
fn malformed_or_unbounded_selectors_are_rejected() {
    for selector in [
        json!({"collection": "/services/stripe"}),
        json!({"collection": "/services/stripe/refunds"}),
        json!({"collection": "/services/*/refunds", "created": true}),
        json!({"collection": "/services/stripe/refunds", "key_prefix": ""}),
        json!({"collection": "services/stripe/refunds", "created": true}),
    ] {
        let check: RecordCheck =
            serde_json::from_value(json!({"selector": selector, "equals": 0})).unwrap();
        assert!(check.validate().is_err());
    }
}

#[test]
fn literal_record_fields_are_exact_not_subset_matches() {
    let check: RecordCheck = serde_json::from_value(json!({
        "selector": {
            "collection": "/services/stripe/refunds",
            "matching": {"charge": "ch_duplicate"},
            "created": true
        },
        "equals": 1,
        "fields": {"/metadata": {}, "/reason": "duplicate"}
    }))
    .unwrap();
    let mut state = completed("re_actual");
    state["services"]["stripe"]["refunds"]["re_actual"]["metadata"] = json!({});
    state["services"]["stripe"]["refunds"]["re_actual"]["reason"] = json!("duplicate");
    check.check(Some(&initial()), &state).unwrap();
    state["services"]["stripe"]["refunds"]["re_actual"]["metadata"] =
        json!({"unrequested": "extra data"});
    assert!(check.check(Some(&initial()), &state).is_err());
}
