use mock_core::world::ServiceState;
use serde_json::{Value, json};

use crate::{Ctx, util::s};

pub(crate) fn initialize(state: &mut ServiceState, _now: i64) {
    for collection in ["alarms", "alarm_history"] {
        let keys = state
            .store
            .collection(collection)
            .map(|records| {
                records
                    .iter()
                    .map(|(key, _)| key.to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for key in keys {
            let record = state
                .store
                .collection_mut(collection)
                .get_mut(&key)
                .unwrap();
            if record.get("_region").is_none() {
                let region = record
                    .get("AlarmArn")
                    .and_then(Value::as_str)
                    .and_then(|arn| arn.split(':').nth(3))
                    .filter(|region| !region.is_empty())
                    .unwrap_or("us-east-1")
                    .to_owned();
                record["_region"] = json!(region);
            }
        }
    }
}

fn key(ctx: &Ctx, name: &str) -> Option<String> {
    ctx.docs("alarms")
        .into_iter()
        .find(|(_, alarm)| {
            ctx.in_region(alarm)
                && s(alarm, "AlarmName") == name
                && alarm
                    .get("AlarmArn")
                    .is_none_or(|arn| arn == super::alarm_arn(ctx, name).as_str())
        })
        .map(|(key, _)| key)
}

pub(super) fn get(ctx: &Ctx, name: &str) -> Option<Value> {
    key(ctx, name).and_then(|key| ctx.get("alarms", &key))
}

pub(super) fn put(ctx: &mut Ctx, name: &str, mut alarm: Value) {
    let key = key(ctx, name).unwrap_or_else(|| {
        let mut candidate = name.to_owned();
        let mut suffix = 0_u64;
        while ctx.get("alarms", &candidate).is_some() {
            candidate = format!("{}/{name}#{suffix}", ctx.region);
            suffix += 1;
        }
        candidate
    });
    alarm["_region"] = json!(ctx.region);
    ctx.put("alarms", &key, alarm);
}

pub(super) fn remove(ctx: &mut Ctx, name: &str) -> Option<Value> {
    key(ctx, name).and_then(|key| ctx.remove("alarms", &key))
}

pub(super) fn get_mut<'ctx>(ctx: &'ctx mut Ctx, name: &str) -> Option<&'ctx mut Value> {
    let key = key(ctx, name)?;
    ctx.get_mut("alarms", &key)
}

pub(super) fn add_history(ctx: &mut Ctx, name: &str, history: Value) {
    loop {
        let sequence = ctx.svc.ids.next_int("alarm-history", 1);
        let key = format!("{name}#{sequence}");
        if ctx.get("alarm_history", &key).is_none() {
            ctx.put("alarm_history", &key, history);
            return;
        }
    }
}
