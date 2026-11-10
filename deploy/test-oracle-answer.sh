#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORK="$(mktemp -d /tmp/tool-bench-oracle-answer.XXXXXX)"
cleanup() {
  result=$?
  if [ "$result" = 0 ]; then
    rm -rf "$WORK"
  else
    printf 'failure artifacts: %s\n' "$WORK" >&2
  fi
}
trap cleanup EXIT
SCENARIO="$WORK/scenarios/fixture/sentry/captured-answer"
mkdir -p "$SCENARIO" "$WORK/bin"
printf '%s\n' 'Retrieve the checkout-api project DSN in lumen-labs and write it to /app/answer.md.' \
  > "$SCENARIO/instruction.md"
cp -R scenarios/ops/sentry/create-notification-project/dataset "$SCENARIO/"
cat > "$SCENARIO/scenario.toml" <<'TOML'
[scenario]
id = "fixture/sentry/captured-answer"
title = "Private generated-answer transport fixture"
now = "2026-10-20T16:00:00Z"
seed = "oracle-answer-transport"
services = ["sentry"]
tags = ["lookup", "sentry"]

[expect]
version = 2

[expect.answer]
regex = "^https://"

[[solution.call]]
endpoint = "sentry"
tool = "execute_sentry_tool"
arguments = { name = "find_dsns", arguments = { organizationSlug = "lumen-labs", projectSlug = "checkout-api" } }

[[solution.call.capture]]
name = "dsn"
pointer = "/structuredContent/dsns"
select = { matching = { name = "Default" }, pointer = "/dsn" }

[solution]
answer = "${dsn}"
answer_template = true
TOML
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --scenarios "$WORK/scenarios" \
  --out "$WORK/tasks" --world-image fixture-world --tools-image fixture-tools >/dev/null
SOLUTION="$WORK/tasks/fixture__sentry__captured-answer/solution"
cp "$SOLUTION/solve.sh" "$WORK/generated.sh"
cp "$SOLUTION/answer.template.json" "$WORK/original-template.json"
sed "s|/app/answer.md|$WORK/answer.md|g" "$WORK/generated.sh" > "$SOLUTION/solve.sh"
cat > "$WORK/bin/curl" <<'CURL'
#!/bin/sh
set -eu
inline=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -d) inline="$2"; shift 2 ;;
    --data-binary) shift 2 ;;
    *) shift ;;
  esac
done
if [ -n "$inline" ]; then
  if [ "$(printf '%s' "$inline" | jq -r '.method')" = initialize ]; then
    printf 'HTTP/1.1 200 OK\r\nmcp-session-id: answer-fixture\r\n\r\n'
  fi
  exit 0
fi
printf '%s\n' call >> "$ORACLE_ANSWER_COUNTER"
cat "$ORACLE_ANSWER_RESULT"
CURL
chmod +x "$WORK/bin/curl"
for mode in valid untrusted_data tool_error missing_capture ambiguous_capture unbound_template invalid_template; do
  cp "$WORK/original-template.json" "$SOLUTION/answer.template.json"
  printf '%s' 'https://actual_returned@ingest.example.net/73023' > "$WORK/expected.md"
  if [ "$mode" = untrusted_data ]; then
    printf '%s' '${private}; $(touch should-not-exist) "quoted" 日本' > "$WORK/expected.md"
  fi
  jq -n --rawfile dsn "$WORK/expected.md" \
    '{dsns:[{id:"actual_returned",name:"Default",dsn:$dsn}]} as $payload
    | {jsonrpc:"2.0",id:2,result:{structuredContent:$payload,
        content:[{type:"text",text:($payload|tojson)}]}}' > "$WORK/result.json"
  case "$mode" in
    tool_error) jq '.result.isError=true' "$WORK/result.json" > "$WORK/changed.json"; mv "$WORK/changed.json" "$WORK/result.json" ;;
    missing_capture) jq '.result.structuredContent.dsns=[]' "$WORK/result.json" > "$WORK/changed.json"; mv "$WORK/changed.json" "$WORK/result.json" ;;
    ambiguous_capture)
      jq '.result.structuredContent.dsns += .result.structuredContent.dsns' \
        "$WORK/result.json" > "$WORK/changed.json"
      mv "$WORK/changed.json" "$WORK/result.json" ;;
    unbound_template)
      jq '.params.arguments="${not_captured}"' "$WORK/original-template.json" > "$SOLUTION/answer.template.json" ;;
    invalid_template)
      jq '.params.arguments="${invalid.name}"' "$WORK/original-template.json" > "$SOLUTION/answer.template.json" ;;
  esac
  rm -f "$WORK/answer.md" "$WORK/calls"
  if (cd "$WORK" && ORACLE_ANSWER_COUNTER="$WORK/calls" ORACLE_ANSWER_RESULT="$WORK/result.json" \
      PATH="$WORK/bin:$PATH" sh "$SOLUTION/solve.sh") > "$WORK/$mode.log" 2>&1; then
    run_status=0
  else
    run_status=$?
  fi
  test "$(wc -l < "$WORK/calls" | tr -d ' ')" = 1
  case "$mode" in
    valid|untrusted_data)
      test "$run_status" = 0
      cmp "$WORK/expected.md" "$WORK/answer.md"
      test ! -e "$WORK/should-not-exist" ;;
    *)
      test "$run_status" != 0
      test ! -e "$WORK/answer.md" ;;
  esac
  printf '%s\n' "$mode: generated reference answer bytes and failure guard checked"
done
