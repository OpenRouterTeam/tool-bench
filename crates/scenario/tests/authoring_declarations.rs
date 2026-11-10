//! Validation-evidence admission and the machine-readable operation-support inventory, through
//! the same loader (`OracleTools::load`) and coverage inventory that `taskgen` uses.

use std::{
    fs,
    path::{Path, PathBuf},
};

use catalog::Catalog;
use scenario::{
    authoring::{EVIDENCE_GAP, OPERATION_GAP},
    coverage::inventory,
    discover,
    oracle_tools::OracleTools,
};
use serde_json::{Value, json};

const SCENARIO: &str = "code/terraform/unlock-orders-prod";
const TEST_FILE: &str = "crates/fixture/tests/contracts.rs";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalog() -> Catalog {
    Catalog::load(&root().join("catalog/services.toml")).unwrap()
}

fn inventory_at(root: &Path) -> anyhow::Result<Value> {
    let catalog = catalog();
    let tools = OracleTools::load(root, &catalog)?;
    let scenarios = discover(&root.join("scenarios"))?;
    let calls = scenarios
        .iter()
        .map(|scenario| scenario.validate(&root.join("scenarios"), &catalog, &tools))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(serde_json::to_value(inventory(&scenarios, calls, &tools)?)?)
}

fn row<'a>(inventory: &'a Value, id: &str) -> &'a Value {
    inventory["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap()
}

fn gaps(row: &Value) -> Vec<&str> {
    row["source_gaps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|gap| gap.as_str().unwrap())
        .collect()
}

/// A repository root that shares the real scenarios, tool snapshots and operation document
/// but has its own (initially empty) declarations and one fixture test file.
fn fixture_root() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for shared in ["scenarios", "servers", "docs"] {
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            fs::canonicalize(root().join(shared)).unwrap(),
            directory.path().join(shared),
        )
        .unwrap();
    }
    let tests = directory.path().join("crates/fixture/tests");
    fs::create_dir_all(&tests).unwrap();
    fs::write(
        tests.join("contracts.rs"),
        format!(
            r#"const UNLOCK: &str = "{SCENARIO}";

#[tokio::test]
async fn a_valid_alternative() {{}}

#[test]
fn a_collateral_near_miss() {{}}

fn not_a_test() {{}}
"#
        ),
    )
    .unwrap();
    directory
}

fn declare(root: &Path, directory: &str, text: &str) {
    let directory = root.join("catalog").join(directory);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("fixture.toml"), text).unwrap();
}

fn evidence(valid: &str, near_miss: &str) -> String {
    format!(
        "schema_version = 1\n[[scenario]]\nid = \"{SCENARIO}\"\nvalid_alternative = [{valid}]\n[scenario.near_miss]\ncollateral_change = [{near_miss}]\n"
    )
}

fn load_error(root: &Path) -> String {
    format!(
        "{:#}",
        OracleTools::load(root, &catalog()).map(|_| ()).unwrap_err()
    )
}

#[test]
fn the_committed_declarations_load_and_the_inventory_reports_honest_gaps() {
    let inventory = inventory_at(&root()).unwrap();
    let summary = &inventory["authoring"];
    let scored = inventory["summary"]["scored_tasks"].as_u64().unwrap();
    let with = summary["scored_with_validation_evidence"].as_u64().unwrap();
    let without = summary["scored_without_validation_evidence"]
        .as_u64()
        .unwrap();
    assert_eq!(with + without, scored);
    assert!(with > 0 && without > 0, "{summary}");
    assert_eq!(
        inventory["summary"]["scored_source_gaps"][EVIDENCE_GAP],
        without
    );
    let exercised = summary["scored_exercised_operations"].as_u64().unwrap();
    assert_eq!(
        exercised,
        summary["scored_exercised_operations_reviewed"]
            .as_u64()
            .unwrap()
            + summary["scored_exercised_operations_partial"]
                .as_u64()
                .unwrap()
            + summary["scored_exercised_operations_undeclared"]
                .as_u64()
                .unwrap()
    );
    for row in inventory["scenarios"].as_array().unwrap() {
        let evidence = &row["authoring"]["validation_evidence"];
        let admitted = evidence["valid_alternative"].as_u64().unwrap() > 0
            && evidence["near_miss"]
                .as_object()
                .unwrap()
                .values()
                .any(|count| count.as_u64().unwrap() > 0);
        assert_eq!(
            gaps(row).contains(&EVIDENCE_GAP),
            !admitted,
            "{}",
            row["id"]
        );
        let unreviewed = row["authoring"]["unreviewed_operations"]
            .as_array()
            .unwrap();
        assert_eq!(
            gaps(row).contains(&OPERATION_GAP),
            !unreviewed.is_empty(),
            "{}",
            row["id"]
        );
    }
    // A scenario with real declared evidence, and one whose operations were never reviewed.
    let unlock = row(&inventory, SCENARIO);
    assert!(!gaps(unlock).contains(&EVIDENCE_GAP));
    assert!(!gaps(unlock).contains(&OPERATION_GAP));
    let unreviewed = row(&inventory, "business/hubspot/dana-joins-pied-piper");
    assert!(gaps(unreviewed).contains(&EVIDENCE_GAP));
    assert!(gaps(unreviewed).contains(&OPERATION_GAP));
    assert!(
        unreviewed["authoring"]["unreviewed_operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|operation| operation.as_str().unwrap().starts_with("hubspot."))
    );
}

#[test]
fn a_scenario_is_not_admitted_without_both_a_valid_alternative_and_a_near_miss() {
    let fixture = fixture_root();
    let undeclared = inventory_at(fixture.path()).unwrap();
    assert!(gaps(row(&undeclared, SCENARIO)).contains(&EVIDENCE_GAP));
    assert_eq!(
        undeclared["authoring"]["scored_with_validation_evidence"],
        0
    );

    // Oracle-plus-near-miss evidence alone still lacks an independent valid alternative.
    declare(
        fixture.path(),
        "validation-evidence",
        &format!(
            "schema_version = 1\n[[scenario]]\nid = \"{SCENARIO}\"\n[scenario.near_miss]\ncollateral_change = [\"{TEST_FILE}::a_collateral_near_miss\"]\n"
        ),
    );
    let partial = inventory_at(fixture.path()).unwrap();
    assert!(gaps(row(&partial, SCENARIO)).contains(&EVIDENCE_GAP));

    declare(
        fixture.path(),
        "validation-evidence",
        &evidence(
            &format!("\"{TEST_FILE}::a_valid_alternative\""),
            &format!("\"{TEST_FILE}::a_collateral_near_miss\""),
        ),
    );
    let admitted = inventory_at(fixture.path()).unwrap();
    let unlock = row(&admitted, SCENARIO);
    assert!(!gaps(unlock).contains(&EVIDENCE_GAP));
    assert_eq!(
        unlock["authoring"]["validation_evidence"],
        json!({"valid_alternative": 1, "near_miss": {
            "wrong_entity": 0, "collateral_change": 1, "partial": 0, "undone_prohibited_change": 0
        }})
    );
    assert_eq!(admitted["authoring"]["scored_with_validation_evidence"], 1);
}

#[test]
fn evidence_that_does_not_exist_fails_authoring_for_the_right_reason() {
    let valid = format!("\"{TEST_FILE}::a_valid_alternative\"");
    let near = format!("\"{TEST_FILE}::a_collateral_near_miss\"");
    for (text, reason) in [
        (
            evidence(&format!("\"{TEST_FILE}::an_invented_test\""), &near),
            "has no test function named an_invented_test".to_owned(),
        ),
        (
            evidence(&format!("\"{TEST_FILE}::not_a_test\""), &near),
            "has no test function named not_a_test".to_owned(),
        ),
        (
            evidence(&valid, "\"crates/fixture/tests/missing.rs::anything\""),
            "test file crates/fixture/tests/missing.rs does not exist".to_owned(),
        ),
        (
            evidence(&valid, "\"../outside/tests/escape.rs::anything\""),
            "must name a Rust test file under crates/**/tests/".to_owned(),
        ),
        (
            evidence(&valid, &valid),
            "is listed more than once".to_owned(),
        ),
        (
            evidence(&valid, &near).replace(SCENARIO, "code/terraform/no-such-scenario"),
            "no such scenario".to_owned(),
        ),
        (
            // A real test in a file that never names the scenario is not evidence for it.
            evidence(&valid, &near).replace(SCENARIO, "code/terraform/scale-checkout-staging"),
            "is in a file that never names scenario code/terraform/scale-checkout-staging"
                .to_owned(),
        ),
        (
            format!("schema_version = 1\n[[scenario]]\nid = \"{SCENARIO}\"\n"),
            "must reference at least one test".to_owned(),
        ),
    ] {
        let fixture = fixture_root();
        declare(fixture.path(), "validation-evidence", &text);
        let message = load_error(fixture.path());
        assert!(message.contains(&reason), "{message}");
    }
}

fn operation(body: &str) -> String {
    format!("schema_version = 1\n[[operation]]\n{body}\n")
}

const SECTION: &str = "Terraform workflow and representation checkpoint — revision 34";

#[test]
fn an_exercised_operation_is_a_gap_until_it_is_declared_reviewed() {
    let fixture = fixture_root();
    let undeclared = inventory_at(fixture.path()).unwrap();
    let unlock = row(&undeclared, SCENARIO);
    assert!(gaps(unlock).contains(&OPERATION_GAP));
    assert_eq!(
        unlock["authoring"]["unreviewed_operations"],
        json!([
            "terraform.create_run",
            "terraform.force_unlock_workspace",
            "terraform.get_run_details",
            "terraform.list_terraform_projects",
            "terraform.list_workspaces"
        ])
    );
    assert_eq!(undeclared["authoring"]["declared_operations"], 0);

    let mut text = String::from("schema_version = 1\n");
    for (tool, status) in [
        ("create_run", "status = \"reviewed\""),
        ("force_unlock_workspace", "status = \"reviewed\""),
        ("get_run_details", "status = \"reviewed\""),
        ("list_terraform_projects", "status = \"reviewed\""),
        (
            "list_workspaces",
            "status = \"partial\"\nlimitations = \"Search filters are not reviewed.\"",
        ),
    ] {
        text.push_str(&format!(
            "[[operation]]\nendpoint = \"terraform\"\ntool = \"{tool}\"\n{status}\nsection = \"{SECTION}\"\n"
        ));
    }
    declare(fixture.path(), "operation-support", &text);
    let declared = inventory_at(fixture.path()).unwrap();
    let unlock = row(&declared, SCENARIO);
    assert!(!gaps(unlock).contains(&OPERATION_GAP));
    assert_eq!(
        unlock["authoring"]["partially_reviewed_operations"],
        json!(["terraform.list_workspaces"])
    );
    assert_eq!(declared["authoring"]["declared_operations"], 5);
    assert_eq!(
        declared["authoring"]["scored_exercised_operations_reviewed"],
        4
    );
    // Generic tools are reviewed per API operation, never as a whole tool.
    let refund = row(&declared, "business/stripe/refund-duplicate-charge");
    assert!(
        refund["authoring"]["unreviewed_operations"]
            .as_array()
            .unwrap()
            .contains(&json!("stripe.stripe_api_write#PostRefunds"))
    );
}

#[test]
fn operation_declarations_that_name_nothing_real_fail_authoring() {
    let section = format!("section = \"{SECTION}\"");
    for (body, reason) in [
        (
            format!("endpoint = \"terraform\"\ntool = \"invented_tool\"\nstatus = \"reviewed\"\n{section}"),
            "the locked tool snapshot has no such tool",
        ),
        (
            format!("endpoint = \"nowhere\"\ntool = \"create_run\"\nstatus = \"reviewed\"\n{section}"),
            "unknown endpoint",
        ),
        (
            "endpoint = \"terraform\"\ntool = \"create_run\"\nstatus = \"reviewed\"\nsection = \"A section nobody wrote\"".to_owned(),
            "has no section \"A section nobody wrote\"",
        ),
        (
            "endpoint = \"terraform\"\ntool = \"create_run\"\nstatus = \"reviewed\"".to_owned(),
            "needs a documentation section or a test reference",
        ),
        (
            format!("endpoint = \"terraform\"\ntool = \"create_run\"\nstatus = \"partial\"\n{section}"),
            "partial support must state its limitations",
        ),
        (
            format!("endpoint = \"stripe\"\ntool = \"stripe_api_write\"\nstatus = \"reviewed\"\n{section}"),
            "generic tools need an `operation`",
        ),
        (
            format!("endpoint = \"terraform\"\ntool = \"create_run\"\nstatus = \"reviewed\"\ntests = [\"{TEST_FILE}::an_invented_test\"]"),
            "has no test function named an_invented_test",
        ),
        (
            format!("endpoint = \"terraform\"\ntool = \"create_run\"\nstatus = \"certified\"\n{section}"),
            "unknown variant",
        ),
    ] {
        let fixture = fixture_root();
        declare(fixture.path(), "operation-support", &operation(&body));
        let message = load_error(fixture.path());
        assert!(message.contains(reason), "{message}");
    }
    let fixture = fixture_root();
    let entry = format!(
        "[[operation]]\nendpoint = \"terraform\"\ntool = \"create_run\"\nstatus = \"reviewed\"\n{section}\n"
    );
    declare(
        fixture.path(),
        "operation-support",
        &format!("schema_version = 1\n{entry}{entry}"),
    );
    assert!(load_error(fixture.path()).contains("is declared twice"));
}
