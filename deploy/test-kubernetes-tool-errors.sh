#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the metadata-capable world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-kubernetes-errors.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
TASK="$OUT/tasks/ops__kubernetes__scale-checkout-for-sale"
mkdir "$OUT/original"
cp "$TASK/solution/"* "$OUT/original/"
for mode in valid_zero wrong_uid stale_version; do
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$OUT/original/"* "$TASK/solution/"
  resource='{"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"checkout","namespace":"shop-prod","resourceVersion":"0"},"spec":{"replicas":8}}'
  case "$mode" in
    wrong_uid) resource="$(printf '%s' "$resource" | jq -c '.metadata.uid="unrelated-owner"')" ;;
    stale_version) resource="$(printf '%s' "$resource" | jq -c '.metadata.resourceVersion="1"')" ;;
  esac
  jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"resources_get",
    arguments:{apiVersion:"apps/v1",kind:"Deployment",name:"checkout",namespace:"shop-prod"}}}' \
    > "$TASK/solution/call-proof.json"
  jq -n --arg resource "$resource" '{jsonrpc:"2.0",id:2,method:"tools/call",
    params:{name:"resources_create_or_update",arguments:{resource:$resource}}}' \
    > "$TASK/solution/call-extra.json"
  printf '%s\n' 'call 7115 "$DIR/call-proof.json"' 'call 7115 "$DIR/call-extra.json"' \
    'printf "%s\n" ORACLE_K8S_ERROR_REACHED_LATER_CALL' 'call 7115 "$DIR/call-proof.json"' \
    >> "$TASK/solution/solve.sh"
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
  log="$(dirname "$result")/agent/oracle.txt"
  jq -Rse '[split("\n")[]|select(startswith("data: "))|.[6:]|fromjson]
    | any(.[]; any(.result.content[]?; .type=="text"
      and (.text|test("(?m)^\\s*replicas: 8\\s*$"))))' "$log" >/dev/null
  case "$mode" in
    valid_zero)
      test "$run_status" = 0
      test -n "$reward"
      jq -e '.reward==1 and .evaluator_version==27 and .environment_ok==1 and .contract_ok==1
        and .integrity_ok==1 and .runner_admission_checked==1 and .runner_admission_ok==1' "$reward" >/dev/null
      jq -e '.phase=="stopped" and .failure==null' "$receipt" >/dev/null
      grep -q ORACLE_K8S_ERROR_REACHED_LATER_CALL "$log"
      jq -e '[.entries[]|select(.service=="kubernetes" and .method=="PATCH")]
        | length>0 and all(.[];.status==200)' "$(dirname "$reward")/journal.json" >/dev/null
      jq -e 'any(.entries[]; .service=="kubernetes" and .method=="PATCH"
        and (.request_body|type)=="object" and .request_body.metadata.resourceVersion=="0")' \
        "$(dirname "$reward")/journal.json" >/dev/null
      ;;
    *)
      test "$run_status" != 0
      test -z "$reward"
      jq -e '.exception_info!=null' "$result" >/dev/null
      jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
        and .main_artifacts_sha256==null' "$receipt" >/dev/null
      if find "$OUT/$mode" -path '*/artifacts/world/evidence.json' -print -quit | grep -q .; then
        printf '%s\n' "$mode: rejected reference produced authoritative world evidence" >&2
        exit 1
      fi
      if grep -q ORACLE_K8S_ERROR_REACHED_LATER_CALL "$log"; then
        printf '%s\n' "$mode: rejected result authorized a later MCP call" >&2
        exit 1
      fi
      jq -Rse '[split("\n")[]|select(startswith("data: "))|.[6:]|fromjson]
        | any(.[];.result.isError==true)' "$log" >/dev/null
      ;;
  esac
  printf '%s\n' "$mode: native Kubernetes result and host-observed reference completion checks passed"
done
