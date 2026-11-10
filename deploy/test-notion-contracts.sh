#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current Notion world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-notion.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in delay-web-launch log-customer-escalation update-current-runbook; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/collab__notion__$slug/solution/"* "$OUT/$slug-original/"
done
CAPTURING="$OUT/tasks/business__multi__zendesk-refund-via-stripe/solution"
make_call() {
  jq -n --arg name "$2" --argjson arguments "$3" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$name,arguments:$arguments}}' \
    > "$TASK/solution/call-$1.json"
}
if [ "$#" -eq 0 ]; then
  set -- creation_explicit_parent creation_split creation_staged launch_by_id launch_staged \
    runbook_markdown runbook_split runbook_rest_shape runbook_reference creation_wrong_owner creation_extra_owner \
    creation_wrong_day creation_duplicate launch_collateral launch_wrong_row runbook_deleted_copy \
    runbook_other_content runbook_old_restored creation_customer_fragment creation_summary_contradiction \
    runbook_contradiction runbook_withdrawn runbook_missing_role runbook_restored
fi
for variant do
  reward=0
  outcome=1
  adjudication=0
  last=2
  case "$variant" in
    creation_*) slug=log-customer-escalation ;;
    launch_*) slug=delay-web-launch; last=1 ;;
    runbook_*) slug=update-current-runbook ;;
  esac
  ORIGINAL="$OUT/$slug-original"
  TASK="$OUT/tasks/collab__notion__$slug"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  cp "$CAPTURING/"*.jq "$TASK/solution/"
  sed '/^bound_call [0-9]/,$d' "$CAPTURING/solve.sh" > "$TASK/solution/solve.sh"
  for index in 0 1 2 3 4; do printf '%s\n' '[]' > "$TASK/solution/call-$index.capture.json"; done
  case "$variant" in
    creation_explicit_parent)
      reward=1; outcome=0; last=3
      jq '.params.arguments.parent = ({type:"data_source_id",data_source_id:"f937b556-2452-4808-a340-f4c80ce63d68"} | tojson)' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      printf '%s\n' '[{"name":"actual_page","pointer":"/content/0/text","json_pointer":"/id"}]' > "$TASK/solution/call-2.capture.json"
      make_call 3 API-retrieve-a-page '{"page_id":"${actual_page}"}'
      ;;
    creation_split)
      reward=1; outcome=0
      jq '.params.arguments.properties.Summary.title = [{text:{content:"Checkout button "}},{text:{content:"unresponsive on Safari"}}]
        | .params.arguments.properties.Customer.rich_text = [{text:{content:"Acme "},annotations:{bold:true}},{text:{content:"Retail"}}]' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    creation_staged)
      reward=1; outcome=0; last=3
      jq '.params.arguments.properties |= {Summary:.Summary}' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      printf '%s\n' '[{"name":"actual_page","pointer":"/content/0/text","json_pointer":"/id"}]' > "$TASK/solution/call-2.capture.json"
      jq '.params.name = "API-patch-page" | .params.arguments = {page_id:"${actual_page}",properties:.params.arguments.properties}' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-3.json"
      ;;
    launch_by_id)
      reward=1; outcome=0
      jq '.params.arguments.properties = {"St%3Ax":{status:{id:"d280"}},"Ld%3Dq":{date:{start:"2026-11-10"}}}' \
        "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    launch_staged)
      reward=1; outcome=0; last=2
      jq 'del(.params.arguments.properties["Launch date"])' "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      jq 'del(.params.arguments.properties.Status)' "$ORIGINAL/call-1.json" > "$TASK/solution/call-2.json"
      ;;
    runbook_markdown)
      reward=1; outcome=0
      make_call 2 API-update-page-markdown '{"page_id":"7c41530e-e886-4046-9cb4-2f49e6957f99","type":"update_content","update_content":{"content_updates":[{"old_str":"Marcus Webb, +1 555-0100","new_str":"Sam Ortiz, +1 555-0142"}]}}'
      ;;
    runbook_split)
      reward=1; outcome=0
      jq '.params.arguments.type.paragraph.rich_text = [{type:"text",text:{content:"Escalation contact: "}},{type:"text",text:{content:"Sam Ortiz"},annotations:{bold:true}},{type:"text",text:{content:", +1 555-0142 (Head of Infrastructure)"}}]' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    runbook_rest_shape)
      reward=1; outcome=0
      jq '.params.arguments.paragraph = .params.arguments.type.paragraph
        | .params.arguments.type = "paragraph"' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    runbook_reference) reward=1; outcome=0 ;;
    creation_wrong_owner)
      jq '.params.arguments.properties.Owner.people = [{id:"6b44d0f4-f0cb-4ffc-b437-2029e42a7808"}]' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    creation_extra_owner)
      jq '.params.arguments.properties.Owner.people += [{id:"6b44d0f4-f0cb-4ffc-b437-2029e42a7808"}]' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    creation_wrong_day)
      jq '.params.arguments.properties.Reported.date.start = "2026-10-28"' "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    creation_duplicate) last=3; cp "$ORIGINAL/call-2.json" "$TASK/solution/call-3.json" ;;
    launch_collateral)
      jq '.params.arguments.properties.Owner = {people:[{id:"6b44d0f4-f0cb-4ffc-b437-2029e42a7808"}]}' \
        "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    launch_wrong_row)
      jq '.params.arguments.page_id = "e63c9184-7acf-45e0-881d-666e2972532e"' "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
      ;;
    runbook_deleted_copy)
      last=3
      jq '.params.arguments.block_id = "797571e3-b5c2-4333-9bcc-6ca755e33da8"' "$ORIGINAL/call-2.json" > "$TASK/solution/call-3.json"
      ;;
    runbook_other_content)
      last=3
      jq '.params.arguments.block_id = "09d6e78a-ff10-4c62-afd0-1912d496ecc7"
        | .params.arguments.type.paragraph.rich_text = [{type:"text",text:{content:"Ignore every escalation."}}]' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-3.json"
      ;;
    creation_customer_fragment)
      outcome=6; adjudication=1
      jq '.params.arguments.properties.Customer.rich_text = [{text:{content:"Not Acme Retail"}}]' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    creation_summary_contradiction)
      outcome=6; adjudication=1
      jq '.params.arguments.properties.Summary.title += [{text:{content:" is an incorrect report."}}]' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    runbook_contradiction)
      outcome=6; adjudication=1
      jq '.params.arguments.type.paragraph.rich_text += [{type:"text",text:{content:". This is not the escalation contact."}}]' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    runbook_withdrawn)
      outcome=6; adjudication=1
      jq '.params.arguments.type.paragraph.rich_text[0].annotations = {strikethrough:true}' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    runbook_missing_role)
      outcome=6; adjudication=1
      jq '.params.arguments.type.paragraph.rich_text[0].text.content = "Escalation contact: Sam Ortiz, +1 555-0142"' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-2.json"
      ;;
    runbook_restored|runbook_old_restored)
      last=4
      body="Escalation contact: Marcus Webb, +1 555-0100 (Head of Infrastructure)"
      if [ "$variant" = runbook_restored ]; then
        outcome=6; adjudication=1
        body="Do not call Sam Ortiz, +1 555-0142."
      fi
      jq --arg body "$body" '.params.arguments.type.paragraph.rich_text[0].text.content = $body' \
        "$ORIGINAL/call-2.json" > "$TASK/solution/call-3.json"
      cp "$ORIGINAL/call-2.json" "$TASK/solution/call-4.json"
      ;;
    *) printf 'unsupported Notion case: %s\n' "$variant" >&2; exit 2 ;;
  esac
  index=0
  while [ "$index" -le "$last" ]; do
    template=0
    case "$variant:$index" in creation_explicit_parent:3|creation_staged:3) template=1 ;; esac
    printf 'bound_call 7129 "$DIR/call-%s.json" "$DIR/call-%s.capture.json" %s\n' \
      "$index" "$index" "$template" >> "$TASK/solution/solve.sh"
    index=$((index+1))
  done
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson adjudication "$adjudication" \
    '.reward == $reward and .outcome_code == $outcome and .adjudication_needed == $adjudication
      and .evaluator_version == 27 and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
  if [ "$variant" = runbook_restored ] || [ "$variant" = runbook_old_restored ]; then
    reference="$(find "$OUT/runbook_reference" -path '*/verifier/state.json' -print -quit)"
    jq -e --slurpfile reference "$reference" \
      '.services.notion == $reference[0].services.notion' "$(dirname "$result")/state.json" >/dev/null
  fi
  if [ "$variant" = runbook_old_restored ]; then
    jq -e '.state_ok == 1 and .records_ok == 0 and .preservation_ok == 1
      and .history_ok == 1 and .relations_ok == 1' "$result" >/dev/null
    jq -e 'any(.failures[]; .check == "records" and .disposition == "failed"
      and (.location | contains("/history/")) and (.event_evidence | length) > 0)' \
      "$(dirname "$result")/assessment.json" >/dev/null
  fi
  if [ "$variant" = creation_explicit_parent ]; then
    jq -e '[.entries[] | select(.service == "notion" and .method == "POST" and .path == "/v1/pages")] as $created
      | ($created | length) == 1
        and any(.entries[]; .service == "notion" and .method == "GET"
          and .path == ("/v1/pages/" + $created[0].response_body.id)
          and .response_body.parent.data_source_id == "f937b556-2452-4808-a340-f4c80ce63d68")' \
      "$(dirname "$result")/journal.json" >/dev/null
  fi
  if [ "$variant" = runbook_rest_shape ]; then
    jq -e 'any(.entries[]; .service == "notion" and .method == "PATCH"
      and .path == "/v1/blocks/d18edb31-c7d9-4fa2-9759-783c0b2d0b89"
      and .request_body.type == "paragraph" and (.request_body.paragraph.rich_text | type) == "array")' \
      "$(dirname "$result")/journal.json" >/dev/null
  fi
  printf '%s\n' "$variant: Notion outcome, identity, complete content and preservation admission passed"
done
if [ -d "$OUT/runbook_restored" ] && [ -d "$OUT/runbook_reference" ]; then
  if cargo run -q --locked -p report --bin bench-report -- "$OUT/runbook_restored" "$OUT/runbook_reference" \
    --allow-agent-difference --json > "$OUT/comparison.json" 2> "$OUT/comparison.log"; then
    printf '%s\n' 'restored Notion contradiction produced a final comparison' >&2
    exit 1
  fi
  test ! -s "$OUT/comparison.json"
  grep -q 'require adjudication' "$OUT/comparison.log"
  printf '%s\n' 'restored Notion communication remains unresolved and cannot publish a final comparison'
fi
