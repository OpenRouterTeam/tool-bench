# Report fixtures

`stock/` holds Harbor trial directories as stock Harbor 0.23.0 writes them for generated
tasks (`RunnerAdmissionMode::Optional`): no runner receipt, no realized manifest,
`"runner_attested": false` in `verifier/verification.json`. `tests/arms.rs` reads them
through `bench-report`'s default path.

| Directory | What it is |
|---|---|
| `stock/oracle/` | Two real trials of the scripted oracle agent. Both pass. |
| `stock/nop/` | Two real trials of the scripted nop agent: one fails with no world request, one never started (`docker compose` failed, so `result.json` has an exception and no reward). |
| `stock/variants/` | Hand-made from the real trials above by `make_variants.py`: a wrong answer (fail), an unreviewed answer notation (unresolved), a withheld reward (environment error, `reward.invalid.json`), a context overflow, and a trial without `result.json`. |

The real trials are copies with the bulk removed: the verifier's reserialized
`evidence.json`, `state.json`, `initial-state.json`, `journal.json` and `events.json`, the
agent and trial logs, and (for the exception) most of the traceback. Nothing the
verification record binds by hash was changed: `artifacts/world/evidence.json`,
`artifacts/agent/answer.md` and the verifier's contract, task-source and task-context
copies are byte for byte what the trial produced.

In the variants only `result.json`, `config.json` and `lock.json` are written by hand.
Each variant's `verifier/` is the output of the real `bench-verify` on a real trial's
evidence with one input changed. They are labelled as a `claude-code` run with token and
cost figures so the report's agent and usage rows have something to show; no model
produced them.

## Evaluator revisions

These trials were graded by the evaluator named in their `verification.json`. Only that
revision can re-run them, so the tests expect `reproduced` while `EVALUATOR_VERSION` still
equals it and `recorded` afterwards. The outcome counts they assert do not change.

If the tests report a trial that **failed** re-verification, the current evaluator grades
a recorded input differently from the revision of the same number that recorded it.
Either grading changed without an `EVALUATOR_VERSION` bump, or the fixtures need
replacing: copy fresh stock-Harbor trials of the same tasks, trim them as described
above, and run `make_variants.py`.
