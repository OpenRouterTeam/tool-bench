"""Check actual replica HTTP request bytes against locked MCP argument schemas.

This is post-hoc argument admission, not universal runtime enforcement,
operation-support certification, hosted conformance or model-receipt proof.
The delegated program checker conservatively declines possible template strings.
"""

import argparse
import base64
import hashlib
import json
import subprocess
import tempfile
from decimal import Decimal
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def decode(content):
    def pairs(items):
        result = {}
        for name, value in items:
            if name in result:
                raise ValueError("duplicate JSON member: " + name)
            result[name] = value
        return result

    def nonfinite(value):
        raise ValueError("nonfinite JSON number: " + value)

    return json.loads(content, object_pairs_hook=pairs, parse_float=Decimal, parse_constant=nonfinite)


def same(left, right):
    if type(left) in (int, Decimal):
        return type(right) in (int, Decimal) and left == right
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(same(value, right[key]) for key, value in left.items())
    if isinstance(left, list):
        return len(left) == len(right) and all(same(before, after) for before, after in zip(left, right))
    return left == right


def request_bytes(entry):
    if entry.get("service") != "google" or entry.get("method") != "POST" \
            or entry.get("path") != "/_mcp/tools/call" or entry.get("query") is not None:
        raise ValueError("unsupported Google request context")
    replay = entry.get("replay")
    body = replay.get("body") if isinstance(replay, dict) else None
    if not isinstance(body, dict) or not isinstance(body.get("data"), str):
        raise ValueError("actual request bytes are missing")
    if body.get("encoding") == "utf8":
        content = body["data"].encode("utf8")
    elif body.get("encoding") == "base64":
        content = base64.b64decode(body["data"], validate=True)
    else:
        raise ValueError("unsupported request byte encoding")
    parsed = decode(content)
    if not isinstance(parsed, dict) or set(parsed) != {"name", "arguments"} \
            or not isinstance(parsed["name"], str) or not parsed["name"] \
            or not isinstance(parsed["arguments"], dict) or not same(parsed, entry.get("request_body")):
        raise ValueError("actual request bytes and parsed journal body disagree")
    return content


def check(verifier, programcheck):
    journal = decode((Path(verifier) / "journal.json").read_bytes())
    entries = journal.get("entries") if isinstance(journal, dict) else None
    if not isinstance(entries, list):
        raise ValueError("journal entries must be an array")
    operations, requests = set(), []
    with tempfile.TemporaryDirectory(prefix="tool-bench-google-dispatched-arguments.") as temporary:
        temporary = Path(temporary)
        for entry in entries:
            if not isinstance(entry, dict):
                raise ValueError("journal entry is not an object")
            identity = entry.get("operation_id")
            if type(identity) is not int or identity <= 0 or identity in operations:
                raise ValueError("invalid or duplicate operation identity")
            operations.add(identity)
            if entry.get("service") != "google":
                continue
            content = request_bytes(entry)
            destination = temporary / f"operation-{identity}.json"
            destination.write_bytes(
                b'{"jsonrpc":"2.0","id":2,"method":"tools/call","params":' + content + b"}",
            )
            requests.append({"operation_id": identity, "tool": entry["request_body"]["name"],
                             "request_sha256": "sha256:" + hashlib.sha256(content).hexdigest()})
        if not requests:
            raise ValueError("no actual Google calls to validate; no-op trials are outside this check")
        result = subprocess.run(
            [str(programcheck), "google-workspace", str(temporary)],
            cwd=ROOT, capture_output=True, text=True, check=False,
        )
        if result.returncode:
            raise ValueError("actual dispatched arguments rejected: " + result.stderr.strip())
        admitted = decode(result.stdout)
        if not isinstance(admitted, dict):
            raise ValueError("schema checker did not return an object")
        validations = admitted.get("requests")
        if admitted.get("scope") != "locked_schema_checks_of_retained_static_request_files" \
                or type(admitted.get("validated_request_files")) is not int \
                or admitted.get("validated_request_files") != len(requests) \
                or not isinstance(validations, list) or len(validations) != len(requests):
            raise ValueError("schema checker did not cover every dispatched request")
        expected = {f"operation-{row['operation_id']}.json": row for row in requests}
        snapshot = "sha256:" + hashlib.sha256(
            (ROOT / "servers/snapshots/google-workspace.json").read_bytes(),
        ).hexdigest()
        seen = set()
        for row in validations:
            if not isinstance(row, dict) or not isinstance(row.get("path"), str) \
                    or not isinstance(row.get("validation"), dict):
                raise ValueError("invalid schema-checker request row")
            name = Path(row.get("path", "")).name
            validation = row.get("validation", {})
            if name not in expected or name in seen or Path(row["path"]).resolve() != (temporary / name).resolve() \
                    or validation.get("endpoint") != "google-workspace" \
                    or validation.get("tool") != expected[name]["tool"] \
                    or validation.get("arguments") != "static_validated" \
                    or validation.get("deferred_paths") != [] \
                    or validation.get("snapshot_path") != "servers/snapshots/google-workspace.json" \
                    or validation.get("snapshot_sha256") != snapshot:
                raise ValueError("schema-checker request or locked-definition binding mismatch")
            seen.add(name)
    return {"scope": "post_hoc_actual_replica_http_argument_schema_admission",
            "verifier": str(Path(verifier).resolve()), "snapshot_sha256": snapshot,
            "validated_operations": len(requests), "requests": requests,
            "runtime_schema_enforcement_claimed": False, "model_receipt_claimed": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--programcheck", required=True, type=Path)
    parser.add_argument("verifier", type=Path)
    options = parser.parse_args()
    print(json.dumps(check(options.verifier, options.programcheck.resolve()), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
