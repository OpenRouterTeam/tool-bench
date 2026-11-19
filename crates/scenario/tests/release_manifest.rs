//! The committed release manifest is exactly what the scenario sources freeze
//! to, and any reviewed-metadata change is a detected difference (items 15, 19).
//!
//! The first test is a freshness gate like `taskgen --check`: after an
//! intended scenario, contract or evaluator change, refreeze with
//! `cargo run -p scenario --bin release-manifest -- --manifest
//! releases/tool-bench-public-dev-2.json --freeze` (a new release identity is
//! required once results have been published against the old one).
//!
//! `releases/tool-bench-public-dev-1.json` is kept as the record of the earlier release.
//! It is no longer what the sources freeze to: eleven of its search tasks are archived
//! under `releases/archive/tool-bench-public-dev-1/`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use mock_core::family::DifficultyFamily;
use scenario::{
    discover,
    release::{self, Release, SplitAssignment},
    release_manifest,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> PathBuf {
    root().join("releases/tool-bench-public-dev-2.json")
}

fn committed() -> Release {
    release_manifest::read(&manifest()).unwrap()
}

#[test]
fn the_committed_release_freezes_the_current_scored_sources() {
    let scenarios = discover(&root().join("scenarios")).unwrap();
    let release = committed();
    release_manifest::check(&release, &scenarios).unwrap();
    release_manifest::check_synthetic_seals(&release, &manifest()).unwrap();
    assert_eq!(release.tasks.len(), 150);
    assert!(
        release
            .tasks
            .iter()
            .all(|task| task.split == SplitAssignment::PublicDevelopment
                && !task.task_id.starts_with("diagnostic/")
                && !task.task_id.starts_with("example/"))
    );
    // No held-out set exists: only a synthetic seal, which supports no held-out claim.
    assert!(!release.has_real_held_out());
    assert!(
        release
            .external_bundle_seals
            .iter()
            .all(|seal| seal.synthetic_fixture)
    );
    let counts = release.family_counts();
    assert_eq!(
        counts
            .iter()
            .map(|count| count.public_development)
            .sum::<usize>(),
        150
    );
    assert!(counts.iter().all(|count| count.held_out == 0
        && !matches!(
            count.family,
            DifficultyFamily::NoAction | DifficultyFamily::Clarification
        )));
    // The design was fixed before any result: repeats, replacement and denominators.
    assert_eq!(release.comparison.repeats_per_task, 3);
    assert!(!release.comparison.sequential_stopping);
    assert_eq!(release.comparison.denominators.len(), 3);
}

/// Tasks both releases score under one name that release 2 changed: the instruction, the
/// dataset or the contract differs. Sorted by id. A release 1 result for one of these belongs
/// to the earlier version of the task, whose sources are the repository at release 1's commit.
const CHANGED_IN_PLACE: &[&str] = &[
    "business/hubspot/dana-joins-pied-piper",
    "business/hubspot/largest-open-deal",
    "business/hubspot/log-call-note-initech",
    "business/multi/hubspot-largest-deal-salesforce-owner",
    "business/multi/zendesk-order-status-shopify",
    "business/multi/zendesk-refund-via-stripe",
    "business/shopify/relaunch-tote",
    "business/zendesk/create-ticket-for-caller",
    "business/zendesk/escalate-outage-ticket",
    "business/zendesk/find-invoice-attachment",
    "business/zendesk/reply-refund-and-solve",
    "cloud/aws/find-unencrypted-bucket",
    "cloud/mongodb/refund-order",
    "cloud/multi/aws-jira-decommission-worker",
    "cloud/multi/aws-replica-lag-pagerduty-notion",
    "cloud/okta/failed-signins-report",
    "cloud/supabase/top-customer-march-revenue",
    "code/argocd/find-degraded-app",
    "code/argocd/restart-indexer-worker",
    "code/argocd/roll-back-billing-worker",
    "code/argocd/sync-prod-payments-api",
    "code/gitlab/diagnose-failed-pipeline",
    "code/multi/argocd-degraded-gitlab-commit",
    "code/multi/sentry-spike-vercel-rollback",
    "code/multi/vercel-deploy-github-pr-lookup",
    "code/terraform/payments-db-upgrade-failure",
    "code/terraform/unlock-orders-prod",
    "collab/google/schedule-q1-planning",
    "collab/jira/assign-go-no-go-review",
    "collab/jira/blocked-sprint-item",
    "collab/linear/pull-issue-into-cycle",
    "collab/multi/jira-sentry-crash-impact",
    "collab/multi/slack-pagerduty-dbre-oncall",
    "collab/notion/launch-date-owner",
    "ops/datadog/schedule-replica-downtime",
    "ops/grafana/annotate-checkout-deploy",
    "ops/grafana/silence-replica-maintenance",
    "ops/grafana/top-5xx-checkout-pod",
    "ops/kubernetes/crashloop-root-cause",
    "ops/multi/crashloop-oncall-lookup",
    "ops/multi/datadog-latency-argocd-rollback",
    "ops/multi/pagerduty-sentry-github-issue",
    "ops/multi/sentry-release-commit-lookup",
    "ops/pagerduty/cover-shift-override",
    "ops/pagerduty/payments-oncall-now",
    "ops/pagerduty/reassign-replication-lag",
    "ops/sentry/top-unresolved-checkout",
    "search/exa/enrich-lead-list",
    "search/firecrawl/crawl-api-reference",
    "search/multi/api-key-deletion-audit-github",
];

/// Release 1 stays readable, and what release 2 changed is named: eleven search tasks left
/// (their sources are archived) and eleven joined, and of the 139 tasks the two share only
/// the 50 in `CHANGED_IN_PLACE` differ in instruction, dataset or contract. The manifest does
/// not freeze instruction bytes, so release 1's are compared from the hashes recorded beside
/// the archive.
#[test]
fn the_earlier_release_is_kept_and_every_difference_from_it_is_named() {
    let archive = root().join("releases/archive/tool-bench-public-dev-1");
    let earlier =
        release_manifest::read(&root().join("releases/tool-bench-public-dev-1.json")).unwrap();
    let current = committed();
    assert_eq!(earlier.release_id, "tool-bench-public-dev-1");
    assert_eq!(current.release_id, "tool-bench-public-dev-2");
    assert_eq!(earlier.comparison, current.comparison);
    // Release 2 also grades with a later evaluator, so no result pools across the two.
    assert_eq!(
        (earlier.evaluator_version, current.evaluator_version),
        (46, 47)
    );
    let by_id = |release: &Release| -> BTreeMap<String, release::ReleaseTask> {
        release
            .tasks
            .iter()
            .map(|task| (task.task_id.clone(), task.clone()))
            .collect()
    };
    let (before, after) = (by_id(&earlier), by_id(&current));
    let left: Vec<_> = before
        .keys()
        .filter(|id| !after.contains_key(*id))
        .collect();
    let joined: Vec<_> = after
        .keys()
        .filter(|id| !before.contains_key(*id))
        .collect();
    assert_eq!((left.len(), joined.len()), (11, 11));
    assert!(left.iter().all(|id| id.starts_with("search/")), "{left:?}");
    assert!(
        joined
            .iter()
            .all(|id| id.starts_with("search/") && id.ends_with("-any-search")),
        "{joined:?}"
    );
    for id in &left {
        let archived = archive.join("scenarios").join(id).join("scenario.toml");
        assert!(archived.is_file(), "{id} has no archived source");
    }

    let earlier_instructions: BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(archive.join("instruction-sha256.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        earlier_instructions.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>()
    );
    let instructions: BTreeMap<String, String> = discover(&root().join("scenarios"))
        .unwrap()
        .into_iter()
        .map(|scenario| {
            let digest = mock_core::runner::sha256(scenario.instruction.as_bytes());
            (scenario.meta.id, digest)
        })
        .collect();
    let changed: Vec<&str> = before
        .iter()
        .filter_map(|(id, task)| {
            let now = after.get(id)?;
            assert_eq!(task.family, now.family, "{id}");
            let same = task.dataset_sha256 == now.dataset_sha256
                && task.contract_sha256 == now.contract_sha256
                && earlier_instructions[id] == instructions[id];
            (!same).then_some(id.as_str())
        })
        .collect();
    assert_eq!(changed, CHANGED_IN_PLACE);
    assert_eq!(CHANGED_IN_PLACE.len(), 50);
}

#[test]
fn changed_families_tasks_and_evaluators_are_named_differences() {
    let mut scenarios = discover(&root().join("scenarios")).unwrap();
    let release = committed();
    let index = scenarios
        .iter()
        .position(|scenario| scenario.meta.id == "business/stripe/refund-duplicate-charge")
        .unwrap();
    let family = scenarios[index].meta.family.as_mut().unwrap();
    family.dependency_depth += 1;
    let error = release_manifest::check(&release, &scenarios)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(
            "business/stripe/refund-duplicate-charge: family assignment changed after the freeze"
        ),
        "{error}"
    );
    assert!(error.contains("1 difference(s)"), "{error}");

    let scenarios = discover(&root().join("scenarios")).unwrap();
    let mut shorter = release.clone();
    shorter.tasks.remove(0);
    let error = release_manifest::check(&shorter, &scenarios)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("scored task is not in the release"),
        "{error}"
    );
    let mut evaluator = release.clone();
    evaluator.evaluator_version -= 1;
    assert!(
        release_manifest::check(&evaluator, &scenarios)
            .unwrap_err()
            .to_string()
            .contains("release froze evaluator")
    );
    let mut seal = release.clone();
    seal.external_bundle_seals[0].content_sha256 = format!("sha256:{}", "0".repeat(64));
    assert!(release_manifest::check_synthetic_seals(&seal, &manifest()).is_err());

    // Refreezing reproduces the committed manifest byte for byte.
    let mut refrozen = release.clone();
    refrozen.tasks.clear();
    release_manifest::refreeze(&mut refrozen, &scenarios).unwrap();
    assert_eq!(
        release_manifest::encode(&refrozen),
        std::fs::read_to_string(manifest()).unwrap()
    );
}

#[test]
fn scored_scenarios_cannot_load_without_a_valid_reviewed_family() {
    let source = root().join("scenarios/business/stripe/refund-duplicate-charge");
    let load = |edit: fn(&str) -> String| {
        let directory = tempfile::tempdir().unwrap();
        let target = directory
            .path()
            .join("business/stripe/refund-duplicate-charge");
        std::fs::create_dir_all(&target).unwrap();
        let text = std::fs::read_to_string(source.join("scenario.toml")).unwrap();
        std::fs::write(target.join("scenario.toml"), edit(&text)).unwrap();
        std::fs::copy(source.join("instruction.md"), target.join("instruction.md")).unwrap();
        scenario::Scenario::load(&target).map_err(|error| format!("{error:#}"))
    };
    let loaded = load(|text| text.to_owned()).unwrap();
    assert_eq!(
        loaded.meta.family.unwrap().primary,
        DifficultyFamily::EntityAmbiguity
    );
    let missing = load(|text| {
        let (start, end) = (
            text.find("[scenario.family]").unwrap(),
            text.find("\n# ").unwrap(),
        );
        format!("{}{}", &text[..start], &text[end..])
    })
    .unwrap_err();
    assert!(
        missing.contains("[scenario.family] table is required"),
        "{missing}"
    );
    for (edit, message) in [
        (
            (|text: &str| text.replace("primary = \"entity_ambiguity\"", "primary = \"stripe\""))
                as fn(&str) -> String,
            "unknown variant",
        ),
        (
            |text| text.replace("primary = \"entity_ambiguity\"", "primary = \"no_action\""),
            "unscored diagnostic tracks",
        ),
        (
            |text| text.replace("dependency_depth = 2", "dependency_depth = 9"),
            "dependency_depth must be at most",
        ),
        (
            |text| text.replace("dependency_depth = 2", "dependency_depth = 2\ncalls = 3"),
            "unknown field",
        ),
    ] {
        let error = load(edit).unwrap_err();
        assert!(error.contains(message), "{message}: {error}");
    }
}
