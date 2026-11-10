# Pilot harness: act as the benchmark agent by hand

Lets a person or a subagent attempt one task through the real MCP servers, then grades the
resulting world with the host `bench-verify`. Needs Docker, `jq` and Python 3.11+.

```sh
deploy/pilot/prepare.sh                    # world image, bench-verify, tasks in /tmp/tb-pilot/tasks
deploy/pilot/up.sh code__github__file-login-bug 3      # slot 3 of 0-19; prints the prompt file
#   ... the agent works with /tmp/tb-pilot/agents/3/mcp ...
deploy/pilot/grade.sh 3 [/tmp/tb-pilot/agents/3/app/answer.md]
deploy/pilot/down.sh 3
deploy/pilot/selftest.sh                   # reference solution = 1, no action = 0, two slots
```

- `prepare.sh [--world-image TAG] [--tools-image TAG] [--out DIR]` builds `tool-bench-world:pilot`
  from this tree, reuses `tool-bench-tools:e45`, builds `target/debug/bench-verify` and regenerates
  the tasks. Safe to re-run: live slots keep their image and inputs but use the new `bench-verify`.
- `up.sh <task> <slot>` takes a directory under `$PILOT_TASKS` (default `/tmp/tb-pilot/tasks`)
  or an absolute task path, and publishes tools port `71NN` on `127.0.0.1:(20000 + slot*100 + NN)`.
  State is in `/tmp/tb-pilot/slots/<slot>/`; a graded earlier run moves to `/tmp/tb-pilot/archive/`.
- `mcp.py` reads the slot from `PILOT_SLOT`; `/tmp/tb-pilot/agents/<slot>/mcp` runs it with
  that set. It keeps one MCP session per server for the life of the slot, as a harness does.
- `grade.sh <slot> [answer-file]` finalizes the world and writes everything to
  `/tmp/tb-pilot/slots/<slot>/verifier/` (`assessment.json`, `journal.json`, `state.json`, ...).
  It prints `reward`, `outcome` and `adjudication_needed`, then one line per failure. A finalized
  world cannot be reused: a second call says so and only re-checks the saved evidence. The
  grader must come from the same tree as the tasks; `BENCH_VERIFY=<path>` selects another binary.
- `down.sh <slot>` removes compose project `tbpilot-<slot>` only. Operator-only `replay.sh <slot>`
  replays the task's reference solution through `mcp.py`.

## Text to hand the acting agent

`up.sh` fills in the instruction and the `agents/<slot>/` paths and writes `slots/<slot>/agent-prompt.md`.

<!-- agent-prompt:begin -->
```text
You are standing in for an AI agent that has been given the business request below. The
company's systems are reachable only as MCP tools, through this command-line client:

  {MCP} servers                                  # the MCP servers
  {MCP} tools <server> [--grep REGEX]            # tool names, one line each
  {MCP} schema <server> <tool>                   # full description and input schema
  {MCP} call <server> <tool> '<json arguments>'  # arguments may also be @file or - (stdin)

Rules:
- Work only through that client. Do not read its source, any repository on this machine,
  Docker, or anything else under /tmp/tb-pilot or /tmp/tb-stock, and do not look anywhere
  else for information about this request.
- If the request asks for a file under /app/, write it under {APP}/ instead
  (/app/answer.md becomes {APP}/answer.md).
- Nobody can answer questions. Do the work as you judge best, then stop.
- Finish with a short report: what you did, every tool call that changed something, and
  anything in the request or the tools that was unclear.

Request:

{INSTRUCTION}
```
<!-- agent-prompt:end -->
