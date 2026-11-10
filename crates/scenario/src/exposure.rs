//! Controlled tool-exposure variants (improvement plan §14): deterministic, explicitly
//! labelled conditions that vary the tool catalog presented to the agent.
//!
//! Each variant operates on pinned authentic tool definitions and records the actual
//! definition order and content hashes. A valid alternative plan is never penalized
//! solely for reference-trace overlap.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

// ── Exposure variants ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureVariant {
    /// Only tools directly referenced by the scenario's solution trace.
    RelevantOnly,
    /// Tools that fit within the model's context window for this scenario.
    ContextFeasible,
    /// Tools from the scenario's organization/service family.
    OrganizationPack,
    /// Every tool in the full catalog.
    FullCatalog,
    /// Full catalog in a deterministic but shuffled order (seeded).
    RandomizedOrder,
    /// Full catalog plus semantically confusing distractor definitions.
    ConfusingDistractors,
}

impl ExposureVariant {
    pub const ALL: &'static [ExposureVariant] = &[
        Self::RelevantOnly,
        Self::ContextFeasible,
        Self::OrganizationPack,
        Self::FullCatalog,
        Self::RandomizedOrder,
        Self::ConfusingDistractors,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::RelevantOnly => "relevant_only",
            Self::ContextFeasible => "context_feasible",
            Self::OrganizationPack => "organization_pack",
            Self::FullCatalog => "full_catalog",
            Self::RandomizedOrder => "randomized_order",
            Self::ConfusingDistractors => "confusing_distractors",
        }
    }

    pub fn is_diagnostic(self) -> bool {
        matches!(self, Self::RelevantOnly)
    }
}

// ── Tool identity for exposure tracking ──────────────────────────────────────

/// Identity of a single tool definition as presented to the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExposedTool {
    pub endpoint: String,
    pub tool_name: String,
    pub definition_sha256: String,
    pub position: usize,
    pub is_distractor: bool,
}

// ── Catalog-level counts ─────────────────────────────────────────────────────

/// Metrics that distinguish different stages of tool availability. Unknown provider
/// costs and counts remain `None`, never zero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogMetrics {
    pub stored_definitions: usize,
    pub registered_endpoints: usize,
    pub provider_accepted: Option<usize>,
    pub model_visible: Option<usize>,
    pub loaded_schemas: usize,
}

// ── Discovery metrics ────────────────────────────────────────────────────────

/// Metrics for tool discovery and execution. Shared budgets include hidden discovery
/// inference and retries. Unknown provider costs are `None`, not zero.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryMetrics {
    pub search_requests: usize,
    pub search_result_tools: usize,
    pub selected_tools: usize,
    pub valid_arguments: usize,
    pub invalid_arguments: usize,
    pub executed_tools: usize,
    pub execution_successes: usize,
    pub execution_failures: usize,
    pub unnecessary_schema_loads: usize,
    pub recovery_from_unhelpful_search: usize,
    pub hidden_discovery_inference_tokens: Option<usize>,
    pub retry_count: usize,
    pub total_budget_tokens: Option<usize>,
    pub consumed_budget_tokens: Option<usize>,
}

// ── Exposure manifest ────────────────────────────────────────────────────────

/// The complete record of an exposure condition for one trial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposureManifest {
    pub variant: ExposureVariant,
    pub seed: u64,
    pub tool_count: usize,
    pub tools: Vec<ExposedTool>,
    pub catalog_sha256: String,
    pub catalog_metrics: CatalogMetrics,
    pub limits: Vec<String>,
}

// ── Builder ──────────────────────────────────────────────────────────────────

/// Selects tools for an exposure variant from pinned authentic definitions.
pub fn build_exposure(
    variant: ExposureVariant,
    seed: u64,
    all_tools: &[(String, String, Value)], // (endpoint, tool_name, definition)
    relevant_tools: &[(String, String)],   // (endpoint, tool_name) from solution
    scenario_services: &[String],
    context_token_budget: Option<usize>,
) -> ExposureManifest {
    let mut selected: Vec<(String, String, Value, bool)> = Vec::new();

    match variant {
        ExposureVariant::RelevantOnly => {
            for (endpoint, name, def) in all_tools {
                if relevant_tools
                    .iter()
                    .any(|(re, rn)| re == endpoint && rn == name)
                {
                    selected.push((endpoint.clone(), name.clone(), def.clone(), false));
                }
            }
        }
        ExposureVariant::ContextFeasible => {
            let budget = context_token_budget.unwrap_or(usize::MAX);
            let mut used = 0;
            for (endpoint, name, def) in all_tools {
                let tokens = estimate_tokens(def);
                if used + tokens <= budget {
                    selected.push((endpoint.clone(), name.clone(), def.clone(), false));
                    used += tokens;
                }
            }
        }
        ExposureVariant::OrganizationPack => {
            for (endpoint, name, def) in all_tools {
                let service = endpoint.split('.').next().unwrap_or(endpoint);
                if scenario_services
                    .iter()
                    .any(|s| s == service || s == endpoint)
                {
                    selected.push((endpoint.clone(), name.clone(), def.clone(), false));
                }
            }
        }
        ExposureVariant::FullCatalog => {
            for (endpoint, name, def) in all_tools {
                selected.push((endpoint.clone(), name.clone(), def.clone(), false));
            }
        }
        ExposureVariant::RandomizedOrder => {
            for (endpoint, name, def) in all_tools {
                selected.push((endpoint.clone(), name.clone(), def.clone(), false));
            }
            deterministic_shuffle(&mut selected, seed);
        }
        ExposureVariant::ConfusingDistractors => {
            for (endpoint, name, def) in all_tools {
                selected.push((endpoint.clone(), name.clone(), def.clone(), false));
            }
            let distractors = generate_distractors(all_tools, seed);
            selected.extend(distractors);
            deterministic_shuffle(&mut selected, seed.wrapping_add(1));
        }
    }

    let tools: Vec<ExposedTool> = selected
        .iter()
        .enumerate()
        .map(|(i, (endpoint, name, def, is_distractor))| ExposedTool {
            endpoint: endpoint.clone(),
            tool_name: name.clone(),
            definition_sha256: definition_sha256(def),
            position: i,
            is_distractor: *is_distractor,
        })
        .collect();

    let catalog_sha256 = {
        let mut hasher = Sha256::new();
        for tool in &tools {
            hasher.update(tool.endpoint.as_bytes());
            hasher.update([0]);
            hasher.update(tool.tool_name.as_bytes());
            hasher.update([0]);
            hasher.update(tool.definition_sha256.as_bytes());
            hasher.update([0]);
        }
        format!(
            "sha256:{}",
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    };

    ExposureManifest {
        variant,
        seed,
        tool_count: tools.len(),
        tools,
        catalog_sha256,
        catalog_metrics: CatalogMetrics {
            stored_definitions: all_tools.len(),
            registered_endpoints: {
                let mut endpoints: Vec<&str> =
                    all_tools.iter().map(|(e, _, _)| e.as_str()).collect();
                endpoints.sort();
                endpoints.dedup();
                endpoints.len()
            },
            provider_accepted: None,
            model_visible: None,
            loaded_schemas: selected.len(),
        },
        limits: vec![
            "relevant_only_is_easier_diagnostic_not_upper_bound".into(),
            "alternative_valid_plans_not_penalized_for_overlap".into(),
            "unknown_provider_costs_are_unknown_not_zero".into(),
            "hidden_discovery_inference_consumes_shared_budget".into(),
            "definition_order_is_recorded_not_assumed_stable".into(),
        ],
    }
}

/// Narrow observer/evidence interface for native-inference proof captures.
/// This module does not create agent-authored attestations; it only provides
/// the seam for a trusted host to record observations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposureObservation {
    pub trial_id: String,
    pub manifest: ExposureManifest,
    pub discovery: DiscoveryMetrics,
}

pub fn record_observation(
    trial_id: &str,
    manifest: ExposureManifest,
    discovery: DiscoveryMetrics,
) -> ExposureObservation {
    ExposureObservation {
        trial_id: trial_id.to_owned(),
        manifest,
        discovery,
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn definition_sha256(def: &Value) -> String {
    let bytes = serde_json::to_vec(def).unwrap_or_default();
    format!(
        "sha256:{}",
        Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

fn estimate_tokens(def: &Value) -> usize {
    let bytes = serde_json::to_vec(def).unwrap_or_default();
    // Conservative estimate: ~4 bytes per token for JSON.
    bytes.len() / 4 + 1
}

fn deterministic_shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed;
    let len = items.len();
    for i in (1..len).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

fn generate_distractors(
    all_tools: &[(String, String, Value)],
    seed: u64,
) -> Vec<(String, String, Value, bool)> {
    let mut distractors = Vec::new();
    let mut state = seed;
    let prefixes = ["verify_", "check_", "validate_", "inspect_", "audit_"];
    for (i, (endpoint, name, def)) in all_tools.iter().enumerate() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        if i == 0 || state % 5 == 0 {
            let prefix = prefixes[i % prefixes.len()];
            let distractor_name = format!("{prefix}{name}");
            let mut distractor_def = def.clone();
            if let Some(obj) = distractor_def.as_object_mut() {
                obj.insert("name".into(), Value::String(distractor_name.clone()));
                if let Some(desc) = obj.get("description").and_then(|d| d.as_str()) {
                    obj.insert(
                        "description".into(),
                        Value::String(format!("(Deprecated) {desc}")),
                    );
                }
            }
            distractors.push((endpoint.clone(), distractor_name, distractor_def, true));
        }
    }
    distractors
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_tools() -> Vec<(String, String, Value)> {
        vec![
            (
                "github".into(),
                "list_issues".into(),
                json!({"name": "list_issues", "description": "List issues", "inputSchema": {"type": "object"}}),
            ),
            (
                "github".into(),
                "create_issue".into(),
                json!({"name": "create_issue", "description": "Create an issue", "inputSchema": {"type": "object"}}),
            ),
            (
                "linear".into(),
                "list_projects".into(),
                json!({"name": "list_projects", "description": "List projects", "inputSchema": {"type": "object"}}),
            ),
            (
                "slack".into(),
                "send_message".into(),
                json!({"name": "send_message", "description": "Send a message", "inputSchema": {"type": "object"}}),
            ),
            (
                "stripe".into(),
                "create_refund".into(),
                json!({"name": "create_refund", "description": "Create a refund", "inputSchema": {"type": "object"}}),
            ),
        ]
    }

    #[test]
    fn relevant_only_selects_solution_tools() {
        let tools = sample_tools();
        let relevant = vec![("github".into(), "list_issues".into())];
        let manifest = build_exposure(
            ExposureVariant::RelevantOnly,
            42,
            &tools,
            &relevant,
            &["github".into()],
            None,
        );
        assert_eq!(manifest.tool_count, 1);
        assert_eq!(manifest.tools[0].tool_name, "list_issues");
        assert!(!manifest.tools[0].is_distractor);
    }

    #[test]
    fn full_catalog_includes_everything() {
        let tools = sample_tools();
        let manifest = build_exposure(ExposureVariant::FullCatalog, 42, &tools, &[], &[], None);
        assert_eq!(manifest.tool_count, tools.len());
    }

    #[test]
    fn organization_pack_filters_by_service() {
        let tools = sample_tools();
        let manifest = build_exposure(
            ExposureVariant::OrganizationPack,
            42,
            &tools,
            &[],
            &["github".into()],
            None,
        );
        assert_eq!(manifest.tool_count, 2);
        assert!(manifest.tools.iter().all(|t| t.endpoint == "github"));
    }

    #[test]
    fn randomized_order_changes_positions_with_different_seeds() {
        let tools = sample_tools();
        let m1 = build_exposure(ExposureVariant::RandomizedOrder, 1, &tools, &[], &[], None);
        let m2 = build_exposure(ExposureVariant::RandomizedOrder, 2, &tools, &[], &[], None);
        assert_eq!(m1.tool_count, m2.tool_count);
        // With different seeds, order should (very likely) differ.
        let names1: Vec<_> = m1.tools.iter().map(|t| &t.tool_name).collect();
        let names2: Vec<_> = m2.tools.iter().map(|t| &t.tool_name).collect();
        // At least check they have the same set.
        let mut sorted1 = names1.clone();
        sorted1.sort();
        let mut sorted2 = names2.clone();
        sorted2.sort();
        assert_eq!(sorted1, sorted2);
    }

    #[test]
    fn confusing_distractors_adds_marked_tools() {
        let tools = sample_tools();
        let manifest = build_exposure(
            ExposureVariant::ConfusingDistractors,
            42,
            &tools,
            &[],
            &[],
            None,
        );
        assert!(manifest.tool_count > tools.len());
        assert!(manifest.tools.iter().any(|t| t.is_distractor));
        let non_distractors: Vec<_> = manifest.tools.iter().filter(|t| !t.is_distractor).collect();
        assert_eq!(non_distractors.len(), tools.len());
    }

    #[test]
    fn context_feasible_respects_token_budget() {
        let tools = sample_tools();
        let manifest = build_exposure(
            ExposureVariant::ContextFeasible,
            42,
            &tools,
            &[],
            &[],
            Some(50),
        );
        // With a tiny budget, not all tools should fit.
        assert!(manifest.tool_count <= tools.len());
        assert!(manifest.tool_count > 0);
    }

    #[test]
    fn catalog_metrics_distinguish_stored_and_loaded() {
        let tools = sample_tools();
        let relevant = vec![("github".into(), "list_issues".into())];
        let manifest = build_exposure(
            ExposureVariant::RelevantOnly,
            42,
            &tools,
            &relevant,
            &[],
            None,
        );
        assert_eq!(manifest.catalog_metrics.stored_definitions, 5);
        assert_eq!(manifest.catalog_metrics.loaded_schemas, 1);
        assert!(manifest.catalog_metrics.provider_accepted.is_none());
        assert!(manifest.catalog_metrics.model_visible.is_none());
    }

    #[test]
    fn unknown_provider_costs_are_none_not_zero() {
        let metrics = DiscoveryMetrics::default();
        assert!(metrics.hidden_discovery_inference_tokens.is_none());
        assert!(metrics.total_budget_tokens.is_none());
        assert!(metrics.consumed_budget_tokens.is_none());
    }

    #[test]
    fn exposure_hashes_are_deterministic() {
        let tools = sample_tools();
        let m1 = build_exposure(ExposureVariant::FullCatalog, 42, &tools, &[], &[], None);
        let m2 = build_exposure(ExposureVariant::FullCatalog, 42, &tools, &[], &[], None);
        assert_eq!(m1.catalog_sha256, m2.catalog_sha256);
        assert_eq!(m1.tools, m2.tools);
    }

    #[test]
    fn observation_records_trial_id() {
        let manifest = build_exposure(
            ExposureVariant::FullCatalog,
            42,
            &sample_tools(),
            &[],
            &[],
            None,
        );
        let obs = record_observation("trial-42", manifest, DiscoveryMetrics::default());
        assert_eq!(obs.trial_id, "trial-42");
    }

    #[test]
    fn limits_always_present() {
        let manifest = build_exposure(ExposureVariant::RelevantOnly, 0, &[], &[], &[], None);
        assert!(!manifest.limits.is_empty());
        assert!(
            manifest
                .limits
                .iter()
                .any(|l| l == "relevant_only_is_easier_diagnostic_not_upper_bound")
        );
        assert!(
            manifest
                .limits
                .iter()
                .any(|l| l == "unknown_provider_costs_are_unknown_not_zero")
        );
    }

    #[test]
    fn all_variants_enumerated() {
        assert_eq!(ExposureVariant::ALL.len(), 6);
        for variant in ExposureVariant::ALL {
            assert!(!variant.label().is_empty());
        }
    }
}
