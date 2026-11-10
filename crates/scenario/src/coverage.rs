use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    Scenario,
    oracle_tools::{ArgumentValidation, CallValidation, OracleTools, SnapshotIdentity},
};

#[derive(Debug, Serialize)]
pub struct Inventory {
    pub schema_version: u32,
    pub scope: &'static str,
    pub validator: &'static str,
    pub limits: Vec<&'static str>,
    pub summary: Summary,
    pub authoring: crate::authoring::Summary,
    pub snapshots: Vec<SnapshotIdentity>,
    pub scenarios: Vec<ScenarioCoverage>,
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub scored_tasks: usize,
    pub unscored_examples: usize,
    pub unscored_diagnostics: usize,
    /// Scored tasks by reviewed primary difficulty family.
    pub scored_families: BTreeMap<&'static str, usize>,
    /// Scored tasks that also exercise each secondary axis.
    pub scored_secondary_families: BTreeMap<&'static str, usize>,
    /// Scored tasks by information/operation dependency depth.
    pub scored_dependency_depths: BTreeMap<u32, usize>,
    pub scored_version_two: usize,
    pub scored_legacy: usize,
    pub scored_declared_unscorable: usize,
    pub scored_source_gaps: BTreeMap<&'static str, usize>,
    pub scored_lookups: usize,
    pub scored_writes: usize,
    pub reference_calls: usize,
    pub static_argument_schemas: usize,
    pub deferred_argument_schemas: usize,
    pub declared_result_contracts: BTreeMap<crate::oracle_results::ResultContract, usize>,
    pub declared_api_error_permissions: usize,
}

#[derive(Debug, Serialize)]
pub struct ScenarioCoverage {
    pub id: String,
    pub source: mock_core::provenance::DatasetSource,
    pub instruction_sha256: String,
    pub scored: bool,
    /// The reviewed `[scenario.family]` assignment (absent only for examples).
    pub family: Option<crate::family::ScenarioFamily>,
    pub scripted_user: bool,
    pub services: Vec<String>,
    pub tags: Vec<String>,
    pub contract_version: u32,
    pub preservation_required: bool,
    pub history_required: bool,
    pub state_checks: usize,
    pub record_checks: usize,
    pub metric_alarm_checks: usize,
    pub firecrawl_crawl_checks: usize,
    pub firecrawl_monitor_checks: usize,
    pub quantity_checks: usize,
    pub interval_coverage_checks: usize,
    pub tag_scope_checks: usize,
    pub array_length_permissions: usize,
    pub latest_record_checks: usize,
    pub cause_scoped_permissions: usize,
    pub initial_scope_permissions: usize,
    pub related_record_filters: usize,
    pub relationship_checks: usize,
    pub causal_checks: usize,
    pub information_prerequisites: usize,
    pub declared_content: ContentCoverage,
    pub effect_permissions: BTreeMap<&'static str, usize>,
    pub answer_check: &'static str,
    pub reference_answer_template: bool,
    pub declared_environment_faults: Vec<String>,
    pub authoring: crate::authoring::ScenarioAuthoring,
    pub source_gaps: Vec<&'static str>,
    pub reference_calls: Vec<CallValidation>,
}

#[derive(Debug, Default, Serialize)]
pub struct ContentCoverage {
    pub literal_business_text: usize,
    pub literal_html: usize,
    pub reviewed_static_text: usize,
    pub optional_reviewed_text: usize,
    pub conditional_reviewed_text: usize,
    pub identity_bound_text: usize,
    pub reviewed_field_projections: usize,
    pub source_bound_answers: usize,
}

fn reference_filters(selector: &mock_core::relations::RecordSelector) -> usize {
    selector
        .references
        .iter()
        .map(|reference| 1 + reference_filters(&reference.target))
        .sum()
}

pub fn inventory(
    scenarios: &[Scenario],
    calls: Vec<Vec<CallValidation>>,
    tools: &OracleTools,
) -> anyhow::Result<Inventory> {
    anyhow::ensure!(
        scenarios.len() == calls.len(),
        "coverage needs one validated call list per scenario"
    );
    let mut summary = Summary::default();
    let mut rows = Vec::new();
    let mut authoring = crate::authoring::Accumulator::default();
    for (scenario, reference_calls) in scenarios.iter().zip(calls) {
        anyhow::ensure!(
            reference_calls.len() == scenario.solution.calls.len(),
            "coverage requires every reference call for {}",
            scenario.meta.id
        );
        anyhow::ensure!(
            reference_calls
                .iter()
                .zip(&scenario.solution.calls)
                .all(|(checked, actual)| checked.endpoint == actual.endpoint
                    && checked.tool == actual.tool),
            "coverage reference identities differ for {}",
            scenario.meta.id
        );
        let scored = scenario.scored();
        if scored {
            summary.scored_tasks += 1;
            if let Some(family) = &scenario.meta.family {
                *summary
                    .scored_families
                    .entry(family.primary.key())
                    .or_default() += 1;
                for secondary in &family.secondary {
                    *summary
                        .scored_secondary_families
                        .entry(secondary.key())
                        .or_default() += 1;
                }
                *summary
                    .scored_dependency_depths
                    .entry(family.dependency_depth)
                    .or_default() += 1;
            }
            summary.scored_declared_unscorable +=
                usize::from(!scenario.expect.environment_faults.is_empty());
            if scenario.expect.version == 2 {
                summary.scored_version_two += 1
            } else {
                summary.scored_legacy += 1
            }
            summary.scored_lookups +=
                usize::from(scenario.meta.tags.iter().any(|tag| tag == "lookup"));
            summary.scored_writes +=
                usize::from(scenario.meta.tags.iter().any(|tag| tag == "write"));
        } else if scenario.meta.id.starts_with("diagnostic/") {
            summary.unscored_diagnostics += 1
        } else {
            summary.unscored_examples += 1
        }
        let static_count = reference_calls
            .iter()
            .filter(|call| call.arguments == ArgumentValidation::StaticValidated)
            .count();
        summary.reference_calls += reference_calls.len();
        summary.static_argument_schemas += static_count;
        summary.deferred_argument_schemas += reference_calls.len() - static_count;
        for call in &reference_calls {
            *summary
                .declared_result_contracts
                .entry(call.result_contract)
                .or_insert(0) += 1;
            summary.declared_api_error_permissions += call.allowed_api_errors.len();
        }
        let mut declared_content = ContentCoverage {
            literal_business_text: scenario
                .expect
                .state
                .iter()
                .filter(|check| check.text_equals.is_some())
                .count(),
            ..ContentCoverage::default()
        };
        let mut related_record_filters = 0;
        for record in &scenario.expect.records {
            declared_content.conditional_reviewed_text +=
                usize::from(record.firecrawl_monitor.is_some());
            related_record_filters += reference_filters(&record.selector);
            declared_content.literal_html += record.html_text.len();
            declared_content.reviewed_field_projections +=
                usize::from(record.reviewed_fields.is_some());
            for text in record.reviewed_text.values() {
                if let Some(bound) = &text.bound {
                    related_record_filters += bound
                        .identities
                        .values()
                        .map(|identity| reference_filters(&identity.selector))
                        .sum::<usize>();
                }
                declared_content.optional_reviewed_text += usize::from(text.optional);
                if text.bound.is_some() {
                    declared_content.identity_bound_text += 1
                } else {
                    declared_content.reviewed_static_text += 1
                }
            }
        }
        for relationship in &scenario.expect.relations {
            related_record_filters +=
                reference_filters(&relationship.source) + reference_filters(&relationship.target);
        }
        for ordering in &scenario.expect.ordering {
            related_record_filters += reference_filters(&ordering.prerequisite.selector)
                + reference_filters(&ordering.action.selector);
        }
        if let Some(check) = scenario
            .expect
            .answer
            .as_ref()
            .and_then(|answer| answer.sentry_dsn.as_ref())
        {
            related_record_filters += reference_filters(&check.project)
                + reference_filters(&check.keys)
                + reference_filters(&check.ingestion_origin.selector);
            declared_content.source_bound_answers += 1;
        }
        if let Some(check) = scenario
            .expect
            .answer
            .as_ref()
            .and_then(|answer| answer.oncall.as_ref())
        {
            related_record_filters += reference_filters(&check.schedule);
            declared_content.source_bound_answers += 1;
        }
        if let Some(check) = scenario
            .expect
            .answer
            .as_ref()
            .and_then(|answer| answer.source_facts.as_ref())
        {
            related_record_filters += reference_filters(&check.source.selector);
            declared_content.source_bound_answers += 1;
        }
        if let Some(check) = scenario
            .expect
            .answer
            .as_ref()
            .and_then(|answer| answer.record_facts.as_ref())
        {
            related_record_filters += reference_filters(&check.selector);
            declared_content.source_bound_answers += 1;
        }
        let mut effect_permissions = BTreeMap::new();
        let mut initial_scope_permissions = 0;
        if let Some(policy) = &scenario.expect.preservation {
            for rule in &policy.allow {
                initial_scope_permissions += usize::from(
                    rule.initial_matching.is_some() || !rule.initial_references.is_empty(),
                );
                related_record_filters += rule
                    .initial_references
                    .iter()
                    .chain(&rule.references)
                    .map(|reference| 1 + reference_filters(&reference.target))
                    .sum::<usize>();
                let name = match rule.kind {
                    mock_core::preservation::ChangeKind::Update => "update",
                    mock_core::preservation::ChangeKind::Create => "create",
                    mock_core::preservation::ChangeKind::Delete => "delete",
                    mock_core::preservation::ChangeKind::Append => "append",
                    mock_core::preservation::ChangeKind::RemoveField => "remove_field",
                    mock_core::preservation::ChangeKind::RestoreField => "restore_field",
                };
                *effect_permissions.entry(name).or_insert(0) += 1;
            }
        }
        let preservation_required =
            scenario.expect.version >= 2 || scenario.expect.preservation.is_some();
        let history_required =
            scenario.expect.audit_history || !scenario.expect.ordering.is_empty();
        let mut source_gaps = Vec::new();
        if !scenario.expect.environment_faults.is_empty() {
            source_gaps.push("declared_environment_fault")
        }
        if !preservation_required {
            source_gaps.push("no_default_preservation")
        }
        if !history_required {
            source_gaps.push("no_authoritative_history_requirement")
        }
        if scenario.expect.answer.as_ref().is_some_and(|answer| {
            answer.sentry_dsn.is_none()
                && answer.oncall.is_none()
                && answer.source_facts.is_none()
                && answer.record_facts.is_none()
        }) {
            source_gaps.push("legacy_answer_semantics")
        }
        if scenario.expect.answer.as_ref().is_some_and(|answer| {
            answer.source == mock_core::grading::AnswerSource::LegacyAnswerFile
                && (answer.sentry_dsn.is_some()
                    || answer.oncall.is_some()
                    || answer.source_facts.is_some()
                    || answer.record_facts.is_some())
        }) {
            source_gaps.push("legacy_answer_file_collection");
        }
        if static_count != reference_calls.len() {
            source_gaps.push("captured_argument_constraints_deferred")
        }
        let scenario_authoring =
            authoring.review(scenario, &reference_calls, tools, scored, &mut source_gaps);
        if scored {
            for gap in &source_gaps {
                *summary.scored_source_gaps.entry(*gap).or_default() += 1;
            }
        }
        rows.push(ScenarioCoverage {
            id: scenario.meta.id.clone(),
            interval_coverage_checks: scenario
                .expect
                .records
                .iter()
                .filter(|check| check.interval_coverage.is_some())
                .count(),
            tag_scope_checks: scenario
                .expect
                .records
                .iter()
                .filter(|check| check.tag_scope.is_some())
                .count(),
            array_length_permissions: scenario.expect.preservation.as_ref().map_or(0, |policy| {
                policy
                    .allow
                    .iter()
                    .map(|rule| rule.array_lengths.len())
                    .sum()
            }),
            source: mock_core::dataset::Dataset::load(&scenario.dir)?.source,
            instruction_sha256: mock_core::runner::sha256(scenario.instruction.as_bytes()),
            scored,
            family: scenario.meta.family.clone(),
            scripted_user: mock_core::dataset::Dataset::load(&scenario.dir)?
                .scripted_user
                .is_some(),
            services: scenario.meta.services.clone(),
            tags: scenario.meta.tags.clone(),
            contract_version: scenario.expect.version,
            preservation_required,
            history_required,
            state_checks: scenario.expect.state.len(),
            record_checks: scenario.expect.records.len(),
            metric_alarm_checks: scenario
                .expect
                .records
                .iter()
                .filter(|check| check.metric_alarm.is_some())
                .count(),
            firecrawl_crawl_checks: scenario
                .expect
                .records
                .iter()
                .filter(|check| check.firecrawl_crawl.is_some())
                .count(),
            firecrawl_monitor_checks: scenario
                .expect
                .records
                .iter()
                .filter(|check| check.firecrawl_monitor.is_some())
                .count(),
            quantity_checks: scenario
                .expect
                .records
                .iter()
                .map(|check| check.quantity_fields.len())
                .sum(),
            latest_record_checks: scenario
                .expect
                .records
                .iter()
                .filter(|check| check.selector.latest_by.is_some())
                .count(),
            cause_scoped_permissions: scenario.expect.preservation.as_ref().map_or(0, |policy| {
                policy
                    .allow
                    .iter()
                    .filter(|rule| rule.cause.is_some())
                    .count()
            }),
            initial_scope_permissions,
            related_record_filters,
            relationship_checks: scenario.expect.relations.len(),
            causal_checks: scenario.expect.ordering.len() + scenario.expect.information.len(),
            information_prerequisites: scenario.expect.information.len(),
            declared_content,
            effect_permissions,
            answer_check: if scenario
                .expect
                .answer
                .as_ref()
                .is_some_and(|answer| answer.sentry_dsn.is_some())
            {
                "source_bound_dsn_with_prose_reading"
            } else if scenario
                .expect
                .answer
                .as_ref()
                .is_some_and(|answer| answer.oncall.is_some())
            {
                "source_bound_oncall_with_prose_reading"
            } else if scenario
                .expect
                .answer
                .as_ref()
                .is_some_and(|answer| answer.source_facts.is_some())
            {
                "source_bound_facts_with_typed_rows_and_prose_reading"
            } else if scenario
                .expect
                .answer
                .as_ref()
                .is_some_and(|answer| answer.record_facts.is_some())
            {
                "source_bound_record_facts"
            } else if scenario.expect.answer.is_some() {
                "legacy_file_substrings_or_regex"
            } else {
                "none"
            },
            reference_answer_template: scenario.solution.answer_template,
            declared_environment_faults: scenario
                .expect
                .environment_faults
                .iter()
                .map(|fault| fault.code.clone())
                .collect(),
            authoring: scenario_authoring,
            source_gaps,
            reference_calls,
        });
    }
    Ok(Inventory {
        schema_version: 1,
        scope: "source_declarations_and_locked_argument_schemas_only",
        validator: "jsonschema-0.60.2-offline-with-format-annotations",
        limits: vec![
            "schema_validity_does_not_establish_operation_semantics",
            "result_contract_declarations_are_not_collected_tool_result_evidence",
            "accepted_command_results_do_not_prove_underlying_http_execution",
            "only_referenced_tool_schemas_are_compiled",
            "captured_values_are_not_fabricated_as_valid_arguments",
            "content_declarations_do_not_prove_semantic_completeness",
            "trial_oracle_counterexample_evidence_is_not_inferred_from_source",
            "dataset_source_hash_is_not_materialized_runtime_state",
            "native_runtime_completion_and_model_unawareness_are_not_established",
        ],
        summary,
        authoring: authoring.finish(tools),
        snapshots: tools.identities(),
        scenarios: rows,
    })
}
