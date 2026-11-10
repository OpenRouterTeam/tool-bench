import argparse
import collections
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def read(path):
    return json.loads(path.read_bytes())


def sha(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--world-id", required=True)
    parser.add_argument("--tools-id", required=True)
    parser.add_argument("--evaluator", type=int, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--verifier", type=Path, required=True)
    parser.add_argument("--expected-total", type=int, required=True)
    parser.add_argument("directories", type=Path, nargs="+")
    options = parser.parse_args()
    inventory_bytes = options.inventory.read_bytes()
    sources = {
        row["id"]: row["source"]
        for row in json.loads(inventory_bytes)["scenarios"]
    }
    counts = collections.Counter()
    implementations = set()
    baselines = {}
    observed = set()
    for directory in options.directories:
        assessments = sorted(
            path for path in directory.rglob("assessment.json")
            if path.parent.name == "verifier"
        )
        assert assessments, directory
        for assessment_path in assessments:
            assert assessment_path.resolve() not in observed, assessment_path
            observed.add(assessment_path.resolve())
            verifier = assessment_path.parent
            trial = verifier.parent
            assessment = read(assessment_path)
            verification = read(verifier / "verification.json")
            manifest = read(verifier / "realized-manifest.json")
            receipt = read(verifier / "runner-admission.json")
            evidence_path = trial / "artifacts/world/evidence.json"
            evidence = read(evidence_path)
            outcome = assessment["outcome"]
            assert assessment["evaluator_version"] == options.evaluator, trial
            assert outcome in {"pass", "fail", "unresolved", "environment_error"}, trial
            assert all(assessment[field] is True for field in [
                "contract_ok", "integrity_ok", "runner_admission_checked",
                "runner_admission_ok", "preservation_checked", "history_checked",
            ]), trial
            assert assessment["environment_ok"] == (outcome != "environment_error"), trial
            assert manifest["execution_kind"] == "scripted", trial
            assert manifest["images"]["world"]["image_id"] == options.world_id, trial
            assert manifest["images"]["tools"]["image_id"] == options.tools_id, trial
            scenario = verification["scenario"]
            assert verification["world_source"] == evidence["source"] == sources[scenario], trial
            baseline = (
                sha(json.dumps(read(verifier / "initial-state.json"), sort_keys=True).encode()),
                manifest["images"]["main"]["filesystem_sha256"],
                manifest["images"]["main"]["runtime_config_sha256"],
                json.dumps(manifest["images"]["main"]["resource_limits"], sort_keys=True),
                verification["task_context_sha256"],
            )
            assert scenario not in baselines or baselines[scenario] == baseline, trial
            baselines[scenario] = baseline
            for name, field in [
                ("contract.json", "contract_sha256"),
                ("task-source.json", "task_source_sha256"),
                ("task-context.json", "task_context_sha256"),
                ("realized-manifest.json", "manifest_sha256"),
                ("runner-admission.json", "runner_admission_sha256"),
                ("runner-policy.json", "runner_policy_sha256"),
            ]:
                assert sha((verifier / name).read_bytes()) == verification[field], (trial, name)
            assert sha(evidence_path.read_bytes()) == verification["evidence_sha256"] == receipt["evidence_sha256"], trial
            assert evidence["finalization"]["phase"] == "sealed", trial
            assert evidence["finalization"]["pending"] == [], trial
            assert evidence["finalization"]["failure"] is None, trial
            assert receipt["phase"] == "stopped" and receipt["failure"] is None, trial
            assert receipt["main"]["running"] is False, trial
            implementations.add((receipt["adapter_sha256"], manifest["harness"]["code_sha256"]))
            with tempfile.TemporaryDirectory(prefix="tool-bench-reassessment-") as destination:
                command = [
                    str(options.verifier), "--scenario", scenario,
                    "--contract", str(verifier / "contract.json"),
                    "--evidence", str(evidence_path), "--output", destination,
                ]
                for flag, name in [
                    ("--task-source", "task-source.json"),
                    ("--task-context", "task-context.json"),
                    ("--realized-manifest", "realized-manifest.json"),
                    ("--runner-policy", "runner-policy.json"),
                    ("--runner-admission", "runner-admission.json"),
                ]:
                    command.extend([flag, str(verifier / name)])
                if verification.get("context_overflow"):
                    command.append("--context-overflow")
                if verification["answer_sha256"] is not None:
                    answer = trial / "artifacts/agent/answer.md"
                    assert sha(answer.read_bytes()) == verification["answer_sha256"], trial
                    command.extend(["--answer", str(answer)])
                reproduced = subprocess.run(command, capture_output=True, text=True)
                assert reproduced.returncode == 0, (trial, reproduced.stderr)
                assert read(Path(destination) / "assessment.json") == assessment, trial
            report = subprocess.run(
                [str(options.report), str(trial), str(trial), "--json"],
                capture_output=True, text=True,
            )
            if outcome in {"unresolved", "environment_error"}:
                assert report.returncode != 0 and not report.stdout.strip(), (trial, report.stderr)
            else:
                assert report.returncode == 0 and json.loads(report.stdout), (trial, report.stderr)
            if outcome == "environment_error":
                assert any(
                    failure["check"] == "environment" and failure.get("event_evidence")
                    for failure in assessment["failures"]
                ), trial
            counts[outcome] += 1
        print(f"{directory}: {len(assessments)} assessments source/byte/receipt checked and reproduced")
    assert len(implementations) == 1, implementations
    assert len(observed) == options.expected_total, (len(observed), options.expected_total)
    print(f"primary_total={len(observed)} outcomes={dict(counts)}")
    print(f"source_inventory_sha256={sha(inventory_bytes)}")
    print(f"world_image_id={options.world_id} tools_image_id={options.tools_id}")
    print(f"adapter_and_harbor={sorted(implementations)} same_task_baselines={len(baselines)}")


if __name__ == "__main__":
    main()
