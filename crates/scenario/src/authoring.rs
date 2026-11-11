//! Machine-checked authoring declarations consulted by the coverage inventory:
//!
//! - `catalog/validation-evidence/*.toml` names, per scenario, the tests that exercise a valid
//!   alternative plan and deliberately wrong near-miss executions.
//! - `catalog/operation-support/*.toml` lists the operations whose modeled behavior was reviewed.
//!
//! Loading verifies that every reference exists (test functions, scenarios, locked tools and
//! documentation sections). It cannot verify what a test or a review means; a missing
//! declaration is reported as an inventory gap instead of being assumed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, bail, ensure};
use catalog::Catalog;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Scenario, SolutionCall,
    oracle_tools::{ArgumentValidation, CallValidation, OracleTools},
};

pub const EVIDENCE_DIR: &str = "catalog/validation-evidence";
pub const OPERATIONS_DIR: &str = "catalog/operation-support";
pub const OPERATIONS_DOC: &str = "docs/operation-support.md";

pub const CAPTURED_GAP: &str = "captured_argument_constraints_deferred";
pub const EVIDENCE_GAP: &str = "validation_evidence_missing";
pub const OPERATION_GAP: &str = "unreviewed_reference_operation";

pub const NEAR_MISS_KINDS: &[&str] = &[
    "wrong_entity",
    "collateral_change",
    "partial",
    "undone_prohibited_change",
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NearMisses {
    #[serde(default)]
    wrong_entity: Vec<String>,
    #[serde(default)]
    collateral_change: Vec<String>,
    #[serde(default)]
    partial: Vec<String>,
    #[serde(default)]
    undone_prohibited_change: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceEntry {
    id: String,
    #[serde(default)]
    valid_alternative: Vec<String>,
    #[serde(default)]
    near_miss: NearMisses,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceFile {
    schema_version: u32,
    #[serde(default, rename = "scenario")]
    scenarios: Vec<EvidenceEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    /// The exercised behavior was reviewed against the documented contract.
    Reviewed,
    /// Reviewed with declared limitations.
    Partial,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationEntry {
    endpoint: String,
    tool: String,
    /// Required for generic tools whose arguments select the API operation.
    #[serde(default)]
    operation: Option<String>,
    status: SupportStatus,
    #[serde(default)]
    limitations: Option<String>,
    /// A heading in `docs/operation-support.md` recording the review.
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    tests: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationFile {
    schema_version: u32,
    #[serde(default, rename = "operation")]
    operations: Vec<OperationEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EvidenceCounts {
    pub valid_alternative: usize,
    pub near_miss: BTreeMap<&'static str, usize>,
}

impl EvidenceCounts {
    pub fn admitted(&self) -> bool {
        self.valid_alternative > 0 && self.near_miss.values().any(|count| *count > 0)
    }
}

type OperationKey = (String, String, Option<String>);

#[derive(Debug, Clone, Default)]
pub struct Declarations {
    evidence: BTreeMap<String, EvidenceCounts>,
    operations: BTreeMap<OperationKey, SupportStatus>,
}

/// The reviewed-operation identities a reference call exercises.
///
/// Generic tools select the API operation through an argument, so the tool name alone does not
/// identify what was reviewed.
pub fn operations(call: &SolutionCall) -> Vec<(String, String, Option<String>)> {
    let key = |operation: Option<String>| (call.endpoint.clone(), call.tool.clone(), operation);
    match (call.endpoint.as_str(), call.tool.as_str()) {
        ("stripe", "stripe_api_read" | "stripe_api_write") => vec![key(Some(
            call.arguments["stripe_api_operation_id"]
                .as_str()
                .unwrap_or("unidentified")
                .to_owned(),
        ))],
        ("aws-api", "call_aws") => {
            let commands = match &call.arguments["cli_command"] {
                Value::String(command) => vec![command.as_str()],
                Value::Array(commands) => commands.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            if commands.is_empty() {
                return vec![key(Some("unidentified".into()))];
            }
            commands
                .iter()
                .map(|command| {
                    let words: Vec<_> = command.split_whitespace().skip(1).take(2).collect();
                    key(Some(if words.len() == 2 {
                        words.join(" ")
                    } else {
                        "unidentified".into()
                    }))
                })
                .collect()
        }
        _ => vec![key(None)],
    }
}

fn discriminated(endpoint: &str, tool: &str) -> bool {
    matches!(
        (endpoint, tool),
        ("stripe", "stripe_api_read" | "stripe_api_write") | ("aws-api", "call_aws")
    )
}

pub fn display(key: &(String, String, Option<String>)) -> String {
    match &key.2 {
        Some(operation) => format!("{}.{}#{operation}", key.0, key.1),
        None => format!("{}.{}", key.0, key.1),
    }
}

/// Reads test sources once and resolves `path::function` references to real test functions.
struct Tests<'a> {
    root: &'a Path,
    sources: BTreeMap<String, String>,
}

impl Tests<'_> {
    fn source(&mut self, path: &str) -> anyhow::Result<&str> {
        ensure!(
            path.starts_with("crates/")
                && path.ends_with(".rs")
                && path.contains("/tests/")
                && !path.split('/').any(|part| part == ".." || part.is_empty()),
            "test reference {path:?} must name a Rust test file under crates/**/tests/"
        );
        if !self.sources.contains_key(path) {
            let text = fs::read_to_string(self.root.join(path))
                .with_context(|| format!("test file {path} does not exist"))?;
            self.sources.insert(path.to_owned(), text);
        }
        Ok(&self.sources[path])
    }

    /// The source of the file holding the referenced test, after checking the function exists.
    fn resolve(&mut self, reference: &str) -> anyhow::Result<&str> {
        let (path, name) = reference
            .split_once("::")
            .with_context(|| format!("test reference {reference:?} must be <path>::<function>"))?;
        let source = self.source(path)?;
        ensure!(
            is_test_function(source, name),
            "{path} has no test function named {name}"
        );
        Ok(source)
    }
}

fn is_test_function(source: &str, name: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    lines.iter().enumerate().any(|(index, line)| {
        let declaration = line.trim_start();
        let declaration = declaration.strip_prefix("pub ").unwrap_or(declaration);
        let declaration = declaration.strip_prefix("async ").unwrap_or(declaration);
        declaration
            .strip_prefix("fn ")
            .and_then(|rest| rest.strip_prefix(name))
            .is_some_and(|rest| rest.trim_start().starts_with('('))
            && lines[..index]
                .iter()
                .rev()
                .map(|line| line.trim())
                .take_while(|line| line.starts_with("#[") || line.starts_with("//"))
                .any(|line| line == "#[test]" || line.starts_with("#[tokio::test"))
    })
}

fn toml_files(directory: &Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

impl Declarations {
    /// Loads and verifies every declaration under `root`. Absent directories declare nothing,
    /// which the inventory reports as gaps.
    pub fn load(root: &Path, catalog: &Catalog, tools: &OracleTools) -> anyhow::Result<Self> {
        let mut tests = Tests {
            root,
            sources: BTreeMap::new(),
        };
        let mut declarations = Self::default();
        for path in toml_files(&root.join(EVIDENCE_DIR))? {
            let context = || format!("validation evidence {}", path.display());
            let file: EvidenceFile =
                toml::from_str(&fs::read_to_string(&path)?).with_context(context)?;
            ensure!(file.schema_version == 1, "{}: unknown schema", context());
            for entry in file.scenarios {
                let counts = evidence(root, &mut tests, &entry)
                    .with_context(|| format!("{}: {}", context(), entry.id))?;
                if declarations
                    .evidence
                    .insert(entry.id.clone(), counts)
                    .is_some()
                {
                    bail!("{}: {} is declared twice", context(), entry.id);
                }
            }
        }
        let headings = operation_headings(root);
        for path in toml_files(&root.join(OPERATIONS_DIR))? {
            let context = || format!("operation support {}", path.display());
            let file: OperationFile =
                toml::from_str(&fs::read_to_string(&path)?).with_context(context)?;
            ensure!(file.schema_version == 1, "{}: unknown schema", context());
            for entry in file.operations {
                let key = (
                    entry.endpoint.clone(),
                    entry.tool.clone(),
                    entry.operation.clone(),
                );
                operation(catalog, tools, &headings, &mut tests, &entry)
                    .with_context(|| format!("{}: {}", context(), display(&key)))?;
                if declarations
                    .operations
                    .insert(key.clone(), entry.status)
                    .is_some()
                {
                    bail!("{}: {} is declared twice", context(), display(&key));
                }
            }
        }
        Ok(declarations)
    }

    pub fn evidence(&self, scenario: &str) -> EvidenceCounts {
        self.evidence.get(scenario).cloned().unwrap_or_default()
    }

    pub fn support(&self, key: &(String, String, Option<String>)) -> Option<SupportStatus> {
        self.operations.get(key).copied()
    }

    pub fn declared_operations(&self) -> usize {
        self.operations.len()
    }
}

fn evidence(
    root: &Path,
    tests: &mut Tests,
    entry: &EvidenceEntry,
) -> anyhow::Result<EvidenceCounts> {
    ensure!(
        entry.id.split('/').count() == 3
            && !entry
                .id
                .split('/')
                .any(|part| part == ".." || part.is_empty())
            && root
                .join("scenarios")
                .join(&entry.id)
                .join("scenario.toml")
                .is_file(),
        "no such scenario"
    );
    let mut seen = BTreeSet::new();
    let mut check = |references: &[String]| -> anyhow::Result<usize> {
        for reference in references {
            ensure!(
                seen.insert(reference.clone()),
                "{reference} is listed more than once; one test is one kind of evidence"
            );
            let source = tests.resolve(reference)?;
            ensure!(
                source.contains(&entry.id),
                "{reference} is in a file that never names scenario {}",
                entry.id
            );
        }
        Ok(references.len())
    };
    let valid_alternative = check(&entry.valid_alternative)?;
    let mut near_miss = BTreeMap::new();
    for (kind, references) in NEAR_MISS_KINDS.iter().zip([
        &entry.near_miss.wrong_entity,
        &entry.near_miss.collateral_change,
        &entry.near_miss.partial,
        &entry.near_miss.undone_prohibited_change,
    ]) {
        near_miss.insert(*kind, check(references)?);
    }
    ensure!(
        valid_alternative > 0 || near_miss.values().any(|count| *count > 0),
        "an evidence entry must reference at least one test"
    );
    Ok(EvidenceCounts {
        valid_alternative,
        near_miss,
    })
}

fn operation_headings(root: &Path) -> BTreeSet<String> {
    fs::read_to_string(root.join(OPERATIONS_DOC))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            line.strip_prefix("## ")
                .or_else(|| line.strip_prefix("### "))
        })
        .map(|heading| heading.trim().to_owned())
        .collect()
}

fn operation(
    catalog: &Catalog,
    tools: &OracleTools,
    headings: &BTreeSet<String>,
    tests: &mut Tests,
    entry: &OperationEntry,
) -> anyhow::Result<()> {
    ensure!(
        catalog.endpoint(&entry.endpoint).is_some(),
        "unknown endpoint"
    );
    ensure!(
        tools.has_tool(&entry.endpoint, &entry.tool),
        "the locked tool snapshot has no such tool"
    );
    ensure!(
        discriminated(&entry.endpoint, &entry.tool)
            == entry
                .operation
                .as_ref()
                .is_some_and(|operation| !operation.trim().is_empty()),
        "generic tools need an `operation`; other tools must not declare one"
    );
    ensure!(
        (entry.status == SupportStatus::Partial)
            == entry
                .limitations
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty()),
        "partial support must state its limitations; reviewed support must not"
    );
    ensure!(
        entry.section.is_some() || !entry.tests.is_empty(),
        "a reviewed operation needs a documentation section or a test reference"
    );
    if let Some(section) = &entry.section {
        ensure!(
            headings.contains(section),
            "{OPERATIONS_DOC} has no section {section:?}"
        );
    }
    for reference in &entry.tests {
        tests.resolve(reference)?;
    }
    Ok(())
}

/// Per-scenario authoring status exported in the coverage inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScenarioAuthoring {
    /// `none`, `validated` (declared, statically compatible and runtime-checked) or `deferred`.
    pub captured_arguments: &'static str,
    pub validation_evidence: EvidenceCounts,
    pub reviewed_operations: Vec<String>,
    pub partially_reviewed_operations: Vec<String>,
    pub unreviewed_operations: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub limits: Vec<&'static str>,
    pub scored_captured_argument_calls: usize,
    pub scored_captured_argument_calls_validated: usize,
    pub scored_with_validation_evidence: usize,
    pub scored_without_validation_evidence: usize,
    pub scored_with_every_near_miss_kind: usize,
    pub scored_exercised_operations: usize,
    pub scored_exercised_operations_reviewed: usize,
    pub scored_exercised_operations_partial: usize,
    pub scored_exercised_operations_undeclared: usize,
    pub declared_operations: usize,
}

#[derive(Debug, Default)]
pub struct Accumulator {
    summary: Summary,
    exercised: BTreeMap<OperationKey, Option<SupportStatus>>,
}

impl Accumulator {
    /// Adjusts `gaps` for one scenario and returns its authoring status.
    pub fn review(
        &mut self,
        scenario: &Scenario,
        calls: &[CallValidation],
        tools: &OracleTools,
        scored: bool,
        gaps: &mut Vec<&'static str>,
    ) -> ScenarioAuthoring {
        let deferred: Vec<_> = calls
            .iter()
            .filter(|call| call.arguments != ArgumentValidation::StaticValidated)
            .collect();
        let validated = deferred
            .iter()
            .filter(|call| {
                call.captured
                    .as_ref()
                    .is_some_and(|captured| captured.closed())
            })
            .count();
        if !deferred.is_empty() && validated == deferred.len() {
            gaps.retain(|gap| *gap != CAPTURED_GAP);
        }
        let validation_evidence = tools.declarations.evidence(&scenario.meta.id);
        if !validation_evidence.admitted() {
            gaps.push(EVIDENCE_GAP);
        }
        let mut reviewed = BTreeSet::new();
        let mut partial = BTreeSet::new();
        let mut unreviewed = BTreeSet::new();
        for key in scenario.solution.calls.iter().flat_map(operations) {
            let support = tools.declarations.support(&key);
            match support {
                Some(SupportStatus::Reviewed) => reviewed.insert(display(&key)),
                Some(SupportStatus::Partial) => partial.insert(display(&key)),
                None => unreviewed.insert(display(&key)),
            };
            if scored {
                self.exercised.insert(key, support);
            }
        }
        if !unreviewed.is_empty() {
            gaps.push(OPERATION_GAP);
        }
        if scored {
            self.summary.scored_captured_argument_calls += deferred.len();
            self.summary.scored_captured_argument_calls_validated += validated;
            if validation_evidence.admitted() {
                self.summary.scored_with_validation_evidence += 1;
            } else {
                self.summary.scored_without_validation_evidence += 1;
            }
            self.summary.scored_with_every_near_miss_kind += usize::from(
                validation_evidence.valid_alternative > 0
                    && NEAR_MISS_KINDS.iter().all(|kind| {
                        validation_evidence
                            .near_miss
                            .get(kind)
                            .is_some_and(|count| *count > 0)
                    }),
            );
        }
        ScenarioAuthoring {
            captured_arguments: if deferred.is_empty() {
                "none"
            } else if validated == deferred.len() {
                "validated"
            } else {
                "deferred"
            },
            validation_evidence,
            reviewed_operations: reviewed.into_iter().collect(),
            partially_reviewed_operations: partial.into_iter().collect(),
            unreviewed_operations: unreviewed.into_iter().collect(),
        }
    }

    pub fn finish(self, tools: &OracleTools) -> Summary {
        let count = |status: Option<SupportStatus>| {
            self.exercised
                .values()
                .filter(|support| **support == status)
                .count()
        };
        Summary {
            limits: vec![
                "validation_evidence_references_are_existence_checked_not_semantically_verified",
                "operation_support_is_a_bounded_review_record_not_vendor_conformance",
                "captured_argument_validation_covers_declared_constraints_and_locked_input_schemas_only",
            ],
            scored_exercised_operations: self.exercised.len(),
            scored_exercised_operations_reviewed: count(Some(SupportStatus::Reviewed)),
            scored_exercised_operations_partial: count(Some(SupportStatus::Partial)),
            scored_exercised_operations_undeclared: count(None),
            declared_operations: tools.declarations.declared_operations(),
            ..self.summary
        }
    }
}
