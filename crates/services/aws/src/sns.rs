//! SNS (query protocol), read side. The account keeps no topic documents: the topics that
//! exist are the ones its CloudWatch alarms already notify, so `ListTopics` and
//! `GetTopicAttributes` answer for those. Every other operation keeps the generic empty
//! success.

use std::collections::BTreeSet;

use axum::{http::StatusCode, response::Response};
use serde_json::Value;

use crate::{
    Ctx, Req, query,
    util::{Params, el, tel},
};

const NS: &str = "http://sns.amazonaws.com/doc/2010-03-31/";
const ACTION_FIELDS: [&str; 3] = ["AlarmActions", "OKActions", "InsufficientDataActions"];

/// Topic ARNs of this account and the request's region that an alarm action delivers to.
fn topics(ctx: &Ctx) -> BTreeSet<String> {
    let prefix = format!("arn:aws:sns:{}:{}:", ctx.region, ctx.account);
    ctx.docs("alarms")
        .iter()
        .flat_map(|(_, alarm)| {
            ACTION_FIELDS
                .iter()
                .filter_map(|field| alarm.get(*field).and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str)
                .filter(|arn| arn.starts_with(&prefix))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn attribute(key: &str, value: &str) -> String {
    el(
        "entry",
        &format!("{}{}", tel("key", key), tel("value", value)),
    )
}

/// `None` leaves the operation to the generic empty success.
pub fn handle(ctx: &mut Ctx, req: &Req) -> Option<Response> {
    let p = Params::parse(&req.body_text());
    let action = p.str("Action");
    let body = match action.as_str() {
        "ListTopics" => el(
            "Topics",
            &topics(ctx)
                .iter()
                .map(|arn| el("member", &tel("TopicArn", arn)))
                .collect::<String>(),
        ),
        "GetTopicAttributes" => {
            let arn = p.str("TopicArn");
            if !topics(ctx).contains(&arn) {
                let rid = ctx.request_id();
                return Some(query::error(
                    NS,
                    StatusCode::NOT_FOUND,
                    "NotFound",
                    "Topic does not exist",
                    &rid,
                ));
            }
            el(
                "Attributes",
                &format!(
                    "{}{}{}",
                    attribute("TopicArn", &arn),
                    attribute("Owner", &ctx.account),
                    attribute("DisplayName", "")
                ),
            )
        }
        _ => return None,
    };
    let rid = ctx.request_id();
    Some(query::ok(NS, &action, Some(body), &rid))
}
