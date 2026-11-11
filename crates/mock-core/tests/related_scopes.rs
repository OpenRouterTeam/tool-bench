use mock_core::{
    preservation::Preservation,
    relations::{RecordCheck, RecordSelector},
};
use serde_json::{Value, json};

fn initial() -> Value {
    json!({"services":{"sales":{
        "customers":{
            "target":{"id":7101,"name":"Priya Shah","email":"priya.shah@fastmail.com"},
            "other":{"id":7102,"name":"Priya Shah","email":"priya.shah@acme.io"}
        },
        "orders":{
            "10":{"id":10,"customer_id":7101,"status":"pending","total":89.9},
            "11":{"id":11,"customer_id":7101,"status":"pending","total":45},
            "12":{"id":12,"customer_id":7101,"status":"paid","total":120},
            "13":{"id":13,"customer_id":7102,"status":"pending","total":300}
        }
    }}})
}

fn customer_reference() -> Value {
    json!({
        "source_field":"/customer_id",
        "target":{"collection":"/services/sales/customers",
            "matching":{"email":"priya.shah@fastmail.com"}},
        "target_field":"/id"
    })
}

fn definition() -> Value {
    json!({"allow":[{
        "pointer":"/services/sales/orders/*/status",
        "matching":{"status":"cancelled"},
        "initial_matching":{"status":"pending"},
        "initial_references":[customer_reference()],
        "max_count":"all_matching"
    }]})
}

fn remaining() -> RecordCheck {
    serde_json::from_value(json!({
        "selector":{"collection":"/services/sales/orders",
            "matching":{"status":"pending"},"references":[customer_reference()]},
        "equals":0
    }))
    .unwrap()
}

#[test]
fn baseline_related_permissions_and_goals_follow_actual_customer_identity() {
    let policy: Preservation = serde_json::from_value(definition()).unwrap();
    let goal = remaining();
    let initial = initial();
    let selected: RecordSelector = goal.selector.clone();
    assert_eq!(
        selected
            .selected_identities(Some(&initial), &initial)
            .unwrap(),
        ["10", "11"]
    );
    let mut completed = initial.clone();
    completed["services"]["sales"]["orders"]["10"]["status"] = json!("cancelled");
    assert!(goal.check(Some(&initial), &completed).is_err());
    completed["services"]["sales"]["orders"]["11"]["status"] = json!("cancelled");
    goal.check(Some(&initial), &completed).unwrap();
    assert!(policy.check(&initial, &completed).unwrap().is_empty());
    for protected in ["12", "13"] {
        let mut collateral = completed.clone();
        collateral["services"]["sales"]["orders"][protected]["status"] = json!("cancelled");
        assert!(!policy.check(&initial, &collateral).unwrap().is_empty());
    }
}

#[test]
fn newly_created_child_permissions_follow_actual_parent_identity_not_a_predicted_slug() {
    let initial = json!({"services":{"sentry":{
        "projects":{"existing":{"id":"70","slug":"existing","name":"Unrelated"}},
        "keys":{}
    }}});
    let definition = json!({"allow":[
        {"pointer":"/services/sentry/projects/*","kind":"create","matching":{"name":"Target"}},
        {"pointer":"/services/sentry/keys/*","kind":"create","references":[{
            "source_field":"/project",
            "target":{"collection":"/services/sentry/projects","created":true,"matching":{"name":"Target"}},
            "target_field":"/slug"
        }]}
    ]});
    let policy: Preservation = serde_json::from_value(definition.clone()).unwrap();
    policy.validate().unwrap();
    let mut completed = initial.clone();
    completed["services"]["sentry"]["projects"]["actual"] =
        json!({"id":"91","slug":"actual-name-not-predicted","name":"Target"});
    completed["services"]["sentry"]["keys"]["actual-key"] =
        json!({"project":"actual-name-not-predicted","public":"returned-key"});
    assert!(policy.check(&initial, &completed).unwrap().is_empty());
    for parent in ["existing", "missing", "notification-service"] {
        let mut wrong = completed.clone();
        wrong["services"]["sentry"]["keys"]["actual-key"]["project"] = json!(parent);
        assert!(
            !policy.check(&initial, &wrong).unwrap().is_empty(),
            "{parent}"
        );
    }
    let mut duplicate = definition.clone();
    let repeated = duplicate["allow"][1]["references"][0].clone();
    duplicate["allow"][1]["references"]
        .as_array_mut()
        .unwrap()
        .push(repeated);
    let duplicate: Preservation = serde_json::from_value(duplicate).unwrap();
    assert!(duplicate.validate().is_err());
    let mut unsupported = definition;
    unsupported["allow"][1]["kind"] = json!("append");
    let unsupported: Preservation = serde_json::from_value(unsupported).unwrap();
    assert!(unsupported.validate().is_err());
}

#[test]
fn related_scopes_accept_renumbered_targets_and_more_than_the_authored_row_count() {
    let policy: Preservation = serde_json::from_value(definition()).unwrap();
    let mut initial = initial();
    initial["services"]["sales"]["customers"]["target"]["id"] = json!(91042);
    for order in ["10", "11", "12"] {
        initial["services"]["sales"]["orders"][order]["customer_id"] = json!(91042);
    }
    let template = initial["services"]["sales"]["orders"]["11"].clone();
    let mut eligible = vec!["10".to_owned(), "11".to_owned()];
    for index in 0..37 {
        let identity = (1000 + index).to_string();
        let mut order = template.clone();
        order["id"] = json!(1000 + index);
        initial["services"]["sales"]["orders"][&identity] = order;
        eligible.push(identity);
    }
    let mut completed = initial.clone();
    for identity in &eligible {
        completed["services"]["sales"]["orders"][identity]["status"] = json!("cancelled");
    }
    assert_eq!(eligible.len(), 39);
    remaining().check(Some(&initial), &completed).unwrap();
    assert!(policy.check(&initial, &completed).unwrap().is_empty());
    let mut bounded = definition();
    bounded["allow"][0]["max_count"] = json!(2);
    let bounded: Preservation = serde_json::from_value(bounded).unwrap();
    assert!(!bounded.check(&initial, &completed).unwrap().is_empty());
}

#[test]
fn related_initial_permissions_do_not_rebind_after_an_authorized_lookup_change() {
    let mut definition = definition();
    for customer in ["target", "other"] {
        definition["allow"].as_array_mut().unwrap().push(json!({
            "pointer":format!("/services/sales/customers/{customer}/email")
        }));
    }
    let policy: Preservation = serde_json::from_value(definition).unwrap();
    let initial = initial();
    let mut moved = initial.clone();
    moved["services"]["sales"]["customers"]["target"]["email"] = json!("prior@fastmail.com");
    moved["services"]["sales"]["customers"]["other"]["email"] = json!("priya.shah@fastmail.com");
    assert!(policy.check(&initial, &moved).unwrap().is_empty());
    moved["services"]["sales"]["orders"]["13"]["status"] = json!("cancelled");
    assert!(!policy.check(&initial, &moved).unwrap().is_empty());
}

#[test]
fn related_selection_preserves_identity_types_and_rejects_duplicate_targets() {
    let mut check = remaining();
    check.selector.matching.clear();
    let mut state = initial();
    assert_eq!(
        check.selector.selected_identities(None, &state).unwrap(),
        ["10", "11", "12"]
    );
    state["services"]["sales"]["orders"]["11"]["customer_id"] = json!("7101");
    assert_eq!(
        check.selector.selected_identities(None, &state).unwrap(),
        ["10", "12"]
    );
    let duplicate = state["services"]["sales"]["customers"]["target"].clone();
    state["services"]["sales"]["customers"]["duplicate"] = duplicate;
    assert!(check.selector.selected_identities(None, &state).is_err());
}

#[test]
fn malformed_initial_scopes_and_unbounded_update_permissions_are_invalid() {
    for matching in [json!({}), json!(null), json!([]), json!("pending")] {
        let mut invalid = definition();
        invalid["allow"][0]["initial_matching"] = matching;
        let policy: Preservation = serde_json::from_value(invalid).unwrap();
        assert!(policy.validate().is_err());
    }
    for kind in ["create", "append", "remove_field", "restore_field"] {
        let mut invalid = definition();
        invalid["allow"][0]["kind"] = json!(kind);
        let policy: Preservation = serde_json::from_value(invalid).unwrap();
        assert!(policy.validate().is_err(), "{kind}");
    }
    let mut invalid = definition();
    invalid["allow"][0]
        .as_object_mut()
        .unwrap()
        .remove("initial_matching");
    invalid["allow"][0]
        .as_object_mut()
        .unwrap()
        .remove("initial_references");
    let policy: Preservation = serde_json::from_value(invalid).unwrap();
    assert!(policy.validate().is_err());
    let mut invalid = definition();
    invalid["allow"][0]["initial_references"][0]["target"]["created"] = json!(true);
    let policy: Preservation = serde_json::from_value(invalid).unwrap();
    assert!(policy.validate().is_err());
}

#[test]
fn invalid_reference_paths_duplicates_and_excessive_nesting_are_rejected() {
    for field in ["", "/customer_id/*"] {
        let mut selector = remaining().selector;
        selector.references[0].source_field = field.to_owned();
        assert!(selector.validate().is_err());
    }
    let mut selector = remaining().selector;
    selector.references.push(selector.references[0].clone());
    assert!(selector.validate().is_err());
    let mut nested = json!({"collection":"/services/sales/customers","matching":{"id":7101}});
    for _ in 0..17 {
        nested = json!({"collection":"/services/sales/customers","matching":{"id":7101},
            "references":[{"source_field":"/id","target":nested,"target_field":"/id"}]});
    }
    let selector: RecordSelector = serde_json::from_value(nested).unwrap();
    assert!(selector.validate().is_err());
    let selector: RecordSelector = serde_json::from_value(json!({
        "collection":"/services/sales/orders","matching":{"status":"pending"}
    }))
    .unwrap();
    assert!(
        serde_json::to_value(selector)
            .unwrap()
            .get("references")
            .is_none()
    );
}
