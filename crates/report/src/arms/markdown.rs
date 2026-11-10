//! Markdown rendering of an arm report and of a task explanation.

use std::fmt::Write as _;

use super::{
    ArmSummary, Assurance, Distribution, GroupRate, PairedComparison, PairedTask, PassRate, Report,
    Status, TrialExplanation,
};

/// Unassessed trials listed per arm before the rest is left to `--json`.
const LISTED: usize = 25;

fn percent(rate: Option<f64>) -> String {
    rate.map_or_else(|| "n/a".to_owned(), |rate| format!("{:.1}%", rate * 100.0))
}

fn rate_cell(rate: &PassRate) -> String {
    let mut cell = format!("{} ({}/{})", percent(rate.rate), rate.passed, rate.assessed);
    if rate.excluded > 0 {
        let _ = write!(cell, ", {} excluded", rate.excluded);
    }
    cell
}

fn spread(stats: &Option<Distribution>) -> String {
    stats.as_ref().map_or_else(
        || "n/a".to_owned(),
        |stats| format!("{:.1} / {:.1} (n={})", stats.mean, stats.median, stats.n),
    )
}

/// One table with a column per arm. `rows` yields a label and that row's cell per arm.
fn table(out: &mut String, heading: &str, arms: &[ArmSummary], rows: &[(String, Vec<String>)]) {
    let _ = write!(out, "| {heading} |");
    for arm in arms {
        let _ = write!(out, " {} |", arm.label);
    }
    out.push_str("\n|---|");
    out.push_str(&"---|".repeat(arms.len()));
    out.push('\n');
    for (label, cells) in rows {
        let _ = writeln!(out, "| {label} | {} |", cells.join(" | "));
    }
    out.push('\n');
}

fn row(
    arms: &[ArmSummary],
    label: &str,
    cell: impl Fn(&ArmSummary) -> String,
) -> (String, Vec<String>) {
    (label.to_owned(), arms.iter().map(cell).collect())
}

fn group_rows(
    arms: &[ArmSummary],
    groups: impl Fn(&ArmSummary) -> &[GroupRate],
) -> Vec<(String, Vec<String>)> {
    let mut names: Vec<&str> = arms
        .iter()
        .flat_map(|arm| groups(arm).iter().map(|group| group.group.as_str()))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .map(|name| {
            row(arms, name, |arm| {
                groups(arm)
                    .iter()
                    .find(|group| group.group == name)
                    .map_or_else(
                        || "no trials".to_owned(),
                        |group| {
                            format!(
                                "{}; {} task{}",
                                rate_cell(&group.rate),
                                group.tasks,
                                if group.tasks == 1 { "" } else { "s" }
                            )
                        },
                    )
            })
        })
        .collect()
}

fn paired_cell(passed: usize, assessed: usize, rate: f64) -> String {
    format!("{passed}/{assessed} ({:.0}%)", rate * 100.0)
}

fn comparison(out: &mut String, comparison: &PairedComparison) {
    let (a, b) = (&comparison.a, &comparison.b);
    let _ = writeln!(out, "## Paired comparison: {b} − {a}\n");
    for warning in &comparison.warnings {
        let _ = writeln!(out, "> **Warning:** {warning}\n");
    }
    if comparison.paired_tasks == 0 {
        let _ = writeln!(
            out,
            "No task has an assessed trial in both arms, so there is nothing to pair.\n"
        );
        return;
    }
    let _ = writeln!(
        out,
        "- Paired tasks (an assessed trial in both arms): {}",
        comparison.paired_tasks
    );
    let _ = writeln!(
        out,
        "- Mean per-task pass rate: {a} {}, {b} {}",
        percent(Some(comparison.mean_a)),
        percent(Some(comparison.mean_b))
    );
    let interval = comparison.ci95.map_or_else(String::new, |(low, high)| {
        format!(
            " (95% paired bootstrap over tasks: {:+.1} to {:+.1})",
            low * 100.0,
            high * 100.0
        )
    });
    let _ = writeln!(
        out,
        "- Difference: {:+.1} percentage points{interval}",
        comparison.mean_diff * 100.0
    );
    let _ = writeln!(
        out,
        "- Tasks where {b} is better / {a} is better / tie: {} / {} / {}",
        comparison.b_better, comparison.a_better, comparison.ties
    );
    if let Some(p) = comparison.sign_test_p {
        let _ = writeln!(
            out,
            "- Two-sided sign test over the tasks that differ: p {}",
            if p < 0.001 {
                "< 0.001".to_owned()
            } else {
                format!("= {p:.3}")
            }
        );
    }
    let _ = writeln!(
        out,
        "- Trials left out of the pass rate: {a} {:.1}%, {b} {:.1}%",
        comparison.excluded_share_a * 100.0,
        comparison.excluded_share_b * 100.0
    );
    if !comparison.only_in_a.is_empty() || !comparison.only_in_b.is_empty() {
        let _ = writeln!(
            out,
            "- Tasks only in {a}: {}; only in {b}: {}",
            comparison.only_in_a.len(),
            comparison.only_in_b.len()
        );
    }
    out.push('\n');
    if comparison.flips.is_empty() {
        let _ = writeln!(out, "No paired task differs between the arms.\n");
        return;
    }
    let _ = writeln!(
        out,
        "### Tasks that flip ({})\n\n| Task | {a} passed/assessed | {b} passed/assessed | Δ points |\n|---|---|---|---|",
        comparison.flips.len()
    );
    for PairedTask {
        task,
        passed_a,
        assessed_a,
        rate_a,
        passed_b,
        assessed_b,
        rate_b,
    } in &comparison.flips
    {
        let _ = writeln!(
            out,
            "| {task} | {} | {} | {:+.0} |",
            paired_cell(*passed_a, *assessed_a, *rate_a),
            paired_cell(*passed_b, *assessed_b, *rate_b),
            (rate_b - rate_a) * 100.0
        );
    }
    out.push('\n');
}

impl Report {
    pub fn markdown(&self) -> String {
        let arms = &self.arms;
        let mut out = String::from("# tool-bench arm report\n\n");
        let _ = writeln!(
            out,
            "Built from evaluator {}. A pass rate is passes over assessed trials (pass, fail, context overflow, integrity violation). Unresolved trials and trials without an assessment are counted separately and are never treated as model failures.\n",
            self.evaluator_version
        );
        for arm in arms {
            for warning in &arm.warnings {
                let _ = writeln!(out, "> **Warning ({}):** {warning}\n", arm.label);
            }
        }
        for paired in &self.comparisons {
            for warning in &paired.warnings {
                let _ = writeln!(
                    out,
                    "> **Warning ({} vs {}):** {warning}\n",
                    paired.b, paired.a
                );
            }
        }

        out.push_str("## Arms\n\n");
        let mut rows = vec![
            row(arms, "Run directory", |arm| format!("`{}`", arm.directory)),
            row(arms, "Agent", |arm| match arm.agents.as_slice() {
                [] => "not recorded".to_owned(),
                [agent] => agent.name.clone(),
                agents => agents
                    .iter()
                    .map(|agent| format!("{} ({} trials)", agent.name, agent.trials))
                    .collect::<Vec<_>>()
                    .join("; "),
            }),
            row(arms, "Evaluator", |arm| {
                arm.evaluator_versions
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            }),
            row(arms, "Trials", |arm| arm.trials.to_string()),
            row(arms, "Distinct tasks", |arm| arm.tasks.to_string()),
            row(arms, "Attempts per task", |arm| {
                match arm.attempts_per_task {
                    (low, high) if low == high => low.to_string(),
                    (low, high) => format!("{low} to {high}"),
                }
            }),
            row(arms, "Assessed trials", |arm| {
                arm.pass_rate.assessed.to_string()
            }),
            row(arms, "Passed", |arm| arm.pass_rate.passed.to_string()),
            row(arms, "**Pass rate (assessed trials)**", |arm| {
                format!("**{}**", percent(arm.pass_rate.rate))
            }),
            row(arms, "95% interval (Wilson, per trial)", |arm| {
                arm.pass_rate.ci95.map_or_else(
                    || "n/a".to_owned(),
                    |(low, high)| format!("{:.1}% to {:.1}%", low * 100.0, high * 100.0),
                )
            }),
            row(arms, "Excluded from the pass rate", |arm| {
                arm.pass_rate.excluded.to_string()
            }),
            row(arms, "Tasks passed on every / some / no attempt", |arm| {
                let tasks = &arm.task_consistency;
                format!(
                    "{} / {} / {}",
                    tasks.passed_every_attempt, tasks.passed_some_attempts, tasks.passed_no_attempt
                )
            }),
            row(arms, "API calls per assessed trial, mean / median", |arm| {
                spread(&arm.api_calls)
            }),
            row(arms, "API calls per passed trial, mean / median", |arm| {
                spread(&arm.api_calls_passed)
            }),
            row(arms, "Failed trials with no API call", |arm| {
                arm.failed_without_api_calls.to_string()
            }),
            row(arms, "Trials with MCP bypass", |arm| {
                arm.mcp_bypass_trials.to_string()
            }),
        ];
        if arms
            .iter()
            .any(|arm| !arm.exceptions_on_assessed_trials.is_empty())
        {
            rows.push(row(arms, "Host exceptions on assessed trials", |arm| {
                if arm.exceptions_on_assessed_trials.is_empty() {
                    "none".to_owned()
                } else {
                    arm.exceptions_on_assessed_trials
                        .iter()
                        .map(|count| format!("{} × {}", count.trials, count.name))
                        .collect::<Vec<_>>()
                        .join("; ")
                }
            }));
        }
        rows.push(row(arms, "Agent seconds, mean / median", |arm| {
            spread(&arm.usage.agent_seconds)
        }));
        rows.push(row(arms, "Tokens in / cached / out", |arm| {
            let usage = &arm.usage;
            if usage.trials_reporting_tokens == 0 {
                return "not recorded".to_owned();
            }
            let number =
                |value: Option<u64>| value.map_or("?".to_owned(), |value| value.to_string());
            format!(
                "{} / {} / {} ({} of {} trials)",
                number(usage.input_tokens),
                number(usage.cache_tokens),
                number(usage.output_tokens),
                usage.trials_reporting_tokens,
                arm.trials
            )
        }));
        rows.push(row(arms, "Cost, USD", |arm| match arm.usage.cost_usd {
            Some(cost) => format!(
                "{cost:.2} ({} of {} trials)",
                arm.usage.trials_reporting_cost, arm.trials
            ),
            None => "not recorded".to_owned(),
        }));
        table(&mut out, "", arms, &rows);
        out.push_str("Timing, tokens and cost are whatever the harness wrote to `result.json`; the evaluator does not see or check them.\n\n");

        out.push_str("## Assurance\n\n");
        let rows: Vec<_> = Assurance::ALL
            .into_iter()
            .map(|assurance| {
                row(arms, assurance.label(), |arm| {
                    arm.assurance[assurance.key()].to_string()
                })
            })
            .collect();
        table(
            &mut out,
            "Trials by how far the result was checked",
            arms,
            &rows,
        );
        let _ = writeln!(out, "{}\n", self.assurance_scope);

        out.push_str("## Outcomes\n\n");
        let always = [
            Status::Pass,
            Status::Fail,
            Status::ContextOverflow,
            Status::IntegrityViolation,
            Status::Unresolved,
            Status::WithheldReward,
            Status::VerifierError,
            Status::HarnessException,
        ];
        let mut rows = Vec::new();
        for status in Status::ALL {
            if !always.contains(&status) && arms.iter().all(|arm| arm.outcomes[status.key()] == 0) {
                continue;
            }
            if status == Status::Unresolved {
                rows.push(row(arms, "*assessed (pass-rate denominator)*", |arm| {
                    format!("*{}*", arm.pass_rate.assessed)
                }));
            }
            rows.push(row(arms, status.label(), |arm| {
                arm.outcomes[status.key()].to_string()
            }));
        }
        rows.push(row(arms, "*excluded from the pass rate*", |arm| {
            format!("*{}*", arm.pass_rate.excluded)
        }));
        table(&mut out, "Outcome", arms, &rows);

        out.push_str("## Pass rate by task group\n\n");
        table(
            &mut out,
            "Group",
            arms,
            &group_rows(arms, |arm| &arm.groups),
        );
        out.push_str("## Single-service and multi-service tasks\n\n");
        table(
            &mut out,
            "Scope",
            arms,
            &group_rows(arms, |arm| &arm.scopes),
        );
        out.push_str("Cells are pass rate (passed/assessed trials), then trials excluded from that rate, then distinct tasks.\n\n");

        out.push_str("## Failed checks\n\n");
        let mut checks: Vec<(&str, usize)> = Vec::new();
        for arm in arms {
            for count in &arm.failed_checks {
                match checks.iter_mut().find(|(name, _)| *name == count.name) {
                    Some((_, total)) => *total += count.trials,
                    None => checks.push((&count.name, count.trials)),
                }
            }
        }
        checks.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
        if checks.is_empty() {
            out.push_str("No trial was graded fail or integrity violation.\n\n");
        } else {
            let rows: Vec<_> = checks
                .iter()
                .map(|(name, _)| {
                    row(arms, name, |arm| {
                        arm.failed_checks
                            .iter()
                            .find(|count| count.name == *name)
                            .map_or(0, |count| count.trials)
                            .to_string()
                    })
                })
                .collect();
            table(&mut out, "Check", arms, &rows);
            out.push_str("Trials graded fail or integrity violation in which each check was not satisfied, most frequent first. One trial can fail several checks.\n\n");
        }

        if arms.iter().any(|arm| !arm.unassessed.is_empty()) {
            out.push_str("## Trials left out of the pass rate\n\n");
            for arm in arms.iter().filter(|arm| !arm.unassessed.is_empty()) {
                let _ = writeln!(out, "### {} ({})\n", arm.label, arm.unassessed.len());
                for trial in arm.unassessed.iter().take(LISTED) {
                    let _ = writeln!(
                        out,
                        "- `{}` (`{}`): {}{}",
                        trial.task,
                        trial.trial,
                        trial.status.label(),
                        if trial.reason.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", trial.reason)
                        }
                    );
                }
                if arm.unassessed.len() > LISTED {
                    let _ = writeln!(
                        out,
                        "- … and {} more; `--json` lists every trial.",
                        arm.unassessed.len() - LISTED
                    );
                }
                out.push('\n');
            }
        }

        for paired in &self.comparisons {
            comparison(&mut out, paired);
        }
        out
    }
}

/// Markdown for `--explain`: one section per trial of the task.
pub fn explanation(trials: &[TrialExplanation]) -> String {
    let mut out = String::new();
    if let Some(first) = trials.first() {
        let _ = writeln!(out, "# Why: {}\n", first.trial.task);
    }
    for explained in trials {
        let trial = &explained.trial;
        let _ = writeln!(
            out,
            "## Arm {}, trial `{}`: {}\n",
            explained.arm,
            trial.trial,
            trial.status.label()
        );
        let _ = writeln!(out, "- Directory: `{}`", explained.path);
        let _ = writeln!(out, "- Assurance: {}", trial.assurance.label());
        if let Some(note) = &trial.note {
            let _ = writeln!(out, "- Note: {note}");
        }
        if let Some(reward) = trial.reward {
            let _ = writeln!(out, "- Reward: {reward}");
        }
        if let Some(calls) = trial.metric("api_calls") {
            let _ = writeln!(
                out,
                "- API calls: {calls}; MCP bypass: {}",
                trial.metric("mcp_bypass").unwrap_or(0.0)
            );
        }
        if let Some(exception) = &trial.exception {
            let _ = writeln!(
                out,
                "- Host exception: {}: {}",
                exception.kind, exception.message
            );
        }
        if !explained.checks_not_ok.is_empty() {
            let _ = writeln!(
                out,
                "- Checks not satisfied: {}",
                explained.checks_not_ok.join(", ")
            );
        }
        if trial.failed_checks.is_empty() {
            if trial.status == Status::Pass {
                out.push_str("- Every check passed.\n");
            } else {
                out.push_str("- The assessment lists no failed check.\n");
            }
        } else {
            out.push_str("- Assessment failures:\n");
            for (index, failure) in trial.failed_checks.iter().enumerate() {
                let text =
                    |value: &serde_json::Value| value.as_str().unwrap_or("unknown").to_owned();
                let _ = write!(
                    out,
                    "  {}. **{}** ({}) at `{}`: {}",
                    index + 1,
                    text(&failure.check),
                    text(&failure.disposition),
                    failure.location,
                    failure.reason
                );
                if !failure.journal_evidence.is_empty() {
                    let _ = write!(out, " [journal entries {:?}]", failure.journal_evidence);
                }
                out.push('\n');
            }
        }
        match &explained.answer_excerpt {
            Some(answer) => {
                out.push_str("- Collected answer:\n\n");
                for line in answer.lines() {
                    let _ = writeln!(out, "  > {line}");
                }
            }
            None => out.push_str("- No answer file was collected.\n"),
        }
        out.push('\n');
    }
    out
}
