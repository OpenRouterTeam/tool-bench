#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the reviewed Supabase world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the pinned tools image}"
OUT="$(mktemp -d /tmp/tool-bench-supabase.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in cancel-pending-orders-duplicate-customer enable-rls-flagged-table pause-legacy-marketing-project; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/cloud__supabase__$slug/solution/"* "$OUT/$slug-original/"
done
PROD=qmvbxtzkplrnwdhyfaco
STAGING=hzrkcwpxnqdlmvbtejsa
LEGACY=wgtplxkqrzvnmbcdhfye
CURRENT=bdnfrjskvlqmzpxtwcgh
CANCEL="update public.orders set status = 'cancelled' where status = 'pending' and customer_id = (select id from public.customers where email = 'priya.shah@fastmail.com')"
RLS="alter table public.invoices enable row level security"
add() {
  jq -n --arg tool "$1" --argjson arguments "$2" \
    '{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:$tool,arguments:$arguments}}' \
    > "$TASK/solution/control-$calls.json"
  if [ "$reject" = 1 ]; then
    printf 'if call 7118 "$DIR/control-%s.json"; then exit 1; fi\n' "$calls" >> "$TASK/solution/solve.sh"
    errors=$((errors + 1))
  else
    printf 'call 7118 "$DIR/control-%s.json"\n' "$calls" >> "$TASK/solution/solve.sh"
  fi
  calls=$((calls + 1))
}
query() {
  add execute_sql "$(jq -cn --arg project "$1" --arg query "$2" \
    '{project_id:$project,query:$query}')"
}
migration() {
  add apply_migration "$(jq -cn --arg project "$1" --arg name "$2" --arg query "$3" \
    '{project_id:$project,name:$name,query:$query}')"
}
cancel() { query "$PROD" "$CANCEL"; }
rls() { migration "$PROD" secure_invoices "$RLS"; }
pause() {
  add pause_project "$(jq -cn --arg project "$1" '{project_id:$project}')"
}
if [ "$#" -eq 0 ]; then
  set -- cancel_reference cancel_individual cancel_join cancel_exists cancel_cte cancel_migration \
    cancel_all_pending cancel_paid cancel_money cancel_customer cancel_staging cancel_restored \
    rls_reference rls_retry rls_catalog rls_discovery rls_wrong_table rls_staging rls_sql_only \
    rls_unrelated_history rls_policy rls_policy_restored rls_forced rls_failed_migration \
    pause_reference pause_current pause_restored pause_paid_recovery \
    gap_transaction gap_grant_catalog gap_regclass
fi
for variant do
  reward=0
  outcome=1
  environment=1
  reject=0
  errors=0
  calls=0
  case "$variant" in
    cancel_*|gap_grant_catalog) slug=cancel-pending-orders-duplicate-customer ;;
    rls_*|gap_transaction|gap_regclass) slug=enable-rls-flagged-table ;;
    pause_*) slug=pause-legacy-marketing-project ;;
    *) printf 'unsupported Supabase variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  TASK="$OUT/tasks/cloud__supabase__$slug"
  ORIGINAL="$OUT/$slug-original"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  sed -e '/^bound_call [0-9]/,$d' -e '/^call [0-9]/,$d' \
    "$ORIGINAL/solve.sh" > "$TASK/solution/solve.sh"
  case "$variant" in
    cancel_reference) cancel; reward=1; outcome=0 ;;
    cancel_individual)
      for identity in 5003 5011; do
        query "$PROD" "update public.orders set status = 'cancelled' where id = $identity"
      done
      reward=1; outcome=0 ;;
    cancel_join)
      query "$PROD" "update public.orders as orders set status = 'cancelled' from public.customers as customers where orders.customer_id = customers.id and customers.email = 'priya.shah@fastmail.com' and orders.status = 'pending'"
      reward=1; outcome=0 ;;
    cancel_exists)
      query "$PROD" "update public.orders as orders set status = 'cancelled' where orders.status = 'pending' and exists (select 1 from public.customers as customers where customers.id = orders.customer_id and customers.email = 'priya.shah@fastmail.com')"
      reward=1; outcome=0 ;;
    cancel_cte)
      query "$PROD" "with target_customer(target_id) as (select id from public.customers where email = 'priya.shah@fastmail.com') update public.orders set status = 'cancelled' where status = 'pending' and customer_id in (select target_id from target_customer)"
      reward=1; outcome=0 ;;
    cancel_migration)
      migration "$PROD" cancel_unpaid_orders "$CANCEL"
      reward=1; outcome=0 ;;
    cancel_all_pending)
      query "$PROD" "update public.orders set status = 'cancelled' where status = 'pending'"; cancel ;;
    cancel_paid)
      query "$PROD" "update public.orders set status = 'cancelled' where customer_id = 1042"; cancel ;;
    cancel_money)
      query "$PROD" "update public.orders set total = 0 where id = 5011"; cancel ;;
    cancel_customer)
      query "$PROD" "update public.customers set country = 'unrequested' where id = 1042"; cancel ;;
    cancel_staging) query "$STAGING" "$CANCEL"; cancel ;;
    cancel_restored)
      for state in cancelled pending; do
        query "$PROD" "update public.orders set status = '$state' where id = 5009"
      done
      cancel ;;
    rls_reference) rls; reward=1; outcome=0 ;;
    rls_retry) rls; rls; rls; reward=1; outcome=0 ;;
    rls_catalog)
      rls
      query "$PROD" "select c.relrowsecurity as enabled from pg_catalog.pg_class as c join pg_catalog.pg_namespace as n on c.relnamespace = n.oid where n.nspname = 'public' and c.relname = 'invoices'"
      query "$PROD" "select count(*) as count from supabase_migrations.schema_migrations"
      reward=1; outcome=0 ;;
    rls_discovery)
      for schemas in '["public"]' '[]' '["public","unknown"]'; do
        add list_tables "$(jq -cn --arg project "$PROD" --argjson schemas "$schemas" \
          '{project_id:$project,schemas:$schemas,verbose:true}')"
      done
      add list_extensions "$(jq -cn --arg project "$PROD" '{project_id:$project}')"
      rls
      add list_tables "$(jq -cn --arg project "$PROD" \
        '{project_id:$project,schemas:["public"],verbose:false}')"
      add list_migrations "$(jq -cn --arg project "$PROD" '{project_id:$project}')"
      reward=1; outcome=0 ;;
    rls_wrong_table)
      migration "$PROD" wrong_table "alter table public.customers enable row level security" ;;
    rls_staging) migration "$STAGING" wrong_project "$RLS"; rls ;;
    rls_sql_only) query "$PROD" "$RLS" ;;
    rls_unrelated_history)
      migration "$PROD" unrelated "select 1"; query "$PROD" "$RLS" ;;
    rls_policy|rls_policy_restored)
      query "$PROD" "create policy open_invoices on public.invoices for select using (true)"
      if [ "$variant" = rls_policy_restored ]; then
        query "$PROD" "drop policy open_invoices on public.invoices"
      fi
      rls ;;
    rls_forced)
      rls; query "$PROD" "alter table public.invoices force row level security" ;;
    rls_failed_migration)
      reject=1
      migration "$PROD" failed_rls "$RLS; select missing_column from public.invoices"
      reject=0
      rls
      reward=1; outcome=0 ;;
    pause_reference)
      pause "$LEGACY"
      add get_project "$(jq -cn --arg project "$LEGACY" '{id:$project}')"
      reward=1; outcome=0 ;;
    pause_current) pause "$CURRENT"; pause "$LEGACY" ;;
    pause_restored)
      pause "$CURRENT"
      add restore_project "$(jq -cn --arg project "$CURRENT" '{project_id:$project}')"
      pause "$LEGACY" ;;
    pause_paid_recovery)
      reject=1; pause "$PROD"; reject=0; pause "$LEGACY"
      reward=1; outcome=0 ;;
    gap_transaction)
      reject=1; migration "$PROD" rolled_back "$RLS; rollback"; reject=0; rls
      environment=0; outcome=5 ;;
    gap_grant_catalog)
      cancel
      reject=1
      query "$PROD" "grant select on public.invoices to anon; select * from pg_available_extensions()"
      reject=0
      environment=0; outcome=5 ;;
    gap_regclass)
      rls
      reject=1
      query "$PROD" "select relrowsecurity from pg_class where oid = 'public.invoices'::regclass"
      reject=0
      environment=0; outcome=5 ;;
    *) printf 'unsupported Supabase variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" --argjson environment "$environment" '
    .reward==$reward and .outcome_code==$outcome and .environment_ok==$environment
      and .evaluator_version==29 and .contract_ok==1 and .integrity_ok==1 and .adjudication_needed==0
      and .runner_admission_checked==1 and .runner_admission_ok==1
      and .preservation_checked==1 and .history_checked==1' "$result" >/dev/null || {
        printf '%s\n' "$variant: unexpected assessment; inspect $result" >&2
        exit 1
      }
  assessment="$(dirname "$result")/assessment.json"
  case "$variant" in
    gap_*) predicate='.outcome=="environment_error" and any(.failures[];
      .check=="environment" and ((.event_evidence // [])|length)>0)' ;;
    cancel_restored|rls_policy_restored|pause_restored)
      predicate='.records_ok and .preservation_ok==false and .history_ok==false
        and any(.failures[]; ((.event_evidence // [])|length)>0)' ;;
    cancel_all_pending|cancel_paid|cancel_money|cancel_customer|cancel_staging|rls_staging|rls_forced|rls_unrelated_history|pause_current)
      predicate='.records_ok and .preservation_ok==false and .history_ok==false' ;;
    rls_wrong_table|rls_policy|rls_sql_only) predicate='.records_ok==false' ;;
    *) predicate='.outcome=="pass"' ;;
  esac
  jq -e "$predicate" "$assessment" >/dev/null
  jq -e --argjson expected "$calls" --argjson errors "$errors" '
    [.entries[]|select(.service=="supabase" and (.path|startswith("/v1/projects/")))] as $requests
    | ($requests|length)==$expected
      and ([$requests[]|select(.status<200 or .status>=300)]|length)==$errors' \
    "$(dirname "$result")/journal.json" >/dev/null
  if [ "$variant" = rls_discovery ]; then
    jq -e --slurpfile contract crates/services/supabase/assets/native-introspection-v0_15_0.json '
      [.entries[]|select(.service=="supabase" and (.path|endswith("/database/query")))
        | .request_body] as $requests
      | $contract[0] as $contract
      | any($requests[]; .query==($contract.tables_prefix+"where schema in ($1)")
          and .parameters==["public"])
        and any($requests[]; .query==($contract.tables_prefix+"where schema not in ($1, $2, $3, $4)")
          and .parameters==$contract.system_schemas)
        and any($requests[]; .query==($contract.tables_prefix+"where schema in ($1, $2)")
          and .parameters==["public","unknown"])
        and any($requests[]; .query==$contract.extensions_query)' \
      "$(dirname "$result")/journal.json" >/dev/null
  fi
  if [ "$variant" = rls_catalog ]; then
    jq -e 'any(.entries[]; .service=="supabase"
      and (.request_body.query // ""|startswith("select c.relrowsecurity"))
      and .response_body==[{"enabled":true}])' "$(dirname "$result")/journal.json" >/dev/null
  fi
  printf '%s\n' "$variant: native MCP requests, business/history predicates and environment disposition checked"
done
for pair in 'cancel_restored cancel_reference' 'rls_policy_restored rls_reference' 'pause_restored pause_reference'; do
  set -- $pair
  if [ -d "$OUT/$1" ] && [ -d "$OUT/$2" ]; then
    restored="$(find "$OUT/$1" -path '*/verifier/state.json' -print -quit)"
    reference="$(find "$OUT/$2" -path '*/verifier/state.json' -print -quit)"
    jq -e --slurpfile reference "$reference" '.services==$reference[0].services' "$restored" >/dev/null
    printf '%s\n' "$1: identical final business state retains unauthorized committed history"
  fi
done
