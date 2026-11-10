//! Information-treatment admission and the `surface_audit` status in the report's real
//! load and comparison path, driven by manifests the real taskgen writes.

mod support;

use std::{fs, path::Path};

use catalog::Catalog;
use mock_core::{
    conditions::{AwarenessCell, ExposureVariant, InformationAxis},
    runner::sha256,
};
use report::{Trial, compare_declared, compare_treatment, load_trials, trial_summary};
use scenario::{
    Scenario, awareness,
    exposure::{Exposure, PinnedCatalog},
    profile::WorkspaceProfile,
    taskgen::{Conditions, Images, Treatments, write_task_with_treatments},
};
use serde_json::{Value, json};

const TASK: &str = "example__github__basic";

fn repository() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A clean sealed audit as the strict runner binds it into its receipt.
fn clean_audit() -> Value {
    let mut sealed: Value = serde_json::from_str(include_str!(
        "../../mock-core/tests/fixtures/sealed-surface-audit.json"
    ))
    .unwrap();
    sealed["outcome"] = json!("clean");
    sealed["finding_count"] = json!(0);
    sealed["findings"] = json!([]);
    sealed
}

/// One verified trial whose bound condition manifests come from a real taskgen run under
/// `cell` and `exposure`.
fn arm(
    directory: &Path,
    name: &str,
    cell: &str,
    exposure: ExposureVariant,
    audit: Option<Value>,
) -> Vec<Trial> {
    let root = directory.join(name);
    support::write_verified(&root, name, TASK, true);
    let cell: AwarenessCell = cell.parse().unwrap();
    let catalog = Catalog::load(&repository().join("catalog/services.toml")).unwrap();
    let requested = Exposure {
        variant: exposure,
        seed: 11,
        token_budget: None,
        pinned: PinnedCatalog::load(&repository(), &catalog).unwrap(),
    };
    let generated = tempfile::tempdir().unwrap();
    write_task_with_treatments(
        &Scenario::load(&repository().join("scenarios/example/github/basic")).unwrap(),
        &catalog,
        &Images {
            world: "world:fixture".into(),
            tools: "tools:fixture".into(),
        },
        generated.path(),
        Conditions {
            workspace_profile: awareness::workspace_profile(cell, WorkspaceProfile::Support)
                .unwrap(),
            framing: awareness::framing(cell),
            review_policy: None,
            locked_tools: None,
        },
        Treatments {
            exposure: Some(&requested),
            awareness: Some(cell),
            runner_admission: Default::default(),
        },
    )
    .unwrap();
    let tests = generated.path().join(TASK).join("tests");
    let verifier = root.join(name).join("verifier");
    let read = |path: &Path| -> Value { serde_json::from_slice(&fs::read(path).unwrap()).unwrap() };
    let produced = read(&tests.join("task-context.json"));
    let mut context = read(&verifier.join("task-context.json"));
    for key in [
        "instruction_sha256",
        "profile_sha256",
        "framing_sha256",
        "environment_files",
        "connector_env_sha256",
        "exposure_sha256",
    ] {
        match produced.get(key) {
            Some(value) => context[key] = value.clone(),
            None => {
                context.as_object_mut().unwrap().remove(key);
            }
        }
    }
    let context_bytes = serde_json::to_vec(&context).unwrap();
    let mut manifest = read(&verifier.join("realized-manifest.json"));
    manifest["task_context_sha256"] = json!(sha256(&context_bytes));
    manifest["images"]["tools"]["connector_env_sha256"] = context["connector_env_sha256"].clone();
    // The agent image is built from the profile files, so it differs with the workspace.
    manifest["images"]["main"]["filesystem_sha256"] = json!(sha256(
        context["profile_sha256"].as_str().unwrap().as_bytes()
    ));
    manifest["context"] = context;
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let mut admission = read(&verifier.join("runner-admission.json"));
    admission["manifest_sha256"] = json!(sha256(&manifest_bytes));
    if let Some(audit) = audit {
        admission["surface_audit"] = audit;
    }
    let admission_bytes = serde_json::to_vec(&admission).unwrap();
    let mut verification = read(&verifier.join("verification.json"));
    verification["task_context_sha256"] = json!(sha256(&context_bytes));
    verification["manifest_sha256"] = json!(sha256(&manifest_bytes));
    verification["runner_admission_sha256"] = json!(sha256(&admission_bytes));
    fs::write(verifier.join("task-context.json"), context_bytes).unwrap();
    fs::write(verifier.join("realized-manifest.json"), manifest_bytes).unwrap();
    fs::write(verifier.join("runner-admission.json"), admission_bytes).unwrap();
    fs::write(
        verifier.join("verification.json"),
        serde_json::to_vec(&verification).unwrap(),
    )
    .unwrap();
    for file in ["framing.json", "profile.json", "exposure.json"] {
        if tests.join(file).exists() {
            fs::copy(tests.join(file), verifier.join(file)).unwrap();
        }
    }
    load_trials(&root).unwrap()
}

const FULL: ExposureVariant = ExposureVariant::FullCatalog;

#[test]
fn frozen_release_and_information_treatments_are_admitted_together() {
    let directory = tempfile::tempdir().unwrap();
    let neutral = arm(directory.path(), "neutral", "neutral_minimal", FULL, None);
    let evaluation = arm(
        directory.path(),
        "evaluation",
        "evaluation_minimal",
        FULL,
        None,
    );
    let provenance = neutral[0].provenance.as_ref().unwrap();
    let source = &provenance.source;
    let mut release: mock_core::release::Release = serde_json::from_value(json!({
        "schema_version":mock_core::release::RELEASE_SCHEMA,
        "release_id":"synthetic-information-admission",
        "frozen_at":"2026-11-10T00:00:00Z",
        "evaluator_version":provenance.evaluator_version,
        "family_schema_version":mock_core::family::FAMILY_SCHEMA,
        "external_bundle_seals":[],
        "comparison":{
            "schema_version":mock_core::release::COMPARISON_SCHEMA,
            "repeats_per_task":1,
            "budgets":{"agent_timeout_sec_max":900,"inference_tokens_max":null},
            "order_policy":"interleaved_arms",
            "replacement_policy":{"kind":"no_replacement"},
            "declared_treatments":["information:framing"],
            "denominators":["attempts","tasks","families"],
            "sequential_stopping":false
        },
        "tasks":[{
            "task_id":source.scenario,
            "split":"public_development",
            "family":"baseline",
            "family_sha256":sha256(b"synthetic family"),
            "scenario_sha256":source.scenario_sha256,
            "dataset_sha256":source.dataset_sha256,
            "contract_sha256":source.contract_sha256
        }]
    }))
    .unwrap();
    let comparison = report::release::compare_treatment(
        &neutral,
        &evaluation,
        &release,
        false,
        Some(InformationAxis::Framing),
    )
    .unwrap();
    assert_eq!(comparison.release_admission.level, report::release::FROZEN);
    assert_eq!(
        comparison.comparison.declared_treatments,
        ["information:framing"]
    );
    assert!(report::release::compare(&neutral, &evaluation, &release, false).is_err());
    release.comparison.declared_treatments.clear();
    assert!(
        report::release::compare_treatment(
            &neutral,
            &evaluation,
            &release,
            false,
            Some(InformationAxis::Framing),
        )
        .is_err()
    );
}

#[test]
fn matched_arms_compare_and_each_trial_reports_its_own_surface_audit_status() {
    let directory = tempfile::tempdir().unwrap();
    let a = arm(
        directory.path(),
        "a",
        "neutral_minimal",
        FULL,
        Some(clean_audit()),
    );
    let b = arm(directory.path(), "b", "neutral_minimal", FULL, None);
    let comparison = compare_declared(&a, &b, false).unwrap();
    assert!(comparison.information_treatment.is_none());
    assert!(comparison.declared_treatments.is_empty());
    assert_eq!(comparison.surface_audit_a["clean"], 1);
    assert_eq!(comparison.surface_audit_b["unqualified"], 1);

    let summary = trial_summary(&a);
    let trial = &summary["trials"][0];
    assert_eq!(trial["surface_audit"]["status"], "clean");
    assert!(
        trial["surface_audit"]["claim"]
            .as_str()
            .unwrap()
            .contains("does not establish")
    );
    assert_eq!(trial["information_condition"]["framing"], "neutral");
    assert_eq!(trial["information_condition"]["exposure"], "full_catalog");
    assert_eq!(
        trial_summary(&b)["trials"][0]["surface_audit"]["status"],
        "unqualified"
    );
}

#[test]
fn an_undeclared_information_difference_is_rejected_with_the_difference_named() {
    let directory = tempfile::tempdir().unwrap();
    let neutral = arm(directory.path(), "a", "neutral_minimal", FULL, None);
    for (name, cell, exposure, item) in [
        (
            "framing",
            "evaluation_minimal",
            FULL,
            "instruction_framing_prefix",
        ),
        (
            "workspace",
            "neutral_ordinary",
            FULL,
            "profile_information/standing_scope_guidance",
        ),
        (
            "exposure",
            "neutral_minimal",
            ExposureVariant::OrganizationPack,
            "exposure_variant",
        ),
    ] {
        let other = arm(directory.path(), name, cell, exposure, None);
        let error = format!(
            "{:#}",
            compare_declared(&neutral, &other, false).unwrap_err()
        );
        assert!(error.contains("not matched in information"), "{error}");
        assert!(error.contains(item), "{error}");
    }
}

#[test]
fn a_declared_framing_treatment_lists_its_differences_and_earns_blinded_only_with_a_clean_audit() {
    let directory = tempfile::tempdir().unwrap();
    let evaluation = arm(
        directory.path(),
        "evaluation",
        "evaluation_minimal",
        FULL,
        None,
    );
    let unaudited = arm(directory.path(), "unaudited", "neutral_minimal", FULL, None);
    let comparison = compare_treatment(
        &unaudited,
        &evaluation,
        false,
        Some(InformationAxis::Framing),
    )
    .unwrap();
    assert_eq!(comparison.declared_treatments, ["information:framing"]);
    let treatment = comparison.information_treatment.unwrap();
    assert_eq!(treatment.conditions_a, ["neutral_minimal/full_catalog"]);
    assert_eq!(treatment.conditions_b, ["evaluation_minimal/full_catalog"]);
    assert!(
        treatment
            .differences
            .iter()
            .all(|item| item.axis == InformationAxis::Framing)
    );
    assert!(treatment.differences.iter().any(|item| {
        item.item == "instruction_framing_prefix" && item.a == "neutral" && item.b == "evaluation"
    }));
    assert_ne!(treatment.neutral_framing_label, "blinded");
    assert!(
        treatment
            .claim
            .contains("does not show that the model was unaware")
    );
    assert_eq!(treatment.prespecified_measures.len(), 5);
    assert!(
        treatment
            .limits
            .contains(&"does_not_establish_that_the_model_was_unaware_of_being_evaluated")
    );

    let audited = arm(
        directory.path(),
        "audited",
        "neutral_minimal",
        FULL,
        Some(clean_audit()),
    );
    let comparison =
        compare_treatment(&audited, &evaluation, false, Some(InformationAxis::Framing)).unwrap();
    assert_eq!(
        comparison
            .information_treatment
            .unwrap()
            .neutral_framing_label,
        "blinded"
    );

    let mut partial = clean_audit();
    partial["captured"]
        .as_array_mut()
        .unwrap()
        .retain(|surface| surface != "harness_messages" && surface != "runtime_processes");
    let partial = arm(
        directory.path(),
        "partial",
        "neutral_minimal",
        FULL,
        Some(partial),
    );
    let comparison =
        compare_treatment(&partial, &evaluation, false, Some(InformationAxis::Framing)).unwrap();
    assert_ne!(
        comparison
            .information_treatment
            .unwrap()
            .neutral_framing_label,
        "blinded"
    );

    // Declaring a treatment the arms do not differ on is not a treatment.
    assert!(
        compare_treatment(&audited, &unaudited, false, Some(InformationAxis::Framing)).is_err()
    );
}

#[test]
fn a_declared_axis_never_covers_a_difference_on_another_axis() {
    let directory = tempfile::tempdir().unwrap();
    let base = arm(directory.path(), "base", "neutral_minimal", FULL, None);
    let both = arm(directory.path(), "both", "evaluation_ordinary", FULL, None);
    let error = format!(
        "{:#}",
        compare_treatment(&base, &both, false, Some(InformationAxis::Framing)).unwrap_err()
    );
    assert!(error.contains("outside the declared"), "{error}");
    assert!(
        error.contains("profile_information/standing_scope_guidance"),
        "{error}"
    );

    // A richer workspace is itself an information treatment and must be declared as one.
    let ordinary = arm(directory.path(), "ordinary", "neutral_ordinary", FULL, None);
    let treatment = compare_treatment(&base, &ordinary, false, Some(InformationAxis::Workspace))
        .unwrap()
        .information_treatment
        .unwrap();
    for item in [
        "profile_information/standing_scope_guidance",
        "profile_information/full_connection_inventory",
        "business_reference_time_supplied",
    ] {
        assert!(
            treatment
                .differences
                .iter()
                .any(|difference| difference.item == item),
            "{item}"
        );
    }
    assert!(
        treatment
            .limits
            .iter()
            .any(|limit| limit.contains("information_treatment_not_an_awareness_isolate"))
    );
}

#[test]
fn a_declared_exposure_treatment_is_explicit_about_what_was_registered() {
    let directory = tempfile::tempdir().unwrap();
    let full = arm(directory.path(), "full", "neutral_ordinary", FULL, None);
    let pack = arm(
        directory.path(),
        "pack",
        "neutral_ordinary",
        ExposureVariant::OrganizationPack,
        None,
    );
    let comparison =
        compare_treatment(&full, &pack, false, Some(InformationAxis::Exposure)).unwrap();
    assert_eq!(comparison.declared_treatments, ["information:exposure"]);
    let treatment = comparison.information_treatment.unwrap();
    assert_eq!(
        treatment.conditions_b,
        ["neutral_ordinary/organization_pack"]
    );
    let item = |name: &str| {
        treatment
            .differences
            .iter()
            .find(|difference| difference.item == name)
            .unwrap()
    };
    assert_eq!(
        (
            item("exposure_variant").a.as_str(),
            item("exposure_variant").b.as_str()
        ),
        ("full_catalog", "organization_pack")
    );
    assert_eq!(item("registered_endpoints").b, "1");
    assert_eq!(
        item("workspace_connection_inventory").axis,
        InformationAxis::Exposure
    );
    // The same pair is not a workspace or framing treatment.
    assert!(compare_treatment(&full, &pack, false, Some(InformationAxis::Workspace)).is_err());
}

#[test]
fn condition_manifests_the_context_does_not_bind_cannot_carry_a_declared_treatment() {
    let directory = tempfile::tempdir().unwrap();
    let neutral = arm(directory.path(), "neutral", "neutral_minimal", FULL, None);
    arm(
        directory.path(),
        "tampered",
        "evaluation_minimal",
        FULL,
        None,
    );
    let framing = directory
        .path()
        .join("tampered/tampered/verifier/framing.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&framing).unwrap()).unwrap();
    manifest["framing"] = json!("neutral");
    fs::write(&framing, manifest.to_string()).unwrap();
    let tampered = load_trials(&directory.path().join("tampered")).unwrap();
    assert!(tampered[0].provenance.as_ref().unwrap().condition.is_none());
    let error = format!(
        "{:#}",
        compare_treatment(&neutral, &tampered, false, Some(InformationAxis::Framing)).unwrap_err()
    );
    assert!(
        error.contains("requires bound condition manifests"),
        "{error}"
    );
    // Undeclared, the byte-level context binding still rejects the pair.
    assert!(compare_declared(&neutral, &tampered, false).is_err());
}
