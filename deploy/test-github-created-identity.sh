#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current GitHub world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-github-identity.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/code__github__file-login-bug"
CAPTURING="$OUT/tasks/business__multi__zendesk-refund-via-stripe/solution"
cp "$CAPTURING/"*.jq "$TASK/solution/"
sed '/^bound_call [0-9]/,$d' "$CAPTURING/solve.sh" > "$TASK/solution/solve.sh"
jq '.params.arguments.body = "" | .params.arguments.labels = ["bug","auth"]' "$TASK/solution/call-0.json" > "$OUT/create.json"
mv "$OUT/create.json" "$TASK/solution/call-0.json"
printf '%s\n' '[{"name":"created_issue_url","pointer":"/content/0/text","json_pointer":"/url"}]' \
  > "$TASK/solution/call-0.capture.json"
jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{
  name:"issue_write",arguments:{method:"update",owner:"octo-org",repo:"app",
  issue_number:"${created_issue_number}",body:"Observed after deploy 412."}
}}' > "$TASK/solution/call-1.json"
printf '%s\n' '[]' > "$TASK/solution/call-1.capture.json"
cat >> "$TASK/solution/solve.sh" <<'EOF'
bound_call 7101 "$DIR/call-0.json" "$DIR/call-0.capture.json" 0
jq '.created_issue_number = (.created_issue_url | capture("/issues/(?<number>[0-9]+)$").number | tonumber)' \
  "$WORK/bindings.json" > "$WORK/next-bindings.json"
mv "$WORK/next-bindings.json" "$WORK/bindings.json"
bound_call 7101 "$DIR/call-1.json" "$DIR/call-1.capture.json" 1
EOF
sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/trial" > "$OUT/trial.log" 2>&1
result="$(find "$OUT/trial" -path '*/verifier/reward.json' -print -quit)"
test -n "$result"
jq -e '.reward == 1 and .outcome_code == 0 and .adjudication_needed == 0
  and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
  and .runner_admission_checked == 1 and .runner_admission_ok == 1
  and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
jq -e '
  [.entries[] | select(.service == "github" and .method == "POST"
    and .path == "/api/v3/repos/octo-org/app/issues")] as $created
  | [.entries[] | select(.service == "github" and .method == "PATCH")] as $updated
  | ($created | length) == 1 and ($updated | length) == 1
    and $updated[0].path == ("/api/v3/repos/octo-org/app/issues/" + ($created[0].response_body.number | tostring))
    and $updated[0].request_body.body == "Observed after deploy 412."
' "$(dirname "$result")/journal.json" >/dev/null
printf '%s\n' 'GitHub creation URL supplies a typed actual issue number to the subsequent MCP update; staged completion with existing classification labels passes'
