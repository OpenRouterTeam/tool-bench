"""Replay result adapters over direct Rust HTTP exports, not MCP-client receipts."""

import argparse
import json
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read(source):
    def pairs(items):
        result = {}
        for name, value in items:
            if name in result:
                raise ValueError("duplicate JSON member: " + name)
            result[name] = value
        return result

    def nonfinite(value):
        raise ValueError("nonfinite JSON number: " + value)

    return json.loads(source.read_bytes(), object_pairs_hook=pairs, parse_constant=nonfinite)


def check(directory):
    contracts = directory / "result-contracts.json"
    registry = read(contracts)
    if not isinstance(registry, dict) or not isinstance(registry.get("profiles"), dict):
        raise ValueError("missing result-contract registry")
    profiles = registry.get("profiles", {}).get("7130", {})
    reviewed = {name for name, profile in profiles.items() if profile == "google_workspace_replica_v1"}
    if not reviewed:
        raise ValueError("the declared Google replica result profile is missing")
    journals = sorted(directory.glob("*/verifier/journal.json"))
    if not journals:
        raise ValueError("no direct source-control journals")
    count = 0
    with tempfile.TemporaryDirectory(prefix="tool-bench-google-result-replay.") as temporary:
        temporary = Path(temporary)
        for source in journals:
            entries = read(source).get("entries")
            if not isinstance(entries, list):
                raise ValueError("journal entries must be an array")
            seen = set()
            for entry in entries:
                if not isinstance(entry, dict):
                    raise ValueError("journal records must be objects")
                identity = entry.get("operation_id")
                if type(identity) is not int or identity <= 0 or identity in seen:
                    raise ValueError("invalid or duplicate journal operation")
                seen.add(identity)
                body = entry.get("request_body")
                if not isinstance(body, dict) or body.get("name") not in reviewed:
                    continue
                if entry.get("service") != "google" or entry.get("method") != "POST" \
                        or entry.get("path") != "/_mcp/tools/call" or entry.get("status") != 200:
                    raise ValueError("result lacks the selected replica HTTP context")
                request = {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": body}
                (temporary / "request.json").write_text(json.dumps(request))
                (temporary / "result.json").write_text(json.dumps(entry.get("response_body")))
                result = subprocess.run([
                    "jq", "-e", "-L", str(ROOT / "crates/scenario/assets"),
                    "--arg", "endpoint", "7130", "--slurpfile", "request", str(temporary / "request.json"),
                    "--slurpfile", "contracts", str(contracts),
                    "-f", str(ROOT / "crates/scenario/assets/oracle-tool-result.jq"),
                    str(temporary / "result.json"),
                ], capture_output=True, text=True, check=False)
                if result.returncode:
                    raise ValueError(f"{source}: operation {identity} adapter rejected: {result.stderr.strip()}")
                if read(temporary / "result.json") != json.loads(result.stdout):
                    raise ValueError("result adapter changed the captured payload")
                count += 1
    if count == 0:
        raise ValueError("no eligible result evidence")
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    options = parser.parse_args()
    count = check(options.directory.resolve())
    print(f"Google selected-replica result replay: {count} genuine direct HTTP results passed unchanged")
    print("Constructed request framing is not evidence of MCP-client delivery, SDK execution or model receipt.")


if __name__ == "__main__":
    main()
