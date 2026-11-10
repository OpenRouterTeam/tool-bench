# Documentation

Start with [production.md](production.md). The project overview is in
[`../README.md`](../README.md).

## Run it

- [production.md](production.md): a production run end to end, reading a trial, comparing arms with `bench-report`, changing the tool-set size, releasing a change.
- [exposure-and-surface-audit.md](exposure-and-surface-audit.md): the `taskgen --exposure` variants in detail, and the strict controller's surface audit.
- [workspace-profiles.md](workspace-profiles.md): optional workspace files and evaluation framing as declared conditions.
- [research/tool-payload.md](research/tool-payload.md): measured size of the tool set, per endpoint.

## Understand the grading

- [contracts.md](contracts.md): what a task contract can require of state, records, history and written text.
- [answer-fact-contracts.md](answer-fact-contracts.md): how a lookup reply is read and when it passes, fails or is unresolved.
- [operation-support.md](operation-support.md): which service operations the mocks model, per service, with their limits.
- [oracle-results.md](oracle-results.md): the checks on a reference solution's tool results.
- [runner-admission.md](runner-admission.md): the strict controller's collection protocol and receipt.
- [execution-provenance.md](execution-provenance.md): the hashes that bind a trial to its sources, and `bench-report --require-attested`.
- [native-inference-capture.md](native-inference-capture.md): inference capture under the strict controller.
- [content-review.md](content-review.md): optional signed review for strict-controller trials.
- [agent-surfaces.md](agent-surfaces.md): what the agent can see of its connections.
- [research/2026-11-03-benchmark-review.md](research/2026-11-03-benchmark-review.md), [comparisons](research/2026-11-03-benchmark-comparisons.md), [improvement plan](research/2026-11-03-benchmark-improvement-plan.md): the review that the current grading answers.

## Change it

- [runbook.md](runbook.md): images, regenerating tasks, local checks, the strict controller, replay, tool-list capture.
- [`../deploy/pilot/README.md`](../deploy/pilot/README.md): attempt one task by hand, or with an AI subagent, through the real MCP servers and have it graded.
- [`../deploy/production/README.md`](../deploy/production/README.md): run the production worker's own code against generated tasks; [findings](../deploy/production/FINDINGS.md) from the last rehearsal.
- [authoring.md](authoring.md): writing a service mock and its scenarios.
- [authoring-admission.md](authoring-admission.md): what `taskgen` validates, and the generated inventory (`coverage/scenario-inventory.json`).
- [organization-materialization.md](organization-materialization.md): a shared organization dataset with per-task overlays.
- [semantic-fault-schedules.md](semantic-fault-schedules.md): declared transient errors and lost responses.
- [families-clarification-release.md](families-clarification-release.md): difficulty families, the scripted-user diagnostics and the frozen release manifest.
- [research/mcp-servers.md](research/mcp-servers.md): which MCP server each service uses and why.
- [superpowers/specs/2026-10-30-tool-bench-design.md](superpowers/specs/2026-10-30-tool-bench-design.md): the original design.
- [provenance.md](provenance.md): upstream sources and licences.
- [`../CONTRIBUTING.md`](../CONTRIBUTING.md): checks before a pull request.

## History

- [history/](history/README.md): working notes from the grading overhaul. Superseded.
- [research/arms-2026-10-31.md](research/arms-2026-10-31.md), [research/smoke-2026-10-31.md](research/smoke-2026-10-31.md): model runs under an earlier evaluator. Not comparable with current results.
- [superpowers/plans/](superpowers/plans/): the original implementation plans.
