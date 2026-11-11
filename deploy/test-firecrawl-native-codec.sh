#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the locked tools image}"
OUT="${CONTROL_OUT:-$(mktemp -d /tmp/tool-bench-firecrawl-native-codec.XXXXXXXX)}"
test -d "$OUT"
test -z "$(find "$OUT" -mindepth 1 -maxdepth 1 -print -quit)"
OUT="$(cd "$OUT" && pwd)"
NAME="tool-bench-firecrawl-codec-$(basename "$OUT" | tr -cd 'a-zA-Z0-9-')"
printf 'artifacts: %s\n' "$OUT"
docker image inspect "$TOOLS_IMAGE" > "$OUT/tools-image.json"
cp deploy/fixtures/firecrawl-native-codec.mjs "$OUT/controller.mjs"
cp "$ROOT/deploy/test-firecrawl-native-codec.sh" "$OUT/driver.sh"
cp servers/snapshots/firecrawl.json "$OUT/locked-tools.json"
mkdir "$OUT/guards"
cp crates/scenario/assets/*.jq "$OUT/guards/"
cp tasks/search__firecrawl__crawl-api-reference/solution/result-contracts.json "$OUT/contracts.json"
jq -e '.profiles["7102"].firecrawl_crawl=="firecrawl_native_v1"' "$OUT/contracts.json" >/dev/null
CONTAINER="$(docker create --name "$NAME" \
  --label "tool-bench.control=$OUT" --network none --read-only --tmpfs /tmp \
  --cap-drop ALL --security-opt no-new-privileges \
  --mount "type=bind,source=$OUT/controller.mjs,target=/fixture/controller.mjs,readonly" \
  --entrypoint node "$TOOLS_IMAGE" /fixture/controller.mjs)"
printf '%s\n' "$CONTAINER" > "$OUT/container-id"
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true' EXIT
docker container inspect "$CONTAINER" > "$OUT/container-created.json"
jq -e 'length==1 and .[0].HostConfig.NetworkMode=="none"
  and .[0].HostConfig.ReadonlyRootfs==true' "$OUT/container-created.json" >/dev/null
docker start --attach "$CONTAINER" > "$OUT/results.json" 2> "$OUT/controller.log"
docker container inspect "$CONTAINER" > "$OUT/container-stopped.json"
jq -e 'length==1 and .[0].State.Running==false and .[0].State.ExitCode==0' \
  "$OUT/container-stopped.json" >/dev/null
PYTHONDONTWRITEBYTECODE=1 python3 - "$OUT" <<'PY'
import json
import subprocess
import sys
from pathlib import Path

output = Path(sys.argv[1])
payload = json.loads((output / "results.json").read_bytes())
assert payload["package"]["version"] == "3.36.0"
snapshot = json.loads((output / "locked-tools.json").read_bytes())
assert sorted(payload["definitions"]["result"]["tools"], key=lambda tool: tool["name"]) == sorted(
    snapshot["tools"], key=lambda tool: tool["name"]
)
assert len(payload["results"]) == 21
requests = output / "requests"
requests.mkdir()
results = []
for control in payload["results"]:
    assert any(request["case"] == control["name"] for request in payload["requests"])
    response = control["response"]
    request = requests / (control["name"] + ".json")
    request.write_text(json.dumps({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": control["tool"], "arguments": control["arguments"]},
    }, indent=2) + "\n")
    decoded = subprocess.run(
        ["jq", "-Rse", "--argjson", "expected_id", json.dumps(response["id"]),
         "-f", str(output / "guards/oracle-response.jq")],
        input=json.dumps(response), text=True, capture_output=True,
    )
    checked = None
    if decoded.returncode == 0:
        checked = subprocess.run(
            ["jq", "-e", "-L", str(output / "guards"),
             "--arg", "endpoint", "7102", "--slurpfile", "request", str(request),
             "--slurpfile", "contracts", str(output / "contracts.json"),
             "-f", str(output / "guards/oracle-tool-result.jq")],
            input=decoded.stdout, text=True, capture_output=True,
        )
    accepted = decoded.returncode == 0 and checked.returncode == 0
    results.append({
        "name": control["name"], "expected_accepted": control["accepted"],
        "actual_accepted": accepted, "mcp_guard_exit": decoded.returncode,
        "native_guard_exit": checked.returncode if checked else None,
        "stderr": decoded.stderr + (checked.stderr if checked else ""),
    })
(output / "guard-comparison.json").write_text(json.dumps(results, indent=2) + "\n")
assert all(row["expected_accepted"] == row["actual_accepted"] for row in results), results
print(f'{len(results)} native codec controls agree; {len(snapshot["tools"])} locked definitions match')
print("Loopback codec validation is not world, hosted, model or full-release qualification.")
PY
if [ -n "${PROGRAMCHECK_BINARY:-}" ]; then
  "$PROGRAMCHECK_BINARY" firecrawl "$OUT/requests" > "$OUT/argument-schemas.json"
else
  cargo run -q --locked -p scenario --bin programcheck -- firecrawl "$OUT/requests" \
    > "$OUT/argument-schemas.json"
fi
jq -e '.validated_request_files==21' "$OUT/argument-schemas.json" >/dev/null
docker rm "$CONTAINER" > "$OUT/container-removed.txt"
docker ps -aq --no-trunc --filter "id=$CONTAINER" > "$OUT/owned-survivors.txt"
test ! -s "$OUT/owned-survivors.txt"
PYTHONDONTWRITEBYTECODE=1 python3 - "$OUT" <<'PY'
import hashlib
import sys
from pathlib import Path

output = Path(sys.argv[1])
files = sorted(candidate for candidate in output.rglob("*") if candidate.is_file())
(output / "SHA256SUMS").write_text("".join(
    hashlib.sha256(candidate.read_bytes()).hexdigest()
    + "  " + candidate.relative_to(output).as_posix() + "\n"
    for candidate in files
))
for row in (output / "SHA256SUMS").read_text().splitlines():
    expected, name = row.split("  ", 1)
    assert hashlib.sha256((output / name).read_bytes()).hexdigest() == expected, name
print(f"{len(files)} codec payload checksums independently re-read; zero owned survivors")
PY
printf '%s\n' 'native result guards and retained static argument schemas passed'
