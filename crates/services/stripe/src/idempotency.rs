//! Stripe Idempotency-Key support.
//!
//! When a write request includes an `Idempotency-Key` header, the first successful result
//! is cached.  A replay with the same key returns the cached result.  A replay with different
//! parameters is rejected (400, `idempotency_key_in_use`).
//!
//! The cache lives in the service's session store under the `_idempotency` collection,
//! keyed by the idempotency key value.

use serde_json::{Value, json};

use crate::error::{Res, StripeError};
use mock_core::world::WorldState;

const COLLECTION: &str = "_idempotency";

/// Check for a cached idempotent result before executing.
/// Returns `Some(Ok(cached_response))` if replaying, `Some(Err(_))` if params mismatch,
/// `None` if this is a new key.
pub fn check(world: &WorldState, key: &str, params_fingerprint: &str) -> Option<Res<Value>> {
    let entry = world
        .service(crate::NAME)?
        .session
        .collection(COLLECTION)?
        .get(key)?
        .clone();
    let stored = entry["params_fingerprint"].as_str().unwrap_or_default();
    if stored != params_fingerprint {
        return Some(Err(
            StripeError::invalid(format!(
                "Keys for idempotent requests can only be used with the same parameters they were first used with. Try using a key other than '{key}' if you meant to execute a different request."
            ))
            .code("idempotency_key_in_use")
            .status(400),
        ));
    }
    Some(Ok(entry["response"].clone()))
}

/// Store the result of a successful idempotent write.
pub fn store(
    world: &mut WorldState,
    key: &str,
    operation_id: &str,
    params_fingerprint: &str,
    response: &Value,
) {
    let entry = json!({
        "key": key,
        "operation_id": operation_id,
        "params_fingerprint": params_fingerprint,
        "response": response,
    });
    world
        .service_mut(crate::NAME)
        .session
        .collection_mut(COLLECTION)
        .insert(key.to_owned(), entry);
}

/// Deterministic fingerprint: canonical JSON of the operation + params.
pub fn fingerprint(operation_id: &str, args: &crate::api::Args, params: &Value) -> String {
    mock_core::provenance::canonical_sha256(&json!({
        "operation": operation_id,
        "path_arguments": args.0,
        "parameters": normalize(params),
    }))
}

fn normalize(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), normalize(value)))
                .collect(),
        ),
        Value::Array(array) => Value::Array(array.iter().map(normalize).collect()),
        Value::Bool(value) => Value::String(value.to_string()),
        Value::Number(value) => Value::String(value.to_string()),
        other => other.clone(),
    }
}
