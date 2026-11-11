mod support;

use mock_core::{dataset::Dataset, grading::Outcome};
use serde_json::{Value, json};
use support::ContractTrial;

const APPROVE: &str = "code/terraform/approve-nat-gateway-change";
const SCALE: &str = "code/terraform/scale-checkout-staging";
const UNLOCK: &str = "code/terraform/unlock-orders-prod";
const NETWORK: &str = "ws-X6Cm5W6LhJnKhlo2";
const NETWORK_RUN: &str = "run-1q2oy8I2MqSd5oM4";
const CHECKOUT: &str = "ws-FxCPzmnaeIMOUbSd";
const COUNT: &str = "var-4eRpJG8P0JYsXLTJ";
const ORDERS: &str = "ws-gOHPQz7D1ZRgqBJq";
const ORDERS_RUN: &str = "run-jCcDubzVP5ob0NM7";
const LEGACY: &str = "ws-QMfGO0wb8mhyWgrq";

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_terraform::Terraform,
        &[("authorization", "Bearer local-fixture-credential")],
        prepare,
    )
}

async fn approve(trial: &ContractTrial, comment: &str) {
    trial
        .call(
            "POST",
            &format!("/api/v2/runs/{NETWORK_RUN}/actions/apply"),
            json!({"comment":comment}),
        )
        .await;
    for _attempt in 0..8 {
        trial
            .call("GET", &format!("/api/v2/runs/{NETWORK_RUN}"), Value::Null)
            .await;
        if trial.state()["runs"][NETWORK_RUN]["status"] == "applied" {
            return;
        }
    }
}

async fn variable(trial: &ContractTrial, identity: &str, attributes: Value) {
    trial
        .call(
            "PATCH",
            &format!("/api/v2/workspaces/{CHECKOUT}/vars/{identity}"),
            json!({"data":{"type":"vars","id":identity,"attributes":attributes}}),
        )
        .await;
}

async fn create_run(trial: &ContractTrial, workspace: &str, attributes: Value) -> String {
    let response = trial
        .call(
            "POST",
            "/api/v2/runs",
            json!({"data":{"type":"runs","attributes":attributes,
                "relationships":{"workspace":{"data":{"type":"workspaces","id":workspace}}}}}),
        )
        .await;
    response["data"]["id"].as_str().unwrap().to_owned()
}

async fn scale(trial: &ContractTrial, extra: Value) -> String {
    variable(trial, COUNT, json!({"key":"desired_count","value":"4"})).await;
    let mut attributes = json!({
        "message":"Scale checkout staging to 4 tasks for load test",
        "auto-apply":false,
    });
    for (field, value) in extra.as_object().unwrap() {
        attributes[field] = value.clone();
    }
    create_run(trial, CHECKOUT, attributes).await
}

async fn unlock(trial: &ContractTrial) {
    trial
        .call(
            "POST",
            &format!("/api/v2/workspaces/{ORDERS}/actions/force-unlock"),
            Value::Null,
        )
        .await;
    create_run(
        trial,
        ORDERS,
        json!({"plan-only":true,"auto-apply":false,
            "message":"Verify orders-api-prod after clearing stuck lock"}),
    )
    .await;
}

fn assert_outcome(trial: &ContractTrial, expected: Outcome) {
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, expected, "{assessment:?}");
}

#[tokio::test]
async fn the_three_ordinary_http_workflows_reach_the_requested_outcomes() {
    let trial = prepared(APPROVE, |_| {});
    approve(&trial, "Approved per CHG-2291").await;
    assert_outcome(&trial, Outcome::Pass);
    let trial = prepared(SCALE, |_| {});
    scale(&trial, json!({})).await;
    assert_outcome(&trial, Outcome::Pass);
    let trial = prepared(UNLOCK, |_| {});
    unlock(&trial).await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn a_change_reference_does_not_make_the_wrong_approval_comment_correct() {
    let trial = prepared(APPROVE, |_| {});
    approve(&trial, "Rejected CHG-2291").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn successful_approval_does_not_authorize_canceling_an_unrelated_run() {
    let trial = prepared(APPROVE, |_| {});
    trial
        .call(
            "POST",
            &format!("/api/v2/runs/{ORDERS_RUN}/actions/cancel"),
            json!({"comment":"Unrequested cancellation"}),
        )
        .await;
    approve(&trial, "Approved per CHG-2291").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn completed_apply_materializes_the_planned_nat_and_eip_in_the_new_state() {
    let trial = prepared(APPROVE, |_| {});
    approve(&trial, "Approved per CHG-2291").await;
    let state = trial.state();
    let current = state["workspaces"][NETWORK]["current-state-version"]
        .as_str()
        .unwrap();
    let resources = state["state_versions"][current]["resources"]
        .as_array()
        .unwrap();
    for (kind, name) in [("aws_eip", "nat_b"), ("aws_nat_gateway", "b")] {
        assert!(
            resources
                .iter()
                .any(|resource| resource["type"] == kind && resource["name"] == name),
            "an applied status/count is not the created {kind}.{name}: {resources:?}"
        );
    }
}

#[tokio::test]
async fn a_speculative_plan_cannot_be_confirmed_into_an_applied_run() {
    let trial = prepared(APPROVE, |dataset| {
        dataset
            .services
            .get_mut("terraform")
            .unwrap()
            .collection_mut("runs")
            .get_mut(NETWORK_RUN)
            .unwrap()["plan-only"] = json!(true);
    });
    let initial = trial.state();
    let (status, _) = trial
        .request(
            "POST",
            &format!("/api/v2/runs/{NETWORK_RUN}/actions/apply"),
            json!({"comment":"Approved per CHG-2291"}),
        )
        .await;
    assert!(status.is_client_error(), "{status}");
    assert_eq!(trial.state(), initial);
}

#[tokio::test]
async fn a_destroy_plan_is_not_the_requested_normal_checkout_review() {
    let trial = prepared(SCALE, |_| {});
    scale(&trial, json!({"is-destroy":true})).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn checkout_scaling_preserves_other_variables_and_their_history() {
    let trial = prepared(SCALE, |_| {});
    variable(
        &trial,
        "var-w6vOLf60kvxWb9hr",
        json!({"key":"max_capacity","value":"99"}),
    )
    .await;
    scale(&trial, json!({})).await;
    variable(
        &trial,
        "var-w6vOLf60kvxWb9hr",
        json!({"key":"max_capacity","value":"6"}),
    )
    .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn a_message_prefix_is_not_the_exact_requested_run_message() {
    let trial = prepared(SCALE, |_| {});
    scale(
        &trial,
        json!({"message":"Scale checkout staging to 4 tasks; approval has been revoked"}),
    )
    .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn the_plan_does_not_claim_updated_input_effects_before_the_input_changes() {
    let trial = prepared(SCALE, |_| {});
    let run = create_run(
        &trial,
        CHECKOUT,
        json!({"message":"Inspect existing input","auto-apply":false}),
    )
    .await;
    let state = trial.state();
    assert_eq!(state["runs"][&run]["input-variables"]["desired_count"], "2");
    let plan = state["runs"][&run]["plan"].as_str().unwrap();
    let changes = state["plans"][plan]["resource-changes-detail"]
        .as_array()
        .unwrap();
    assert!(
        !changes
            .iter()
            .any(|change| change["address"] == "aws_ecs_service.checkout"
                && change["after"]["desired_count"] == 4),
        "a canned plan is not evidence that this run read desired_count=4"
    );
}

#[tokio::test]
async fn clearing_the_target_lock_does_not_authorize_legacy_workspace_edits() {
    let trial = prepared(UNLOCK, |_| {});
    trial
        .call(
            "PATCH",
            &format!("/api/v2/workspaces/{LEGACY}"),
            json!({"data":{"type":"workspaces","id":LEGACY,
                "attributes":{"description":"Unrequested legacy edit"}}}),
        )
        .await;
    unlock(&trial).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn cancellation_then_speculative_verification_is_a_valid_recovery_alternative() {
    let trial = prepared(UNLOCK, |_| {});
    trial
        .call(
            "POST",
            &format!("/api/v2/runs/{ORDERS_RUN}/actions/cancel"),
            json!({"comment":"Recover the crashed Commerce run"}),
        )
        .await;
    create_run(
        &trial,
        ORDERS,
        json!({"plan-only":true,"auto-apply":false,
            "message":"Verify orders-api-prod after cancellation"}),
    )
    .await;
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn force_unlocking_and_canceling_the_crashed_run_together_are_one_recovery() {
    // The force-unlock tool's description says to prefer canceling the run first, so an
    // agent may do both: the second step finds nothing left to release or clears the run.
    for cancel_first in [true, false] {
        let trial = prepared(UNLOCK, |_| {});
        let cancel = format!("/api/v2/runs/{ORDERS_RUN}/actions/cancel");
        let force = format!("/api/v2/workspaces/{ORDERS}/actions/force-unlock");
        if cancel_first {
            trial.call("POST", &cancel, Value::Null).await;
            let (status, _) = trial.request("POST", &force, Value::Null).await;
            assert_eq!(status.as_u16(), 409);
        } else {
            trial.call("POST", &force, Value::Null).await;
            trial.call("POST", &cancel, Value::Null).await;
        }
        create_run(&trial, ORDERS, json!({"plan-only":true})).await;
        assert_outcome(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn applied_state_download_preserves_the_resolved_eip_nat_and_route_relationships() {
    let trial = prepared(APPROVE, |_| {});
    approve(&trial, "Approved per CHG-2291").await;
    let state = trial.state();
    let version = state["workspaces"][NETWORK]["current-state-version"]
        .as_str()
        .unwrap();
    let file = trial
        .call(
            "GET",
            &format!("/archivist/v1/object/state-{version}"),
            Value::Null,
        )
        .await;
    let resources = file["resources"].as_array().unwrap();
    let resource = |kind: &str, name: &str| {
        resources
            .iter()
            .find(|row| row["type"] == kind && row["name"] == name)
            .unwrap()
    };
    let eip = &resource("aws_eip", "nat_b")["instances"][0]["attributes"];
    let nat = &resource("aws_nat_gateway", "b")["instances"][0]["attributes"];
    let route = resource("aws_route_table", "private")["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|instance| instance["index_key"] == 1)
        .unwrap();
    assert_eq!(eip["id"], nat["allocation_id"]);
    assert_eq!(nat["id"], route["attributes"]["nat_gateway_id"]);
    assert!(!file.to_string().contains("(known after apply)"));
    let count: usize = resources
        .iter()
        .map(|row| row["instances"].as_array().unwrap().len())
        .sum();
    assert_eq!(state["workspaces"][NETWORK]["resource-count"], count);
}

#[tokio::test]
async fn absent_execution_outcomes_cannot_produce_a_false_applied_status_or_partial_state() {
    let trial = prepared(APPROVE, |dataset| {
        dataset
            .services
            .get_mut("terraform")
            .unwrap()
            .take_collection("execution_outcomes");
    });
    let initial = trial.state();
    let (status, body) = trial
        .request(
            "POST",
            &format!("/api/v2/runs/{NETWORK_RUN}/actions/apply"),
            json!({"comment":"Approved per CHG-2291"}),
        )
        .await;
    assert!(status.is_server_error(), "{status}: {body}");
    assert_eq!(trial.state(), initial);
    assert_outcome(&trial, Outcome::EnvironmentError);
}

#[tokio::test]
async fn computed_execution_facts_cannot_override_known_plan_configuration() {
    let trial = prepared(APPROVE, |dataset| {
        dataset
            .services
            .get_mut("terraform")
            .unwrap()
            .collection_mut("execution_outcomes")
            .get_mut("plan-vOqCAeqHb08EsDyk")
            .unwrap()["resources"]["aws_eip.nat_b"]["attributes"]["domain"] = json!("standard");
    });
    let initial = trial.state();
    let (status, _) = trial
        .request(
            "POST",
            &format!("/api/v2/runs/{NETWORK_RUN}/actions/apply"),
            json!({"comment":"Approved per CHG-2291"}),
        )
        .await;
    assert!(status.is_server_error());
    assert_eq!(trial.state(), initial);
    assert_outcome(&trial, Outcome::EnvironmentError);
}

#[tokio::test]
async fn numeric_equivalent_input_text_produces_the_same_real_plan_values() {
    for value in ["4", "4.000", "0.4e1"] {
        let trial = prepared(SCALE, |_| {});
        variable(&trial, COUNT, json!({"key":"desired_count","value":value})).await;
        let run = create_run(
            &trial,
            CHECKOUT,
            json!({"message":"Inspect numeric input","auto-apply":false}),
        )
        .await;
        let state = trial.state();
        let plan = state["runs"][&run]["plan"].as_str().unwrap();
        let service = state["plans"][plan]["resource-changes-detail"]
            .as_array()
            .unwrap()
            .iter()
            .find(|change| change["address"] == "aws_ecs_service.checkout")
            .unwrap();
        assert_eq!(service["after"]["desired_count"], 4);
        let public = trial
            .call("GET", &format!("/api/v2/runs/{run}"), Value::Null)
            .await;
        assert!(public["data"]["attributes"].get("input-bindings").is_none());
        assert!(!public.to_string().contains("dd-staging-0f9c"));
    }
}

#[tokio::test]
async fn planning_again_after_apply_does_not_repeat_changes_from_the_old_template() {
    let trial = prepared(SCALE, |_| {});
    let first = scale(&trial, json!({})).await;
    trial
        .call(
            "POST",
            &format!("/api/v2/runs/{first}/actions/apply"),
            json!({"comment":"Exercise recorded state convergence"}),
        )
        .await;
    let next = create_run(
        &trial,
        CHECKOUT,
        json!({"message":"Inspect convergence","auto-apply":false}),
    )
    .await;
    let state = trial.state();
    let plan = state["runs"][&next]["plan"].as_str().unwrap();
    assert_eq!(state["plans"][plan]["resource-changes-detail"], json!([]));
    assert_eq!(state["runs"][&next]["status"], "planned_and_finished");
}

#[tokio::test]
async fn raw_and_stable_state_downloads_have_distinct_consistent_representations() {
    let trial = prepared(APPROVE, |_| {});
    let initial = trial.state();
    let previous = initial["workspaces"][NETWORK]["current-state-version"]
        .as_str()
        .unwrap();
    let before = trial
        .call(
            "GET",
            &format!("/archivist/v1/object/state-{previous}"),
            Value::Null,
        )
        .await;
    approve(&trial, "Approved per CHG-2291").await;
    let state = trial.state();
    let current = state["workspaces"][NETWORK]["current-state-version"]
        .as_str()
        .unwrap();
    let raw = trial
        .call(
            "GET",
            &format!("/archivist/v1/object/state-{current}"),
            Value::Null,
        )
        .await;
    let stable = trial
        .call(
            "GET",
            &format!("/archivist/v1/object/state-json-{current}"),
            Value::Null,
        )
        .await;
    assert_eq!(raw["version"], 4);
    assert_eq!(raw["serial"], state["state_versions"][current]["serial"]);
    assert_eq!(raw["lineage"], before["lineage"]);
    assert!(raw["serial"].as_u64().unwrap() > before["serial"].as_u64().unwrap());
    assert_eq!(stable["format_version"], "1.0");
    assert_eq!(stable["terraform_version"], raw["terraform_version"]);
    assert!(stable.get("serial").is_none());
    assert!(stable.get("lineage").is_none());
    assert!(stable.get("resources").is_none());
    let resources = stable["values"]["root_module"]["resources"]
        .as_array()
        .unwrap();
    let eip = resources
        .iter()
        .find(|row| row["address"] == "aws_eip.nat_b")
        .unwrap();
    assert_eq!(eip["name"], "nat_b");
    assert_eq!(eip["provider_name"], "registry.terraform.io/hashicorp/aws");
    assert_eq!(eip["values"]["id"], "eipalloc-0a91bc32de45f6078");
    assert!(eip.get("attributes").is_none());
    assert_eq!(
        stable["values"]["outputs"]["vpc_id"]["value"],
        raw["outputs"]["vpc_id"]["value"]
    );
    assert_eq!(
        trial
            .call(
                "GET",
                &format!("/archivist/v1/object/state-{previous}"),
                Value::Null
            )
            .await,
        before
    );
}

#[tokio::test]
async fn public_plan_uses_structured_unknowns_and_separate_instance_identity() {
    let trial = prepared(APPROVE, |_| {});
    let plan = trial
        .call(
            "GET",
            "/archivist/v1/object/plan-json-plan-vOqCAeqHb08EsDyk",
            Value::Null,
        )
        .await;
    assert_eq!(plan["format_version"], "1.2");
    let changes = plan["resource_changes"].as_array().unwrap();
    let route = changes
        .iter()
        .find(|row| row["address"] == "aws_route_table.private[1]")
        .unwrap();
    assert_eq!(route["name"], "private");
    assert_eq!(route["index"], 1);
    assert_eq!(route["change"]["after_unknown"]["nat_gateway_id"], true);
    assert!(route["change"]["after"]["nat_gateway_id"].is_null());
    let nat = changes
        .iter()
        .find(|row| row["address"] == "aws_nat_gateway.b")
        .unwrap();
    assert_eq!(nat["change"]["after_unknown"]["allocation_id"], true);
    assert_eq!(nat["change"]["after"]["connectivity_type"], "public");
    assert!(!plan.to_string().contains("(known after apply)"));
    let planned = plan["planned_values"]["root_module"]["resources"]
        .as_array()
        .unwrap();
    assert!(
        planned
            .iter()
            .any(|row| row["address"] == "aws_eip.nat_b" && row["values"]["domain"] == "vpc")
    );
}

#[tokio::test]
async fn stable_state_groups_nested_modules_without_flattening_instance_addresses() {
    let trial = prepared(APPROVE, |dataset| {
        dataset
            .services
            .get_mut("terraform")
            .unwrap()
            .collection_mut("state_versions")
            .get_mut("sv-2HkRH31vgu6V7Nh1")
            .unwrap()["resource-instances"] = json!([{
            "address":"module.network[0].module.private.aws_route_table.routes[2]",
            "module":"module.network[0].module.private",
            "type":"aws_route_table","name":"routes","index":2,
            "provider":"registry.terraform.io/hashicorp/aws",
            "attributes":{"id":"rtb-fixture","tags":{"Name":"Retained"}}
        }]);
    });
    let state = trial
        .call(
            "GET",
            "/archivist/v1/object/state-json-sv-2HkRH31vgu6V7Nh1",
            Value::Null,
        )
        .await;
    let outer = &state["values"]["root_module"]["child_modules"][0];
    assert_eq!(outer["address"], "module.network[0]");
    let inner = &outer["child_modules"][0];
    assert_eq!(inner["address"], "module.network[0].module.private");
    let resource = &inner["resources"][0];
    assert_eq!(resource["index"], 2);
    assert_eq!(resource["name"], "routes");
    assert_eq!(resource["values"]["tags"]["Name"], "Retained");
}

#[tokio::test]
async fn quoted_or_malformed_addresses_are_not_guessed_into_resource_identity() {
    for address in [
        "aws_eip.nat[\"a.b\"]",
        "module..aws_eip.nat_b",
        "unexpected.module.aws_eip.nat_b",
    ] {
        let trial = prepared(APPROVE, |dataset| {
            dataset
                .services
                .get_mut("terraform")
                .unwrap()
                .collection_mut("plans")
                .get_mut("plan-vOqCAeqHb08EsDyk")
                .unwrap()["resource-changes-detail"][0]["address"] = json!(address);
        });
        let initial = trial.state();
        let (status, _) = trial
            .request(
                "POST",
                &format!("/api/v2/runs/{NETWORK_RUN}/actions/apply"),
                json!({"comment":"Approved per CHG-2291"}),
            )
            .await;
        assert_eq!(status.as_u16(), 501, "{address}");
        assert_eq!(trial.state(), initial);
    }
}

#[tokio::test]
async fn a_saved_plan_rejects_disjoint_serial_or_lineage_changes_before_apply() {
    for (field, replacement) in [
        ("serial", json!(97)),
        ("lineage", json!("replacement-history")),
    ] {
        let trial = prepared(APPROVE, |_| {});
        trial
            .world
            .lock()
            .unwrap()
            .service_mut("terraform")
            .store
            .collection_mut("state_versions")
            .get_mut("sv-2HkRH31vgu6V7Nh1")
            .unwrap()[field] = replacement;
        let initial = trial.state();
        let (status, _) = trial
            .request(
                "POST",
                &format!("/api/v2/runs/{NETWORK_RUN}/actions/apply"),
                json!({"comment":"Approved per CHG-2291"}),
            )
            .await;
        assert_eq!(status.as_u16(), 409, "{field}");
        assert_eq!(trial.state(), initial);
    }
}

#[tokio::test]
async fn equivalent_checkout_values_are_valid_outcomes_not_only_plan_inputs() {
    for value in ["4.000", "0.4e1", " +0004 "] {
        let trial = prepared(SCALE, |_| {});
        variable(&trial, COUNT, json!({"value":value})).await;
        create_run(
            &trial,
            CHECKOUT,
            json!({
                "message":"Scale checkout staging to 4 tasks for load test","auto-apply":false
            }),
        )
        .await;
        assert_outcome(&trial, Outcome::Pass);
    }
}

#[tokio::test]
async fn a_restored_wrong_desired_count_does_not_erase_the_unauthorized_transition() {
    let trial = prepared(SCALE, |_| {});
    variable(&trial, COUNT, json!({"value":"99"})).await;
    scale(&trial, json!({})).await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn speculative_refresh_and_applied_runs_are_not_a_normal_unapproved_review() {
    for attributes in [
        json!({"plan-only":true}),
        json!({"refresh-only":true}),
        json!({"auto-apply":true}),
    ] {
        let trial = prepared(SCALE, |_| {});
        scale(&trial, attributes).await;
        assert_outcome(&trial, Outcome::Fail);
    }
}

#[tokio::test]
async fn speculative_verification_must_follow_not_precede_lock_recovery() {
    let trial = prepared(UNLOCK, |_| {});
    create_run(
        &trial,
        ORDERS,
        json!({
            "plan-only":true,"auto-apply":false,"message":"Inspect drift before recovery"
        }),
    )
    .await;
    trial
        .call(
            "POST",
            &format!("/api/v2/workspaces/{ORDERS}/actions/force-unlock"),
            Value::Null,
        )
        .await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn force_unlock_does_not_claim_to_have_canceled_the_crashed_run() {
    let trial = prepared(UNLOCK, |_| {});
    unlock(&trial).await;
    assert_eq!(trial.state()["runs"][ORDERS_RUN]["status"], "applying");
    assert_outcome(&trial, Outcome::Pass);
}

#[tokio::test]
async fn resolved_resource_ids_must_be_related_not_merely_present_in_the_state() {
    let trial = prepared(APPROVE, |dataset| {
        dataset
            .services
            .get_mut("terraform")
            .unwrap()
            .collection_mut("execution_outcomes")
            .get_mut("plan-vOqCAeqHb08EsDyk")
            .unwrap()["resources"]["aws_route_table.private[1]"]["attributes"]["nat_gateway_id"] =
            json!("nat-an-unrelated-gateway");
    });
    approve(&trial, "Approved per CHG-2291").await;
    assert_outcome(&trial, Outcome::Fail);
}

#[tokio::test]
async fn an_outcome_record_without_attributes_is_not_resolved_execution() {
    let trial = prepared(APPROVE, |dataset| {
        dataset
            .services
            .get_mut("terraform")
            .unwrap()
            .collection_mut("execution_outcomes")
            .get_mut("plan-vOqCAeqHb08EsDyk")
            .unwrap()["resources"]["aws_nat_gateway.b"]
            .as_object_mut()
            .unwrap()
            .remove("attributes");
    });
    let initial = trial.state();
    let (status, _) = trial
        .request(
            "POST",
            &format!("/api/v2/runs/{NETWORK_RUN}/actions/apply"),
            json!({"comment":"Approved per CHG-2291"}),
        )
        .await;
    assert_eq!(status.as_u16(), 501);
    assert_eq!(trial.state(), initial);
    assert_outcome(&trial, Outcome::EnvironmentError);
}

#[tokio::test]
async fn literal_known_after_apply_text_is_not_a_global_execution_wildcard() {
    let trial = prepared(APPROVE, |dataset| {
        let service = dataset.services.get_mut("terraform").unwrap();
        let change = &mut service
            .collection_mut("plans")
            .get_mut("plan-vOqCAeqHb08EsDyk")
            .unwrap()["resource-changes-detail"][0];
        change["after"]["tags"]["Name"] = json!("(known after apply)");
        change["after-unknown"] = json!({});
        service
            .collection_mut("execution_outcomes")
            .get_mut("plan-vOqCAeqHb08EsDyk")
            .unwrap()["resources"]["aws_eip.nat_b"]["attributes"]["tags"]["Name"] =
            json!("Not the literal value");
    });
    let initial = trial.state();
    let (status, _) = trial
        .request(
            "POST",
            &format!("/api/v2/runs/{NETWORK_RUN}/actions/apply"),
            json!({"comment":"Approved per CHG-2291"}),
        )
        .await;
    assert_eq!(status.as_u16(), 501);
    assert_eq!(trial.state(), initial);
}

#[tokio::test]
async fn independently_overridden_display_plans_are_not_successful_execution_evidence() {
    let trial = prepared(APPROVE, |dataset| {
        dataset
            .services
            .get_mut("terraform")
            .unwrap()
            .collection_mut("plans")
            .get_mut("plan-vOqCAeqHb08EsDyk")
            .unwrap()["json-output"] = json!({"resource_changes":[]});
    });
    let initial = trial.state();
    let (status, _) = trial
        .request(
            "GET",
            "/archivist/v1/object/plan-json-plan-vOqCAeqHb08EsDyk",
            Value::Null,
        )
        .await;
    assert_eq!(status.as_u16(), 501);
    assert_eq!(trial.state(), initial);
    assert_outcome(&trial, Outcome::EnvironmentError);
}

#[tokio::test]
async fn unsupported_state_modes_and_provider_aliases_are_not_rendered_as_managed_defaults() {
    for field in ["mode", "provider"] {
        for prefix in ["state-", "state-json-"] {
            let trial = prepared(APPROVE, |dataset| {
                let mut instance = json!({
                    "address":"aws_eip.nat_a","module":"root","type":"aws_eip","name":"nat_a",
                    "index":null,"provider":"registry.terraform.io/hashicorp/aws","attributes":{}
                });
                instance[field] = if field == "mode" {
                    json!("data")
                } else {
                    json!("provider[\"registry.terraform.io/hashicorp/aws\"].alternate")
                };
                dataset
                    .services
                    .get_mut("terraform")
                    .unwrap()
                    .collection_mut("state_versions")
                    .get_mut("sv-2HkRH31vgu6V7Nh1")
                    .unwrap()["resource-instances"] = json!([instance]);
            });
            let (status, _) = trial
                .request(
                    "GET",
                    &format!("/archivist/v1/object/{prefix}sv-2HkRH31vgu6V7Nh1"),
                    Value::Null,
                )
                .await;
            assert_eq!(status.as_u16(), 501, "{prefix}: {field}");
            assert_outcome(&trial, Outcome::EnvironmentError);
        }
    }
}

#[tokio::test]
async fn nested_plan_unknown_masks_preserve_known_nulls_and_array_positions() {
    let trial = prepared(APPROVE, |dataset| {
        dataset
            .services
            .get_mut("terraform")
            .unwrap()
            .collection_mut("plans")
            .get_mut("plan-vOqCAeqHb08EsDyk")
            .unwrap()["resource-changes-detail"][0]["after"]["example"] =
            json!({"known_null":null,"items":[{"id":"(known after apply)"},{"id":"known"}]});
    });
    let plan = trial
        .call(
            "GET",
            "/archivist/v1/object/plan-json-plan-vOqCAeqHb08EsDyk",
            Value::Null,
        )
        .await;
    let change = &plan["resource_changes"][0]["change"];
    assert!(change["after"]["example"]["known_null"].is_null());
    assert!(
        change["after_unknown"]["example"]
            .get("known_null")
            .is_none()
    );
    assert_eq!(
        change["after_unknown"]["example"]["items"],
        json!([{"id":true},false])
    );
    assert_eq!(change["after"]["example"]["items"][1]["id"], "known");
}
