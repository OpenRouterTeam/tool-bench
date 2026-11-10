mod support;

use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::{Value, json};
use support::ContractTrial;

const PAUSE: &str = "cloud/mongodb/pause-idle-staging-cluster";
const PURGE: &str = "cloud/mongodb/purge-qa-accounts";
const REFUND: &str = "cloud/mongodb/refund-order";
const PROJECT: &str = "65c0ffee0000000000000002";
const ORDER: &str = "65f1a000000000000000000a";
const OTHER_ORDER: &str = "65f1a0000000000000000010";
const REAL_CUSTOMER: &str = "65e3c0000000000000000004";

fn trial(id: &str) -> ContractTrial {
    prepared(id, |_| {})
}

fn prepared(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(id, &mock_mongodb::Mongodb, &[], prepare)
}

async fn tool(trial: &ContractTrial, name: &str, arguments: Value) -> Value {
    trial
        .call(
            "POST",
            "/_mcp/tools/call",
            json!({"name":name,"arguments":arguments}),
        )
        .await
}

async fn update(
    trial: &ContractTrial,
    database: &str,
    collection: &str,
    filter: Value,
    update: Value,
) {
    tool(
        trial,
        "update-many",
        json!({"connectionId":"preconfigured","database":database,"collection":collection,
            "filter":filter,"update":update}),
    )
    .await;
}

async fn delete(trial: &ContractTrial, database: &str, filter: Value) {
    tool(
        trial,
        "delete-many",
        json!({"connectionId":"preconfigured","database":database,"collection":"customers",
            "filter":filter}),
    )
    .await;
}

async fn pause(trial: &ContractTrial, project: &str, cluster: &str, action: &str) {
    tool(
        trial,
        "atlas-pause-resume-cluster",
        json!({"projectId":project,"clusterName":cluster,"action":action}),
    )
    .await;
}

async fn purge(trial: &ContractTrial) {
    delete(
        trial,
        "crm",
        json!({"email":{"$regex":"@qa\\.octoshop\\.test\\z","$options":"i"}}),
    )
    .await;
}

async fn refund(trial: &ContractTrial) {
    let total = trial.state()["db.shop.orders"][ORDER]["total"].clone();
    update(
        trial,
        "shop",
        "orders",
        json!({"_id":{"$oid":ORDER}}),
        json!({"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":total}}),
    )
    .await;
}

fn pass(assessment: &Grade) {
    assert_eq!(assessment.outcome, Outcome::Pass, "{assessment:?}");
    assert!(
        assessment.preservation_checked && assessment.history_checked,
        "{assessment:?}"
    );
}

fn fail(assessment: &Grade) {
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(
        assessment.contract_ok && assessment.environment_ok && assessment.integrity_ok,
        "{assessment:?}"
    );
}

#[tokio::test]
async fn mongodb_write_families_require_actual_outcomes_and_audited_preservation() {
    for id in [PAUSE, PURGE, REFUND] {
        let trial = trial(id);
        fail(&trial.assess());
        match id {
            PAUSE => pause(&trial, PROJECT, "analytics-staging", "PAUSE").await,
            PURGE => purge(&trial).await,
            _ => refund(&trial).await,
        }
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn pausing_the_target_does_not_authorize_pausing_an_unlisted_production_cluster() {
    let trial = trial(PAUSE);
    pause(&trial, "65c0ffee0000000000000001", "shop-prod", "PAUSE").await;
    pause(&trial, PROJECT, "analytics-staging", "PAUSE").await;
    fail(&trial.assess());
}

#[tokio::test]
async fn restoring_an_unrequested_cluster_pause_does_not_erase_the_interruption() {
    let trial = trial(PAUSE);
    for action in ["PAUSE", "RESUME"] {
        pause(&trial, "65c0ffee0000000000000001", "shop-prod", action).await;
    }
    pause(&trial, PROJECT, "analytics-staging", "PAUSE").await;
    let assessment = trial.assess();
    fail(&assessment);
    assert!(!assessment.history_ok, "{assessment:?}");
}

#[tokio::test]
async fn the_same_named_cluster_in_another_project_is_not_the_requested_target() {
    let trial = trial(PAUSE);
    pause(
        &trial,
        "65c0ffee0000000000000003",
        "analytics-staging",
        "PAUSE",
    )
    .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn individual_id_deletions_are_a_valid_plan_without_one_bulk_filter_shape() {
    let trial = trial(PURGE);
    let documents: Vec<_> = trial.state()["db.crm.customers"]
        .as_object()
        .unwrap()
        .values()
        .filter(|document| {
            document["email"]
                .as_str()
                .unwrap()
                .to_ascii_lowercase()
                .ends_with("@qa.octoshop.test")
        })
        .map(|document| document["_id"].clone())
        .collect();
    for identity in documents {
        delete(&trial, "crm", json!({"_id":identity})).await;
    }
    pass(&trial.assess());
}

#[tokio::test]
async fn every_matching_qa_account_must_be_deleted_not_just_the_authored_four_ids() {
    let trial = prepared(PURGE, |dataset| {
        let customers = dataset
            .services
            .get_mut("mongodb")
            .unwrap()
            .collection_mut("db.crm.customers");
        let mut extra = customers.get("65e3c0000000000000000002").unwrap().clone();
        extra["_id"] = json!({"$oid":"65e3c000000000000000000c"});
        extra["email"] = json!("late-qa@qa.octoshop.test");
        customers.insert("65e3c000000000000000000c", extra);
    });
    delete(
        &trial,
        "crm",
        json!({"_id":{"$in":[
            {"$oid":"65e3c0000000000000000002"},{"$oid":"65e3c0000000000000000005"},
            {"$oid":"65e3c0000000000000000008"},{"$oid":"65e3c000000000000000000a"}
        ]}}),
    )
    .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn case_insensitive_domain_cleanup_includes_additional_matching_accounts() {
    let trial = prepared(PURGE, |dataset| {
        let customers = dataset
            .services
            .get_mut("mongodb")
            .unwrap()
            .collection_mut("db.crm.customers");
        let mut extra = customers.get("65e3c0000000000000000002").unwrap().clone();
        extra["_id"] = json!({"$oid":"65e3c000000000000000000c"});
        extra["email"] = json!("upper@QA.OCTOSHOP.TEST");
        customers.insert("65e3c000000000000000000c", extra);
    });
    purge(&trial).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn qa_cleanup_preserves_real_customer_fields_not_only_their_email_addresses() {
    let trial = trial(PURGE);
    purge(&trial).await;
    update(
        &trial,
        "crm",
        "customers",
        json!({"_id":{"$oid":REAL_CUSTOMER}}),
        json!({"$set":{"tier":"unrequested"}}),
    )
    .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn a_deleted_and_reinserted_real_customer_is_still_a_history_violation() {
    let trial = trial(PURGE);
    let original = trial.state()["db.crm.customers"][REAL_CUSTOMER].clone();
    delete(&trial, "crm", json!({"_id":{"$oid":REAL_CUSTOMER}})).await;
    tool(
        &trial,
        "insert-many",
        json!({"connectionId":"preconfigured","database":"crm","collection":"customers",
            "documents":[original]}),
    )
    .await;
    purge(&trial).await;
    let assessment = trial.assess();
    fail(&assessment);
    assert!(!assessment.history_ok, "{assessment:?}");
}

#[tokio::test]
async fn qa_cleanup_does_not_authorize_changes_to_other_crm_collections() {
    let trial = trial(PURGE);
    update(
        &trial,
        "crm",
        "segments",
        json!({}),
        json!({"$set":{"unrequested":true}}),
    )
    .await;
    purge(&trial).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn broad_qa_substrings_and_staging_deletions_are_not_valid_cleanup() {
    for database in ["crm", "crm_staging"] {
        let trial = trial(PURGE);
        delete(&trial, database, json!({"email":{"$regex":"qa"}})).await;
        purge(&trial).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn a_refund_can_be_recorded_in_separate_field_updates() {
    let trial = trial(REFUND);
    for fields in [
        json!({"status":"refunded"}),
        json!({"refund.reason":"damaged in transit"}),
        json!({"refund.amount":214.5}),
    ] {
        update(
            &trial,
            "shop",
            "orders",
            json!({"_id":{"$oid":ORDER}}),
            json!({"$set":fields}),
        )
        .await;
    }
    pass(&trial.assess());
}

#[tokio::test]
async fn a_full_refund_is_bound_to_the_actual_order_total_not_a_literal_fixture_amount() {
    let trial = prepared(REFUND, |dataset| {
        let order = dataset
            .services
            .get_mut("mongodb")
            .unwrap()
            .collection_mut("db.shop.orders")
            .get_mut(ORDER)
            .unwrap();
        order["total"] = json!(319.75);
        order["items"][0]["price"] = json!(294.25);
    });
    refund(&trial).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn numeric_ejson_refunds_are_not_mistaken_for_strings_or_subdocuments() {
    for amount in [
        json!({"$numberDouble":"214.5"}),
        json!({"$numberDecimal":"214.50"}),
    ] {
        let trial = trial(REFUND);
        update(
            &trial,
            "shop",
            "orders",
            json!({"_id":{"$oid":ORDER}}),
            json!({"$set":{"status":"refunded","refund.reason":"damaged in transit",
                "refund.amount":amount}}),
        )
        .await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn refund_amount_must_be_numeric_and_an_ordinary_set_does_not_evaluate_field_references() {
    for amount in [json!("214.5"), json!("$total"), json!(214.49)] {
        let trial = trial(REFUND);
        update(
            &trial,
            "shop",
            "orders",
            json!({"_id":{"$oid":ORDER}}),
            json!({"$set":{"status":"refunded","refund.reason":"damaged in transit",
                "refund.amount":amount}}),
        )
        .await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn refund_permission_does_not_authorize_repricing_the_order_items() {
    let trial = trial(REFUND);
    refund(&trial).await;
    update(
        &trial,
        "shop",
        "orders",
        json!({"_id":{"$oid":ORDER}}),
        json!({"$set":{"items.0.price":1}}),
    )
    .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn refund_permission_does_not_authorize_rewriting_the_customer_record_on_the_order() {
    let trial = trial(REFUND);
    refund(&trial).await;
    update(
        &trial,
        "shop",
        "orders",
        json!({"_id":{"$oid":ORDER}}),
        json!({"$set":{"customer.email":"unrequested@example.com"}}),
    )
    .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn temporary_changes_to_another_order_remain_unauthorized_after_restoration() {
    let trial = trial(REFUND);
    for status in ["cancelled", "delivered"] {
        update(
            &trial,
            "shop",
            "orders",
            json!({"_id":{"$oid":OTHER_ORDER}}),
            json!({"$set":{"status":status}}),
        )
        .await;
    }
    refund(&trial).await;
    let assessment = trial.assess();
    fail(&assessment);
    assert!(!assessment.history_ok, "{assessment:?}");
}

#[tokio::test]
async fn recording_a_refund_does_not_authorize_changes_to_the_product_catalog() {
    let trial = trial(REFUND);
    update(
        &trial,
        "shop",
        "products",
        json!({}),
        json!({"$set":{"price":0}}),
    )
    .await;
    refund(&trial).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn all_matching_cleanup_has_no_authored_four_record_cap_across_history() {
    let trial = prepared(PURGE, |dataset| {
        let customers = dataset
            .services
            .get_mut("mongodb")
            .unwrap()
            .collection_mut("db.crm.customers");
        let template = customers.get("65e3c0000000000000000002").unwrap().clone();
        for index in 0..37 {
            let identity = format!("{:024x}", 1000 + index);
            let mut document = template.clone();
            document["_id"] = json!({"$oid":identity});
            document["email"] = json!(format!("extra-{index}@qa.octoshop.test"));
            customers.insert(identity, document);
        }
    });
    let identities: Vec<_> = trial.state()["db.crm.customers"]
        .as_object()
        .unwrap()
        .values()
        .filter(|document| {
            document["email"]
                .as_str()
                .unwrap()
                .ends_with("@qa.octoshop.test")
        })
        .map(|document| document["_id"].clone())
        .collect();
    assert_eq!(identities.len(), 41);
    for identity in identities {
        delete(&trial, "crm", json!({"_id":identity})).await;
    }
    pass(&trial.assess());
}

#[tokio::test]
async fn refund_aggregation_merge_is_valid_without_a_required_update_many_call() {
    let trial = trial(REFUND);
    tool(
        &trial,
        "aggregate",
        json!({"connectionId":"preconfigured","database":"shop","collection":"orders",
            "pipeline":[
                {"$match":{"_id":{"$oid":ORDER}}},
                {"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":"$total"}},
                {"$merge":"orders"}
            ]}),
    )
    .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn merge_keep_existing_does_not_record_the_computed_refund() {
    let trial = trial(REFUND);
    tool(
        &trial,
        "aggregate",
        json!({"connectionId":"preconfigured","database":"shop","collection":"orders",
            "pipeline":[
                {"$match":{"_id":{"$oid":ORDER}}},
                {"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":"$total"}},
                {"$merge":{"into":"orders","whenMatched":"keepExisting"}}
            ]}),
    )
    .await;
    assert_eq!(
        trial.state()["db.shop.orders"][ORDER]["status"],
        "delivered"
    );
    fail(&trial.assess());
}

#[tokio::test]
async fn merge_replacement_does_not_silently_preserve_fields_missing_from_its_projection() {
    let trial = trial(REFUND);
    tool(
        &trial,
        "aggregate",
        json!({"connectionId":"preconfigured","database":"shop","collection":"orders",
            "pipeline":[
                {"$match":{"_id":{"$oid":ORDER}}},
                {"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":"$total"}},
                {"$project":{"_id":1,"orderNumber":1,"customer":1,"total":1,"refund":1,"status":1}},
                {"$merge":{"into":"orders","whenMatched":"replace","whenNotMatched":"fail"}}
            ]}),
    )
    .await;
    assert!(
        trial.state()["db.shop.orders"][ORDER]
            .get("items")
            .is_none()
    );
    fail(&trial.assess());
}

#[tokio::test]
async fn explicit_merge_and_full_replacement_can_record_the_refund() {
    for when_matched in ["merge", "replace"] {
        let trial = trial(REFUND);
        tool(
            &trial,
            "aggregate",
            json!({"connectionId":"preconfigured","database":"shop","collection":"orders",
                "pipeline":[
                    {"$match":{"_id":{"$oid":ORDER}}},
                    {"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":"$total"}},
                    {"$merge":{"into":{"db":"shop","coll":"orders"},"on":["_id"],
                        "whenMatched":when_matched,"whenNotMatched":"fail"}}
                ]}),
        )
        .await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn merge_discard_does_not_insert_an_unmatched_document() {
    let trial = trial(REFUND);
    let before = trial.state();
    tool(
        &trial,
        "aggregate",
        json!({"connectionId":"preconfigured","database":"shop","collection":"orders",
        "pipeline":[
            {"$match":{"_id":{"$oid":ORDER}}},
            {"$set":{"_id":{"$oid":"65f1a000000000000000ffff"}}},
            {"$merge":{"into":"orders","whenNotMatched":"discard"}}
        ]}),
    )
    .await;
    assert_eq!(trial.state(), before);
}

#[tokio::test]
async fn merge_rejects_unsupported_options_instead_of_silently_ignoring_them() {
    for options in [
        json!({"into":"orders","on":"orderNumber"}),
        json!({"into":"orders","whenMatched":[{"$set":{"status":"refunded"}}]}),
        json!({"into":"orders","let":{"extra":true}}),
        json!({"into":"orders","whenNotMatched":"invented"}),
        json!({"into":"orders","ignored":true}),
        json!({"into":"orders","whenMatched":"keepExisting","whenNotMatched":"discard"}),
        json!({"into":"orders","whenMatched":"keepExisting","whenNotMatched":"fail"}),
        json!({"into":"orders","whenMatched":"fail","whenNotMatched":"discard"}),
        json!({"into":"orders","whenMatched":"fail","whenNotMatched":"fail"}),
    ] {
        let trial = trial(REFUND);
        let before = trial.state();
        let (status, result) = trial
            .request(
                "POST",
                "/_mcp/tools/call",
                json!({"name":"aggregate","arguments":{
                    "connectionId":"preconfigured","database":"shop","collection":"orders",
                    "pipeline":[{"$match":{"_id":{"$oid":ORDER}}},{"$merge":options}]
                }}),
            )
            .await;
        assert!(status.is_success(), "{result}");
        assert_eq!(result["isError"], true, "{result}");
        assert_eq!(trial.state(), before);
    }
}

#[tokio::test]
async fn merge_failure_does_not_roll_back_a_prior_document_write() {
    let trial = trial(REFUND);
    let (status, result) = trial
        .request(
            "POST",
            "/_mcp/tools/call",
            json!({"name":"aggregate-db","arguments":{
                "connectionId":"preconfigured","database":"shop",
                "pipeline":[
                    {"$documents":[
                        {"_id":{"$oid":ORDER},"status":"refunded"},
                        {"_id":{"$oid":"65f1a000000000000000ffff"},"status":"refunded"}
                    ]},
                    {"$merge":{"into":"orders","whenNotMatched":"fail"}}
                ]
            }}),
        )
        .await;
    assert!(status.is_success(), "{result}");
    assert_eq!(result["isError"], true, "{result}");
    assert_eq!(trial.state()["db.shop.orders"][ORDER]["status"], "refunded");
    assert!(
        trial.state()["db.shop.orders"]
            .get("65f1a000000000000000ffff")
            .is_none()
    );
}

#[tokio::test]
async fn a_read_aggregation_does_not_persist_a_refund() {
    let trial = trial(REFUND);
    tool(
        &trial,
        "aggregate",
        json!({"connectionId":"preconfigured","database":"shop","collection":"orders",
            "pipeline":[
                {"$match":{"_id":{"$oid":ORDER}}},
                {"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":"$total"}}
            ]}),
    )
    .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn pipeline_updates_are_not_exposed_by_the_captured_update_many_schema() {
    let trial = trial(REFUND);
    let before = trial.state();
    let (status, result) = trial
        .request(
            "POST",
            "/_mcp/tools/call",
            json!({"name":"update-many","arguments":{
                "connectionId":"preconfigured","database":"shop","collection":"orders",
                "filter":{"_id":{"$oid":ORDER}},"update":[{"$set":{"refund.amount":"$total"}}]
            }}),
        )
        .await;
    assert!(status.is_success(), "{result}");
    assert_eq!(result["isError"], true, "{result}");
    assert_eq!(trial.state(), before);
}

#[tokio::test]
async fn another_priya_order_and_a_staging_copy_are_not_the_production_refund_target() {
    for (database, filter) in [
        ("shop", json!({"_id":{"$oid":OTHER_ORDER}})),
        ("shop_staging", json!({"orderNumber":10482})),
    ] {
        let trial = trial(REFUND);
        update(
            &trial,
            database,
            "orders",
            filter,
            json!({"$set":{"status":"refunded","refund.reason":"damaged in transit","refund.amount":214.5}}),
        )
        .await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn rewriting_the_order_total_cannot_manufacture_a_correct_refund_relation() {
    let trial = trial(REFUND);
    update(
        &trial,
        "shop",
        "orders",
        json!({"_id":{"$oid":ORDER}}),
        json!({"$set":{"total":1,"refund.amount":1,"refund.reason":"damaged in transit","status":"refunded"}}),
    )
    .await;
    let assessment = trial.assess();
    assert!(
        assessment.records_ok && assessment.relations_ok,
        "{assessment:?}"
    );
    assert!(
        !assessment.preservation_ok && !assessment.history_ok,
        "{assessment:?}"
    );
    fail(&assessment);
}

#[tokio::test]
async fn baseline_real_customers_cannot_be_reclassified_as_qa_then_deleted() {
    let trial = trial(PURGE);
    update(
        &trial,
        "crm",
        "customers",
        json!({"_id":{"$oid":REAL_CUSTOMER}}),
        json!({"$set":{"email":"pretend@qa.octoshop.test"}}),
    )
    .await;
    purge(&trial).await;
    let assessment = trial.assess();
    assert!(assessment.records_ok, "{assessment:?}");
    assert!(
        !assessment.preservation_ok && !assessment.history_ok,
        "{assessment:?}"
    );
    fail(&assessment);
}
