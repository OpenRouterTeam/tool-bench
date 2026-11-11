mod support;

use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::json;
use support::ContractTrial;

const PROD: &str = "qmvbxtzkplrnwdhyfaco";
const CANCEL: &str = "cloud/supabase/cancel-pending-orders-duplicate-customer";
const RLS: &str = "cloud/supabase/enable-rls-flagged-table";
const PAUSE: &str = "cloud/supabase/pause-legacy-marketing-project";
const LEGACY: &str = "wgtplxkqrzvnmbcdhfye";
const EXTRA: &str = "zqmdxrvkhtnlwfsbycpa";

fn prepared(prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    prepared_for(CANCEL, prepare)
}

fn prepared_for(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    ContractTrial::load_with_dataset(
        id,
        &mock_supabase::Supabase,
        &[("authorization", "Bearer sbp_toolbench")],
        prepare,
    )
}

async fn sql(trial: &ContractTrial, query: &str) {
    trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":query}),
        )
        .await;
}

async fn cancel(trial: &ContractTrial) {
    sql(
        trial,
        "update public.orders set status = 'cancelled' where status = 'pending' and customer_id = (select id from public.customers where email = 'priya.shah@fastmail.com')",
    )
    .await;
}

async fn migration(trial: &ContractTrial, name: &str, query: &str) {
    trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/migrations"),
            json!({"name":name,"query":query}),
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
async fn admitted_schema_defaults_precede_the_baseline_and_reads_do_not_rewrite_them() {
    let trial = prepared(|_| {});
    let before = trial.state();
    assert_eq!(
        before["tables"]["qmvbxtzkplrnwdhyfaco.public.customers"]["rls_forced"],
        false
    );
    assert_eq!(
        before["tables"]["qmvbxtzkplrnwdhyfaco.public.invoices"]["policies"],
        json!([])
    );
    sql(&trial, "select id, total from public.orders").await;
    assert_eq!(trial.state(), before);
    let assessment = trial.assess();
    assert!(
        assessment.preservation_ok && assessment.history_ok,
        "{assessment:?}"
    );
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
}

#[test]
#[should_panic(expected = "unsupported Supabase table fixture shape")]
fn unsupported_fixture_fields_cannot_be_silently_removed_from_the_baseline() {
    prepared(|dataset| {
        dataset
            .services
            .get_mut("supabase")
            .unwrap()
            .collection_mut("tables")
            .get_mut("qmvbxtzkplrnwdhyfaco.public.invoices")
            .unwrap()["unmodeled_field"] = json!(true);
    });
}

#[tokio::test]
async fn cancelling_the_authored_two_orders_does_not_satisfy_all_actual_unpaid_orders() {
    let trial = prepared(|dataset| {
        let orders = dataset
            .services
            .get_mut("supabase")
            .unwrap()
            .collection_mut("db.qmvbxtzkplrnwdhyfaco.public.orders");
        assert!(!orders.contains("5999"));
        let mut added = orders.get("5003").unwrap().clone();
        added["id"] = json!(5999);
        orders.insert("5999", added);
    });
    trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"update public.orders set status = 'cancelled' where id in (5003, 5011)"}),
        )
        .await;
    assert_eq!(
        trial.state()["db.qmvbxtzkplrnwdhyfaco.public.orders"]["5999"]["status"],
        "pending"
    );
    let assessment = trial.assess();
    assert_eq!(assessment.outcome, Outcome::Fail, "{assessment:?}");
    assert!(!assessment.records_ok, "{assessment:?}");
    assert!(
        assessment.contract_ok && assessment.environment_ok && assessment.integrity_ok,
        "{assessment:?}"
    );
}

#[tokio::test]
async fn cancelling_the_complete_pending_set_preserves_business_state_and_history() {
    let trial = prepared(|_| {});
    cancel(&trial).await;
    pass(&trial.assess());
    cancel(&trial).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn correct_cancellation_follows_a_renumbered_exact_email_customer() {
    let trial = prepared(|dataset| {
        let service = dataset.services.get_mut("supabase").unwrap();
        let customers = service.collection_mut("db.qmvbxtzkplrnwdhyfaco.public.customers");
        let mut target = customers.remove("1042").unwrap();
        target["id"] = json!(91042);
        customers.insert("91042", target);
        let orders = service.collection_mut("db.qmvbxtzkplrnwdhyfaco.public.orders");
        let affected: Vec<_> = orders
            .iter()
            .filter(|(_, order)| order["customer_id"] == 1042)
            .map(|(identity, _)| identity.to_owned())
            .collect();
        for identity in affected {
            orders.get_mut(&identity).unwrap()["customer_id"] = json!(91042);
        }
    });
    cancel(&trial).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn a_cancellation_data_migration_is_an_exposed_valid_alternative() {
    let trial = prepared(|_| {});
    trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/migrations"),
            json!({"name":"cancel_unpaid_customer_orders",
                "query":"update public.orders set status = 'cancelled' where status = 'pending' and customer_id = (select id from public.customers where email = 'priya.shah@fastmail.com')"}),
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn unsupported_transaction_control_cannot_fabricate_a_committed_rls_migration() {
    let trial = prepared_for(RLS, |_| {});
    let before = trial.state();
    let (status, result) = trial
        .request(
            "POST",
            &format!("/v1/projects/{PROD}/database/migrations"),
            json!({"name":"rolled_back_rls",
                "query":"begin; alter table public.invoices enable row level security; rollback;"}),
        )
        .await;
    assert_eq!(
        status.as_u16(),
        400,
        "{result}; assessment: {:?}",
        trial.assess()
    );
    assert_eq!(trial.state(), before);
    let assessment = trial.assess();
    assert_eq!(
        assessment.outcome,
        Outcome::EnvironmentError,
        "{assessment:?}"
    );
    assert!(
        assessment.contract_ok && assessment.integrity_ok,
        "{assessment:?}"
    );
    assert!(
        assessment.failures.iter().any(|failure| {
            failure.location == "environment/unsupported_sql_statement"
                && !failure.event_evidence.is_empty()
        }),
        "{assessment:?}"
    );
    trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/migrations"),
            json!({"name":"enable_rls_valid",
                "query":"alter table public.invoices enable row level security;"}),
        )
        .await;
    assert_eq!(
        trial.state()["tables"]["qmvbxtzkplrnwdhyfaco.public.invoices"]["rls_enabled"],
        true
    );
    assert_eq!(trial.assess().outcome, Outcome::EnvironmentError);
}

#[tokio::test]
async fn a_transaction_that_wraps_the_whole_script_commits_like_the_bare_statements() {
    // Postgres runs `BEGIN; ...; COMMIT;` as the statements between them. The careful form of
    // either write is the same work, not an unsupported statement that voids the trial.
    for query in [
        "begin; update public.orders set status = 'cancelled' where customer_id = 1042 and status = 'pending'; commit;",
        "BEGIN;\nUPDATE public.orders SET status = 'cancelled' WHERE id IN (5003, 5011) RETURNING id;\nCOMMIT;",
        "start transaction; update public.orders set status = 'cancelled' where customer_id = 1042 and status = 'pending'; end",
    ] {
        let trial = prepared(|_| {});
        sql(&trial, query).await;
        pass(&trial.assess());
    }
    let tracked = prepared_for(RLS, |_| {});
    migration(
        &tracked,
        "enable_rls_on_invoices",
        "BEGIN;\nALTER TABLE public.invoices ENABLE ROW LEVEL SECURITY;\nCOMMIT;",
    )
    .await;
    pass(&tracked.assess());
    // The wrapper changes nothing about what is written: the wrong customer's order still
    // fails, and a failing statement inside it still commits nothing.
    let collateral = prepared(|_| {});
    sql(
        &collateral,
        "begin; update public.orders set status = 'cancelled' where status = 'pending' and customer_id in (1042, 1107); commit;",
    )
    .await;
    fail(&collateral.assess());
    let broken = prepared(|_| {});
    let before = broken.state();
    let (status, _) = broken
        .request(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"begin; update public.orders set status = 'cancelled' where customer_id = 1042 and status = 'pending'; select missing_column from public.orders; commit;"}),
        )
        .await;
    assert_eq!(status.as_u16(), 400);
    assert_eq!(broken.state(), before);
    // A transaction that ends before the script does is still not modelled.
    let partial = prepared(|_| {});
    let (status, _) = partial
        .request(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"begin; update public.orders set status = 'cancelled' where id = 5003; commit; update public.orders set status = 'cancelled' where id = 5011"}),
        )
        .await;
    assert_eq!(status.as_u16(), 400);
    assert_eq!(partial.assess().outcome, Outcome::EnvironmentError);
}

#[tokio::test]
async fn ordinary_sql_errors_do_not_become_world_capability_failures() {
    for query in ["selec 1", "create extension imaginary_test_extension"] {
        let trial = prepared(|_| {});
        let (status, result) = trial
            .request(
                "POST",
                &format!("/v1/projects/{PROD}/database/query"),
                json!({"query":query}),
            )
            .await;
        assert_eq!(status.as_u16(), 400, "{result}");
        cancel(&trial).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn migration_catalog_reads_honor_sql_projection_filters_and_aggregates() {
    let trial = prepared(|_| {});
    let rows = trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"select count(*) as count from supabase_migrations.schema_migrations where version = '20250902150000'"}),
        )
        .await;
    assert_eq!(rows, json!([{"count":1}]));
}

#[tokio::test]
async fn a_history_read_does_not_short_circuit_a_valid_preceding_cancellation() {
    let trial = prepared(|_| {});
    let rows = trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"update public.orders set status = 'cancelled' where status = 'pending' and customer_id = (select id from public.customers where email = 'priya.shah@fastmail.com'); select count(*) as count from supabase_migrations.schema_migrations"}),
        )
        .await;
    assert_eq!(rows, json!([{"count":4}]));
    pass(&trial.assess());
}

#[tokio::test]
async fn unsupported_catalog_writes_cannot_return_fabricated_read_results() {
    let trial = prepared_for(RLS, |_| {});
    let before = trial.state();
    let (status, result) = trial
        .request(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"delete from supabase_migrations.schema_migrations"}),
        )
        .await;
    assert_eq!(status.as_u16(), 400, "{result}");
    assert_eq!(trial.state(), before);
    assert_eq!(trial.assess().outcome, Outcome::EnvironmentError);
}

#[tokio::test]
async fn equivalent_cancellation_queries_do_not_have_to_copy_the_reference_sql() {
    for query in [
        "update public.orders as orders set status = 'cancelled' from public.customers as customers where orders.customer_id = customers.id and customers.email = 'priya.shah@fastmail.com' and orders.status = 'pending'",
        "update public.orders as orders set status = 'cancelled' where orders.status = 'pending' and exists (select 1 from public.customers as customers where customers.id = orders.customer_id and customers.email = 'priya.shah@fastmail.com')",
        "update public.orders set status = 'cancelled', total = total where status = 'pending' and customer_id in (select id from public.customers where email = 'priya.shah@fastmail.com')",
        "with target_customer(target_id) as (select id from public.customers where email = 'priya.shah@fastmail.com') update public.orders set status = 'cancelled' where status = 'pending' and customer_id in (select target_id from target_customer)",
    ] {
        let trial = prepared(|_| {});
        sql(&trial, query).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn cancellation_does_not_authorize_paid_orders_money_or_customer_edits() {
    for query in [
        "update public.orders set status = 'cancelled' where customer_id = 1042",
        "update public.orders set status = 'cancelled' where status = 'pending'",
        "update public.orders set total = 0 where id = 5011",
        "update public.customers set country = 'unrequested' where id = 1042",
    ] {
        let trial = prepared(|_| {});
        sql(&trial, query).await;
        cancel(&trial).await;
        let assessment = trial.assess();
        fail(&assessment);
        assert!(assessment.records_ok, "{assessment:?}");
        assert!(
            !assessment.preservation_ok && !assessment.history_ok,
            "{assessment:?}"
        );
    }
}

#[tokio::test]
async fn restoring_another_orders_status_does_not_erase_committed_collateral() {
    let trial = prepared(|_| {});
    for status in ["cancelled", "pending"] {
        sql(
            &trial,
            &format!("update public.orders set status = '{status}' where id = 5009"),
        )
        .await;
    }
    cancel(&trial).await;
    let reference = prepared(|_| {});
    cancel(&reference).await;
    assert_eq!(trial.state(), reference.state());
    let assessment = trial.assess();
    fail(&assessment);
    assert!(!assessment.history_ok, "{assessment:?}");
}

#[tokio::test]
async fn catalog_introspection_cannot_hide_an_unsupported_statement_after_success() {
    let trial = prepared(|_| {});
    cancel(&trial).await;
    let (status, result) = trial
        .request(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"grant select on public.invoices to anon; select * from pg_available_extensions()"}),
        )
        .await;
    assert_eq!(
        status.as_u16(),
        400,
        "{result}; assessment: {:?}",
        trial.assess()
    );
    assert_eq!(trial.assess().outcome, Outcome::EnvironmentError);
}

#[tokio::test]
async fn rls_catalog_verification_returns_the_requested_projection_not_a_table_summary() {
    let trial = prepared_for(RLS, |_| {});
    let rows = trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"select c.relrowsecurity as enabled from pg_catalog.pg_class as c join pg_catalog.pg_namespace as n on c.relnamespace = n.oid where n.nspname = 'public' and c.relname = 'invoices'"}),
        )
        .await;
    assert_eq!(rows, json!([{"enabled":false}]));
    migration(
        &trial,
        "secure_invoices",
        "alter table public.invoices enable row level security",
    )
    .await;
    let rows = trial
        .call(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"select c.relrowsecurity as enabled from pg_catalog.pg_class as c join pg_catalog.pg_namespace as n on c.relnamespace = n.oid where n.nspname = 'public' and c.relname = 'invoices'"}),
        )
        .await;
    assert_eq!(rows, json!([{"enabled":true}]));
    pass(&trial.assess());
}

fn native_query_contract() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../services/supabase/assets/native-introspection-v0_15_0.json"
    ))
    .unwrap()
}

#[tokio::test]
async fn native_table_templates_obey_inclusion_exclusion_and_bound_schema_values() {
    let trial = prepared(|_| {});
    let contract = native_query_contract();
    let prefix = contract["tables_prefix"].as_str().unwrap();
    let before = trial.state();
    for (suffix, parameters, count) in [
        ("where schema in ($1)", json!(["public"]), 3),
        ("where schema in ($1, $2)", json!(["public", "unknown"]), 3),
        (
            "where schema not in ($1, $2, $3, $4)",
            contract["system_schemas"].clone(),
            4,
        ),
        ("where schema not in ($1)", json!(["public"]), 1),
        ("where schema in ($1)", json!(["public') or true; --"]), 0),
    ] {
        let rows = trial
            .call(
                "POST",
                &format!("/v1/projects/{PROD}/database/query"),
                json!({"query":format!("{prefix}{suffix}"),"parameters":parameters,"read_only":true}),
            )
            .await;
        assert_eq!(rows.as_array().unwrap().len(), count);
        assert!(
            rows.as_array().unwrap().iter().all(|table| {
                parameters.as_array().unwrap().contains(&table["schema"])
                    != suffix.starts_with("where schema not in")
                    && table["columns"].is_array()
            }),
            "{rows}"
        );
    }
    assert_eq!(trial.state(), before);
    cancel(&trial).await;
    pass(&trial.assess());
}

#[tokio::test]
async fn native_template_binding_errors_are_not_environment_faults() {
    let trial = prepared(|_| {});
    cancel(&trial).await;
    let contract = native_query_contract();
    let query = format!(
        "{}where schema in ($1)",
        contract["tables_prefix"].as_str().unwrap()
    );
    for parameters in [json!([]), json!(["public", "auth"]), json!([true])] {
        let (status, _) = trial
            .request(
                "POST",
                &format!("/v1/projects/{PROD}/database/query"),
                json!({"query":query,"parameters":parameters}),
            )
            .await;
        assert_eq!(status.as_u16(), 400);
    }
    pass(&trial.assess());
}

#[tokio::test]
async fn appended_sql_cannot_be_hidden_by_exact_native_introspection_queries() {
    let contract = native_query_contract();
    for query in [
        format!(
            "{}; grant select on public.invoices to anon",
            contract["extensions_query"].as_str().unwrap()
        ),
        format!(
            "{}where schema in ($1); update public.orders set status = 'cancelled'",
            contract["tables_prefix"].as_str().unwrap()
        ),
    ] {
        let trial = prepared(|_| {});
        cancel(&trial).await;
        let before = trial.state();
        let (status, _) = trial
            .request(
                "POST",
                &format!("/v1/projects/{PROD}/database/query"),
                json!({"query":query,"parameters":["public"]}),
            )
            .await;
        assert_eq!(status.as_u16(), 400);
        assert_eq!(trial.state(), before);
        assert_eq!(trial.assess().outcome, Outcome::EnvironmentError);
    }
}

#[tokio::test]
async fn regclass_lookup_cannot_silently_compare_a_relation_name_to_its_oid() {
    let trial = prepared_for(RLS, |_| {});
    migration(
        &trial,
        "secure_invoices",
        "alter table public.invoices enable row level security",
    )
    .await;
    let (status, _) = trial
        .request(
            "POST",
            &format!("/v1/projects/{PROD}/database/query"),
            json!({"query":"select relrowsecurity from pg_class where oid = 'public.invoices'::regclass"}),
        )
        .await;
    assert_eq!(status.as_u16(), 400);
    assert_eq!(trial.assess().outcome, Outcome::EnvironmentError);
}

#[tokio::test]
async fn an_unrelated_history_entry_cannot_make_sql_only_rls_a_tracked_migration() {
    let trial = prepared_for(RLS, |_| {});
    migration(&trial, "unrelated_read", "select 1").await;
    sql(
        &trial,
        "alter table public.invoices enable row level security",
    )
    .await;
    assert_eq!(
        trial.state()["tables"]["qmvbxtzkplrnwdhyfaco.public.invoices"]["rls_enabled"],
        true
    );
    fail(&trial.assess());
}

#[tokio::test]
async fn tracked_rls_accepts_a_different_migration_name_and_quoted_identifiers() {
    let trial = prepared_for(RLS, |_| {});
    migration(
        &trial,
        "secure_backend_invoices",
        "ALTER TABLE \"public\".\"invoices\" ENABLE ROW LEVEL SECURITY;",
    )
    .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn failed_migrations_commit_neither_prior_statements_nor_history() {
    let trial = prepared_for(RLS, |_| {});
    let before = trial.state();
    let (status, _) = trial
        .request(
            "POST",
            &format!("/v1/projects/{PROD}/database/migrations"),
            json!({
                "name":"failed_rls",
                "query":"alter table public.invoices enable row level security; select missing_column from public.invoices"
            }),
        )
        .await;
    assert_eq!(status.as_u16(), 400);
    assert_eq!(trial.state(), before);
    migration(
        &trial,
        "secure_invoices",
        "alter table public.invoices enable row level security",
    )
    .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn migration_retries_and_individual_cancellations_have_no_trial_wide_call_cap() {
    let trial = prepared_for(RLS, |_| {});
    for name in ["enable_once", "enable_again", "verify_enabled"] {
        migration(
            &trial,
            name,
            "alter table public.invoices enable row level security",
        )
        .await;
    }
    pass(&trial.assess());
    let trial = prepared(|_| {});
    for identity in [5003, 5011] {
        sql(
            &trial,
            &format!("update public.orders set status = 'cancelled' where id = {identity}"),
        )
        .await;
    }
    pass(&trial.assess());
}

#[tokio::test]
async fn rls_does_not_authorize_policies_other_tables_or_staging() {
    for query in [
        "create policy open_invoices on public.invoices for select using (true)",
        "alter table public.invoices force row level security",
        "alter table public.orders disable row level security",
        "update public.invoices set number = 'unrequested' where id = 1",
    ] {
        let trial = prepared_for(RLS, |_| {});
        migration(
            &trial,
            "secure_invoices",
            "alter table public.invoices enable row level security",
        )
        .await;
        sql(&trial, query).await;
        let assessment = trial.assess();
        fail(&assessment);
        assert!(
            !assessment.preservation_ok && !assessment.history_ok,
            "{assessment:?}"
        );
    }
    let trial = prepared_for(RLS, |_| {});
    trial
        .call(
            "POST",
            "/v1/projects/hzrkcwpxnqdlmvbtejsa/database/migrations",
            json!({"name":"unrequested_staging","query":"select 1"}),
        )
        .await;
    migration(
        &trial,
        "secure_invoices",
        "alter table public.invoices enable row level security",
    )
    .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn removing_a_temporary_policy_does_not_erase_an_unauthorized_exposure() {
    let trial = prepared_for(RLS, |_| {});
    sql(
        &trial,
        "create policy open_invoices on public.invoices for select using (true)",
    )
    .await;
    sql(&trial, "drop policy open_invoices on public.invoices").await;
    migration(
        &trial,
        "secure_invoices",
        "alter table public.invoices enable row level security",
    )
    .await;
    let reference = prepared_for(RLS, |_| {});
    migration(
        &reference,
        "secure_invoices",
        "alter table public.invoices enable row level security",
    )
    .await;
    assert_eq!(trial.state(), reference.state());
    fail(&trial.assess());
    assert!(!trial.assess().history_ok);
}

#[tokio::test]
async fn restoring_an_unlisted_project_pause_does_not_erase_the_interruption() {
    let trial = prepared_for(PAUSE, |dataset| {
        let projects = dataset
            .services
            .get_mut("supabase")
            .unwrap()
            .collection_mut("projects");
        let mut added = projects.get(LEGACY).unwrap().clone();
        added["id"] = json!(EXTRA);
        added["name"] = json!("reporting-prod");
        if let Some(reference) = added.get_mut("ref") {
            *reference = json!(EXTRA);
        }
        projects.insert(EXTRA, added);
    });
    for action in ["pause", "restore"] {
        trial
            .call("POST", &format!("/v1/projects/{EXTRA}/{action}"), json!({}))
            .await;
    }
    trial
        .call("POST", &format!("/v1/projects/{LEGACY}/pause"), json!({}))
        .await;
    let assessment = trial.assess();
    fail(&assessment);
    assert!(!assessment.history_ok, "{assessment:?}");
}

#[tokio::test]
async fn pausing_the_legacy_project_requires_terminal_state_and_audited_preservation() {
    let trial = prepared_for(PAUSE, |_| {});
    fail(&trial.assess());
    trial
        .call("POST", &format!("/v1/projects/{LEGACY}/pause"), json!({}))
        .await;
    let observed = trial
        .call("GET", &format!("/v1/projects/{LEGACY}"), json!(null))
        .await;
    assert_eq!(observed["status"], "INACTIVE");
    pass(&trial.assess());
}

#[tokio::test]
async fn paid_project_pause_rejects_without_mutation_and_allows_valid_recovery() {
    let trial = prepared_for(PAUSE, |_| {});
    let before = trial.state();
    let (status, result) = trial
        .request("POST", &format!("/v1/projects/{PROD}/pause"), json!({}))
        .await;
    assert_eq!(status.as_u16(), 400, "{result}");
    assert_eq!(trial.state(), before);
    trial
        .call("POST", &format!("/v1/projects/{LEGACY}/pause"), json!({}))
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn unknown_organization_plan_cannot_fabricate_pause_eligibility() {
    for plan in [json!(null), json!("future_plan")] {
        let trial = prepared_for(PAUSE, |dataset| {
            let organization = dataset
                .services
                .get_mut("supabase")
                .unwrap()
                .collection_mut("organizations")
                .get_mut("northwind-marketing")
                .unwrap()
                .as_object_mut()
                .unwrap();
            if plan.is_null() {
                organization.shift_remove("plan");
            } else {
                organization.insert("plan".to_owned(), plan.clone());
            }
        });
        let before = trial.state();
        let (status, _) = trial
            .request("POST", &format!("/v1/projects/{LEGACY}/pause"), json!({}))
            .await;
        assert_eq!(status.as_u16(), 503);
        assert_eq!(trial.state(), before);
        let assessment = trial.assess();
        assert_eq!(assessment.outcome, Outcome::EnvironmentError);
        assert!(
            assessment.failures.iter().any(|failure| {
                failure.location == "environment/unsupported_project_pause_eligibility"
                    && !failure.event_evidence.is_empty()
            }),
            "{assessment:?}"
        );
    }
}
