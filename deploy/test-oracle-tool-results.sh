#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-oracle-tool-results.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --include-examples --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for task in cloud__aws__grant-auditor-access example__aws__missing-bucket-policy; do
  mkdir "$OUT/$task-original"
  cp "$OUT/tasks/$task/solution/"* "$OUT/$task-original/"
done
for mode in valid client_error_after_effect service_error_after_effect mixed_batch_error \
    empty_batch allowed_notfound undeclared_notfound wrong_notfound_code; do
  case "$mode" in
    allowed_notfound|undeclared_notfound|wrong_notfound_code) task=example__aws__missing-bucket-policy ;;
    *) task=cloud__aws__grant-auditor-access ;;
  esac
  TASK="$OUT/tasks/$task"
  ORIGINAL="$OUT/$task-original"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  case "$mode" in
    client_error_after_effect|service_error_after_effect|mixed_batch_error|empty_batch)
      jq -n '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"call_aws",arguments:{
        cli_command:[
          "aws iam list-attached-user-policies --user-name priya.raman-audit",
          "aws iam get-group --group-name auditors"
        ]}}}' > "$TASK/solution/call-proof.json"
      printf '%s\n' 'call 7116 "$DIR/call-proof.json"' >> "$TASK/solution/solve.sh"
      case "$mode" in
        client_error_after_effect) commands='"aws cloudwatch put-metric-alarm"' ;;
        service_error_after_effect) commands='"aws cloudwatch put-metric-alarm --alarm-name missing-configuration"' ;;
        mixed_batch_error) commands='["aws iam list-users","aws cloudwatch put-metric-alarm --alarm-name missing-configuration"]' ;;
        empty_batch) commands='[]' ;;
      esac
      jq -n --argjson commands "$commands" '{jsonrpc:"2.0",id:2,method:"tools/call",
        params:{name:"call_aws",arguments:{cli_command:$commands}}}' > "$TASK/solution/call-extra.json"
      printf '%s\n' 'call 7116 "$DIR/call-extra.json"' \
        'printf "%s\n" ORACLE_RESULT_GUARD_REACHED_LATER_CALL' \
        'call 7116 "$DIR/call-proof.json"' >> "$TASK/solution/solve.sh"
      ;;
    undeclared_notfound)
      jq '.allowed_aws_api_errors=[]' "$ORIGINAL/result-contracts.json" \
        > "$TASK/solution/result-contracts.json" ;;
    wrong_notfound_code)
      jq '.params.arguments.cli_command="aws s3api get-bucket-policy --bucket nonexistent-bucket"' \
        "$ORIGINAL/call-0.json" > "$TASK/solution/call-0.json" ;;
  esac
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
  case "$mode" in
    valid|empty_batch|allowed_notfound)
      test "$run_status" = 0
      test -n "$reward"
      jq -e '.exception_info==null' "$result" >/dev/null
      jq -e '.reward==1 and .environment_ok==1 and .contract_ok==1 and .integrity_ok==1
        and .runner_admission_checked==1 and .runner_admission_ok==1' "$reward" >/dev/null
      jq -e '.phase=="stopped" and .failure==null' "$receipt" >/dev/null
      if [ "$mode" = empty_batch ]; then
        grep -q ORACLE_RESULT_GUARD_REACHED_LATER_CALL "$log"
      fi
      ;;
    *)
      test "$run_status" != 0
      test -z "$reward"
      jq -e '.exception_info!=null' "$result" >/dev/null
      jq -e '.phase=="failed" and .failure!=null and .evidence_sha256==null
        and .main_artifacts_sha256==null' "$receipt" >/dev/null
      if find "$OUT/$mode" -path '*/artifacts/world/evidence.json' -print -quit | grep -q .; then
        printf '%s\n' 'rejected wrapped command produced authoritative evidence' >&2
        exit 1
      fi
      if grep -q ORACLE_RESULT_GUARD_REACHED_LATER_CALL "$log"; then
        printf '%s\n' 'a rejected wrapped result authorized a later MCP call' >&2
        exit 1
      fi
      jq -Rse '
        [split("\n")[] | select(startswith("data: ")) | .[6:] | fromjson]
        | any(.[]; .result.isError==false
          and any(.result.structuredContent.result[]?;
            .error!=null or (.response.status_code // 0)>=400))
      ' "$log" >/dev/null
      ;;
  esac
  case "$mode" in
    client_error_after_effect|service_error_after_effect|mixed_batch_error|empty_batch)
      jq -Rse '
        [split("\n")[] | select(startswith("data: ")) | .[6:] | fromjson
          | .result.structuredContent.result[]?] as $records
        | any($records[];
            .cli_command=="aws iam list-attached-user-policies --user-name priya.raman-audit"
            and .response.status_code==200 and (.response.as_json|fromjson
              | any(.AttachedPolicies[]; .PolicyArn=="arn:aws:iam::aws:policy/SecurityAudit")))
          and any($records[];
            .cli_command=="aws iam get-group --group-name auditors"
            and .response.status_code==200 and (.response.as_json|fromjson
              | any(.Users[]; .UserName=="priya.raman-audit")))
      ' "$log" >/dev/null
      ;;
  esac
  printf '%s\n' "$mode: actual MCP/SDK results control program completion and scoring admission"
done
