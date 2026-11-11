//! Every environment-failure code in the workspace, and which kind it is.
//!
//! `fail_operation` takes a free-form code and grading reads the kind from it
//! (`events::is_unsupported_refusal`): a refusal of an operation the mock does not model
//! contains `unsupported`, an internal error does not. A refusal voids a trial only when
//! the trial does not otherwise pass, so a code of the wrong kind either voids trials the
//! agent completed or passes trials the mock broke. These tests read the sources so that
//! no code is added without being placed in the table below.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use mock_core::events::is_unsupported_refusal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The agent asked for an operation the mock does not model, and every call site
    /// refuses before its first write.
    Refusal,
    /// An unsupported operation with a call site that can refuse after a write. It voids
    /// the trial like an internal error (`REFUSALS_AFTER_A_WRITE` in `events.rs`).
    RefusalAfterWrite,
    /// The mock itself failed.
    Internal,
}

/// Before adding a `Refusal`, read each of its call sites and every caller of the helper
/// it sits in: nothing may be written before the refusal on any path, including earlier
/// items of a batch and records created and then removed again.
const CODES: &[(&str, Kind)] = &[
    ("business_clock_invalid", Kind::Internal),
    ("caller_resolution_failed", Kind::Internal),
    ("datadog_monitor_selector_unsupported", Kind::Refusal),
    ("execution_panicked", Kind::Internal),
    ("firecrawl_monitor_target_unsupported", Kind::Refusal),
    ("firecrawl_schedule_unsupported", Kind::Refusal),
    ("google_calendar_conference_unsupported", Kind::Refusal),
    ("google_calendar_time_unsupported", Kind::Refusal),
    ("google_drive_access_option_unsupported", Kind::Refusal),
    ("google_sheet_expression_unsupported", Kind::Refusal),
    ("operation_panicked", Kind::Internal),
    ("request_evidence_missing", Kind::Internal),
    ("unsupported_argocd_operation", Kind::RefusalAfterWrite),
    ("unsupported_grafana_matcher", Kind::Refusal),
    ("unsupported_okta_operation", Kind::Refusal),
    ("unsupported_pagerduty_precision", Kind::Refusal),
    ("unsupported_project_pause_eligibility", Kind::Refusal),
    ("unsupported_sql_statement", Kind::RefusalAfterWrite),
    ("unsupported_terraform_execution", Kind::RefusalAfterWrite),
    ("unsupported_terraform_planning", Kind::Refusal),
    ("unsupported_terraform_representation", Kind::Refusal),
    ("unsupported_vercel_operation", Kind::RefusalAfterWrite),
];

/// The two ways a code reaches the event log, and which argument carries it.
const RECORDERS: &[(&str, usize)] = &[("fail_operation(", 1), ("events.fail(", 2)];

/// `fail_operation` itself passes its `code` parameter on; nothing else may.
const FORWARDING_FILE: &str = "mock-core/src/world.rs";

/// The top-level arguments of the call whose opening parenthesis was just read.
fn arguments(after_open: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut depth = 0_usize;
    let mut characters = after_open.chars();
    while let Some(character) = characters.next() {
        match character {
            '"' => {
                current.push(character);
                while let Some(inner) = characters.next() {
                    current.push(inner);
                    match inner {
                        '\\' => current.extend(characters.next()),
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(character);
            }
            ')' | ']' | '}' if depth > 0 => {
                depth -= 1;
                current.push(character);
            }
            ')' => break,
            ',' if depth == 0 => arguments.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    arguments.push(current);
    arguments
        .into_iter()
        .map(|argument| argument.trim().to_owned())
        .filter(|argument| !argument.is_empty())
        .collect()
}

/// The code when the argument is a string literal, alone or converted to a `String`.
fn literal(argument: &str) -> Option<&str> {
    let (code, rest) = argument.strip_prefix('"')?.split_once('"')?;
    matches!(rest, "" | ".into()" | ".to_owned()" | ".to_string()").then_some(code)
}

/// Codes recorded in one source file, by line. `Err` names a call whose code cannot be
/// read from the source.
fn recorded(name: &str, source: &str) -> Result<Vec<(usize, String)>, String> {
    let mut found = Vec::new();
    for (recorder, position) in RECORDERS {
        for (offset, _) in source.match_indices(recorder) {
            let before = &source[..offset];
            if before.ends_with("fn ") {
                continue;
            }
            let line = before.matches('\n').count() + 1;
            let arguments = arguments(&source[offset + recorder.len()..]);
            let argument = arguments
                .get(*position)
                .ok_or_else(|| format!("{name}:{line}: {recorder}…) has no code argument"))?;
            match literal(argument) {
                Some(code) => found.push((line, code.to_owned())),
                None if name.ends_with(FORWARDING_FILE) && argument.starts_with("code") => {}
                None => {
                    return Err(format!(
                        "{name}:{line}: the code `{argument}` is not a string literal, so its kind cannot be checked here"
                    ));
                }
            }
        }
    }
    Ok(found)
}

fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if !matches!(name.as_str(), "tests" | "examples" | "target") {
                sources(&path, found);
            }
        } else if name.ends_with(".rs") {
            found.push(path);
        }
    }
}

/// Every code recorded by the crates' sources, with the places that record it.
fn workspace_codes() -> BTreeMap<String, Vec<String>> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    sources(&crates, &mut files);
    let mut codes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in files {
        let name = format!(
            "crates/{}",
            file.strip_prefix(&crates).unwrap().to_string_lossy()
        )
        .replace('\\', "/");
        let source = fs::read_to_string(&file).unwrap();
        for (line, code) in recorded(&name, &source).unwrap_or_else(|error| panic!("{error}")) {
            codes
                .entry(code)
                .or_default()
                .push(format!("{name}:{line}"));
        }
    }
    codes
}

#[test]
fn every_code_recorded_in_the_sources_is_classified() {
    let table: BTreeMap<_, _> = CODES.iter().copied().collect();
    assert_eq!(table.len(), CODES.len(), "a code is listed twice");
    let in_sources = workspace_codes();
    for (code, sites) in &in_sources {
        assert!(
            table.contains_key(code.as_str()),
            "{sites:?} records the environment-failure code `{code}`, which is not classified. \
             If the call refuses an operation the mock does not model, the code must contain \
             `unsupported` and every path to the call must refuse before its first write: add \
             it to CODES as Refusal, or as RefusalAfterWrite and to REFUSALS_AFTER_A_WRITE in \
             events.rs if it cannot. If the mock itself failed, the code must not contain \
             `unsupported`: add it as Internal."
        );
    }
    for (code, _) in CODES {
        assert!(
            in_sources.contains_key(*code),
            "`{code}` is classified but no source records it any more; remove it from CODES"
        );
    }
}

#[test]
fn a_code_says_which_kind_it_is() {
    for (code, kind) in CODES {
        assert_eq!(
            code.contains("unsupported"),
            *kind != Kind::Internal,
            "`{code}`: a refusal's code contains `unsupported` and no other code does"
        );
        assert_eq!(
            is_unsupported_refusal(code),
            *kind == Kind::Refusal,
            "`{code}` is {kind:?} here but grading reads it differently"
        );
    }
    // Codes grading has never seen are internal errors unless they follow the convention.
    assert!(is_unsupported_refusal("linear_cycle_option_unsupported"));
    assert!(!is_unsupported_refusal("linear_cycle_option_unavailable"));
    assert!(!is_unsupported_refusal(""));
}

#[test]
fn the_reader_finds_the_code_in_every_call_shape() {
    let source = r#"
        pub fn fail_operation(&mut self, operation: &OperationContext, code: &str, reason: &str) {}
        world.fail_operation(&operation, "one_unsupported", &reason);
        world.lock().expect("world lock").fail_operation(
            &operation,
            "two_unsupported",
            &format!("{} {}", req.method, req.path),
        );
        world.events.fail(
            self.context.clone(),
            time.clone(),
            "three_failed".into(),
            "an admitted operation panicked, before completing".into(),
        );
    "#;
    assert_eq!(
        recorded("crates/services/example/src/lib.rs", source).unwrap(),
        [
            (3, "one_unsupported".to_owned()),
            (4, "two_unsupported".to_owned()),
            (9, "three_failed".to_owned()),
        ]
    );
    // A code that is computed, or passed through outside `fail_operation` itself, is an
    // error: the table could not vouch for it.
    for source in [
        "world.fail_operation(&operation, code, reason);",
        "world.fail_operation(&operation, CODE, \"the reason\");",
        "world.fail_operation(&operation, &format!(\"{service}_unsupported\"), reason);",
    ] {
        assert!(recorded("crates/services/example/src/lib.rs", source).is_err());
    }
    assert_eq!(
        recorded(
            "crates/mock-core/src/world.rs",
            "world.fail_operation(operation, code, reason);"
        )
        .unwrap(),
        []
    );
}
