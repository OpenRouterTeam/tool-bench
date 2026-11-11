# Production-worker rehearsal: findings

**Next action:** run `nop` over all 150 tasks in production (free; expect 150 `I` and 0 `S`), and
decide items 1 to 3 below before the first paid run.

**Verdict:** nothing in the new task format broke under the real worker. All 26 scripted attempts
ended as expected (oracle reward 1, nop reward 0, corrupted contract withheld), and all 150 task
directories pass the worker's planner, validation and config loading. Ten things degrade or are
risky; three of them matter before the first paid run.

Run on 2026-11-10 against `openrouter-web@6a864076` (`services/gcp-harbor-trial`). `origin/main`
moved twice during the work (`158b7e9c`, `6a864076`, `113db1b9`); the worker directory is
identical at all three. `harbor==0.23.0` from its `uv.lock`, macOS with OrbStack (Docker 29.8.2,
Compose 5.5.0), images `tool-bench-world:e45-std` and `tool-bench-tools:e45`. Worker paths below
are under
`services/gcp-harbor-trial/src/gcp_harbor_trial/`; Harbor paths are under `harbor/` in the venv;
`taskgen.rs` is `crates/scenario/src/taskgen.rs` in this checkout at the time of writing.

## What was done about these findings

| Finding | In this repository | Still open in openrouter-web |
|---|---|---|
| 1. Withheld trials | Kept void on purpose: a trial the mock could not serve is not scored either way, because scoring only the failures among them would count the mock's limits against the agent. `docs/production.md` tells readers to compare the skipped count per arm. | Add `RewardFileNotFoundError` to `INFRA_SKIP_REASONS` so the row carries a reason. |
| 2. No `oracle` in production | `deploy/stock-campaign.sh` runs oracle and no-op through stock Harbor locally. | Add `oracle` to `HarborAgentName`, or run `nop` over all 150 as the canary. |
| 3. 41 s of stop waits | Fixed: `stop_signal: SIGKILL` on all three services and `STOPSIGNAL SIGKILL` in the verifier image. | |
| 4. Verifier images at verify time | Fixed: tasks no longer declare `network_mode = "no-network"` for the verifier. | |
| 5. `expected_tool_names` misses 15 tasks | | Extend the regex in `expected_tools.py` to `bound_call` lines (a warning only). |
| 6. Agent image built per trial | Not changed. | Publish the agent image, or accept about a minute per trial. |
| 7. Agent container has open internet | Not changed: the mock services are reachable only through the MCP servers, and direct access is an integrity violation. | Decide whether the allowlist should apply. |
| 8. Three artifacts never exist | Fixed: optional-mode tasks no longer declare them. | |
| 9. Verifier is handed secrets | `bench-verify` uses none. | Stop passing the agent's variables to the verifier. |
| 10. Trial-name collision | Fixed: `taskgen` refuses two tasks that share their first 44 characters. | |

## 1. Exercised for real vs substituted

| Piece | Status | Detail |
| --- | --- | --- |
| Worker entry `trial.main()`: arguments, admin token, `execute_attempt`, `status.json`, exit codes | Real | 26 attempts |
| `load_config`: `validate_task_dir`, `expected_tool_names`, forced `BullseyeSecurityPinnedEnvironment` | Real | |
| `run_harbor`: `private_trial_console`, `trial_session_proxy`, rate-limit wrapper, `Trial.create(config).run()` | Real, one substitution | proxy socket bound to `127.0.0.1`, not the bridge gateway |
| Forced environment class in both environments: apt pin exec as root, patched egress sidecar build | Real | |
| Task: compose with `world` and `tools`, collect hook, artifacts, separate `no-network` verifier, `bench-verify` | Real | |
| Planner: `RunSpec`, `BENCHMARKS["tool-bench"]`, `plan_run`, `job_config`, `expand_trials`, `write_plan` | Real, two substitutions | `clone_dataset` copies local tasks; `oracle` renamed in after validation |
| `finalize` and `result_row`: `collect_results`, `planned_attempts`, `aggregate`, `result_rows`, `to_parquet` | Real, one substitution | GCS bucket is a directory shim |
| `McpReadyWaitMixin` (readiness probe, expected-tool check) | Real code, substitute host | runs on a no-model agent, outside `execute_attempt` |
| Unmodified entry point on this Mac | Fails, as expected | `WORKER_ERROR`, exit 20: cannot bind 192.168.215.1 |
| Argo, GKE scheduling, per-pod dind with registry mirror and auth, ClickHouse billing, model harnesses | Not exercised | |

## 2. Results per task (scripted pass, one trial at a time)

| Task | oracle | nop |
| --- | --- | --- |
| `business__stripe__customer-mrr` (lookup, answer file) | reward 1, `C`, 73 s | reward 0, `I`, 64 s |
| `code__github__file-login-bug` (creates a record) | reward 1, `C`, 74 s | reward 0, `I`, 81 s |
| `ops__multi__pagerduty-sentry-github-issue` (multi-service write) | reward 1, `C`, 71 s | reward 0, `I`, 106 s |
| `search__brave__hosting-sla-lookup` | reward 1, `C`, 66 s (retries: 87 s, 64 s) | reward 0, `I`, 83 s |
| `business__multi__hubspot-largest-deal-salesforce-owner` (47-character trial name) | reward 1, `C`, 73 s | reward 0, `I`, 69 s |

No trial exception in any of them. An earlier manual pass over the first four tasks gave the same
outcomes (72 to 125 s). Raw data: `/tmp/tb-prod-worker/runs/`, `/tmp/tb-prod-worker/pass1/`.

## 3. What production would record

| Case | `status.json` | Pod exit | Exception | Reward | `score_value` | `explanation` | Run totals |
| --- | --- | --- | --- | --- | --- | --- | --- |
| oracle pass | `COMPLETED` | 0 | none | 1.0 | `C` | null | accuracy 1.0, 5 of 5 |
| nop fail | `COMPLETED` | 0 | none | 0.0 | `I` | null | accuracy 0.0, 0 of 5 |
| unassessable (truncated `tests/expect.json`) | `TRIAL_ERROR` | 0 | `RewardFileNotFoundError` | none | `S` | null | accuracy 0.0 over 0 questions, 1 errored trial |

The verifier wrote `reward.invalid.json` and `assessment.json` (`outcome: invalid_contract`), and
no `reward.json`.

## 4. Findings, ranked

### Decide before the first paid run

**1. A withheld trial is recorded as skipped: no reason, a successful pod, and two different scores.**
- What happens: the row is `S` with a null `explanation`, the task drops out of accuracy, and the
  pod exits 0. Harbor's `result.json` counts the same attempt as reward 0.
- Evidence: five oracle passes plus the withheld attempt, taken as one run, give mean reward
  0.833 in `result.json` and accuracy 1.0 over 5 questions in `result.parquet` (`classify.py`).
- Worker: `result_row.py:155-171` (no rewards means `S`), `result_row.py:47-49` and `188-199` (no
  reason for this exception), `result_row.py:238-250` (skips drop out), `outcome.py:17` (exit 0),
  `finalize.py:260` (Harbor's mean treats a missing reward as 0).
- Why it matters: anything that makes `contract_ok` or `environment_ok` false removes the task
  from the score instead of failing it (`crates/mock-core/src/bin/bench-verify.rs:552`). If an
  agent can cause that, it raises its own accuracy, and nobody can count it from the parquet.
- Fix, tool-bench: go through every `contract` and `environment` failure reason in `bench-verify`
  and make each one an agent can cause write `reward.json` with reward 0. Withhold only for
  harness faults.
- Fix, worker: add `RewardFileNotFoundError` to `INFRA_SKIP_REASONS` (for example
  `infra:verifier-withheld`), and decide whether such attempts should exit non-zero so the
  workflow can retry them.

**2. Production cannot run `oracle`; `nop` is the only free canary for this format.**
- Worker: `run_spec.py:37-50` (`HarborAgentName` has `nop`, not `oracle`); the planner then exits
  2 at `plan.py:509-514`.
- Fix, no code: run `nop` on all 150 first. It exercises images, compose, the collect hook, the
  verifier environment and classification. Expect 150 `I`; any `S` is a real problem.
- Fix, worker: add `"oracle"` to the literal, then require 150 `C` from an oracle run.

**3. About 41 s of every trial is Docker waiting 10 s, four times, for containers that ignore SIGTERM.**
- What happens: agent `main` and verifier `main` run Harbor's `sh -c "sleep infinity"` as PID 1;
  `world` and `tools` do not exit on SIGTERM either. Docker events show kill, 10 s, kill for
  each. Stop waits were 40.6 to 42.6 s in every standard trial (wall 64 to 106 s).
- Worker: nothing of its own. It pins `harbor==0.23.0` (`pyproject.toml:8`), whose
  `trial/trial.py:1409` stops `main` and `environments/docker/docker.py:1107` runs `compose down`.
  Stock Harbor behaves the same.
- Fix, tool-bench, measured: `stop_signal: SIGKILL` on `main`, `world` and `tools` in the
  generated compose file (`taskgen.rs:566`, `574`, `580`) and `STOPSIGNAL SIGKILL` in
  `tests/Dockerfile` (`taskgen.rs:254`). The same oracle trial went from 73 s to 41 s, 42 s and
  20 s in three runs, still reward 1 (`extras.sh`, step 3). Each container's artifacts are
  collected before it is stopped. That is 80 to 100 node-minutes per 150 trials.

### Fix soon

**4. The verifier needs two Docker Hub images at verify time, after the agent has already run.**
- What happens: for a `no-network` verifier Harbor runs a kernel probe container
  (`alpine:3.23.4@sha256:5b10...`, 30 s limit including the pull) and builds the egress sidecar
  (`FROM gogost/gost:3.2.7-nightly.20260602@sha256:afc0...`). A per-pod dind starts empty, so
  both are fetched on every trial. A failed probe makes Harbor reject the environment
  (`ValueError: network_mode='no-network' is not supported`); a failed build raises
  `RuntimeError`. Either way the attempt becomes `S` after the model spend (item 1).
- Worker: `patches/egress_read_timeout.py:49-59` (builds its own sidecar variant per daemon);
  Harbor `environments/docker/docker.py:918-950` and `971-972`, `environments/base.py:781-789`.
- Status: by reading. Both images were local here (probe plus builds took 2.6 to 5 s).
- Fix, worker: when the task's verifier environment is not public, run the probe and the sidecar
  build at trial start (for example from `patches/terminal_bench/bullseye_security_patch.py:22`),
  or bake both images into the dind image.
- Fix, tool-bench only: drop `network_mode = "no-network"` (`taskgen.rs:431`). That removes the
  dependency and about 3 s, and gives up Harbor-enforced isolation. `bench-verify` needs no network.

**5. `expected_tool_names` returns nothing for 15 tasks.**
- What happens: the worker's pattern matches `call <port> "$DIR/call-N.json"` only. Taskgen's
  `bound_call <port> "$DIR/call-N.json" "$DIR/call-N.capture.json" ...` lines (67 lines, 15 tasks)
  do not match, so those tasks get no `mcp_expected_tools` and the probe's "server lists its
  tools without some the solution calls" check is skipped. Readiness gating is unaffected.
- Worker: `expected_tools.py:18`, `config.py:88-90`, `agents/mcp_ready.py:428-439`.
- Evidence: `ops__multi__pagerduty-sentry-github-issue` loads with empty agent kwargs;
  `business__stripe__customer-mrr` gets `{"stripe": ["stripe_api_read"]}`. The other 135 are complete.
- Fix, tool-bench: emit bound calls as `call <port> "$DIR/call-N.json"` as well and let `call`
  find `call-N.capture.json` and `call-N.arguments-schema.json` by name (`taskgen.rs:699`).
- Fix, worker: `^(?:bound_)?call\s+(\d+)\s+"\$DIR/(call-\d+\.json)"(?:\s.*)?$`.
- The 15 tasks are listed in `/tmp/tb-prod-worker/scale-checks.json`.

**6. The agent image is built with `apt-get` on every trial.**
- What happens: `environment/Dockerfile` (`taskgen.rs:749`) took 62 s uncached here. A per-pod
  dind has no cache, and the build needs deb.debian.org at trial start.
- Worker: `plan.py:293` sets `force_build=False`, so a `docker_image` would be used as is (Harbor
  `environments/definition.py:26-37`), with the pull retries of `patches/shell_entrypoint.py:57-69`.
- Status: by reading, not run. As far as I can tell this is not new to this format.
- Fix, tool-bench: publish that image next to world and tools and set `[environment] docker_image`.

### Low risk

**7. The agent container has open internet; the worker's allowlist is ignored for these tasks.**
- What happens: the worker passes `extra_allowed_hosts`, but the tasks declare no `network_mode`,
  so Harbor keeps `public` and warns six times per trial in `runner.log`. From the agent
  container `https://example.com` returned 200. There is no egress sidecar on the agent side.
- Worker: `plan.py:283-295`, `proxy.py:253-254`; Harbor `trial/network_policy.py:28-36`.
- Fix, tool-bench, untested: `network_mode = "allowlist"` with `allowed_hosts = ["tools"]` under
  `[environment]`. It needs its own rehearsal, because Harbor then moves `main` into the
  sidecar's network namespace.

**8. Three declared artifacts never exist in production.**
- What happens: `runner-admission.json`, `realized-manifest.json` and `inference-evidence.json`
  give 3 `failed` manifest entries, 6 failed `docker cp` calls and about 14 KB of tracebacks in
  `trial.log` per trial. `/app/answer.md` is also `failed` on tasks that write no answer.
  Harmless: under 1 s, and `test.sh` tolerates their absence.
- Worker: none; Harbor `trial/artifact_handler.py:287-341`.
- Fix, tool-bench: leave them out when generating for stock Harbor (`taskgen.rs:382-391`).

**9. The verifier is handed secrets it never uses.**
- What happens: its environment gets the agent's variables (the real `OPENROUTER_API_KEY` in
  production) and `HARBOR_TRIAL_ADMIN_TOKEN`. The new `test.sh` reads neither. The container has
  no network, so exposure is low.
- Worker: `benchmarks.py:72-76`, `plan.py:296-305`.
- Fix, worker: `verifier_inherits_agent_env=False` and no `verifier_env` for tool-bench once the
  old format is retired. Nothing to change in tool-bench.

**10. A trial-name collision would fail planning for the whole run.**
- What happens: names are cut to 44 characters. 12 of 150 are cut today and none collide.
- Worker: `plan.py:115-117`, `plan.py:383-386`.
- Fix, tool-bench: a taskgen or CI check that the 44-character slug prefix is unique
  (`scale_checks.py` reports `trial_name_collisions`).

## 5. Checked, nothing wrong

Planning and config
- `validate_task_dir` and `plan_task`: 150 of 150 accepted, none unsupported.
- `load_config`: 150 of 150 for a scripted agent and for claude-code. `Trial.create`: 150 of 150.
- `select_tasks`: 150 with no filter; 15 for `search__*` with limit 15 (the weekend matrix's
  `tb` and `tbs` cells).
- `sizing.py`: one pod spec for every task: dind 4750m CPU, 14848Mi memory, 39936Mi storage. The
  verifier environment does not change it (`sizing.py:61-77` takes the maximum).
- `mcp_ready_command` builds for all 150 (32 streamable-HTTP servers each).

Environment and verifier
- Forced class: the apt pin exec exits 0 in both environments (no `/etc/apt/sources.list`).
- Separate verifier: starts, and stays `no-network`. Harbor does not turn it into an allowlist;
  the sidecar has `EGRESS_CONTROL_INITIAL_NETWORK_MODE=no-network` and an empty allowlist, and
  every TCP connection from the verifier failed, including to `openrouter.ai`.
- Collect hook: finalized the world with the worker-issued token in every trial.
- Admin token: not in the agent container; `world:9001` is unreachable from it. No resolved
  secret is written to the run directories (`config.json` keeps the `${...}` templates).
- MCP readiness: 32 of 32 servers answered `tools/list` in 2 to 11 s over five runs (limit 120 s).

Scoring and naming
- oracle is `C`, nop is `I`, the latest retry wins, the parquet writes.
- `aggregate` copes with the 23-key reward dict and with a missing reward.
- A 47-character attempt id works (Harbor hashes the verifier project name past 63 characters).

## 6. Numbers

| Measure | Value |
| --- | --- |
| Wall clock per trial | 64 to 106 s, median 73 s (shared daemon) |
| Stop waits inside that | 40.6 to 42.6 s |
| Harbor phases | environment 7.5 to 14.6 s; agent 0 to 4.8 s; verifier 19.7 to 40.7 s |
| Separate verifier, warm | prepare 2.6 to 5.1 s; containers up 2.8 to 3.3 s (sidecar healthy after 2.0 to 2.1 s); exec, upload and `test.sh` 1.7 to 3.2 s; stop wait 10 s; teardown 1.3 to 7.6 s |
| Cold builds | agent image 62 s; verifier image 7 s; first sidecar build 7 s (base layers cached) |
| Peak memory, scripted agents | 362 MiB in total (tools 344 MiB) |
| Peak memory, 32 MCP sessions held | tools 1.83 GiB, world 89 MiB, main 42 MiB; dind limit is 14848Mi |

On a fresh dind every trial pulls world (280 MB) and tools (3.33 GB) as today, and additionally
fetches or builds: the agent image (item 6), the alpine probe (14.6 MB) and the gost sidecar
(76 MB) (item 4), and the verifier image, which is the world image plus 10 files (no extra pull).

**Next:** open `deploy/production/README.md` and run `./scale-checks.sh` (10 s, no containers).
