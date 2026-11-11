import argparse
import json
import subprocess
from pathlib import Path


def read(path):
    return json.loads(path.read_bytes())


def comparison(report, candidate, valid):
    return subprocess.run(
        [str(report), str(candidate), str(valid), "--json"],
        capture_output=True, text=True,
    )

def native_error(message, contract, tool=None):
    protocol = message.get("error")
    if "result" not in message and isinstance(protocol, dict) \
            and isinstance(protocol.get("code"), int) and not isinstance(protocol["code"], bool) \
            and isinstance(protocol.get("message"), str) and protocol["message"]:
        return True
    result = message.get("result", {})
    if result.get("isError") is True:
        return True
    if contract not in {"okta-api-v1", "grafana-api-v1", "pagerduty-api-v1"}:
        return False
    if contract == "pagerduty-api-v1" and tool == "create_schedule_override":
        blocks = result.get("content", [])
        if not isinstance(blocks, list) or not blocks \
                or any(block.get("type") != "text" for block in blocks):
            return False
        try:
            decoded = [json.loads(block["text"]) for block in blocks]
        except (KeyError, TypeError, json.JSONDecodeError):
            return False
        values = decoded[0] if len(decoded) == 1 and isinstance(decoded[0], list) else decoded
        if "structuredContent" in result and result["structuredContent"] != {"result": values}:
            return False
        return any(
            isinstance(value, dict) and set(value) <= {"status", "errors", "override"}
            and type(value.get("status")) is int and 400 <= value["status"] < 600
            and isinstance(value.get("errors"), list) and value["errors"]
            and all(isinstance(error, str) and error for error in value["errors"])
            for value in values
        )
    for block in result.get("content", []):
        if block.get("type") != "text":
            continue
        try:
            payload = json.loads(block["text"])
        except (KeyError, TypeError, json.JSONDecodeError):
            continue
        if contract == "pagerduty-api-v1":
            if tool == "manage_incidents" and isinstance(payload, dict) \
                    and set(payload) <= {"response", "response_summary"} \
                    and len(result.get("content", [])) == 1 \
                    and payload.get("response") == [] \
                    and ("structuredContent" not in result or result["structuredContent"] == payload):
                return True
            continue
        if contract == "grafana-api-v1":
            if isinstance(payload, dict) and set(payload) <= {"status", "headers", "data"} \
                    and "data" in payload and type(payload.get("status")) is int \
                    and 400 <= payload["status"] < 600 \
                    and (payload.get("headers") is None or isinstance(payload["headers"], dict)):
                return True
            continue
        values = payload if isinstance(payload, list) else [payload]
        for value in values:
            if not isinstance(value, dict):
                continue
            if set(value) in ({"error"}, {"exception"}):
                description = next(iter(value.values()))
                if isinstance(description, str) and description:
                    return True
            if set(value) == {"ok", "error", "status_code", "raw"} \
                    and value["ok"] is False and isinstance(value["error"], dict) \
                    and isinstance(value["error"].get("message"), str) and value["error"]["message"]:
                return True
    return False


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--evaluator", type=int, required=True)
    parser.add_argument("--valid", type=Path, required=True)
    parser.add_argument("--invalid", type=Path, action="append", default=[])
    parser.add_argument("--failed", type=Path, action="append", default=[])
    parser.add_argument("--tool-error", type=Path, action="append", default=[])
    parser.add_argument("--tool-marker", default="")
    parser.add_argument(
        "--native-error-contract", choices=["mcp-is-error", "okta-api-v1", "grafana-api-v1", "pagerduty-api-v1"],
        default="mcp-is-error",
    )
    parser.add_argument("--tampered", type=Path, required=True)
    options = parser.parse_args()
    for directory in options.invalid:
        assessments = [
            path for path in directory.rglob("assessment.json")
            if path.parent.name == "verifier"
        ]
        assert len(assessments) == 1, directory
        assessment = read(assessments[0])
        assert assessment["evaluator_version"] == options.evaluator, directory
        assert assessment["outcome"] == "environment_error", directory
        assert assessment["contract_ok"] and assessment["integrity_ok"], directory
        assert not assessment["environment_ok"], directory
        assert any(failure["check"] == "environment" for failure in assessment["failures"]), directory
        result = comparison(options.report, directory, options.valid)
        assert result.returncode != 0 and not result.stdout.strip(), (directory, result.stderr)
        assert "evaluator version" not in result.stderr.lower(), (directory, result.stderr)
        print(f"{directory}: invalid source/admission case excluded from comparison")
    for directory in options.failed + options.tool_error:
        results = [
            path for path in directory.rglob("result.json")
            if "trial_name" in read(path)
        ]
        assert len(results) == 1, (directory, results)
        trial = results[0].parent
        result = read(results[0])
        receipt = read(trial / "runner-admission.json")
        assert result["exception_info"] is not None and result["verifier_result"] is None, directory
        assert receipt["phase"] == "failed" and receipt["failure"], directory
        assert receipt["evidence_sha256"] is None and receipt["main_artifacts_sha256"] is None, directory
        assert not (trial / "artifacts/world/evidence.json").exists(), directory
        assert not list(directory.rglob("reward.json")), directory
        assert not list(directory.rglob("assessment.json")), directory
        if directory in options.tool_error:
            assert options.tool_marker, "tool-error controls require an explicit program marker"
            output = (trial / "agent/oracle.txt").read_text()
            assert f"{options.tool_marker}_BEFORE_EXPECTED_ERROR" in output, directory
            assert f"{options.tool_marker}_REACHED_LATER_CALL" not in output, directory
            messages = []
            failure_output = output.split(f"{options.tool_marker}_BEFORE_EXPECTED_ERROR", 1)[1]
            for line in failure_output.splitlines():
                try:
                    decoded = json.loads(line.removeprefix("data: "))
                except json.JSONDecodeError:
                    continue
                if isinstance(decoded, dict) and decoded.get("jsonrpc") == "2.0" and decoded.get("id") == 2:
                    messages.append(decoded)
            tool = None
            if options.native_error_contract == "pagerduty-api-v1":
                expected = read(directory.parent / ("program-" + directory.name) / "expected-error.json")
                assert expected["jsonrpc"] == "2.0" and expected["id"] == 2 \
                    and expected["method"] == "tools/call", directory
                tool = expected["params"]["name"]
                assert isinstance(expected["params"]["arguments"], dict), directory
            assert any(native_error(message, options.native_error_contract, tool) for message in messages), directory
        print(f"{directory}: failed program has no authoritative evidence or numeric assessment")
    tampered = comparison(options.report, options.tampered, options.valid)
    assert tampered.returncode != 0 and not tampered.stdout.strip(), tampered.stderr
    assert "byte mismatch" in tampered.stderr and "realized-manifest.json" in tampered.stderr, tampered.stderr
    print(f"invalid_cases={len(options.invalid)} unscored_failed_programs={len(options.failed) + len(options.tool_error)}")
    print("Tampered manifest bytes are refused independently.")


if __name__ == "__main__":
    main()
