//! Information-treatment admission for comparisons, and the per-trial `surface_audit`
//! status (improvement plan §7, §8, §14).
//!
//! A comparison is admitted only when its arms carry the same information, or differ on
//! exactly one declared axis whose actual differences are listed in the output. A result
//! supports a sensitivity claim about those listed differences and nothing more; in
//! particular it never shows that a model was unaware of being evaluated.

use std::collections::BTreeMap;

use anyhow::{Context, bail};
use mock_core::{
    conditions::{
        AWARENESS_LIMITS, BEHAVIORAL_MEASURES, BehavioralMeasure, ConditionRecord,
        FramingCondition, InformationAxis, InformationDifference, admit_matched,
    },
    provenance::TaskContext,
    surface_audit::SurfaceAuditStatus,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::Trial;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InformationTreatment {
    pub axis: InformationAxis,
    /// Awareness cells and exposure variants present in each arm.
    pub conditions_a: Vec<String>,
    pub conditions_b: Vec<String>,
    /// Actual information differences between the arms, deduplicated across tasks.
    pub differences: Vec<InformationDifference>,
    /// `blinded` only when every neutral-framing trial has a clean surface audit;
    /// otherwise the neutral arm is reported as not qualified for that label.
    pub neutral_framing_label: &'static str,
    pub prespecified_measures: Vec<BehavioralMeasure>,
    pub claim: &'static str,
    pub limits: Vec<&'static str>,
}

pub(crate) fn axis_label(axis: InformationAxis) -> String {
    match axis {
        InformationAxis::Framing => "information:framing",
        InformationAxis::Workspace => "information:workspace",
        InformationAxis::Exposure => "information:exposure",
    }
    .into()
}

pub(crate) fn trial_surface_audit(trial: &Trial) -> SurfaceAuditStatus {
    trial
        .provenance
        .as_ref()
        .map(|provenance| provenance.surface_audit.clone())
        .unwrap_or_default()
}

pub(crate) fn surface_audit_counts(trials: &[Trial]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for trial in trials {
        *counts.entry(trial_surface_audit(trial).status).or_insert(0) += 1;
    }
    counts
}

/// The task context with the bindings a declared treatment necessarily changes removed,
/// and the image role (if any) whose build that treatment changes.
pub(crate) fn treated_context(
    context: &TaskContext,
    treatment: Option<InformationAxis>,
) -> (Value, Option<&'static str>) {
    let mut value = json!(context);
    let mut drop = |names: &[&str]| {
        for name in names {
            value[*name] = Value::Null;
        }
    };
    let role = match treatment {
        None => None,
        Some(InformationAxis::Framing) => {
            drop(&["instruction_sha256", "framing_sha256"]);
            None
        }
        Some(InformationAxis::Workspace) => {
            drop(&["profile_sha256", "environment_files"]);
            Some("main")
        }
        Some(InformationAxis::Exposure) => {
            drop(&["profile_sha256", "environment_files", "exposure_sha256"]);
            value["connector_env_sha256"] =
                json!(treated_env(&context.connector_env_sha256, treatment));
            Some("main")
        }
    };
    (value, role)
}

pub(crate) fn treated_env(
    environment: &BTreeMap<String, String>,
    treatment: Option<InformationAxis>,
) -> BTreeMap<String, String> {
    environment
        .iter()
        .filter(|(name, _)| {
            treatment != Some(InformationAxis::Exposure) || name.as_str() != "TOOL_BENCH_EXPOSURE"
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn condition_label(record: &ConditionRecord) -> String {
    format!("{}/{}", record.cell().label(), record.exposure.label())
}

/// Rejects arms that are not matched in information. Trials without a bound condition
/// record make no information claim: they pass an undeclared comparison only through the
/// stricter byte-level compatibility check, and can never carry a declared treatment.
pub(crate) fn admit(
    a: &[Trial],
    b: &[Trial],
    declared: Option<InformationAxis>,
) -> anyhow::Result<Option<InformationTreatment>> {
    let by_task = |trials: &[Trial]| -> anyhow::Result<BTreeMap<String, Option<ConditionRecord>>> {
        let mut conditions: BTreeMap<String, Option<ConditionRecord>> = BTreeMap::new();
        for trial in trials {
            let condition = trial
                .provenance
                .as_ref()
                .and_then(|provenance| provenance.condition.clone());
            if conditions
                .insert(trial.task.clone(), condition.clone())
                .is_some_and(|previous| previous != condition)
            {
                bail!(
                    "repeats of {} ran under different information conditions",
                    trial.task
                );
            }
        }
        Ok(conditions)
    };
    let (left, right) = (by_task(a)?, by_task(b)?);
    let mut differences: Vec<InformationDifference> = Vec::new();
    for (task, condition_a) in &left {
        let Some(condition_b) = right.get(task) else {
            continue;
        };
        let (condition_a, condition_b) = match (condition_a, condition_b, declared) {
            (Some(a), Some(b), _) => (a, b),
            (_, _, None) => continue,
            (_, _, Some(_)) => bail!(
                "a declared information treatment requires bound condition manifests for {task}"
            ),
        };
        let found = admit_matched(condition_a, condition_b, declared)
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("information conditions for {task}"))?;
        for difference in found {
            // Per-task hashes differ by construction; keep one row per item and side pair
            // for the label-level items and one row per hashed item.
            let hashed = difference.a.starts_with("sha256:") || difference.b.starts_with("sha256:");
            let known = differences.iter().any(|existing| {
                existing.item == difference.item
                    && (hashed || (existing.a == difference.a && existing.b == difference.b))
            });
            if !known {
                differences.push(if hashed {
                    InformationDifference {
                        a: "differs per task (bound hash)".into(),
                        b: "differs per task (bound hash)".into(),
                        ..difference
                    }
                } else {
                    difference
                });
            }
        }
    }
    let Some(axis) = declared else {
        return Ok(None);
    };
    let labels = |conditions: &BTreeMap<String, Option<ConditionRecord>>| {
        let mut labels: Vec<String> = conditions.values().flatten().map(condition_label).collect();
        labels.sort();
        labels.dedup();
        labels
    };
    let (conditions_a, conditions_b) = (labels(&left), labels(&right));
    if conditions_a.len() != 1 || conditions_b.len() != 1 {
        bail!("each arm of an information treatment must run one condition across its tasks");
    }
    let neutral_clean = a.iter().chain(b).all(|trial| {
        trial.provenance.as_ref().is_some_and(|provenance| {
            provenance
                .condition
                .as_ref()
                .is_none_or(|condition| condition.framing != FramingCondition::Neutral)
                || provenance.surface_audit.is_clean()
        })
    });
    Ok(Some(InformationTreatment {
        axis,
        conditions_a,
        conditions_b,
        differences,
        neutral_framing_label: if neutral_clean {
            "blinded"
        } else {
            "neutral_framing_without_a_clean_surface_audit_not_labelled_blinded"
        },
        prespecified_measures: BEHAVIORAL_MEASURES.to_vec(),
        claim: "Sensitivity of the prespecified behavioral measures to the listed information \
                differences. It does not show that the model was unaware of being evaluated.",
        limits: AWARENESS_LIMITS.to_vec(),
    }))
}
