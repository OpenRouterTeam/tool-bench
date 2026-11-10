use mock_core::{mcp::ToolCall, world::ServiceState};
use serde_json::Value;

pub(super) fn unavailable(call: &ToolCall, state: Option<&ServiceState>) -> Option<&'static str> {
    if call.name != "manage_drive_access" {
        return None;
    }
    unavailable_options(&call.arguments).or_else(|| repeated_grants(&call.arguments, state))
}

fn unavailable_options(arguments: &Value) -> Option<&'static str> {
    let action = arguments["action"].as_str().unwrap_or_default();
    if (matches!(action, "grant" | "update")
        && arguments
            .get("expiration_time")
            .is_some_and(|value| !value.is_null()))
        || (action == "grant_batch"
            && arguments["recipients"]
                .as_array()
                .is_some_and(|recipients| {
                    recipients.iter().any(|recipient| {
                        recipient
                            .get("expiration_time")
                            .is_some_and(|value| !value.is_null())
                    })
                }))
    {
        return Some("Drive permission expiration has no qualified local lifecycle");
    }
    if matches!(action, "grant" | "grant_batch")
        && arguments["send_notification"].as_bool() != Some(false)
        && arguments["email_message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    {
        return Some("Drive custom notification content has no qualified local delivery evidence");
    }
    None
}

fn repeated_grants(arguments: &Value, state: Option<&ServiceState>) -> Option<&'static str> {
    let file = state?
        .store
        .collection("drive_files")?
        .get(arguments["file_id"].as_str()?)?;
    let requested: Vec<(&str, Option<&str>)> = match arguments["action"].as_str() {
        Some("grant") => vec![(
            arguments["share_type"].as_str().unwrap_or("user"),
            arguments["share_with"].as_str(),
        )],
        Some("grant_batch") => arguments["recipients"]
            .as_array()?
            .iter()
            .map(|recipient| {
                (
                    recipient["share_type"].as_str().unwrap_or("user"),
                    recipient["email"]
                        .as_str()
                        .or_else(|| recipient["domain"].as_str()),
                )
            })
            .collect(),
        _ => return None,
    };
    let mut existing: Vec<(&str, Option<&str>)> = file["permissions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|permission| {
            (
                permission["type"].as_str().unwrap_or_default(),
                permission["emailAddress"]
                    .as_str()
                    .or_else(|| permission["domain"].as_str()),
            )
        })
        .collect();
    for (kind, principal) in requested {
        if existing.iter().any(|(existing_kind, existing_principal)| {
            kind == *existing_kind
                && (kind == "anyone"
                    || principal
                        .zip(*existing_principal)
                        .is_some_and(|(requested, existing)| {
                            requested.eq_ignore_ascii_case(existing)
                        }))
        }) {
            return Some("Drive repeated permission-grant semantics are not qualified locally");
        }
        existing.push((kind, principal));
    }
    None
}
