use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use mock_core::{
    evidence::WorldEvidence,
    grading::{EVALUATOR_VERSION, Expect, GradeInput, grade},
    provenance::{DatasetSource, ExecutionManifest, TaskContext},
    runner::{RunnerAdmission, RunnerPolicy},
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const USAGE: &str = "usage: bench-verify --scenario ID --contract FILE --evidence FILE --output DIR [--answer FILE] [--task-source FILE --task-context FILE --realized-manifest FILE] [--runner-policy FILE --runner-admission FILE] [--context-overflow]";

struct Options {
    scenario: String,
    contract: PathBuf,
    evidence: PathBuf,
    output: PathBuf,
    answer: Option<PathBuf>,
    runner_policy: Option<PathBuf>,
    runner_admission: Option<PathBuf>,
    task_source: Option<PathBuf>,
    task_context: Option<PathBuf>,
    realized_manifest: Option<PathBuf>,
    context_overflow: bool,
}

fn options() -> Result<Options, String> {
    let mut arguments = std::env::args().skip(1);
    let mut values = BTreeMap::new();
    let mut context_overflow = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--context-overflow" if !context_overflow => context_overflow = true,
            "--scenario"
            | "--contract"
            | "--evidence"
            | "--output"
            | "--answer"
            | "--runner-policy"
            | "--runner-admission"
            | "--task-source"
            | "--task-context"
            | "--realized-manifest" => {
                let value = arguments
                    .next()
                    .filter(|value| !value.is_empty() && !value.starts_with("--"))
                    .ok_or_else(|| format!("{argument} requires a value\n{USAGE}"))?;
                if values.insert(argument.clone(), value).is_some() {
                    return Err(format!("{argument} was specified more than once\n{USAGE}"));
                }
            }
            _ => return Err(format!("unexpected argument {argument:?}\n{USAGE}")),
        }
    }
    if values.contains_key("--runner-policy") != values.contains_key("--runner-admission") {
        return Err(format!(
            "--runner-policy and --runner-admission must be supplied together\n{USAGE}"
        ));
    }
    if values.contains_key("--runner-policy")
        && ["--task-source", "--task-context", "--realized-manifest"]
            .into_iter()
            .any(|name| !values.contains_key(name))
    {
        return Err(format!(
            "scored runner verification requires task source/context and realized manifest\n{USAGE}"
        ));
    }
    let mut required = |name: &str| {
        values
            .remove(name)
            .ok_or_else(|| format!("{name} is required\n{USAGE}"))
    };
    Ok(Options {
        scenario: required("--scenario")?,
        contract: required("--contract")?.into(),
        evidence: required("--evidence")?.into(),
        output: required("--output")?.into(),
        answer: values.remove("--answer").map(PathBuf::from),
        runner_policy: values.remove("--runner-policy").map(PathBuf::from),
        runner_admission: values.remove("--runner-admission").map(PathBuf::from),
        task_source: values.remove("--task-source").map(PathBuf::from),
        task_context: values.remove("--task-context").map(PathBuf::from),
        realized_manifest: values.remove("--realized-manifest").map(PathBuf::from),
        context_overflow,
    })
}

fn digest(bytes: &[u8]) -> String {
    let hexadecimal: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256:{hexadecimal}")
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Box<dyn Error>> {
    let temporary = path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn verify(options: Options) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(&options.output)?;
    let contract_bytes = fs::read(&options.contract);
    let evidence_bytes = fs::read(&options.evidence);
    let answer_bytes = options.answer.as_ref().map(fs::read).transpose();
    let runner_policy_bytes = options.runner_policy.as_ref().map(fs::read).transpose();
    let runner_admission_bytes = options.runner_admission.as_ref().map(fs::read).transpose();
    let task_source_bytes = options.task_source.as_ref().map(fs::read).transpose();
    let task_context_bytes = options.task_context.as_ref().map(fs::read).transpose();
    let manifest_bytes = options.realized_manifest.as_ref().map(fs::read).transpose();
    let contract = contract_bytes
        .as_ref()
        .map_err(ToString::to_string)
        .and_then(|bytes| {
            serde_json::from_slice::<Expect>(bytes).map_err(|error| error.to_string())
        });
    let evidence = evidence_bytes
        .as_ref()
        .map_err(ToString::to_string)
        .and_then(|bytes| {
            serde_json::from_slice::<WorldEvidence>(bytes).map_err(|error| error.to_string())
        });
    let answer = answer_bytes
        .as_ref()
        .map_err(ToString::to_string)
        .and_then(|bytes| {
            bytes
                .as_ref()
                .map(|bytes| std::str::from_utf8(bytes))
                .transpose()
                .map_err(|error| error.to_string())
        });
    let empty_contract = Expect::default();
    let expected = contract.as_ref().unwrap_or(&empty_contract);
    let mut assessment = match &evidence {
        Ok(evidence) => evidence.assess(
            expected,
            answer.as_ref().ok().copied().flatten(),
            options.context_overflow,
        ),
        Err(_) => grade(&GradeInput {
            expect: expected,
            state: Value::Null,
            journal: &[],
            answer: None,
            tools_ips: Vec::new(),
            context_overflow: options.context_overflow,
            startup: &[],
            initial_state: None,
            events: None,
        }),
    };
    if options.task_source.is_some() {
        let source_check: Result<(), String> = (|| {
            let bytes = task_source_bytes
                .as_ref()
                .map_err(ToString::to_string)?
                .as_ref()
                .ok_or("task source was not collected")?;
            let expected: DatasetSource =
                serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            expected.validate(&options.scenario)?;
            let captured = evidence.as_ref().map_err(ToString::to_string)?;
            if expected != captured.source {
                return Err(
                    "world scenario/data/contract sources do not match the materialized task"
                        .into(),
                );
            }
            Ok(())
        })();
        if let Err(reason) = source_check {
            assessment.invalidate_environment("evidence/task-source", reason);
        }
    }
    if options.runner_policy.is_some() {
        let admission = (|| {
            let policy_bytes = runner_policy_bytes
                .as_ref()
                .map_err(ToString::to_string)?
                .as_ref()
                .ok_or("runner policy was not collected")?;
            let admission_bytes = runner_admission_bytes
                .as_ref()
                .map_err(ToString::to_string)?
                .as_ref()
                .ok_or("runner admission was not collected")?;
            let policy: RunnerPolicy =
                serde_json::from_slice(policy_bytes).map_err(|error| error.to_string())?;
            let admission: RunnerAdmission =
                serde_json::from_slice(admission_bytes).map_err(|error| error.to_string())?;
            admission.validate(
                &policy,
                &options.scenario,
                contract_bytes.as_ref().map_err(ToString::to_string)?,
                evidence_bytes.as_ref().map_err(ToString::to_string)?,
                answer_bytes
                    .as_ref()
                    .map_err(ToString::to_string)?
                    .as_deref(),
            )?;
            let source = task_source_bytes
                .as_ref()
                .map_err(ToString::to_string)?
                .as_ref()
                .ok_or("task source is missing")?;
            let context_bytes = task_context_bytes
                .as_ref()
                .map_err(ToString::to_string)?
                .as_ref()
                .ok_or("task context is missing")?;
            let context: TaskContext =
                serde_json::from_slice(context_bytes).map_err(|error| error.to_string())?;
            let manifest_bytes = manifest_bytes
                .as_ref()
                .map_err(ToString::to_string)?
                .as_ref()
                .ok_or("realized manifest is missing")?;
            let manifest: ExecutionManifest =
                serde_json::from_slice(manifest_bytes).map_err(|error| error.to_string())?;
            manifest.validate(&options.scenario, &context, source, context_bytes)?;
            if admission.manifest_sha256.as_deref() != Some(digest(manifest_bytes).as_str())
                || manifest.session_id != admission.session_id
                || admission
                    .main
                    .as_ref()
                    .is_none_or(|main| main.container_id != manifest.images["main"].container_id)
            {
                return Err("realized manifest is not bound to this runner proof".into());
            }
            Ok(())
        })();
        assessment.admit_runner(admission);
    }
    match &evidence {
        Ok(evidence) => {
            if evidence.scenario != options.scenario {
                assessment.invalidate_environment(
                    "evidence/scenario",
                    "world evidence does not belong to the requested task",
                );
            }
            write_json(&options.output.join("evidence.json"), evidence)?;
            write_json(
                &options.output.join("journal.json"),
                &json!({"entries": evidence.journal}),
            )?;
            write_json(
                &options.output.join("events.json"),
                &json!({"events": evidence.events}),
            )?;
            write_json(&options.output.join("state.json"), &evidence.state)?;
            write_json(
                &options.output.join("initial-state.json"),
                &evidence.initial_state,
            )?;
        }
        Err(reason) => assessment.invalidate_environment("evidence", reason),
    }
    if let Err(reason) = &answer {
        assessment.invalidate_environment("answer/collection", reason);
    }
    if let Err(reason) = &contract {
        assessment.invalidate_contract("contract", reason);
    }
    write_json(
        &options.output.join("verification.json"),
        &json!({
            "schema_version": 3,
            "scenario": options.scenario,
            "evaluator_version": EVALUATOR_VERSION,
            "contract_sha256": contract_bytes.as_ref().ok().map(|bytes| digest(bytes)),
            "contract_semantic_sha256": contract.as_ref().ok().map(mock_core::provenance::canonical_sha256),
            "evidence_schema_version": evidence.as_ref().ok().map(|value| value.schema_version),
            "world_source": evidence.as_ref().ok().map(|value| &value.source),
            "initial_state_sha256": evidence.as_ref().ok().map(|value| &value.initial_state_sha256),
            "task_source_checked": options.task_source.is_some(),
            "task_source_sha256": task_source_bytes.as_ref().ok().and_then(|value| value.as_ref()).map(|bytes| digest(bytes)),
            "task_context_sha256": task_context_bytes.as_ref().ok().and_then(|value| value.as_ref()).map(|bytes| digest(bytes)),
            "manifest_sha256": manifest_bytes.as_ref().ok().and_then(|value| value.as_ref()).map(|bytes| digest(bytes)),
            "evidence_sha256": evidence_bytes.as_ref().ok().map(|bytes| digest(bytes)),
            "answer_sha256": answer_bytes.as_ref().ok().and_then(|bytes| bytes.as_ref()).map(|bytes| digest(bytes)),
            "answer_source": options.answer.as_ref().map(|_| "legacy_answer_file"),
            "context_overflow": options.context_overflow,
            "runner_admission_checked": assessment.runner_admission_checked,
            "runner_admission_ok": assessment.runner_admission_ok,
            "runner_policy_sha256": runner_policy_bytes.as_ref().ok().and_then(|bytes| bytes.as_ref()).map(|bytes| digest(bytes)),
            "runner_admission_sha256": runner_admission_bytes.as_ref().ok().and_then(|bytes| bytes.as_ref()).map(|bytes| digest(bytes))
        }),
    )?;
    for (name, bytes) in [
        ("contract.json", contract_bytes.as_ref().ok()),
        (
            "task-source.json",
            task_source_bytes
                .as_ref()
                .ok()
                .and_then(|value| value.as_ref()),
        ),
        (
            "runner-policy.json",
            runner_policy_bytes
                .as_ref()
                .ok()
                .and_then(|value| value.as_ref()),
        ),
        (
            "task-context.json",
            task_context_bytes
                .as_ref()
                .ok()
                .and_then(|value| value.as_ref()),
        ),
        (
            "realized-manifest.json",
            manifest_bytes
                .as_ref()
                .ok()
                .and_then(|value| value.as_ref()),
        ),
        (
            "runner-admission.json",
            runner_admission_bytes
                .as_ref()
                .ok()
                .and_then(|value| value.as_ref()),
        ),
    ] {
        if let Some(bytes) = bytes {
            fs::write(options.output.join(name), bytes)?;
        }
    }
    write_json(&options.output.join("assessment.json"), &assessment)?;
    write_json(&options.output.join("reward.json"), &assessment.metrics())?;
    println!("{}", serde_json::to_string(&assessment.metrics())?);
    Ok(())
}

fn main() {
    let options = match options() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = verify(options) {
        eprintln!("bench-verify: {error}");
        std::process::exit(1);
    }
}
