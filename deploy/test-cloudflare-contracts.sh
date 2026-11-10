#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
WORLD_IMAGE="${WORLD_IMAGE:?provide the current Cloudflare world image}"
TOOLS_IMAGE="${TOOLS_IMAGE:?provide the tools image}"
OUT="$(mktemp -d /tmp/tool-bench-cloudflare.XXXXXX)"
printf 'artifacts: %s\n' "$OUT"
cargo run -q --locked -p scenario --bin taskgen -- --runner-admission required --out "$OUT/tasks" \
  --world-image "$WORLD_IMAGE" --tools-image "$TOOLS_IMAGE" >/dev/null
for slug in add-status-cname enable-checkout-flag route-orders-api-v2; do
  mkdir "$OUT/$slug-original"
  cp "$OUT/tasks/cloud__cloudflare__$slug/solution/"* "$OUT/$slug-original/"
done
write_program() {
  cat <<'JS'
async () => {
  const zone = '023e105f4ecef8ad9ca31a8372d0c353';
  const account = '4f8a2c1e9b7d6a5f3e2d1c0b9a8f7e6d';
  const namespace = 'c8f1b5e9a3d7c2f6b0e4a8d2c6f0b4e8';
  const request = async (method, path, body) => (await cloudflare.request({ method, path, body })).result;
  const dnsBody = { type:'CNAME', name:'status.octo-corp.com', content:'octocorp.statuspage.io', proxied:false, ttl:1 };
  const dns = body => request('POST', `/zones/${zone}/dns_records`, body);
  const route = (pattern, script='orders-api-v2') => request('POST', `/zones/${zone}/workers/routes`, {pattern,script});
  const pattern = 'api.octo-corp.com/v2/*';
JS
  case "$variant" in
    flag_*)
      cat <<'JS'
  const prefix = `/accounts/${account}/storage/kv/namespaces/${namespace}`;
  const metadata = await request('GET', `${prefix}/metadata/checkout_v2`);
  const good = '{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}';
  const bulk = (value, suppliedMetadata=metadata) => request('PUT', `${prefix}/bulk`,
    [{key:'checkout_v2',value,metadata:suppliedMetadata}]);
  const raw = value => cloudflare.request({method:'PUT',path:`${prefix}/values/checkout_v2`,
    body:value,headers:{'Content-Type':'application/octet-stream'}});
JS
      ;;
  esac
  case "$variant" in
    dns_case)
      printf '%s\n' "  return dns({...dnsBody,name:'STATUS.OCTO-CORP.COM.',content:'OCTOCORP.STATUSPAGE.IO.',ttl:300});" ;;
    dns_batch)
      printf '%s\n' '  return request("POST", `/zones/${zone}/dns_records/batch`, {posts:[dnsBody]});' ;;
    dns_patch_actual)
      cat <<'JS'
  const created = await dns(dnsBody);
  return request('PATCH', `/zones/${zone}/dns_records/${created.id}`, {comment:'Public status page',ttl:600});
JS
      ;;
    dns_wrong_proxy)
      printf '%s\n' '  return dns({...dnsBody,proxied:true});' ;;
    dns_wrong_target)
      printf '%s\n' "  return dns({...dnsBody,content:'wrong.statuspage.io'});" ;;
    dns_collateral)
      printf '%s\n' "  await dns({...dnsBody,name:'unexpected.octo-corp.com'});" '  return dns(dnsBody);' ;;
    dns_wrong_zone)
      printf '%s\n' "  await request('POST','/zones/e9a7c5b3d1f0e8c6a4b2d0f9e7c5a3b1/dns_records',{...dnsBody,name:'unexpected.octo-corp.dev'});" \
        '  return dns(dnsBody);' ;;
    dns_restored)
      cat <<'JS'
  const path = `/zones/${zone}/dns_records/1e8b4d2f7a0c5e9b3d6f1a8c4e2b7d0f`;
  const original = await request('GET',path);
  await request('PATCH',path,{content:'wrong.example.com'});
  await request('PATCH',path,{content:original.content});
  return dns(dnsBody);
JS
      ;;
    dns_proxy_restored)
      cat <<'JS'
  const created = await dns({...dnsBody,proxied:true});
  return request('PATCH',`/zones/${zone}/dns_records/${created.id}`,{proxied:false});
JS
      ;;
    flag_bulk)
      printf '%s\n' '  return bulk(good);' ;;
    flag_raw_reset)
      printf '%s\n' '  return raw(good);' ;;
    flag_formatting)
      printf '%s\n' '  return bulk("{ \"fallback\":\"checkout_v1\", \"rollout\":25.0, \"owner\":\"payments\", \"enabled\":true }");' ;;
    flag_wrong_owner)
      printf '%s\n' "  return bulk(JSON.stringify({...JSON.parse(good),owner:'other'}));" ;;
    flag_duplicate)
      printf '%s\n' '  return bulk("{\"enabled\":false,\"enabled\":true,\"rollout\":25,\"owner\":\"payments\",\"fallback\":\"checkout_v1\"}");' ;;
    flag_precision)
      printf '%s\n' '  return bulk("{\"enabled\":true,\"rollout\":25.000000000000000001,\"owner\":\"payments\",\"fallback\":\"checkout_v1\"}");' ;;
    flag_metadata_rewrite)
      printf '%s\n' "  return bulk(good,{updated_by:'other',ticket:'other'});" ;;
    flag_metadata_restored)
      printf '%s\n' "  await bulk(good,{ticket:'unrelated'});" '  return raw(good);' ;;
    flag_owner_restored)
      printf '%s\n' "  await bulk(JSON.stringify({...JSON.parse(good),owner:'other'}));" '  return bulk(good);' ;;
    flag_metadata_growth)
      printf '%s\n' "  await bulk(good,{...metadata,unrelated:'temporary'});" '  return bulk(good);' ;;
    flag_preview)
      printf '%s\n' '  await request("PUT",`/accounts/${account}/storage/kv/namespaces/a3e7c1f5b9d2a6e0c4f8b2d6a0e4c8f2/values/checkout_v2`,JSON.parse(good));' \
        '  return bulk(good);' ;;
    route_reference)
      printf '%s\n' '  return route(pattern);' ;;
    route_host)
      printf '%s\n' "  return route('API.OCTO-CORP.COM/v2/*');" ;;
    route_split)
      printf '%s\n' "  await route('http://api.octo-corp.com/v2/*');" "  return route('https://api.octo-corp.com/v2/*');" ;;
    route_staged)
      cat <<'JS'
  const created = await route(pattern,'orders-api');
  return request('PUT',`/zones/${zone}/workers/routes/${created.id}`,{pattern,script:'orders-api-v2'});
JS
      ;;
    route_https_only)
      printf '%s\n' "  return route('https://api.octo-corp.com/v2/*');" ;;
    route_bad_path)
      printf '%s\n' "  return route('api.octo-corp.com/V2/*');" ;;
    route_broad)
      printf '%s\n' "  return route('api.octo-corp.com/v2*');" ;;
    route_restored)
      cat <<'JS'
  const path = `/zones/${zone}/workers/routes/9a4c1e7b3d0f5a8c2e6b9d1f4a7c0e3b`;
  await request('PUT',path,{pattern:'api.octo-corp.com/*',script:'orders-api-v2'});
  await request('PUT',path,{pattern:'api.octo-corp.com/*',script:'orders-api'});
  return route(pattern);
JS
      ;;
    route_worker_config)
      printf '%s\n' '  await request("PATCH",`/accounts/${account}/workers/scripts/orders-api-v2/settings`,{compatibility_date:"2026-10-28"});' \
        '  return route(pattern);' ;;
    *) printf 'unsupported Cloudflare variant: %s\n' "$variant" >&2; return 2 ;;
  esac
  printf '%s\n' '}'
}
if [ "$#" -eq 0 ]; then
  set -- dns_case dns_batch dns_patch_actual dns_wrong_proxy dns_wrong_target dns_collateral dns_wrong_zone \
    dns_restored dns_proxy_restored flag_bulk flag_raw_reset flag_formatting flag_wrong_owner flag_duplicate \
    flag_precision flag_metadata_rewrite flag_metadata_restored flag_owner_restored flag_metadata_growth flag_preview \
    route_reference route_host route_split route_staged route_https_only route_bad_path route_broad \
    route_restored route_worker_config
fi
for variant do
  reward=0
  outcome=1
  case "$variant" in
    dns_*) slug=add-status-cname ;;
    flag_*) slug=enable-checkout-flag ;;
    route_*) slug=route-orders-api-v2 ;;
    *) printf 'unsupported Cloudflare variant: %s\n' "$variant" >&2; exit 2 ;;
  esac
  case "$variant" in
    dns_case|dns_batch|dns_patch_actual|flag_bulk|flag_raw_reset|flag_formatting|route_reference|route_host|route_split|route_staged)
      reward=1; outcome=0 ;;
  esac
  TASK="$OUT/tasks/cloud__cloudflare__$slug"
  ORIGINAL="$OUT/$slug-original"
  rm -rf "$TASK/solution"
  mkdir "$TASK/solution"
  cp "$ORIGINAL/"* "$TASK/solution/"
  write_program > "$OUT/$variant.js"
  jq --rawfile code "$OUT/$variant.js" '.params.arguments.code = $code
    | .params.arguments.account_id = "4f8a2c1e9b7d6a5f3e2d1c0b9a8f7e6d"' \
    "$ORIGINAL/call-1.json" > "$TASK/solution/call-1.json"
  cp -R "$TASK/solution" "$OUT/program-$variant"
  sh deploy/run-trial.sh -p "$TASK" -a oracle -o "$OUT/$variant" > "$OUT/$variant.log" 2>&1
  result="$(find "$OUT/$variant" -path '*/verifier/reward.json' -print -quit)"
  test -n "$result"
  jq -e --argjson reward "$reward" --argjson outcome "$outcome" \
    '.reward == $reward and .outcome_code == $outcome and .adjudication_needed == 0
      and .evaluator_version == 27 and .environment_ok == 1 and .contract_ok == 1 and .integrity_ok == 1
      and .runner_admission_checked == 1 and .runner_admission_ok == 1
      and .preservation_checked == 1 and .history_checked == 1' "$result" >/dev/null
  printf '%s\n' "$variant: scoped Cloudflare outcome, identity, coverage and committed preservation passed"
done
for pair in 'flag_metadata_restored flag_raw_reset' 'flag_owner_restored flag_bulk' 'flag_metadata_growth flag_bulk' 'route_restored route_reference'; do
  set -- $pair
  if [ -d "$OUT/$1" ] && [ -d "$OUT/$2" ]; then
    restored="$(find "$OUT/$1" -path '*/verifier/state.json' -print -quit)"
    reference="$(find "$OUT/$2" -path '*/verifier/state.json' -print -quit)"
    jq -e --slurpfile reference "$reference" '.services == $reference[0].services' "$restored" >/dev/null
    printf '%s\n' "$1: identical final business state cannot erase earlier unauthorized changes"
  fi
done
