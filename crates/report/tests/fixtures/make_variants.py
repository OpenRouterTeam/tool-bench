#!/usr/bin/env python3
"""Rebuilds stock/variants from the real trials in stock/oracle and stock/nop.

Each variant keeps a real trial's collected world evidence and changes one input, then
runs the real verifier the way a generated task's tests/test.sh does under stock Harbor
(--allow-unattested-runner --withhold-invalid-reward). Only result.json, config.json and
lock.json are written by hand, in the shape Harbor 0.23.0 gives them.

    cargo build -p mock-core --bin bench-verify
    python3 crates/report/tests/fixtures/make_variants.py target/debug/bench-verify

The evidence is stamped with the evaluator that produced it, so this only works while
EVALUATOR_VERSION equals the fixtures' evaluator. After a bump, copy fresh trials first.
"""

import copy
import json
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent / "stock"
VERIFY = pathlib.Path(sys.argv[1]).resolve()
SEARCH = HERE / "oracle/2026-11-10__13-16-13/search__exa__find-vp-engineering__wM2CQWr"
ZENDESK_NOP = HERE / "nop/2026-11-10__13-29-13/business__zendesk__find-invoice__r26zPNi"
INPUTS = ["contract.json", "task-source.json", "task-context.json", "runner-policy.json",
          "framing.json", "profile.json"]
# The verifier's reserialized diagnostic copies. Nothing binds them, and they are large.
DERIVED = ["evidence.json", "state.json", "initial-state.json", "journal.json", "events.json"]
MODEL = {"name": "claude-code", "version": "2.1.235",
         "model_info": {"name": "claude-sonnet-5", "provider": "anthropic"}}


def variant(name, base, trial, *, answer=None, context_overflow=False, tamper_source=False,
            usage=None):
    out = HERE / "variants" / name / trial
    shutil.rmtree(out.parent, ignore_errors=True)
    tests = out / "tests"
    (out / "verifier").mkdir(parents=True)
    (out / "artifacts/world").mkdir(parents=True)
    tests.mkdir()
    for file in INPUTS:
        shutil.copy(base / "verifier" / file, tests / file)
    shutil.copy(base / "artifacts/world/evidence.json", out / "artifacts/world/evidence.json")
    scenario = json.loads((base / "verifier/verification.json").read_text())["scenario"]
    if tamper_source:
        source = json.loads((tests / "task-source.json").read_text())
        source["dataset_sha256"] = "sha256:" + "0" * 64
        (tests / "task-source.json").write_text(json.dumps(source, indent=2) + "\n")
    command = [
        str(VERIFY), "--scenario", scenario,
        "--contract", str(tests / "contract.json"),
        "--task-source", str(tests / "task-source.json"),
        "--task-context", str(tests / "task-context.json"),
        "--evidence", str(out / "artifacts/world/evidence.json"),
        "--runner-policy", str(tests / "runner-policy.json"),
        "--runner-admission", str(tests / "runner-admission.json"),
        "--realized-manifest", str(tests / "realized-manifest.json"),
        "--inference-evidence", str(tests / "inference-evidence.json"),
        "--output", str(out / "verifier"),
        "--allow-unattested-runner", "--withhold-invalid-reward",
    ]
    if answer is not None:
        (out / "artifacts/agent").mkdir()
        (out / "artifacts/agent/answer.md").write_text(answer)
        command += ["--answer", str(out / "artifacts/agent/answer.md")]
    if context_overflow:
        command.append("--context-overflow")
    status = subprocess.run(command, capture_output=True, text=True).returncode
    shutil.rmtree(tests)
    for file in DERIVED:
        (out / "verifier" / file).unlink(missing_ok=True)

    result = json.loads((base / "result.json").read_text())
    stem = f"/tmp/tb-stock/jobs/variants/{name}"
    result.update(trial_name=trial, trial_uri=f"file://{stem}/{trial}", agent_info=MODEL)
    result["config"].update(trial_name=trial, trials_dir=stem)
    result["config"]["agent"].update(name="claude-code", model_name="anthropic/claude-sonnet-5")
    if usage:
        result["agent_result"] = dict(result["agent_result"] or {}, **usage)
    reward = out / "verifier/reward.json"
    if reward.exists():
        assert status == 0, status
        result["verifier_result"] = {"rewards": json.loads(reward.read_text())}
        result["exception_info"] = None
    else:
        # What Harbor records when the verifier leaves no reward file.
        assert status == 3 and (out / "verifier/reward.invalid.json").exists(), status
        message = (f"No reward file found at {stem}/{trial}/verifier/reward.txt or "
                   f"{stem}/{trial}/verifier/reward.json")
        result["verifier_result"] = None
        result["exception_info"] = {
            "exception_type": "RewardFileNotFoundError",
            "exception_message": message,
            "exception_traceback": f"Traceback (most recent call last):\n  ...\n"
                                   f"harbor.verifier.verifier.RewardFileNotFoundError: {message}\n",
            "occurred_at": result["verifier"]["finished_at"],
        }
    (out / "result.json").write_text(json.dumps(result, indent=4))
    config = json.loads((base / "config.json").read_text())
    config.update(trial_name=trial, trials_dir=stem)
    config.setdefault("agent", {}).update(name="claude-code", model_name="anthropic/claude-sonnet-5")
    (out / "config.json").write_text(json.dumps(config, indent=4))
    lock = json.loads((base / "lock.json").read_text())
    lock["agent"].update(name="claude-code", model_name="anthropic/claude-sonnet-5")
    (out / "lock.json").write_text(json.dumps(lock, indent=4))
    outcome = json.loads((out / "verifier/assessment.json").read_text())["outcome"]
    print(f"{name}: {outcome}")
    return out


USAGE = {"n_input_tokens": 1843210, "n_cache_tokens": 1710034, "n_output_tokens": 6120,
         "cost_usd": 1.42}

# The model named the other Priya Raman: a typed answer failure with real tool calls.
variant("wrong-answer", SEARCH, "search__exa__find-vp-engineering__WRONGAN",
        answer="https://www.linkedin.com/in/priya-raman-4b21a9", usage=USAGE)
# A right URL in a notation the contract has not reviewed: needs adjudication.
variant("unresolved", SEARCH, "search__exa__find-vp-engineering__UNRESLV",
        answer="The VP of Engineering at Tessellate Bio is Priya Raman; her LinkedIn is "
               "linkedin.com/in/priya-raman-tessellate",
        usage=dict(USAGE, cost_usd=1.18))
# The world loaded a dataset other than the task's: an environment error, reward withheld.
variant("withheld-reward", SEARCH, "search__exa__find-vp-engineering__WITHHLD",
        answer="https://www.linkedin.com/in/priya-raman-tessellate", tamper_source=True,
        usage=USAGE)
# No world request at all and the operator flagged a context-length rejection.
variant("context-overflow", ZENDESK_NOP, "business__zendesk__find-invoice__CTXOVER",
        context_overflow=True)

# A trial that started and never wrote result.json (killed pod, lost upload).
incomplete = HERE / "variants/incomplete/business__zendesk__find-invoice__INCOMPL"
shutil.rmtree(incomplete.parent, ignore_errors=True)
incomplete.mkdir(parents=True)
for file in ["config.json", "lock.json"]:
    value = json.loads((ZENDESK_NOP / file).read_text())
    if file == "config.json":
        value.update(trial_name=incomplete.name)
    value.setdefault("agent", {}).update(name="claude-code", model_name="anthropic/claude-sonnet-5")
    (incomplete / file).write_text(json.dumps(value, indent=4))
print("incomplete: no result.json")
