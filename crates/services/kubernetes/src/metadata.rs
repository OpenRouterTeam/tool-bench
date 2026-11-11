use axum::{http::StatusCode, response::Response};
use serde_json::Value;

use crate::{bad_request, conflict, invalid, registry::ResType, status_response};

#[derive(Clone, Copy)]
pub(super) enum UpdateMode {
    ReplaceOrScale,
    ObjectPatch,
}

pub(super) fn check(
    resource: &ResType,
    name: &str,
    current: &Value,
    candidate: &Value,
    mode: UpdateMode,
) -> Option<Response> {
    for field in ["uid", "resourceVersion"] {
        if let Some(value) = candidate["metadata"].get(field)
            && !value.is_null()
            && !value.is_string()
        {
            return Some(bad_request(&format!("metadata.{field} must be a string")));
        }
    }
    let uid_failure = || {
        let requested = candidate["metadata"]["uid"]
            .as_str()
            .filter(|value| !value.is_empty())?;
        if current["metadata"]["uid"].as_str() == Some(requested) {
            return None;
        }
        Some(match mode {
            UpdateMode::ReplaceOrScale => conflict(resource, name),
            UpdateMode::ObjectPatch => invalid(
                resource,
                name,
                &[("FieldValueInvalid", "metadata.uid", "field is immutable")],
            ),
        })
    };
    let version_failure = || {
        let requested = candidate["metadata"]["resourceVersion"]
            .as_str()
            .filter(|value| !value.is_empty())?;
        let parsed = requested
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| requested.parse::<u64>().ok())
            .flatten();
        let Some(parsed) = parsed else {
            return Some(status_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                "metadata.resourceVersion is not a valid unsigned integer",
                None,
            ));
        };
        if parsed == 0 {
            return None;
        }
        let stored = current["metadata"]["resourceVersion"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok());
        if stored == Some(parsed) {
            None
        } else {
            Some(conflict(resource, name))
        }
    };
    match mode {
        UpdateMode::ReplaceOrScale => uid_failure().or_else(version_failure),
        UpdateMode::ObjectPatch => version_failure().or_else(uid_failure),
    }
}
