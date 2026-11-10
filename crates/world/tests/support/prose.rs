//! Table-driven proof that ordinary wording of a free-text field gets a pass/fail verdict.
//!
//! Each proof names one `reviewed_text` (or `reviewed_fields`) check of a real scenario and
//! lists wordings a capable assistant might write. Every wording is written through the mock
//! service into state by `solve` and graded by the real grader: the listed `passes` must pass and
//! the listed `fails` must fail for that text alone. Nothing may be left for adjudication.

use std::{collections::BTreeSet, future::Future, path::Path};

use mock_core::grading::{CheckDisposition, Grade, Outcome};
use scenario::Scenario;

pub struct Proof<'a> {
    /// Scenario id, for example `code/github/file-login-bug`.
    pub scenario: &'a str,
    /// Pointer of the check under test inside its record, for example `/body`. Use `""` for a
    /// `reviewed_fields` projection and `"judged_goal"` for a Firecrawl monitor's goal text.
    pub field: &'a str,
    /// Further failure reasons or locations the same wrong text legitimately causes, such as
    /// `field /html_body` when the service renders the same text into a second checked field.
    pub also: &'a [&'a str],
    /// The instruction dictates the text, so the passing wordings are the dictated text with
    /// different layout or emphasis and may equal a reviewed example.
    pub dictated: bool,
    /// Truthful wordings: terse, a normal paragraph, Markdown, with greeting or extra context.
    pub passes: &'a [&'a str],
    /// Wordings that lack the key fact or carry a wrong identifier, number or name. Optional
    /// narration without mentions has no key fact, so its list holds only known-incorrect text
    /// and may be shorter than two.
    pub fails: &'a [&'a str],
}

fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Proof<'_> {
    /// Runs every wording. `solve` must start a fresh trial, carry out the scenario's reference
    /// solution with the given text in the field under test, and return the grade.
    pub async fn run<Solve, Run>(&self, solve: Solve)
    where
        Solve: Fn(String) -> Run,
        Run: Future<Output = Grade>,
    {
        let name = format!("{} {}", self.scenario, self.field);
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios")
            .join(self.scenario);
        let scenario = Scenario::load(&directory).unwrap();
        let mut accepted = BTreeSet::new();
        let mut rejected = BTreeSet::new();
        let mut declared = 0;
        // Optional narration has no required content: only known-incorrect text can fail it.
        let mut narration = false;
        for record in &scenario.expect.records {
            let text = match self.field {
                "" => {
                    if let Some(fields) = &record.reviewed_fields {
                        declared += 1;
                        assert!(fields.deterministic(), "{name}: no deterministic reading");
                        accepted.extend(fields.accepted.iter().map(|example| {
                            folded(&example.values().cloned().collect::<Vec<_>>().join("\n"))
                        }));
                    }
                    continue;
                }
                "judged_goal" => record
                    .firecrawl_monitor
                    .as_ref()
                    .map(|goal| &goal.judged_goal),
                field => record.reviewed_text.get(field),
            };
            if let Some(text) = text {
                declared += 1;
                assert!(text.deterministic(), "{name}: no deterministic reading");
                narration |= text.optional && text.mentions.is_empty();
                accepted.extend(text.accepted.iter().map(|example| folded(example)));
                rejected.extend(text.rejected.iter().map(|example| folded(example)));
            }
        }
        assert!(declared > 0, "{name}: the scenario declares no such check");
        assert!(
            self.passes.len() >= 4 && (narration || self.fails.len() >= 2),
            "{name}: a proof needs at least four passing and two failing wordings"
        );
        let mut seen = BTreeSet::new();
        for wording in self.passes.iter().chain(self.fails) {
            assert!(
                seen.insert(folded(wording)),
                "{name}: repeated wording {wording:?}"
            );
        }
        for wording in self.passes {
            assert!(
                self.dictated || !accepted.contains(&folded(wording)),
                "{name}: {wording:?} is a reviewed example; write it the way an assistant would"
            );
        }
        for wording in self.fails {
            assert!(
                narration || !rejected.contains(&folded(wording)),
                "{name}: {wording:?} is a rejected example; write a wrong text of your own"
            );
        }
        let markers: Vec<String> = std::iter::once(match self.field {
            "" => "the reviewed fields".to_owned(),
            "judged_goal" => "monitor goal:".to_owned(),
            field => format!("field {field}:"),
        })
        .chain(self.also.iter().map(|marker| (*marker).to_owned()))
        .collect();
        for wording in self.passes {
            let grade = solve((*wording).to_owned()).await;
            assert_eq!(
                grade.outcome,
                Outcome::Pass,
                "{name}: truthful wording must pass\n--- wording ---\n{wording}\n--- failures ---\n{:#?}",
                grade.failures
            );
        }
        for wording in self.fails {
            let grade = solve((*wording).to_owned()).await;
            assert_eq!(
                grade.outcome,
                Outcome::Fail,
                "{name}: wrong wording must fail, not pass or stay unresolved\n--- wording ---\n{wording}\n--- failures ---\n{:#?}",
                grade.failures
            );
            assert!(
                !grade.adjudication_needed && !grade.failures.is_empty(),
                "{name}: {wording:?} left a check for adjudication: {:#?}",
                grade.failures
            );
            for failure in &grade.failures {
                assert!(
                    failure.disposition == CheckDisposition::Failed
                        && markers.iter().any(|marker| {
                            failure.reason.contains(marker) || failure.location.contains(marker)
                        }),
                    "{name}: {wording:?} failed for something other than its text: {failure:#?}"
                );
            }
            // A selector that already reads the same text can reject the record before the text
            // check sees it; `also` names that outcome. Otherwise the check under test must fail.
            assert!(
                !self.also.is_empty()
                    || grade
                        .failures
                        .iter()
                        .any(|failure| failure.reason.contains(&markers[0])),
                "{name}: {wording:?} did not fail the check under test: {:#?}",
                grade.failures
            );
        }
    }
}
