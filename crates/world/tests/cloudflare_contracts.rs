mod support;

use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::{Value, json};
use support::ContractTrial as Trial;

const DNS: &str = "cloud/cloudflare/add-status-cname";
const FLAG: &str = "cloud/cloudflare/enable-checkout-flag";
const ROUTE: &str = "cloud/cloudflare/route-orders-api-v2";
const ZONE: &str = "023e105f4ecef8ad9ca31a8372d0c353";
const ACCOUNT: &str = "4f8a2c1e9b7d6a5f3e2d1c0b9a8f7e6d";
const NAMESPACE: &str = "c8f1b5e9a3d7c2f6b0e4a8d2c6f0b4e8";
const PATTERN: &str = "api.octo-corp.com/v2/*";
const WORKER: &str = "orders-api-v2";

impl Trial {
    fn new(id: &str) -> Self {
        Self::with_dataset(id, |_| {})
    }

    fn with_dataset(id: &str, prepare: impl FnOnce(&mut Dataset)) -> Self {
        Self::load_with_dataset(id, &mock_cloudflare::Cloudflare, &[], prepare)
    }

    async fn execute(&self, code: &str) -> Value {
        let result = self
            .call(
                "POST",
                "/_mcp/tools/call",
                json!({
                    "name":"execute","arguments":{"code":code,"account_id":ACCOUNT}
                }),
            )
            .await;
        let text = result["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap_or_else(|_| json!(text))
    }

    async fn api(&self, method: &str, path: &str, body: Value) -> Value {
        self.execute(&format!(
            "async () => (await cloudflare.request({})).result",
            json!({"method":method,"path":path,"body":body})
        ))
        .await
    }

    async fn dns(&self, body: Value) -> Value {
        self.api("POST", &format!("/zones/{ZONE}/dns_records"), body)
            .await
    }

    async fn route(&self, pattern: &str, script: &str) -> Value {
        self.api(
            "POST",
            &format!("/zones/{ZONE}/workers/routes"),
            json!({"pattern":pattern,"script":script}),
        )
        .await
    }

    async fn flag(&self, text: &str, preserve_metadata: bool, bulk: bool) {
        let metadata = if preserve_metadata {
            self.state()["kv_values"][format!("{NAMESPACE}/checkout_v2")]["metadata"].clone()
        } else {
            Value::Null
        };
        if bulk {
            self.api(
                "PUT",
                &format!("/accounts/{ACCOUNT}/storage/kv/namespaces/{NAMESPACE}/bulk"),
                json!([{"key":"checkout_v2","value":text,"metadata":metadata}]),
            )
            .await;
        } else if preserve_metadata {
            self.execute(&format!(
                "async () => {{
                const form = new FormData();
                form.set('value', {});
                form.set('metadata', JSON.stringify({}));
                return (await cloudflare.request({{method:'PUT',path:{},body:form}})).result;
            }}",
                json!(text),
                metadata,
                json!(format!(
                    "/accounts/{ACCOUNT}/storage/kv/namespaces/{NAMESPACE}/values/checkout_v2"
                ))
            ))
            .await;
        } else {
            self.execute(&format!("async () => (await cloudflare.request({{method:'PUT',path:{},body:{},headers:{{'Content-Type':'application/octet-stream'}}}})).result",
                json!(format!("/accounts/{ACCOUNT}/storage/kv/namespaces/{NAMESPACE}/values/checkout_v2")),json!(text))).await;
        }
    }

    async fn complete(&self, id: &str) {
        match id {
            DNS => {
                self.dns(json!({"type":"CNAME","name":"status","content":"octocorp.statuspage.io","proxied":false})).await;
            }
            FLAG => {
                self.flag(
                    r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}"#,
                    true,
                    false,
                )
                .await;
            }
            _ => {
                self.route(PATTERN, WORKER).await;
            }
        }
    }
}

fn pass(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Pass, "{grade:?}");
    assert!(grade.preservation_checked && grade.history_checked);
}

fn fail(grade: &Grade) {
    assert_eq!(grade.outcome, Outcome::Fail, "{grade:?}");
    assert!(grade.environment_ok && grade.contract_ok && grade.integrity_ok);
}

#[tokio::test]
async fn cloudflare_writes_require_actual_outcomes_and_audited_default_preservation() {
    for id in [DNS, FLAG, ROUTE] {
        let trial = Trial::new(id);
        fail(&trial.assess());
        trial.complete(id).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn dns_creation_uses_actual_ids_and_equivalent_names_not_a_predicted_hash() {
    for name in ["status", "STATUS.OCTO-CORP.COM."] {
        let trial = Trial::with_dataset(DNS, |dataset| dataset.meta.seed.push_str("-shifted"));
        let created=trial.dns(json!({"type":"CNAME","name":name,"content":"OCTOCORP.STATUSPAGE.IO.","proxied":false,"ttl":300})).await;
        assert_ne!(created["id"], "bc9852aa03507f9f32653fdd0d65f467");
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn dns_batch_and_returned_id_metadata_updates_are_valid_plans() {
    let trial = Trial::new(DNS);
    let created=trial.api("POST",&format!("/zones/{ZONE}/dns_records/batch"),json!({
        "posts":[{"type":"CNAME","name":"status","content":"octocorp.statuspage.io","proxied":false}]
    })).await;
    let id = created["posts"][0]["id"].as_str().unwrap();
    trial
        .api(
            "PATCH",
            &format!("/zones/{ZONE}/dns_records/{id}"),
            json!({"comment":"Public status page","ttl":600}),
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn proxied_or_wrong_target_dns_records_are_not_statuspage_completion() {
    for body in [
        json!({"type":"CNAME","name":"status","content":"octocorp.statuspage.io","proxied":true}),
        json!({"type":"CNAME","name":"status","content":"wrong.statuspage.io","proxied":false}),
    ] {
        let trial = Trial::new(DNS);
        trial.dns(body).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn a_restored_existing_dns_record_or_collateral_creation_remains_unauthorized() {
    let trial = Trial::new(DNS);
    let old = "1e8b4d2f7a0c5e9b3d6f1a8c4e2b7d0f";
    let original = trial.state()["dns_records"][old]["content"].clone();
    for content in [json!("wrong.example.com"), original] {
        trial
            .api(
                "PATCH",
                &format!("/zones/{ZONE}/dns_records/{old}"),
                json!({"content":content}),
            )
            .await;
    }
    trial.complete(DNS).await;
    fail(&trial.assess());
    let extra = Trial::new(DNS);
    extra.dns(json!({"type":"CNAME","name":"unexpected","content":"octocorp.statuspage.io","proxied":false})).await;
    extra.complete(DNS).await;
    fail(&extra.assess());
}

#[tokio::test]
async fn temporarily_proxying_the_new_statuspage_record_is_not_erased_by_the_final_fix() {
    let trial = Trial::new(DNS);
    let created=trial.dns(json!({"type":"CNAME","name":"status","content":"octocorp.statuspage.io","proxied":true})).await;
    trial
        .api(
            "PATCH",
            &format!(
                "/zones/{ZONE}/dns_records/{}",
                created["id"].as_str().unwrap()
            ),
            json!({"proxied":false}),
        )
        .await;
    fail(&trial.assess());
}

#[tokio::test]
async fn kv_updates_accept_multipart_bulk_reordering_and_equivalent_numbers() {
    for bulk in [false, true] {
        let trial = Trial::new(FLAG);
        trial.flag("{ \"fallback\":\"checkout_v1\", \"owner\":\"payments\", \"rollout\":25.0, \"enabled\":true }",true,bulk).await;
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn the_existing_kv_reference_must_preserve_metadata_not_merely_the_flag_json() {
    let trial = Trial::new(FLAG);
    for call in &trial.scenario.solution.calls {
        trial
            .execute(call.arguments["code"].as_str().unwrap())
            .await;
    }
    assert_eq!(
        trial.state()["kv_values"][format!("{NAMESPACE}/checkout_v2")]["metadata"],
        json!({"updated_by":"deploy-bot","ticket":"PAY-2210"})
    );
    pass(&trial.assess());
}

#[tokio::test]
async fn kv_documented_metadata_reset_is_not_an_added_user_requirement() {
    let trial = Trial::new(FLAG);
    trial
        .flag(
            r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}"#,
            false,
            false,
        )
        .await;
    assert!(
        trial.state()["kv_values"][format!("{NAMESPACE}/checkout_v2")]
            .get("metadata")
            .is_none()
    );
    pass(&trial.assess());
}

#[tokio::test]
async fn kv_metadata_reset_permission_does_not_authorize_arbitrary_metadata_rewrites() {
    let trial = Trial::new(FLAG);
    trial.api("PUT",&format!("/accounts/{ACCOUNT}/storage/kv/namespaces/{NAMESPACE}/bulk"),json!([
        {"key":"checkout_v2","value":r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}"#,
            "metadata":{"updated_by":"someone-else","ticket":"other"}}
    ])).await;
    fail(&trial.assess());
}
#[tokio::test]
async fn kv_other_json_fields_extra_fields_and_wrong_numeric_types_are_not_authorized() {
    for source in [
        r#"{"enabled":true,"rollout":25,"owner":"other","fallback":"checkout_v1"}"#,
        r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1","extra":true}"#,
        r#"{"enabled":true,"rollout":"25","owner":"payments","fallback":"checkout_v1"}"#,
    ] {
        let trial = Trial::new(FLAG);
        trial.flag(source, true, true).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn restored_kv_owner_edits_cannot_hide_in_one_execute_program() {
    let trial = Trial::new(FLAG);
    trial.execute(&format!("async () => {{
        const path = '/accounts/{ACCOUNT}/storage/kv/namespaces/{NAMESPACE}/bulk';
        const metadata = {{updated_by:'deploy-bot',ticket:'PAY-2210'}};
        for (const owner of ['other','payments']) {{
            await cloudflare.request({{method:'PUT',path,body:[{{key:'checkout_v2',
                value:JSON.stringify({{enabled:true,rollout:25,owner,fallback:'checkout_v1'}}),metadata}}]}});
        }}
    }}")).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn kv_look_alike_keys_and_expiration_changes_remain_protected() {
    let trial = Trial::new(FLAG);
    trial.api("PUT",&format!("/accounts/{ACCOUNT}/storage/kv/namespaces/{NAMESPACE}/bulk"),json!([
        {"key":"checkout_v2_copy","value":"changed"},
        {"key":"checkout_v2","value":r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}"#,
            "metadata":{"updated_by":"deploy-bot","ticket":"PAY-2210"},"expiration_ttl":3600}
    ])).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn worker_routes_use_actual_ids_and_case_insensitive_hosts_but_case_sensitive_paths() {
    let valid = Trial::with_dataset(ROUTE, |dataset| {
        dataset.meta.seed.push_str("-route-shifted")
    });
    let created = valid.route("API.OCTO-CORP.COM/v2/*", WORKER).await;
    assert_ne!(created["id"], "5af521d08a8f83440981f48ff71d8553");
    pass(&valid.assess());
    let wrong = Trial::new(ROUTE);
    wrong.route("api.octo-corp.com/V2/*", WORKER).await;
    fail(&wrong.assess());
}

#[tokio::test]
async fn separate_http_and_https_routes_are_equivalent_complete_coverage() {
    for patterns in [
        vec![
            "http://api.octo-corp.com/v2/*",
            "https://api.octo-corp.com/v2/*",
        ],
        vec![
            "https://api.octo-corp.com/v2/*",
            PATTERN,
            "http://api.octo-corp.com/v2/*",
        ],
    ] {
        let trial = Trial::new(ROUTE);
        for pattern in patterns {
            trial.route(pattern, WORKER).await;
        }
        pass(&trial.assess());
    }
}

#[tokio::test]
async fn worker_routing_may_be_prepared_without_changing_traffic_before_switching() {
    let trial = Trial::new(ROUTE);
    let created = trial.route(PATTERN, "orders-api").await;
    trial
        .api(
            "PUT",
            &format!(
                "/zones/{ZONE}/workers/routes/{}",
                created["id"].as_str().unwrap()
            ),
            json!({"pattern":PATTERN,"script":WORKER}),
        )
        .await;
    pass(&trial.assess());
}

#[tokio::test]
async fn worker_route_coverage_cannot_be_partial_or_widen_to_other_traffic() {
    for pattern in [
        "https://api.octo-corp.com/v2/*",
        "api.octo-corp.com/v2*",
        "*.octo-corp.com/v2/*",
    ] {
        let trial = Trial::new(ROUTE);
        trial.route(pattern, WORKER).await;
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn existing_route_edits_are_rejected_even_when_restored_and_a_good_new_route_exists() {
    let trial = Trial::new(ROUTE);
    let identity = "9a4c1e7b3d0f5a8c2e6b9d1f4a7c0e3b";
    for script in [WORKER, "orders-api"] {
        trial
            .api(
                "PUT",
                &format!("/zones/{ZONE}/workers/routes/{identity}"),
                json!({"pattern":"api.octo-corp.com/*","script":script}),
            )
            .await;
    }
    trial.complete(ROUTE).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn decoded_cache_values_cannot_hide_duplicate_raw_json_members_or_decimal_rounding() {
    for source in [
        r#"{"enabled":false,"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}"#,
        r#"{"enabled":true,"rollout":25.000000000000000001,"owner":"payments","fallback":"checkout_v1"}"#,
    ] {
        let trial = Trial::new(FLAG);
        trial.flag(source, true, true).await;
        assert_eq!(
            trial.state()["kv_values"][format!("{NAMESPACE}/checkout_v2")]["value_json"]["enabled"],
            true
        );
        fail(&trial.assess());
    }
}

#[tokio::test]
async fn an_allowed_metadata_reset_cannot_erase_an_earlier_arbitrary_metadata_rewrite() {
    let trial = Trial::new(FLAG);
    trial.api("PUT",&format!("/accounts/{ACCOUNT}/storage/kv/namespaces/{NAMESPACE}/bulk"),json!([
        {"key":"checkout_v2","value":r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}"#,
            "metadata":{"ticket":"unrelated"}}
    ])).await;
    trial
        .flag(
            r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}"#,
            false,
            false,
        )
        .await;
    let grade = trial.assess();
    fail(&grade);
    assert!(grade.records_ok && !grade.history_ok, "{grade:?}");
}

#[tokio::test]
async fn creating_the_correct_dns_record_does_not_authorize_a_staging_record() {
    let trial = Trial::new(DNS);
    trial.api("POST","/zones/e9a7c5b3d1f0e8c6a4b2d0f9e7c5a3b1/dns_records",
        json!({"type":"CNAME","name":"unexpected","content":"octocorp.statuspage.io","proxied":false})).await;
    trial.complete(DNS).await;
    fail(&trial.assess());
}

#[tokio::test]
async fn routing_changes_do_not_authorize_reconfiguring_the_already_deployed_worker() {
    let trial = Trial::new(ROUTE);
    trial
        .api(
            "PATCH",
            &format!("/accounts/{ACCOUNT}/workers/scripts/{WORKER}/settings"),
            json!({"compatibility_date":"2026-10-28"}),
        )
        .await;
    trial.complete(ROUTE).await;
    let grade = trial.assess();
    fail(&grade);
    assert!(grade.records_ok && !grade.preservation_ok, "{grade:?}");
}

#[tokio::test]
async fn kv_metadata_growth_cannot_be_hidden_by_later_restoration() {
    let trial = Trial::new(FLAG);
    let original =
        trial.state()["kv_values"][format!("{NAMESPACE}/checkout_v2")]["metadata"].clone();
    let mut extra = original.clone();
    extra["unrelated"] = json!("temporary");
    for metadata in [extra, original] {
        trial.api("PUT",&format!("/accounts/{ACCOUNT}/storage/kv/namespaces/{NAMESPACE}/bulk"),json!([
            {"key":"checkout_v2","value":r#"{"enabled":true,"rollout":25,"owner":"payments","fallback":"checkout_v1"}"#,
                "metadata":metadata}
        ])).await;
    }
    let grade = trial.assess();
    fail(&grade);
    assert!(grade.records_ok && !grade.history_ok, "{grade:?}");
}
