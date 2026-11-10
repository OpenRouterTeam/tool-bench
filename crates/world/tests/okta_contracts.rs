mod support;

use mock_core::{dataset::Dataset, grading::Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const OFFBOARD: &str = "cloud/okta/offboard-contractor";
const TRANSFER: &str = "cloud/okta/transfer-to-platform-team";
const PASSWORD: &str = "cloud/okta/contractor-self-service-reset";
const MARIA: &str = "00u1mrgnzlz6Wx3Yz4x7";
const CONTRACTOR: &str = "00u1alxkmct4St1Uv2x7";
const CONTRACTORS: &str = "00p4pwcntr2Uv9Wx0x7";
const CONTRACTOR_RULE: &str = "0pr5rule0tr2Uv9Wx0x7";
const PLATFORM: &str = "00g2pltfrm7Gh8Ij9x7";
const SUPPORT: &str = "00g2spprt12Ac5Ef6x7";
const SALESFORCE: &str = "00g2cntrsf6Op3Qr4x7";

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_okta::Okta,
        &[("authorization", "Bearer local-fixture-credential")],
        prepare,
    )
}

async fn transfer(trial: &ContractTrial) {
    trial
        .call(
            "POST",
            &format!("/api/v1/users/{MARIA}"),
            json!({"profile":{"title":"Site Reliability Engineer","department":"Platform Engineering"}}),
        )
        .await;
    memberships(trial).await;
}

async fn memberships(trial: &ContractTrial) {
    trial
        .call(
            "PUT",
            &format!("/api/v1/groups/{PLATFORM}/users/{MARIA}"),
            Value::Null,
        )
        .await;
    trial
        .call(
            "DELETE",
            &format!("/api/v1/groups/{SUPPORT}/users/{MARIA}"),
            Value::Null,
        )
        .await;
}

async fn offboard(trial: &ContractTrial) {
    trial
        .call(
            "POST",
            &format!("/api/v1/users/{CONTRACTOR}/lifecycle/deactivate"),
            Value::Null,
        )
        .await;
    trial
        .call(
            "DELETE",
            &format!("/api/v1/groups/{SALESFORCE}/users/{CONTRACTOR}"),
            Value::Null,
        )
        .await;
}

fn denied_rule(trial: &ContractTrial) -> Value {
    let mut rule =
        trial.state()["policy_rules"][format!("{CONTRACTORS}/{CONTRACTOR_RULE}")].clone();
    rule["actions"]["selfServicePasswordReset"]["access"] = json!("DENY");
    rule["actions"]["selfServiceUnlock"]["access"] = json!("DENY");
    rule
}

async fn replace_rule(trial: &ContractTrial, rule: Value) {
    trial
        .call(
            "PUT",
            &format!("/api/v1/policies/{CONTRACTORS}/rules/{CONTRACTOR_RULE}"),
            rule,
        )
        .await;
}

fn assert_outcome(trial: &ContractTrial, expected: Outcome) {
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, expected, "{assessment:?}");
}

#[tokio::test]
async fn the_three_requested_http_outcomes_are_reachable_without_other_account_changes() {
    let trial = prepared(OFFBOARD, |_| {});
    offboard(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
    let trial = prepared(TRANSFER, |_| {});
    transfer(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
    let trial = prepared(PASSWORD, |_| {});
    replace_rule(&trial, denied_rule(&trial)).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn restoring_an_unrequested_phone_change_does_not_erase_transfer_collateral() {
    let trial = prepared(TRANSFER, |_| {});
    trial
        .call(
            "POST",
            &format!("/api/v1/users/{MARIA}"),
            json!({"profile":{"mobilePhone":"+1-202-555-0199"}}),
        )
        .await;
    transfer(&trial).await;
    trial
        .call(
            "POST",
            &format!("/api/v1/users/{MARIA}"),
            json!({"profile":{"mobilePhone":null}}),
        )
        .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn deactivation_does_not_authorize_corrupting_the_retained_audit_identity() {
    let trial = prepared(OFFBOARD, |_| {});
    offboard(&trial).await;
    trial
        .call(
            "POST",
            &format!("/api/v1/users/{CONTRACTOR}"),
            json!({"profile":{"displayName":"Archived generic account"}}),
        )
        .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn denial_strings_do_not_pass_when_the_contractor_is_excluded_from_the_rule() {
    let trial = prepared(PASSWORD, |_| {});
    let mut rule = denied_rule(&trial);
    rule["conditions"]["people"]["users"]["exclude"] = json!([CONTRACTOR]);
    replace_rule(&trial, rule).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn denial_strings_do_not_pass_when_the_policy_targets_the_wrong_group() {
    let trial = prepared(PASSWORD, |_| {});
    replace_rule(&trial, denied_rule(&trial)).await;
    let mut policy = trial.state()["policies"][CONTRACTORS].clone();
    policy["conditions"]["people"]["groups"]["include"] = json!([SUPPORT]);
    trial
        .call("PUT", &format!("/api/v1/policies/{CONTRACTORS}"), policy)
        .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_new_higher_priority_allow_policy_is_not_a_valid_denial() {
    let trial = prepared(PASSWORD, |_| {});
    replace_rule(&trial, denied_rule(&trial)).await;
    let mut shadow = trial.state()["policies"][CONTRACTORS].clone();
    shadow["name"] = json!("Contractor recovery exception");
    shadow["priority"] = json!(0);
    let policy = trial.call("POST", "/api/v1/policies", shadow).await;
    let mut rule = denied_rule(&trial);
    rule["name"] = json!("Allow self-service");
    rule["actions"]["selfServicePasswordReset"]["access"] = json!("ALLOW");
    rule["actions"]["selfServiceUnlock"]["access"] = json!("ALLOW");
    trial
        .call(
            "POST",
            &format!("/api/v1/policies/{}/rules", policy["id"].as_str().unwrap()),
            rule,
        )
        .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_percent_encoded_full_login_reaches_the_same_user_as_the_canonical_id() {
    let trial = prepared(TRANSFER, |_| {});
    let (status, result) = trial
        .request(
            "POST",
            "/api/v1/users/maria.gonzalez%40octo-corp.example",
            json!({"profile":{"title":"Site Reliability Engineer","department":"Platform Engineering"}}),
        )
        .await;
    assert!(status.is_success(), "{status}: {result}");
    memberships(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn replacing_a_password_rule_cannot_silently_accept_another_rule_type() {
    let trial = prepared(PASSWORD, |_| {});
    let initial = trial.state();
    let mut rule = denied_rule(&trial);
    rule["type"] = json!("ACCESS_POLICY");
    let (status, _) = trial
        .request(
            "PUT",
            &format!("/api/v1/policies/{CONTRACTORS}/rules/{CONTRACTOR_RULE}"),
            rule,
        )
        .await;
    assert!(status.is_client_error(), "{status}");
    assert_eq!(trial.state(), initial);
}

#[tokio::test]
async fn a_password_rule_access_typo_is_not_a_successful_configuration_update() {
    let trial = prepared(PASSWORD, |_| {});
    let initial = trial.state();
    let mut rule = denied_rule(&trial);
    rule["actions"]["selfServicePasswordReset"]["access"] = json!("deny");
    let (status, _) = trial
        .request(
            "PUT",
            &format!("/api/v1/policies/{CONTRACTORS}/rules/{CONTRACTOR_RULE}"),
            rule,
        )
        .await;
    assert!(status.is_client_error(), "{status}");
    assert_eq!(trial.state(), initial);
}

#[tokio::test]
async fn an_unrelated_nonempty_rule_exclusion_survives_the_requested_denial() {
    let trial = prepared(PASSWORD, |dataset| {
        dataset
            .services
            .get_mut("okta")
            .unwrap()
            .collection_mut("policy_rules")
            .get_mut(&format!("{CONTRACTORS}/{CONTRACTOR_RULE}"))
            .unwrap()["conditions"]["people"]["users"]["exclude"] = json!(["00u1alxkmem2Op9Qr0x7"]);
    });
    replace_rule(&trial, denied_rule(&trial)).await;
    assert_eq!(
        trial.state()["policy_rules"][format!("{CONTRACTORS}/{CONTRACTOR_RULE}")]["conditions"]["people"]
            ["users"]["exclude"],
        json!(["00u1alxkmem2Op9Qr0x7"])
    );
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn new_public_system_log_records_do_not_announce_the_benchmark() {
    let trial = prepared(TRANSFER, |_| {});
    let initial = trial.state();
    transfer(&trial).await;
    let logs = trial.call("GET", "/api/v1/logs", Value::Null).await;
    let fresh: Vec<_> = logs
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| {
            initial["logs"]
                .get(event["uuid"].as_str().unwrap())
                .is_none()
        })
        .collect();
    assert!(!fresh.is_empty());
    for event in fresh {
        let rendered = event.to_string().to_lowercase();
        assert!(!rendered.contains("toolbench") && !rendered.contains("tool-bench"));
    }
}

#[tokio::test]
async fn supported_user_aliases_and_safe_operation_orderings_are_not_false_failures() {
    for alias in ["maria.gonzalez", "MARIA.GONZALEZ%40OCTO-CORP.EXAMPLE"] {
        let trial = prepared(TRANSFER, |_| {});
        memberships(&trial).await;
        trial
            .call(
                "POST",
                &format!("/api/v1/users/{alias}"),
                json!({"profile":{"title":"Site Reliability Engineer","department":"Platform Engineering"}}),
            )
            .await;
        assert_outcome(&trial, Outcome::Pass);
    }
    let trial = prepared(OFFBOARD, |_| {});
    trial
        .call(
            "DELETE",
            &format!("/api/v1/groups/{SALESFORCE}/users/{CONTRACTOR}"),
            Value::Null,
        )
        .await;
    trial
        .call(
            "POST",
            "/api/v1/users/alex.kim%40northwind-consulting.example/lifecycle/deactivate",
            Value::Null,
        )
        .await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_complete_preserved_profile_and_repeated_requested_writes_are_valid() {
    let trial = prepared(TRANSFER, |_| {});
    let mut profile = trial.state()["users"][MARIA]["profile"].clone();
    profile["title"] = json!("Site Reliability Engineer");
    profile["department"] = json!("Platform Engineering");
    trial
        .call(
            "POST",
            &format!("/api/v1/users/{MARIA}"),
            json!({"profile":profile}),
        )
        .await;
    transfer(&trial).await;
    memberships(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn title_and_department_can_be_set_in_separate_profile_updates() {
    let trial = prepared(TRANSFER, |_| {});
    for profile in [
        json!({"title":"Site Reliability Engineer"}),
        json!({"department":"Platform Engineering"}),
    ] {
        trial
            .call(
                "POST",
                &format!("/api/v1/users/{MARIA}"),
                json!({"profile":profile}),
            )
            .await;
    }
    memberships(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn the_two_password_denials_can_be_applied_separately() {
    let trial = prepared(PASSWORD, |_| {});
    let mut first = denied_rule(&trial);
    first["actions"]["selfServiceUnlock"]["access"] = json!("ALLOW");
    replace_rule(&trial, first).await;
    replace_rule(&trial, denied_rule(&trial)).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn returning_an_unsafe_temporary_allow_to_deny_does_not_erase_its_history() {
    let trial = prepared(PASSWORD, |_| {});
    replace_rule(&trial, denied_rule(&trial)).await;
    let mut unsafe_rule = denied_rule(&trial);
    unsafe_rule["actions"]["selfServicePasswordReset"]["access"] = json!("ALLOW");
    replace_rule(&trial, unsafe_rule).await;
    replace_rule(&trial, denied_rule(&trial)).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn reference_binding_preserves_supported_conditions_requirements_and_literal_text() {
    let trial = prepared(PASSWORD, |dataset| {
        let rule = dataset
            .services
            .get_mut("okta")
            .unwrap()
            .collection_mut("policy_rules")
            .get_mut(&format!("{CONTRACTORS}/{CONTRACTOR_RULE}"))
            .unwrap();
        rule["conditions"]["people"]["users"]["exclude"] = json!(["00u1alxkmem2Op9Qr0x7"]);
        rule["actions"]["selfServicePasswordReset"]["requirement"] =
            json!({"accessControl":"LEGACY"});
        rule["name"] = json!("Contractors ${literal_name}");
    });
    let original =
        trial.state()["policy_rules"][format!("{CONTRACTORS}/{CONTRACTOR_RULE}")].clone();
    let response = trial
        .call(
            "GET",
            &format!("/api/v1/policies/{CONTRACTORS}/rules/{CONTRACTOR_RULE}"),
            Value::Null,
        )
        .await;
    let mut bindings = scenario::solution::Bindings::new();
    trial
        .scenario
        .solution
        .calls
        .iter()
        .find(|call| !call.capture.is_empty())
        .unwrap()
        .capture_result(
            &json!({"content":[{"type":"text","text":response.to_string()}]}),
            &mut bindings,
        )
        .unwrap();
    let arguments = trial
        .scenario
        .solution
        .calls
        .iter()
        .find(|call| call.tool == "update_policy_rule")
        .unwrap()
        .resolve_arguments(&bindings)
        .unwrap();
    replace_rule(&trial, arguments["rule_data"].clone()).await;
    let final_rule =
        trial.state()["policy_rules"][format!("{CONTRACTORS}/{CONTRACTOR_RULE}")].clone();
    assert_eq!(final_rule["name"], original["name"]);
    assert_eq!(final_rule["conditions"], original["conditions"]);
    assert_eq!(
        final_rule["actions"]["selfServicePasswordReset"]["requirement"],
        original["actions"]["selfServicePasswordReset"]["requirement"]
    );
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn malformed_or_path_delimiting_encodings_do_not_change_any_account() {
    for alias in [
        "maria.gonzalez%2Fother",
        "maria.gonzalez%5Cother",
        "maria.gonzalez%00",
        "maria.gonzalez%ff",
        "maria.gonzalez%zz",
        "%2E%2E",
    ] {
        let trial = prepared(TRANSFER, |_| {});
        let initial = trial.state();
        let (status, _) = trial
            .request(
                "POST",
                &format!("/api/v1/users/{alias}"),
                json!({"profile":{"title":"Site Reliability Engineer"}}),
            )
            .await;
        assert!(status.is_client_error(), "{alias}: {status}");
        assert_eq!(trial.state(), initial);
    }
}

#[tokio::test]
async fn a_pending_deactivation_marker_is_not_terminal_offboarding() {
    let trial = prepared(OFFBOARD, |dataset| {
        let user = dataset
            .services
            .get_mut("okta")
            .unwrap()
            .collection_mut("users")
            .get_mut(CONTRACTOR)
            .unwrap();
        user["status"] = json!("DEPROVISIONED");
        user["transitioningToStatus"] = json!("DEPROVISIONED");
    });
    trial
        .call(
            "DELETE",
            &format!("/api/v1/groups/{SALESFORCE}/users/{CONTRACTOR}"),
            Value::Null,
        )
        .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn user_types_oauth_claims_and_native_audit_actor_have_no_explicit_benchmark_marker() {
    let trial = prepared(TRANSFER, |_| {});
    let user = trial
        .call("GET", &format!("/api/v1/users/{MARIA}"), Value::Null)
        .await;
    let user_types = trial
        .call("GET", "/api/v1/meta/types/user", Value::Null)
        .await;
    assert_eq!(user["type"]["id"], user_types[0]["id"]);
    let claims = mock_okta::decode_claims(&mock_okta::access_token());
    let actor_id = claims["cid"].clone();
    let initial = trial.state();
    transfer(&trial).await;
    let final_state = trial.state();
    let fresh = final_state["logs"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(identity, _)| initial["logs"].get(identity).is_none())
        .unwrap()
        .1;
    assert_eq!(fresh["actor"]["id"], actor_id);
    for value in [user, user_types, claims] {
        let rendered = value.to_string().to_lowercase();
        assert!(!rendered.contains("toolbench") && !rendered.contains("tool-bench"));
    }
}

#[tokio::test]
async fn ordinary_extra_brand_and_theme_reads_do_not_create_post_baseline_business_changes() {
    let trial = prepared(TRANSFER, |_| {});
    let initial = trial.state();
    let brands = trial.call("GET", "/api/v1/brands", Value::Null).await;
    let brand = brands[0]["id"].as_str().unwrap();
    trial
        .call(
            "GET",
            &format!("/api/v1/brands/{brand}/themes"),
            Value::Null,
        )
        .await;
    assert_eq!(trial.state(), initial);
    transfer(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn unimplemented_operations_are_environment_faults_instead_of_model_failures() {
    let trial = prepared(TRANSFER, |_| {});
    let (status, response) = trial
        .request("GET", "/api/v1/meta/schemas/user/default", Value::Null)
        .await;
    assert!(status.is_server_error());
    assert_eq!(
        response["message"],
        "This operation is unavailable for this Okta connection."
    );
    transfer(&trial).await;
    assert_outcome(&trial, Outcome::EnvironmentError);
    let assessment = trial.assess();
    assert!(assessment.contract_ok && assessment.integrity_ok);
    assert!(assessment.failures.iter().any(|failure| failure.check
        == mock_core::grading::CheckKind::Environment
        && !failure.event_evidence.is_empty()));
}

#[tokio::test]
async fn an_ordinary_missing_user_read_does_not_poison_a_successful_recovery() {
    let trial = prepared(TRANSFER, |_| {});
    let (status, response) = trial
        .request("GET", "/api/v1/users/not-a-real-user", Value::Null)
        .await;
    assert!(status.is_client_error());
    assert_eq!(response["errorCode"], "E0000007");
    transfer(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
}
