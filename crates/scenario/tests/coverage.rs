use catalog::Catalog;
use scenario::{coverage, discover, oracle_tools::OracleTools};
use std::path::Path;

#[test]
fn record_fact_answers_are_inventoried_as_source_bound() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(root, &catalog).unwrap();
    let scenario =
        scenario::Scenario::load(&root.join("scenarios/search/elasticsearch/latency-by-region"))
            .unwrap();
    let calls = scenario
        .validate(&root.join("scenarios"), &catalog, &tools)
        .unwrap();
    let inventory =
        coverage::inventory(std::slice::from_ref(&scenario), vec![calls], &tools).unwrap();
    assert_eq!(
        inventory.scenarios[0].answer_check,
        "source_bound_record_facts",
    );
}

#[test]
fn summary_source_gaps_follow_answer_semantics_and_exclude_unscored_examples() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(root, &catalog).unwrap();
    let mut scenario =
        scenario::Scenario::load(&root.join("scenarios/search/exa/fleet-charger-research"))
            .unwrap();
    let calls = scenario
        .validate(&root.join("scenarios"), &catalog, &tools)
        .unwrap();
    let initial =
        coverage::inventory(std::slice::from_ref(&scenario), vec![calls.clone()], &tools).unwrap();
    assert!(
        !initial
            .summary
            .scored_source_gaps
            .contains_key("legacy_answer_semantics")
    );
    assert_eq!(
        initial.summary.scored_source_gaps["legacy_answer_file_collection"],
        1
    );
    scenario.expect.answer =
        Some(serde_json::from_value(serde_json::json!({"contains": ["diagnostic"]})).unwrap());
    let changed =
        coverage::inventory(std::slice::from_ref(&scenario), vec![calls.clone()], &tools).unwrap();
    let machine = serde_json::to_value(&changed).unwrap();
    assert_eq!(
        machine["summary"]["scored_source_gaps"]["legacy_answer_semantics"],
        1
    );
    assert!(
        !changed
            .summary
            .scored_source_gaps
            .contains_key("legacy_answer_file_collection")
    );
    scenario.meta.id = format!("example/{}", scenario.meta.id);
    let example =
        coverage::inventory(std::slice::from_ref(&scenario), vec![calls], &tools).unwrap();
    assert_eq!(example.summary.scored_tasks, 0);
    assert_eq!(example.summary.unscored_examples, 1);
    assert!(example.summary.scored_source_gaps.is_empty());
    assert!(
        example.scenarios[0]
            .source_gaps
            .contains(&"legacy_answer_semantics")
    );
}

#[test]
fn declared_environment_faults_are_visible_outside_migration_counts() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(root, &catalog).unwrap();
    let mut scenario =
        scenario::Scenario::load(&root.join("scenarios/search/tavily/competitor-pricing-research"))
            .unwrap();
    scenario.expect.environment_faults.clear();
    let calls = scenario
        .validate(&root.join("scenarios"), &catalog, &tools)
        .unwrap();
    let initial =
        coverage::inventory(std::slice::from_ref(&scenario), vec![calls.clone()], &tools).unwrap();
    assert_eq!(initial.summary.scored_declared_unscorable, 0);
    scenario
        .expect
        .environment_faults
        .push(mock_core::grading::EnvironmentFault {
            code: "unsupported_answer_contract".into(),
            reason: "Declared source-only coverage cannot establish scoreability.".into(),
        });
    let inventory =
        coverage::inventory(std::slice::from_ref(&scenario), vec![calls], &tools).unwrap();
    assert_eq!(inventory.summary.scored_tasks, 1);
    assert_eq!(inventory.summary.scored_version_two, 1);
    assert_eq!(inventory.summary.scored_declared_unscorable, 1);
    assert_eq!(
        inventory.scenarios[0].declared_environment_faults,
        ["unsupported_answer_contract"]
    );
    assert!(
        inventory.scenarios[0]
            .source_gaps
            .contains(&"declared_environment_fault")
    );
}

#[test]
fn source_inventory_counts_real_declarations_without_certifying_semantics_or_trial_evidence() {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let catalog = Catalog::load(&root.join("catalog/services.toml")).unwrap();
    let tools = OracleTools::load(root, &catalog).unwrap();
    let scenarios = discover(&root.join("scenarios")).unwrap();
    let validated = scenarios
        .iter()
        .map(|scenario| {
            scenario
                .validate(&root.join("scenarios"), &catalog, &tools)
                .unwrap()
        })
        .collect();
    let inventory = coverage::inventory(&scenarios, validated, &tools).unwrap();
    assert_eq!(inventory.summary.scored_tasks, 150);
    assert_eq!(inventory.summary.unscored_examples, 2);
    assert_eq!(inventory.summary.unscored_diagnostics, 2);
    family_metadata_is_exported(&inventory);
    assert_eq!(inventory.summary.scored_version_two, 150);
    assert_eq!(inventory.summary.scored_legacy, 0);
    assert_eq!(inventory.summary.scored_declared_unscorable, 0);
    assert_eq!(
        inventory.summary.scored_declared_unscorable,
        inventory
            .scenarios
            .iter()
            .filter(|row| row.scored && !row.declared_environment_faults.is_empty())
            .count()
    );
    assert_eq!(inventory.summary.scored_lookups, 50);
    assert_eq!(inventory.summary.scored_writes, 100);
    let firecrawl_writes: Vec<_> = inventory
        .scenarios
        .iter()
        .filter(|row| {
            row.id.starts_with("search/firecrawl/") && row.tags.iter().any(|tag| tag == "write")
        })
        .collect();
    assert_eq!(firecrawl_writes.len(), 3);
    for row in &firecrawl_writes {
        assert!(row.preservation_required && row.history_required);
        assert!(row.cause_scoped_permissions > 0);
        assert_eq!(row.state_checks, 0);
    }
    assert_eq!(
        firecrawl_writes
            .iter()
            .map(|row| row.firecrawl_crawl_checks)
            .sum::<usize>(),
        1
    );
    assert_eq!(
        firecrawl_writes
            .iter()
            .map(|row| row.firecrawl_monitor_checks)
            .sum::<usize>(),
        1
    );
    assert_eq!(
        firecrawl_writes
            .iter()
            .map(|row| row.declared_content.conditional_reviewed_text)
            .sum::<usize>(),
        1
    );
    assert_eq!(
        inventory.summary.declared_result_contracts,
        std::collections::BTreeMap::from([
            (scenario::oracle_results::ResultContract::AwsApiV1, 11),
            (scenario::oracle_results::ResultContract::McpEnvelopeV1, 359),
            (
                scenario::oracle_results::ResultContract::FirecrawlNativeV1,
                8
            ),
            (scenario::oracle_results::ResultContract::ArgoApiV1, 21),
            (scenario::oracle_results::ResultContract::OktaApiV1, 19),
            (scenario::oracle_results::ResultContract::TerraformApiV1, 16),
            (scenario::oracle_results::ResultContract::GrafanaApiV1, 12),
            (scenario::oracle_results::ResultContract::PagerdutyApiV1, 9),
            (scenario::oracle_results::ResultContract::DatadogApiV1, 3),
            (
                scenario::oracle_results::ResultContract::GoogleWorkspaceReplicaV1,
                4,
            ),
        ])
    );
    assert_eq!(inventory.summary.declared_api_error_permissions, 1);
    let google_share = inventory
        .scenarios
        .iter()
        .find(|row| row.id == "collab/google/share-soc2-with-contoso")
        .unwrap();
    assert!(google_share.preservation_required && google_share.history_required);
    assert!(google_share.cause_scoped_permissions > 0);
    assert_eq!(google_share.state_checks, 0);
    for row in inventory.scenarios.iter().filter(|row| {
        row.id.starts_with("collab/google/") && row.tags.iter().any(|tag| tag == "write")
    }) {
        assert!(row.preservation_required && row.history_required);
        assert!(row.cause_scoped_permissions > 0);
        assert_eq!(row.state_checks, 0);
    }
    let datadog_writes: Vec<_> = inventory
        .scenarios
        .iter()
        .filter(|row| {
            row.id.starts_with("ops/datadog/") && row.tags.iter().any(|tag| tag == "write")
        })
        .collect();
    assert_eq!(datadog_writes.len(), 3);
    for row in datadog_writes {
        assert!(row.preservation_required && row.history_required);
        assert!(row.cause_scoped_permissions > 0);
        assert_eq!(row.state_checks, 0);
        assert_eq!(
            row.information_prerequisites,
            usize::from(row.id.ends_with("/mute-host-from-error-logs"))
        );
        assert_eq!(
            row.tag_scope_checks,
            usize::from(row.id.ends_with("/schedule-replica-downtime"))
        );
    }
    assert_eq!(
        inventory
            .scenarios
            .iter()
            .map(|row| row.quantity_checks)
            .sum::<usize>(),
        2
    );
    assert_eq!(
        inventory
            .scenarios
            .iter()
            .map(|row| row.latest_record_checks)
            .sum::<usize>(),
        4
    );
    for id in [
        "search/exa/enrich-lead-list",
        "search/exa/fleet-charger-research",
        "search/tavily/cold-chain-market-research",
    ] {
        let research = inventory.scenarios.iter().find(|row| row.id == id).unwrap();
        assert_eq!(research.declared_content.source_bound_answers, 1);
        assert_eq!(
            research.answer_check,
            "source_bound_facts_with_typed_rows_and_prose_reading"
        );
        assert!(research.declared_environment_faults.is_empty());
        assert!(!research.source_gaps.contains(&"legacy_answer_semantics"));
        assert!(
            research
                .source_gaps
                .contains(&"legacy_answer_file_collection")
        );
    }
    for id in [
        "ops/multi/checkout-saturation-scale-and-resolve",
        "ops/multi/oom-limit-and-sentry-resolve",
    ] {
        let workflow = inventory.scenarios.iter().find(|row| row.id == id).unwrap();
        assert_eq!(workflow.contract_version, 2);
        assert_eq!(workflow.state_checks, 0);
        assert!(workflow.preservation_required && workflow.history_required);
        assert!(workflow.cause_scoped_permissions > 0 && workflow.causal_checks > 0);
        assert_eq!(
            workflow.quantity_checks,
            usize::from(id.ends_with("/oom-limit-and-sentry-resolve"))
        );
    }
    for row in inventory.scenarios.iter().filter(|row| {
        row.id.starts_with("ops/kubernetes/") && row.tags.iter().any(|tag| tag == "write")
    }) {
        assert!(row.preservation_required && row.history_required);
        assert!(row.cause_scoped_permissions > 0);
    }
    let okta_writes: Vec<_> = inventory
        .scenarios
        .iter()
        .filter(|row| {
            row.id.starts_with("cloud/okta/") && row.tags.iter().any(|tag| tag == "write")
        })
        .collect();
    assert_eq!(okta_writes.len(), 3);
    for row in okta_writes {
        assert!(row.preservation_required && row.history_required);
        assert!(row.cause_scoped_permissions > 0);
    }
    let terraform_writes: Vec<_> = inventory
        .scenarios
        .iter()
        .filter(|row| {
            row.id.starts_with("code/terraform/") && row.tags.iter().any(|tag| tag == "write")
        })
        .collect();
    assert_eq!(terraform_writes.len(), 3);
    for row in terraform_writes {
        assert!(row.preservation_required && row.history_required);
        assert!(row.relationship_checks > 0 && row.cause_scoped_permissions > 0);
    }
    let pagerduty_writes: Vec<_> = inventory
        .scenarios
        .iter()
        .filter(|row| {
            row.id.starts_with("ops/pagerduty/") && row.tags.iter().any(|tag| tag == "write")
        })
        .collect();
    assert_eq!(pagerduty_writes.len(), 3);
    for row in pagerduty_writes {
        assert!(row.preservation_required && row.history_required);
        assert!(row.relationship_checks > 0 && row.cause_scoped_permissions > 0);
        assert_eq!(row.state_checks, 0);
    }
    assert_eq!(
        inventory
            .scenarios
            .iter()
            .find(|row| row.id == "ops/pagerduty/cover-shift-override")
            .unwrap()
            .interval_coverage_checks,
        1
    );
    assert_eq!(
        inventory
            .scenarios
            .iter()
            .find(|row| row.id == "ops/pagerduty/reassign-replication-lag")
            .unwrap()
            .array_length_permissions,
        1
    );
    let mongodb_writes: Vec<_> = inventory
        .scenarios
        .iter()
        .filter(|row| {
            row.id.starts_with("cloud/mongodb/") && row.tags.iter().any(|tag| tag == "write")
        })
        .collect();
    assert_eq!(mongodb_writes.len(), 3);
    for row in mongodb_writes {
        assert!(row.preservation_required && row.history_required);
        assert_eq!(row.record_checks, 1);
        assert_eq!(row.state_checks, 0);
    }
    let cancellation = inventory
        .scenarios
        .iter()
        .find(|row| row.id == "cloud/supabase/cancel-pending-orders-duplicate-customer")
        .unwrap();
    assert_eq!(cancellation.related_record_filters, 2);
    assert_eq!(cancellation.initial_scope_permissions, 1);
    let sentry = inventory
        .scenarios
        .iter()
        .find(|row| row.id == "ops/sentry/create-notification-project")
        .unwrap();
    assert!(sentry.preservation_required && sentry.history_required);
    assert_eq!(sentry.declared_content.source_bound_answers, 1);
    assert!(sentry.reference_answer_template);
    assert_eq!(sentry.answer_check, "source_bound_dsn_with_prose_reading");
    assert!(
        sentry
            .source_gaps
            .contains(&"legacy_answer_file_collection")
    );
    let oncall = inventory
        .scenarios
        .iter()
        .find(|row| row.id == "ops/grafana/ack-checkout-page-oncall")
        .unwrap();
    assert!(oncall.preservation_required && oncall.history_required);
    assert_eq!(oncall.declared_content.source_bound_answers, 1);
    assert_eq!(oncall.related_record_filters, 2);
    assert!(oncall.reference_answer_template);
    assert_eq!(
        oncall.answer_check,
        "source_bound_oncall_with_prose_reading"
    );
    assert!(
        oncall
            .source_gaps
            .contains(&"legacy_answer_file_collection")
    );
    assert!(!oncall.source_gaps.contains(&"legacy_answer_semantics"));
    assert_eq!(
        inventory
            .scenarios
            .iter()
            .map(|row| row.metric_alarm_checks)
            .sum::<usize>(),
        1
    );
    assert_eq!(
        inventory
            .scenarios
            .iter()
            .find(|row| row.id == "code/github/approve-merge-dependency-bump")
            .unwrap()
            .declared_content
            .optional_reviewed_text,
        1
    );
    assert!(inventory.summary.deferred_argument_schemas > 0);
    assert_eq!(
        inventory.summary.reference_calls,
        inventory.summary.static_argument_schemas + inventory.summary.deferred_argument_schemas
    );
    assert!(
        inventory
            .limits
            .contains(&"schema_validity_does_not_establish_operation_semantics")
    );
    assert!(
        inventory
            .limits
            .contains(&"trial_oracle_counterexample_evidence_is_not_inferred_from_source")
    );
    assert!(
        inventory
            .snapshots
            .iter()
            .all(|snapshot| snapshot.sha256.starts_with("sha256:"))
    );
}

/// Every scored row carries its reviewed family; diagnostics stay out of the
/// scored denominator and are the only no-action/clarification rows.
fn family_metadata_is_exported(inventory: &coverage::Inventory) {
    let summary = &inventory.summary;
    assert_eq!(summary.scored_families.values().sum::<usize>(), 150);
    assert_eq!(
        summary.scored_dependency_depths.values().sum::<usize>(),
        150
    );
    assert!(summary.scored_families.len() >= 5, "{summary:?}");
    assert!(!summary.scored_families.contains_key("no_action"));
    assert!(!summary.scored_families.contains_key("clarification"));
    for row in &inventory.scenarios {
        let family = row.family.as_ref().map(|family| family.primary.key());
        match row.id.split('/').next().unwrap() {
            "example" => assert!(!row.scored && family.is_none(), "{}", row.id),
            "diagnostic" => {
                assert!(!row.scored, "{}", row.id);
                assert!(matches!(family, Some("no_action" | "clarification")));
                assert_eq!(row.scripted_user, family == Some("clarification"));
                assert!(row.source.materialization.is_some(), "{}", row.id);
            }
            _ => {
                assert!(row.scored && !row.scripted_user, "{}", row.id);
                let family = row.family.as_ref().expect("scored family");
                family.validate().unwrap();
                assert!(summary.scored_families.contains_key(family.primary.key()));
            }
        }
    }
    let serialized = serde_json::to_value(inventory).unwrap();
    assert_eq!(
        serialized["scenarios"][0]["family"]["schema_version"],
        scenario::family::FAMILY_SCHEMA
    );
}
