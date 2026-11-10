#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the reviewed Supabase world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the pinned tools image}"
OUT="$(mktemp -d /tmp/tool-bench-supabase-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/cloud__supabase__enable-rls-flagged-table"
mkdir "$OUT/original"
cp "$TASK/solution/"* "$OUT/original/"
if [ "$#" -eq 0 ]; then set -- transaction regclass paid_pause syntax unavailable_extension; fi
for mode do
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/original/"* "$TASK/solution/"
  tool=execute_sql
  query='select 1'
  case "$mode" in
    transaction) tool=apply_migration; query='begin; alter table public.invoices enable row level security; rollback' ;;
    regclass) query="select relrowsecurity from pg_class where oid = 'public.invoices'::regclass" ;;
    paid_pause) tool=pause_project ;;
    syntax) query='selec 1' ;;
    unavailable_extension) query='create extension imaginary_test_extension' ;;
    *) printf 'unsupported Supabase error mode: %s\n' "$mode" >&2; exit 2 ;;
  esac
  arguments="$(jq -cn --arg query "$query" '{project_id:"qmvbxtzkplrnwdhyfaco",query:$query}')"
  case "$tool" in
    apply_migration) arguments="$(printf '%s' "$arguments" | jq -c '.name="failed_control"')" ;;
    pause_project) arguments='{"project_id":"qmvbxtzkplrnwdhyfaco"}' ;;
  esac
  jq -n --arg tool "$tool" --argjson arguments "$arguments" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/call-error.json"
  printf '%s\n' 'printf "%s\n" ORACLE_SUPABASE_BEFORE_ERROR' \
    'call 7118 "$DIR/call-error.json"' \
    'printf "%s\n" ORACLE_SUPABASE_REACHED_LATER_CALL' \
    'call 7118 "$DIR/call-error.json"' >> "$TASK/solution/solve.sh"
  cp -R "$TASK/solution" "$OUT/program-$mode"
  if sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$mode" > "$OUT/$mode.log" 2>&1; then
    run_status=0
  else
    run_status=$?
  fi
  result="$(find "$OUT/$mode" -name result.json -print -quit)"
  receipt="$(find "$OUT/$mode" -name runner-admission.json -print -quit)"
  reward="$(find "$OUT/$mode" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  test -n "$receipt"
  test "$run_status" != 0
  test -z "$reward"
  jq -e '.exception_info!=null and .verifier_result==null' "$result" >/dev/null
  jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
    and .main_artifacts_sha256==null' "$receipt" >/dev/null
  log="$(dirname "$result")/agent/oracle.txt"
  grep -q ORACLE_SUPABASE_BEFORE_ERROR "$log"
  if grep -q ORACLE_SUPABASE_REACHED_LATER_CALL "$log"; then
    printf '%s\n' "$mode: a failed native result authorized a later MCP call" >&2
    exit 1
  fi
  if find "$OUT/$mode" -path '*/artifacts/world/evidence.json' -print -quit | grep -q .; then
    printf '%s\n' "$mode: a failed program produced authoritative world evidence" >&2
    exit 1
  fi
  jq -Rse '[split("\n")[]|if startswith("data: ") then .[6:] else . end
      | fromjson? | select(type=="object" and .jsonrpc=="2.0" and .id==2)]
    | any(.[];.result.isError==true)
      and any(.[];.result!=null and .result.isError!=true)' "$log" >/dev/null
  printf '%s\n' "$mode: native tool error stops execution before later calls or numeric assessment"
done
