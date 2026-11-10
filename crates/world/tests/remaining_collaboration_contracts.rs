mod support;

use mock_core::{
    dataset::Dataset,
    grading::{Grade, Outcome},
};
use serde_json::{Value, json};
use support::{ContractTrial, ServiceSpec};

const REFUND: &str = "collab/multi/google-refund-damaged-order";
const CUTOVER: &str = "collab/multi/linear-cloudflare-api-origin-cutover";
const CALENDAR: &str = "collab/multi/slack-google-cutover-calendar";

const THREAD: &str = "c41e7a2b9d03f615";
const REQUESTED_CHARGE: &str = "ch_Hk5TzQ9wNc2RbV6mXa8LuE4P";
const REQUESTED_INTENT: &str = "pi_Yb3KdR8mWq1ZtN6xLc4VhP9s";
const LOOKALIKE_CHARGE: &str = "ch_Gd7WnY3kPs9QtF2hZr5MbC8X";
const EARLIER_CHARGE: &str = "ch_Vq2mLx8RtB4nKe7YpW1sHd3J";
const ORDERS: &str = "db.qmvbxtzkplrnwdhyfaco.public.orders";
const CUSTOMERS: &str = "db.qmvbxtzkplrnwdhyfaco.public.customers";
const FULL_REFUND_REPLY: &str = "Hi Priya, your order #5024 has been refunded in full: $62.00.";

const ZONE: &str = "023e105f4ecef8ad9ca31a8372d0c353";
const STAGING_ZONE: &str = "e9a7c5b3d1f0e8c6a4b2d0f9e7c5a3b1";
const API_RECORD: &str = "5b2d7a1f0e3c8b6d4f9a2e1c7b0d3f58";
const APEX_RECORD: &str = "372e67954025e0ba6aaa6d586b9e0b59";
const STAGING_RECORD: &str = "e41a6d0c9b2f7e5a3c8d1b6f4a9e2c07";
const DONE: &str = "4ed11568-6dd0-5b94-bf4e-ba05dd563850";
const CANCELED: &str = "99afb074-55c3-5598-a54a-aeacf48ea5ff";
const IN_PROGRESS: &str = "1bf2e271-f765-5dce-bbdd-56b9366aacc6";
const NEW_ORIGIN: &str = "198.51.100.45";
const CANCELED_ORIGIN: &str = "198.51.100.44";
const CUTOVER_COMMENT: &str = "Updated the existing api.octo-corp.com A record to 198.51.100.45.";

const OPS_TEAM: &str = "c_ops_team@group.calendar.google.com";
const STAGING_REHEARSAL: &str = "7c1e9a4f2b6d8e0a3c5f7b9d1e";
const SAM: &str = "sam.ortiz@octo-corp.example";
const PRIYA: &str = "priya.natarajan@octo-corp.example";
const MARCUS: &str = "marcus.webb@octo-corp.example";
const MIGRATION_CHANNEL: &str = "C05DBMIG2RT";
const THREAD_ROOT: &str = "1792600205.318801";
const WINDOW_REPLY: &str = "The production cutover is on the Ops team calendar.";

fn trial(id: &str) -> ContractTrial {
    trial_with(id, |_| {})
}

fn trial_with(id: &str, prepare: impl FnOnce(&mut Dataset)) -> ContractTrial {
    let services: Vec<ServiceSpec<'_>> = match id {
        REFUND => vec![
            (&mock_google::Google, &[]),
            (
                &mock_supabase::Supabase,
                &[("authorization", "Bearer sbp_toolbench")],
            ),
            (&mock_stripe::Stripe, &[]),
        ],
        CUTOVER => vec![
            (&mock_linear::Linear, &[]),
            (&mock_cloudflare::Cloudflare, &[]),
        ],
        _ => vec![(&mock_slack::Slack, &[]), (&mock_google::Google, &[])],
    };
    ContractTrial::load_many_with_dataset(id, &services, prepare)
}

fn service_for(endpoint: &str) -> &str {
    match endpoint {
        "google-workspace" => "google",
        other => other,
    }
}

fn merged(base: Value, changes: Value) -> Value {
    let mut arguments = base;
    for (field, value) in changes.as_object().unwrap() {
        if value.is_null() {
            arguments.as_object_mut().unwrap().remove(field);
        } else {
            arguments[field] = value.clone();
        }
    }
    arguments
}

async fn tool(trial: &ContractTrial, service: &str, name: &str, arguments: Value) -> Value {
    trial
        .call_from(
            service,
            "POST",
            "/_mcp/tools/call",
            json!({"name":name,"arguments":arguments}),
        )
        .await
}

async fn rejected_tool(
    trial: &ContractTrial,
    service: &str,
    name: &str,
    arguments: Value,
) -> Value {
    let (status, result) = trial
        .request_from(
            service,
            "POST",
            "/_mcp/tools/call",
            json!({"name":name,"arguments":arguments}),
        )
        .await;
    assert!(status.is_success(), "{status} {result}");
    assert_eq!(result["isError"], true, "{result}");
    result
}

async fn sql(trial: &ContractTrial, query: &str) -> Value {
    trial
        .call_from(
            "supabase",
            "POST",
            "/v1/projects/qmvbxtzkplrnwdhyfaco/database/query",
            json!({"query":query}),
        )
        .await
}

async fn replay(trial: &ContractTrial) {
    let calls = trial.scenario.solution.calls.clone();
    for call in calls {
        if call.endpoint == "supabase" {
            match call.tool.as_str() {
                "list_projects" => {
                    trial
                        .call_from("supabase", "GET", "/v1/projects", Value::Null)
                        .await;
                }
                "execute_sql" => {
                    sql(trial, call.arguments["query"].as_str().unwrap()).await;
                }
                other => panic!("unsupported supabase reference tool {other}"),
            }
        } else {
            tool(
                trial,
                service_for(&call.endpoint),
                &call.tool,
                call.arguments.clone(),
            )
            .await;
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

fn state_of(trial: &ContractTrial, service: &str) -> Value {
    trial.world.lock().unwrap().snapshot()["services"][service].clone()
}

async fn refund(trial: &ContractTrial, parameters: Value) -> Value {
    tool(
        trial,
        "stripe",
        "stripe_api_write",
        json!({
            "stripe_api_operation_id":"PostRefunds","parameters":parameters,
            "stripe_context":"acct_1FwY8bQk2LmN4pRs","livemode":false
        }),
    )
    .await
}

async fn full_refund(trial: &ContractTrial) {
    refund(trial, json!({"charge":REQUESTED_CHARGE})).await;
}

async fn mark_refunded(trial: &ContractTrial, order: u32) {
    sql(
        trial,
        &format!(
            "update public.orders set status = 'refunded' where id = {order} returning id, status"
        ),
    )
    .await;
}

fn reply_arguments(changes: Value) -> Value {
    merged(
        json!({"to":"priya.shah@acme.io","thread_id":THREAD,"body":FULL_REFUND_REPLY}),
        changes,
    )
}

async fn reply(trial: &ContractTrial, changes: Value) {
    tool(
        trial,
        "google",
        "send_gmail_message",
        reply_arguments(changes),
    )
    .await;
}

async fn complete_refund_workflow(trial: &ContractTrial) {
    full_refund(trial).await;
    mark_refunded(trial, 5024).await;
    reply(trial, json!({})).await;
}

async fn run_dns(trial: &ContractTrial, method: &str, path: &str, body: Value) -> Value {
    let request = json!({"method":method,"path":path,"body":body});
    let result = tool(
        trial,
        "cloudflare",
        "execute",
        json!({"code":format!("async () => (await cloudflare.request({request})).result")}),
    )
    .await;
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap_or(Value::Null)
}

async fn patch_dns(trial: &ContractTrial, zone: &str, record: &str, body: Value) -> Value {
    run_dns(
        trial,
        "PATCH",
        &format!("/zones/{zone}/dns_records/{record}"),
        body,
    )
    .await
}

async fn move_api_record(trial: &ContractTrial) {
    patch_dns(trial, ZONE, API_RECORD, json!({"content":NEW_ORIGIN})).await;
}

async fn comment_on(trial: &ContractTrial, issue: &str, body: &str) {
    tool(
        trial,
        "linear",
        "linear_createComment",
        json!({"issueId":issue,"body":body}),
    )
    .await;
}

async fn move_issue(trial: &ContractTrial, issue: &str, state: &str) {
    tool(
        trial,
        "linear",
        "linear_updateIssue",
        json!({"id":issue,"stateId":state}),
    )
    .await;
}

async fn complete_cutover_workflow(trial: &ContractTrial) {
    move_api_record(trial).await;
    comment_on(trial, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(trial, "PLAT-241", DONE).await;
}

fn event_arguments(changes: Value) -> Value {
    merged(
        json!({
            "action":"create","calendar_id":OPS_TEAM,
            "summary":"Payments cluster PG16 production cutover",
            "start_time":"2026-11-07T22:00:00","end_time":"2026-11-08T01:00:00",
            "timezone":"America/Chicago","attendees":[SAM,PRIYA]
        }),
        changes,
    )
}

async fn schedule(trial: &ContractTrial, changes: Value) -> String {
    let before = state_of(trial, "google")["calendar_events"].clone();
    tool(trial, "google", "manage_event", event_arguments(changes)).await;
    state_of(trial, "google")["calendar_events"]
        .as_object()
        .unwrap()
        .keys()
        .find(|identity| before.get(identity.as_str()).is_none())
        .unwrap()
        .clone()
}

fn announcement(changes: Value) -> Value {
    merged(
        json!({"channel_id":MIGRATION_CHANNEL,"thread_ts":THREAD_ROOT,"message":WINDOW_REPLY}),
        changes,
    )
}

async fn announce(trial: &ContractTrial, changes: Value) {
    tool(trial, "slack", "slack_send_message", announcement(changes)).await;
}

async fn complete_calendar_workflow(trial: &ContractTrial) {
    schedule(trial, json!({})).await;
    announce(trial, json!({})).await;
}

#[tokio::test]
async fn reference_plans_satisfy_their_contracts_and_no_op_fails() {
    for id in [REFUND, CUTOVER, CALENDAR] {
        let attempt = trial(id);
        fail(&attempt.assess());
        replay(&attempt).await;
        pass(&attempt.assess());
    }
}

#[tokio::test]
async fn refund_accepts_split_intent_refunds_alternative_sql_and_either_closing_order() {
    let attempt = trial(REFUND);
    refund(
        &attempt,
        json!({"payment_intent":REQUESTED_INTENT,"amount":2500}),
    )
    .await;
    refund(
        &attempt,
        json!({"payment_intent":REQUESTED_INTENT,"amount":3700,"reason":"requested_by_customer"}),
    )
    .await;
    reply(
        &attempt,
        json!({"body":"We've issued a full refund of $62.00 for your desk lamp order. Sorry about the cracked base."}),
    )
    .await;
    sql(
        &attempt,
        "update public.orders set status = 'refunded' where id = 5024 and customer_id = (select id from public.customers where email = 'priya.shah@acme.io')",
    )
    .await;
    pass(&attempt.assess());
    assert_eq!(
        state_of(&attempt, "stripe")["charges"][REQUESTED_CHARGE]["amount_refunded"],
        6200
    );
    assert_eq!(
        state_of(&attempt, "stripe")["charges"][LOOKALIKE_CHARGE]["amount_refunded"],
        0
    );
}

#[tokio::test]
async fn refund_accepts_an_explicit_full_amount_and_a_status_migration() {
    let attempt = trial(REFUND);
    refund(
        &attempt,
        json!({"charge":REQUESTED_CHARGE,"amount":6200,"reason":"requested_by_customer"}),
    )
    .await;
    attempt
        .call_from(
            "supabase",
            "POST",
            "/v1/projects/qmvbxtzkplrnwdhyfaco/database/migrations",
            json!({
                "name":"mark_order_5024_refunded",
                "query":"update public.orders set status = 'refunded' where id = 5024"
            }),
        )
        .await;
    reply(&attempt, json!({})).await;
    pass(&attempt.assess());
}

#[tokio::test]
async fn refund_accepts_a_reply_all_thread_reply_without_an_explicit_recipient() {
    let attempt = trial(REFUND);
    full_refund(&attempt).await;
    mark_refunded(&attempt, 5024).await;
    reply(&attempt, json!({"to":null,"reply_all":true})).await;
    pass(&attempt.assess());
}

#[tokio::test]
async fn refund_follows_actual_customer_and_order_identities_after_renumbering() {
    let attempt = trial_with(REFUND, |dataset| {
        let supabase = dataset.services.get_mut("supabase").unwrap();
        let customers = supabase.collection_mut(CUSTOMERS);
        let mut customer = customers.remove("1107").unwrap();
        customer["id"] = json!(9107);
        customers.insert("9107", customer);
        let orders = supabase.collection_mut(ORDERS);
        for order in ["5020", "5022", "5023", "5024"] {
            orders.get_mut(order).unwrap()["customer_id"] = json!(9107);
        }
    });
    complete_refund_workflow(&attempt).await;
    pass(&attempt.assess());
}

#[tokio::test]
async fn refund_rejects_wrong_orders_charges_and_recipients() {
    let wrong_charge = trial(REFUND);
    refund(&wrong_charge, json!({"charge":LOOKALIKE_CHARGE})).await;
    mark_refunded(&wrong_charge, 5025).await;
    reply(&wrong_charge, json!({"to":"priya.shah@fastmail.com"})).await;
    fail(&wrong_charge.assess());

    let earlier_order = trial(REFUND);
    refund(&earlier_order, json!({"charge":EARLIER_CHARGE})).await;
    mark_refunded(&earlier_order, 5023).await;
    reply(&earlier_order, json!({})).await;
    fail(&earlier_order.assess());

    let wrong_status = trial(REFUND);
    full_refund(&wrong_status).await;
    mark_refunded(&wrong_status, 5022).await;
    reply(&wrong_status, json!({})).await;
    fail(&wrong_status.assess());

    let lookalike_status = trial(REFUND);
    full_refund(&lookalike_status).await;
    mark_refunded(&lookalike_status, 5024).await;
    mark_refunded(&lookalike_status, 5025).await;
    reply(&lookalike_status, json!({})).await;
    fail(&lookalike_status.assess());

    let other_recipient = trial(REFUND);
    full_refund(&other_recipient).await;
    mark_refunded(&other_recipient, 5024).await;
    reply(&other_recipient, json!({"to":"priya.shah@fastmail.com"})).await;
    fail(&other_recipient.assess());

    let wrong_amount_intent = trial(REFUND);
    refund(
        &wrong_amount_intent,
        json!({"payment_intent":"pi_Nq7VxT2kLm9RcW4bZh6YpD1s"}),
    )
    .await;
    mark_refunded(&wrong_amount_intent, 5024).await;
    reply(&wrong_amount_intent, json!({})).await;
    fail(&wrong_amount_intent.assess());
}

#[tokio::test]
async fn refund_requires_the_full_amount_status_and_threaded_reply() {
    let partial = trial(REFUND);
    refund(&partial, json!({"charge":REQUESTED_CHARGE,"amount":3100})).await;
    mark_refunded(&partial, 5024).await;
    reply(&partial, json!({})).await;
    fail(&partial.assess());

    let no_refund = trial(REFUND);
    mark_refunded(&no_refund, 5024).await;
    reply(&no_refund, json!({})).await;
    fail(&no_refund.assess());

    let no_status = trial(REFUND);
    full_refund(&no_status).await;
    reply(&no_status, json!({})).await;
    fail(&no_status.assess());

    let no_reply = trial(REFUND);
    full_refund(&no_reply).await;
    mark_refunded(&no_reply, 5024).await;
    fail(&no_reply.assess());

    let new_thread = trial(REFUND);
    full_refund(&new_thread).await;
    mark_refunded(&new_thread, 5024).await;
    reply(
        &new_thread,
        json!({"thread_id":null,"subject":"Your desk lamp refund"}),
    )
    .await;
    fail(&new_thread.assess());

    let draft_only = trial(REFUND);
    full_refund(&draft_only).await;
    mark_refunded(&draft_only, 5024).await;
    tool(
        &draft_only,
        "google",
        "draft_gmail_message",
        reply_arguments(json!({})),
    )
    .await;
    fail(&draft_only.assess());

    let duplicate_reply = trial(REFUND);
    complete_refund_workflow(&duplicate_reply).await;
    reply(&duplicate_reply, json!({})).await;
    fail(&duplicate_reply.assess());

    let extra_recipient = trial(REFUND);
    full_refund(&extra_recipient).await;
    mark_refunded(&extra_recipient, 5024).await;
    reply(&extra_recipient, json!({"cc":"priya.shah@fastmail.com"})).await;
    fail(&extra_recipient.assess());
}

#[tokio::test]
async fn refund_status_and_reply_must_follow_the_actual_refund() {
    let status_first = trial(REFUND);
    mark_refunded(&status_first, 5024).await;
    full_refund(&status_first).await;
    reply(&status_first, json!({})).await;
    fail(&status_first.assess());

    let reply_first = trial(REFUND);
    reply(&reply_first, json!({})).await;
    full_refund(&reply_first).await;
    mark_refunded(&reply_first, 5024).await;
    fail(&reply_first.assess());

    let reply_before_status = trial(REFUND);
    full_refund(&reply_before_status).await;
    reply(&reply_before_status, json!({})).await;
    mark_refunded(&reply_before_status, 5024).await;
    pass(&reply_before_status.assess());
}

#[tokio::test]
async fn refund_audits_restored_collateral_changes() {
    let restored_status = trial(REFUND);
    complete_refund_workflow(&restored_status).await;
    sql(
        &restored_status,
        "update public.orders set status = 'cancelled' where id = 5025",
    )
    .await;
    sql(
        &restored_status,
        "update public.orders set status = 'shipped' where id = 5025",
    )
    .await;
    fail(&restored_status.assess());

    let restored_total = trial(REFUND);
    complete_refund_workflow(&restored_total).await;
    sql(
        &restored_total,
        "update public.orders set total = 1.00 where id = 5024",
    )
    .await;
    sql(
        &restored_total,
        "update public.orders set total = 62.00 where id = 5024",
    )
    .await;
    fail(&restored_total.assess());

    let restored_customer = trial(REFUND);
    complete_refund_workflow(&restored_customer).await;
    sql(
        &restored_customer,
        "update public.customers set country = 'CA' where email = 'priya.shah@acme.io'",
    )
    .await;
    sql(
        &restored_customer,
        "update public.customers set country = 'US' where email = 'priya.shah@acme.io'",
    )
    .await;
    fail(&restored_customer.assess());
}

#[tokio::test]
async fn refund_message_must_confirm_the_refund_and_its_amount_in_any_wording() {
    for wording in [
        "Your refund of $6200.00 has been issued.",
        "We refunded $26.00 for your desk lamp order.",
        "We cannot refund your order.",
        "Your desk lamp order has been refunded in full ($62.00). The money will arrive within 5-10 business days.",
    ] {
        let attempt = trial(REFUND);
        full_refund(&attempt).await;
        mark_refunded(&attempt, 5024).await;
        reply(&attempt, json!({"body":wording})).await;
        fail(&attempt.assess());
    }
    for (wording, truthful) in [
        (
            "Thanks for flagging the damaged lamp; we've refunded the full $62.00 for order 5024.",
            true,
        ),
        (
            "Thanks for flagging the damaged lamp; the money for order 5024 is on its way back to you.",
            false,
        ),
        (
            "Thanks for flagging the damaged lamp; we've refunded $62.50 for order 5024.",
            false,
        ),
    ] {
        let unlisted = trial(REFUND);
        full_refund(&unlisted).await;
        mark_refunded(&unlisted, 5024).await;
        reply(&unlisted, json!({"body":wording})).await;
        if truthful {
            pass(&unlisted.assess());
        } else {
            fail(&unlisted.assess());
        }
    }

    let withdrawn = trial(REFUND);
    full_refund(&withdrawn).await;
    mark_refunded(&withdrawn, 5024).await;
    reply(
        &withdrawn,
        json!({"body":"We refunded the full $62.00 for your lamp."}),
    )
    .await;
    reply(&withdrawn, json!({"body":"We cannot refund your order."})).await;
    fail(&withdrawn.assess());
}

#[tokio::test]
async fn cutover_accepts_patch_put_and_batch_updates_in_either_closing_order() {
    let patch = trial(CUTOVER);
    patch_dns(
        &patch,
        ZONE,
        API_RECORD,
        json!({"content":NEW_ORIGIN,"proxied":true,"ttl":1}),
    )
    .await;
    move_issue(&patch, "PLAT-241", DONE).await;
    comment_on(&patch, "PLAT-241", "Changed the api.octo-corp.com A record from 198.51.100.21 to 198.51.100.45; it remains proxied.").await;
    pass(&patch.assess());

    let replacement = trial(CUTOVER);
    run_dns(
        &replacement,
        "PUT",
        &format!("/zones/{ZONE}/dns_records/{API_RECORD}"),
        json!({
            "type":"A","name":"api.octo-corp.com","content":NEW_ORIGIN,"proxied":true,"ttl":1,
            "comment":"origin-2 (new)","tags":["team:api"]
        }),
    )
    .await;
    comment_on(&replacement, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&replacement, "PLAT-241", DONE).await;
    pass(&replacement.assess());

    let batch = trial(CUTOVER);
    run_dns(
        &batch,
        "POST",
        &format!("/zones/{ZONE}/dns_records/batch"),
        json!({"patches":[{"id":API_RECORD,"content":NEW_ORIGIN}]}),
    )
    .await;
    comment_on(&batch, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&batch, "PLAT-241", DONE).await;
    pass(&batch.assess());
}

#[tokio::test]
async fn cutover_rejects_wrong_dns_entities_values_and_replacements() {
    let staging = trial(CUTOVER);
    patch_dns(
        &staging,
        STAGING_ZONE,
        STAGING_RECORD,
        json!({"content":NEW_ORIGIN}),
    )
    .await;
    comment_on(&staging, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&staging, "PLAT-241", DONE).await;
    fail(&staging.assess());

    let apex = trial(CUTOVER);
    patch_dns(&apex, ZONE, APEX_RECORD, json!({"content":NEW_ORIGIN})).await;
    comment_on(&apex, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&apex, "PLAT-241", DONE).await;
    fail(&apex.assess());

    let canceled_address = trial(CUTOVER);
    patch_dns(
        &canceled_address,
        ZONE,
        API_RECORD,
        json!({"content":CANCELED_ORIGIN}),
    )
    .await;
    comment_on(&canceled_address, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&canceled_address, "PLAT-241", DONE).await;
    fail(&canceled_address.assess());

    let second_record = trial(CUTOVER);
    run_dns(
        &second_record,
        "POST",
        &format!("/zones/{ZONE}/dns_records"),
        json!({"type":"A","name":"api.octo-corp.com","content":NEW_ORIGIN,"proxied":true}),
    )
    .await;
    comment_on(&second_record, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&second_record, "PLAT-241", DONE).await;
    fail(&second_record.assess());

    let replaced = trial(CUTOVER);
    run_dns(
        &replaced,
        "DELETE",
        &format!("/zones/{ZONE}/dns_records/{API_RECORD}"),
        Value::Null,
    )
    .await;
    run_dns(
        &replaced,
        "POST",
        &format!("/zones/{ZONE}/dns_records"),
        json!({"type":"A","name":"api.octo-corp.com","content":NEW_ORIGIN,"proxied":true}),
    )
    .await;
    comment_on(&replaced, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&replaced, "PLAT-241", DONE).await;
    fail(&replaced.assess());

    let unproxied = trial(CUTOVER);
    patch_dns(
        &unproxied,
        ZONE,
        API_RECORD,
        json!({"content":NEW_ORIGIN,"proxied":false}),
    )
    .await;
    comment_on(&unproxied, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&unproxied, "PLAT-241", DONE).await;
    fail(&unproxied.assess());

    let lossy_replacement = trial(CUTOVER);
    run_dns(
        &lossy_replacement,
        "PUT",
        &format!("/zones/{ZONE}/dns_records/{API_RECORD}"),
        json!({"type":"A","name":"api.octo-corp.com","content":NEW_ORIGIN,"proxied":true}),
    )
    .await;
    comment_on(&lossy_replacement, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&lossy_replacement, "PLAT-241", DONE).await;
    fail(&lossy_replacement.assess());

    let retimed = trial(CUTOVER);
    patch_dns(
        &retimed,
        ZONE,
        API_RECORD,
        json!({"content":NEW_ORIGIN,"ttl":300}),
    )
    .await;
    comment_on(&retimed, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&retimed, "PLAT-241", DONE).await;
    fail(&retimed.assess());
}

#[tokio::test]
async fn cutover_audits_wrong_intermediate_and_collateral_dns_states() {
    let wrong_then_right = trial(CUTOVER);
    patch_dns(
        &wrong_then_right,
        ZONE,
        API_RECORD,
        json!({"content":CANCELED_ORIGIN}),
    )
    .await;
    complete_cutover_workflow(&wrong_then_right).await;
    fail(&wrong_then_right.assess());

    let toggled_proxy = trial(CUTOVER);
    patch_dns(&toggled_proxy, ZONE, API_RECORD, json!({"proxied":false})).await;
    patch_dns(&toggled_proxy, ZONE, API_RECORD, json!({"proxied":true})).await;
    complete_cutover_workflow(&toggled_proxy).await;
    fail(&toggled_proxy.assess());

    let staging_restored = trial(CUTOVER);
    patch_dns(
        &staging_restored,
        STAGING_ZONE,
        STAGING_RECORD,
        json!({"content":NEW_ORIGIN}),
    )
    .await;
    patch_dns(
        &staging_restored,
        STAGING_ZONE,
        STAGING_RECORD,
        json!({"content":"203.0.113.12"}),
    )
    .await;
    complete_cutover_workflow(&staging_restored).await;
    fail(&staging_restored.assess());

    let annotated = trial(CUTOVER);
    patch_dns(
        &annotated,
        ZONE,
        API_RECORD,
        json!({"content":NEW_ORIGIN,"comment":"origin-3 (PLAT-241)"}),
    )
    .await;
    comment_on(&annotated, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&annotated, "PLAT-241", DONE).await;
    fail(&annotated.assess());
}

#[tokio::test]
async fn cutover_ticket_changes_are_scoped_to_the_requested_issue() {
    let canceled_ticket = trial(CUTOVER);
    move_api_record(&canceled_ticket).await;
    comment_on(&canceled_ticket, "PLAT-238", CUTOVER_COMMENT).await;
    move_issue(&canceled_ticket, "PLAT-238", DONE).await;
    fail(&canceled_ticket.assess());

    let both_tickets = trial(CUTOVER);
    complete_cutover_workflow(&both_tickets).await;
    comment_on(&both_tickets, "PLAT-238", CUTOVER_COMMENT).await;
    fail(&both_tickets.assess());

    let wrong_state = trial(CUTOVER);
    move_api_record(&wrong_state).await;
    comment_on(&wrong_state, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&wrong_state, "PLAT-241", CANCELED).await;
    fail(&wrong_state.assess());

    let in_progress = trial(CUTOVER);
    move_api_record(&in_progress).await;
    comment_on(&in_progress, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&in_progress, "PLAT-241", IN_PROGRESS).await;
    fail(&in_progress.assess());

    let reassigned = trial(CUTOVER);
    complete_cutover_workflow(&reassigned).await;
    tool(
        &reassigned,
        "linear",
        "linear_updateIssue",
        json!({"id":"PLAT-241","priority":1}),
    )
    .await;
    fail(&reassigned.assess());

    let duplicate_comment = trial(CUTOVER);
    complete_cutover_workflow(&duplicate_comment).await;
    comment_on(&duplicate_comment, "PLAT-241", CUTOVER_COMMENT).await;
    fail(&duplicate_comment.assess());
}

#[tokio::test]
async fn cutover_requires_each_action_and_dns_before_comment_and_done() {
    let no_dns = trial(CUTOVER);
    comment_on(&no_dns, "PLAT-241", CUTOVER_COMMENT).await;
    move_issue(&no_dns, "PLAT-241", DONE).await;
    fail(&no_dns.assess());

    let no_comment = trial(CUTOVER);
    move_api_record(&no_comment).await;
    move_issue(&no_comment, "PLAT-241", DONE).await;
    fail(&no_comment.assess());

    let no_done = trial(CUTOVER);
    move_api_record(&no_done).await;
    comment_on(&no_done, "PLAT-241", CUTOVER_COMMENT).await;
    fail(&no_done.assess());

    let comment_first = trial(CUTOVER);
    comment_on(&comment_first, "PLAT-241", CUTOVER_COMMENT).await;
    move_api_record(&comment_first).await;
    move_issue(&comment_first, "PLAT-241", DONE).await;
    fail(&comment_first.assess());

    let done_first = trial(CUTOVER);
    move_issue(&done_first, "PLAT-241", DONE).await;
    move_api_record(&done_first).await;
    comment_on(&done_first, "PLAT-241", CUTOVER_COMMENT).await;
    fail(&done_first.assess());
}

#[tokio::test]
async fn cutover_comment_must_name_the_record_and_its_new_address_in_any_wording() {
    for wording in [
        "Moved api.octo-corp.com to 198.51.100.44.",
        "Created a new A record for api.octo-corp.com at 198.51.100.45.",
        "I could not make the DNS change.",
    ] {
        let attempt = trial(CUTOVER);
        move_api_record(&attempt).await;
        comment_on(&attempt, "PLAT-241", wording).await;
        move_issue(&attempt, "PLAT-241", DONE).await;
        fail(&attempt.assess());
    }
    for (wording, truthful) in [
        (
            "Pointed the api.octo-corp.com A record at 198.51.100.45 (origin-3); it is still proxied.",
            true,
        ),
        (
            "Cut api.octo-corp.com over to the new origin as requested.",
            false,
        ),
        (
            "Pointed the api.octo-corp.dev A record at 198.51.100.45.",
            false,
        ),
    ] {
        let unlisted = trial(CUTOVER);
        move_api_record(&unlisted).await;
        comment_on(&unlisted, "PLAT-241", wording).await;
        move_issue(&unlisted, "PLAT-241", DONE).await;
        if truthful {
            pass(&unlisted.assess());
        } else {
            fail(&unlisted.assess());
        }
    }
}

#[tokio::test]
async fn calendar_accepts_equivalent_instants_staged_guests_and_reply_wording() {
    let utc = trial(CALENDAR);
    schedule(
        &utc,
        json!({"start_time":"2026-11-08T04:00:00Z","end_time":"2026-11-08T07:00:00Z","timezone":null}),
    )
    .await;
    announce(
        &utc,
        json!({"message":"Done - the cutover is on the calendar."}),
    )
    .await;
    pass(&utc.assess());

    let offsets = trial(CALENDAR);
    schedule(
        &offsets,
        json!({"start_time":"2026-11-07T22:00:00-06:00","end_time":"2026-11-08T01:00:00-06:00","timezone":null}),
    )
    .await;
    announce(&offsets, json!({})).await;
    pass(&offsets.assess());

    let silent = trial(CALENDAR);
    schedule(&silent, json!({"send_updates":"none"})).await;
    announce(&silent, json!({})).await;
    pass(&silent.assess());

    let guest_case = trial(CALENDAR);
    schedule(
        &guest_case,
        json!({"attendees":["SAM.ORTIZ@OCTO-CORP.EXAMPLE","PRIYA.NATARAJAN@OCTO-CORP.EXAMPLE"]}),
    )
    .await;
    announce(&guest_case, json!({})).await;
    pass(&guest_case.assess());

    let staged = trial(CALENDAR);
    let identity = schedule(&staged, json!({"attendees":[SAM]})).await;
    tool(
        &staged,
        "google",
        "manage_event",
        json!({"action":"update","calendar_id":OPS_TEAM,"event_id":identity,"attendees":[SAM,PRIYA]}),
    )
    .await;
    announce(&staged, json!({})).await;
    pass(&staged.assess());
}

#[tokio::test]
async fn calendar_requires_the_agreed_window_calendar_title_and_exact_guests() {
    let cases = [
        ("primary", json!({"calendar_id":"ops@octo-corp.example"})),
        (
            "proposal",
            json!({"start_time":"2026-11-06T20:00:00","end_time":"2026-11-06T23:00:00"}),
        ),
        (
            "daylight offset",
            json!({"start_time":"2026-11-07T22:00:00-05:00","end_time":"2026-11-08T01:00:00-05:00","timezone":null}),
        ),
        (
            "wrong title",
            json!({"summary":"Payments cluster PG16 staging cutover"}),
        ),
        ("shortened", json!({"end_time":"2026-11-08T00:00:00"})),
        ("missing priya", json!({"attendees":[SAM]})),
        ("extra guest", json!({"attendees":[SAM,PRIYA,MARCUS]})),
        ("wrong guest", json!({"attendees":[SAM,MARCUS]})),
        ("no guests", json!({"attendees":null})),
    ];
    for (name, changes) in cases {
        let attempt = trial(CALENDAR);
        schedule(&attempt, changes).await;
        announce(&attempt, json!({})).await;
        let grade = attempt.assess();
        assert_eq!(grade.outcome, Outcome::Fail, "{name}: {grade:?}");
    }
}

#[tokio::test]
async fn calendar_rejects_duplicate_recurring_and_collateral_events() {
    let duplicate = trial(CALENDAR);
    complete_calendar_workflow(&duplicate).await;
    schedule(&duplicate, json!({})).await;
    fail(&duplicate.assess());

    let recurring = trial(CALENDAR);
    schedule(
        &recurring,
        json!({"recurrence":["RRULE:FREQ=WEEKLY;COUNT=2"]}),
    )
    .await;
    announce(&recurring, json!({})).await;
    fail(&recurring.assess());

    let conference = trial(CALENDAR);
    schedule(&conference, json!({"add_google_meet":true})).await;
    announce(&conference, json!({})).await;
    pass(&conference.assess());

    let moved_rehearsal = trial(CALENDAR);
    complete_calendar_workflow(&moved_rehearsal).await;
    tool(
        &moved_rehearsal,
        "google",
        "manage_event",
        json!({
            "action":"update","calendar_id":OPS_TEAM,"event_id":STAGING_REHEARSAL,
            "summary":"Payments cluster PG16 production cutover"
        }),
    )
    .await;
    fail(&moved_rehearsal.assess());

    let restored_rehearsal = trial(CALENDAR);
    complete_calendar_workflow(&restored_rehearsal).await;
    for summary in ["Moved rehearsal", "Payments cluster PG16 staging cutover"] {
        tool(
            &restored_rehearsal,
            "google",
            "manage_event",
            json!({
                "action":"update","calendar_id":OPS_TEAM,"event_id":STAGING_REHEARSAL,"summary":summary
            }),
        )
        .await;
    }
    fail(&restored_rehearsal.assess());

    let deleted_rehearsal = trial(CALENDAR);
    complete_calendar_workflow(&deleted_rehearsal).await;
    tool(
        &deleted_rehearsal,
        "google",
        "manage_event",
        json!({"action":"delete","calendar_id":OPS_TEAM,"event_id":STAGING_REHEARSAL}),
    )
    .await;
    fail(&deleted_rehearsal.assess());
}

#[tokio::test]
async fn calendar_notice_requires_the_exact_complete_event_when_the_notice_commits() {
    let premature = trial(CALENDAR);
    let identity = schedule(
        &premature,
        json!({"start_time":"2026-11-08T03:00:00Z","end_time":"2026-11-08T06:00:00Z","timezone":null}),
    )
    .await;
    announce(&premature, json!({})).await;
    tool(
        &premature,
        "google",
        "manage_event",
        json!({
            "action":"update","calendar_id":OPS_TEAM,"event_id":identity,
            "start_time":"2026-11-08T04:00:00Z","end_time":"2026-11-08T07:00:00Z",
            "timezone":null
        }),
    )
    .await;
    let grade = premature.assess();
    fail(&grade);
    assert!(!grade.ordering_ok, "{grade:?}");
}

#[tokio::test]
async fn calendar_reply_must_be_one_truthful_message_in_the_requested_thread() {
    let top_level = trial(CALENDAR);
    schedule(&top_level, json!({})).await;
    announce(&top_level, json!({"thread_ts":null})).await;
    fail(&top_level.assess());

    let general = trial(CALENDAR);
    schedule(&general, json!({})).await;
    announce(
        &general,
        json!({"channel_id":"C01GENERAL0","thread_ts":null}),
    )
    .await;
    fail(&general.assess());

    let other_thread = trial(CALENDAR);
    schedule(&other_thread, json!({})).await;
    announce(&other_thread, json!({"thread_ts":"1792422000.012010"})).await;
    fail(&other_thread.assess());

    let early = trial(CALENDAR);
    announce(&early, json!({})).await;
    schedule(&early, json!({})).await;
    fail(&early.assess());

    let before_guests = trial(CALENDAR);
    let identity = schedule(&before_guests, json!({"attendees":[SAM]})).await;
    announce(&before_guests, json!({})).await;
    tool(
        &before_guests,
        "google",
        "manage_event",
        json!({"action":"update","calendar_id":OPS_TEAM,"event_id":identity,"attendees":[SAM,PRIYA]}),
    )
    .await;
    fail(&before_guests.assess());

    let silent = trial(CALENDAR);
    schedule(&silent, json!({})).await;
    fail(&silent.assess());

    let unscheduled = trial(CALENDAR);
    announce(&unscheduled, json!({})).await;
    fail(&unscheduled.assess());

    let repeated = trial(CALENDAR);
    complete_calendar_workflow(&repeated).await;
    announce(&repeated, json!({})).await;
    fail(&repeated.assess());

    for wording in [
        "It's on the Ops team calendar for Friday 2026-11-06, 20:00-23:00 CT.",
        "I could not add the cutover to the calendar.",
    ] {
        let attempt = trial(CALENDAR);
        schedule(&attempt, json!({})).await;
        announce(&attempt, json!({"message":wording})).await;
        fail(&attempt.assess());
    }

    let unlisted = trial(CALENDAR);
    schedule(&unlisted, json!({})).await;
    announce(
        &unlisted,
        json!({"message":"Calendar hold added for the PG16 switchover."}),
    )
    .await;
    pass(&unlisted.assess());

    let off_topic = trial(CALENDAR);
    schedule(&off_topic, json!({})).await;
    announce(
        &off_topic,
        json!({"message":"Sounds good, see everyone Saturday night."}),
    )
    .await;
    fail(&off_topic.assess());
}

#[tokio::test]
async fn invalid_tool_requests_do_not_become_business_outcomes() {
    let attempt = trial(CALENDAR);
    rejected_tool(
        &attempt,
        "google",
        "manage_event",
        event_arguments(json!({"calendar_id":"missing@group.calendar.google.com"})),
    )
    .await;
    fail(&attempt.assess());
}
