//! Binds the 2×2 awareness-sensitivity design (improvement plan §8) to taskgen's framing
//! and workspace-profile manifests.
//!
//! The design itself (cells, information differences, prespecified behavioral measures,
//! claim limits and the matched-information admission the report applies) lives in
//! `mock_core::conditions`. This module only maps a cell to the taskgen conditions that
//! materialize it and checks the materialized manifests against the declared cell.
//! Nothing here runs a model or labels one as unaware.

use mock_core::conditions::ExposureManifest;
pub use mock_core::conditions::{
    AWARENESS_LIMITS, AwarenessCell, BEHAVIORAL_MEASURES, ConditionRecord, FramingCondition,
    WorkspaceCondition,
};

use crate::{
    framing::{Framing, FramingManifest},
    profile::{ProfileManifest, WorkspaceProfile},
};

/// Phrases that would reveal the diagnostic design. They must never reach a scored task.
const SCORED_TASK_FORBIDDEN: [&str; 6] = [
    "awareness condition",
    "awareness diagnostic",
    "sensitivity condition",
    "post-run question",
    "evaluation-awareness",
    "diagnostic question",
];

pub fn framing(cell: AwarenessCell) -> Framing {
    match cell.framing {
        FramingCondition::Neutral => Framing::Neutral,
        FramingCondition::Evaluation => Framing::Evaluation,
    }
}

/// The workspace profile that materializes a cell. `ordinary` selects the ordinary profile
/// the caller names; `minimal` admits no other profile.
pub fn workspace_profile(
    cell: AwarenessCell,
    ordinary: WorkspaceProfile,
) -> anyhow::Result<WorkspaceProfile> {
    match (cell.workspace, ordinary) {
        (WorkspaceCondition::Minimal, _) => Ok(WorkspaceProfile::Minimal),
        (WorkspaceCondition::Ordinary, WorkspaceProfile::Minimal) => Ok(WorkspaceProfile::Support),
        (WorkspaceCondition::Ordinary, profile) => Ok(profile),
    }
}

/// Derives the bound condition record from the manifests taskgen just materialized and
/// rejects a task whose resolved content does not realize the declared cell.
pub fn admit(
    declared: Option<AwarenessCell>,
    framing: &FramingManifest,
    profile: &ProfileManifest,
    exposure: Option<&ExposureManifest>,
    rendered_instruction: &str,
    profile_files: impl IntoIterator<Item = impl AsRef<str>>,
) -> anyhow::Result<ConditionRecord> {
    let record = ConditionRecord::from_manifests(
        &serde_json::to_value(framing)?,
        &serde_json::to_value(profile)?,
        exposure,
    )
    .map_err(anyhow::Error::msg)?;
    if let Some(cell) = declared
        && record.cell() != cell
    {
        anyhow::bail!(
            "materialized task realizes {} instead of the declared {}",
            record.cell().label(),
            cell.label()
        );
    }
    let lowered = std::iter::once(rendered_instruction.to_ascii_lowercase()).chain(
        profile_files
            .into_iter()
            .map(|text| text.as_ref().to_ascii_lowercase()),
    );
    for text in lowered {
        if SCORED_TASK_FORBIDDEN
            .iter()
            .any(|marker| text.contains(marker))
        {
            anyhow::bail!("a scored task surface carries an awareness-design label or question");
        }
    }
    Ok(record)
}
