//! Freezes and checks a release manifest against the scenario sources
//! (improvement-plan items 15 and 19).
//!
//! The manifest header (identity, comparison design, external seals) is
//! authored and reviewed by hand. Only the task list and the evaluator/family
//! schema revisions are derived, from the same loader the world uses.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, bail};
use mock_core::{
    dataset::Dataset,
    release::{Release, ReleaseTask, SplitAssignment},
};

use crate::Scenario;

/// The frozen identity of every scored scenario, sorted by id. All of them
/// are public-development tasks: the repository contains no held-out task.
pub fn frozen_tasks(scenarios: &[Scenario]) -> anyhow::Result<Vec<ReleaseTask>> {
    let mut tasks = Vec::new();
    for scenario in scenarios.iter().filter(|scenario| scenario.scored()) {
        let family = scenario
            .meta
            .family
            .as_ref()
            .with_context(|| format!("{}: scored scenarios need a family", scenario.meta.id))?;
        let source = Dataset::load(&scenario.dir)?.source;
        tasks.push(ReleaseTask {
            task_id: scenario.meta.id.clone(),
            split: SplitAssignment::PublicDevelopment,
            family: family.primary,
            family_sha256: family.sha256(),
            scenario_sha256: source.scenario_sha256,
            dataset_sha256: source.dataset_sha256,
            contract_sha256: source.contract_sha256,
        });
    }
    tasks.sort_by(|left, right| left.task_id.cmp(&right.task_id));
    Ok(tasks)
}

/// Replaces the derived parts of `release` with the current sources.
pub fn refreeze(release: &mut Release, scenarios: &[Scenario]) -> anyhow::Result<()> {
    release.tasks = frozen_tasks(scenarios)?;
    release.evaluator_version = mock_core::grading::EVALUATOR_VERSION;
    release.family_schema_version = mock_core::family::FAMILY_SCHEMA;
    release.validate().map_err(anyhow::Error::msg)
}

/// Fails, naming each difference, unless `release` is exactly what the
/// current sources freeze to.
pub fn check(release: &Release, scenarios: &[Scenario]) -> anyhow::Result<()> {
    release.validate().map_err(anyhow::Error::msg)?;
    let mut differences = Vec::new();
    if release.evaluator_version != mock_core::grading::EVALUATOR_VERSION {
        differences.push(format!(
            "release froze evaluator {}, sources are at {}",
            release.evaluator_version,
            mock_core::grading::EVALUATOR_VERSION
        ));
    }
    let current: BTreeMap<_, _> = frozen_tasks(scenarios)?
        .into_iter()
        .map(|task| (task.task_id.clone(), task))
        .collect();
    let frozen: BTreeMap<_, _> = release
        .tasks
        .iter()
        .map(|task| (task.task_id.clone(), task))
        .collect();
    for (id, task) in &current {
        match frozen.get(id) {
            None => differences.push(format!("{id}: scored task is not in the release")),
            Some(frozen) => {
                for (name, left, right) in [
                    (
                        "family assignment",
                        &frozen.family_sha256,
                        &task.family_sha256,
                    ),
                    (
                        "scenario source",
                        &frozen.scenario_sha256,
                        &task.scenario_sha256,
                    ),
                    ("dataset", &frozen.dataset_sha256, &task.dataset_sha256),
                    ("contract", &frozen.contract_sha256, &task.contract_sha256),
                ] {
                    if left != right {
                        differences.push(format!("{id}: {name} changed after the freeze"));
                    }
                }
                if frozen.family != task.family || frozen.split != task.split {
                    differences.push(format!("{id}: family or split changed after the freeze"));
                }
            }
        }
    }
    for id in frozen.keys().filter(|id| !current.contains_key(*id)) {
        differences.push(format!("{id}: released task has no scored source"));
    }
    if !differences.is_empty() {
        bail!(
            "release {} is stale ({} difference(s)); a changed task needs a new frozen release:\n{}",
            release.release_id,
            differences.len(),
            differences.join("\n")
        );
    }
    Ok(())
}

/// Checks each synthetic seal against its committed fixture bytes at
/// `<manifest dir>/fixtures/<bundle_id>.json`. Real bundles stay external.
pub fn check_synthetic_seals(release: &Release, manifest: &Path) -> anyhow::Result<()> {
    for seal in release
        .external_bundle_seals
        .iter()
        .filter(|seal| seal.synthetic_fixture)
    {
        let path = manifest
            .parent()
            .context("manifest directory")?
            .join("fixtures")
            .join(format!("{}.json", seal.bundle_id));
        let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        if mock_core::runner::sha256(&bytes) != seal.content_sha256 {
            bail!(
                "synthetic seal {} does not match its fixture",
                seal.bundle_id
            );
        }
    }
    Ok(())
}

pub fn read(path: &Path) -> anyhow::Result<Release> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("reading {}", path.display()))?)
        .with_context(|| format!("parsing {}", path.display()))
}

pub fn encode(release: &Release) -> String {
    serde_json::to_string_pretty(release).expect("release serializes") + "\n"
}
