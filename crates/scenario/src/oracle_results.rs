use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, bail};
use catalog::Catalog;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::SolutionCall;

const TERRAFORM_REVIEWED_TOOLS: &[&str] = &[
    "list_runs",
    "action_run",
    "get_run_details",
    "list_workspace_variables",
    "update_workspace_variable",
    "create_run",
    "list_terraform_projects",
    "list_workspaces",
    "force_unlock_workspace",
];

const GRAFANA_REVIEWED_TOOLS: &[&str] = &[
    "grafana_api_request",
    "search_dashboards",
    "create_annotation",
    "get_annotations",
    "list_alert_groups",
    "get_alert_group",
    "update_alert_group",
    "list_oncall_teams",
    "list_oncall_schedules",
    "list_oncall_users",
    "get_current_oncall_users",
    "alerting_silences_write",
    "alerting_silences_read",
];

const PAGERDUTY_REVIEWED_TOOLS: &[&str] = &[
    "manage_incidents",
    "add_note_to_incident",
    "create_schedule_override",
];

const DATADOG_REVIEWED_TOOLS: &[&str] = &["schedule_downtime", "cancel_downtime", "mute_host"];

const GOOGLE_REVIEWED_TOOLS: &[&str] = &[
    "modify_sheet_values",
    "manage_event",
    "manage_drive_access",
    "get_drive_file_permissions",
    "get_drive_shareable_link",
];

const FIRECRAWL_REVIEWED_TOOLS: &[&str] = &[
    "firecrawl_map",
    "firecrawl_crawl",
    "firecrawl_check_crawl_status",
    "firecrawl_monitor_create",
    "firecrawl_monitor_list",
    "firecrawl_monitor_get",
    "firecrawl_monitor_update",
    "firecrawl_monitor_run",
    "firecrawl_monitor_checks",
    "firecrawl_monitor_check",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultContract {
    McpEnvelopeV1,
    AwsApiV1,
    ArgoApiV1,
    OktaApiV1,
    TerraformApiV1,
    GrafanaApiV1,
    PagerdutyApiV1,
    DatadogApiV1,
    GoogleWorkspaceReplicaV1,
    FirecrawlNativeV1,
}

impl ResultContract {
    pub fn for_tool(endpoint: &str, tool: &str) -> Self {
        match (endpoint, tool) {
            ("firecrawl", tool) if FIRECRAWL_REVIEWED_TOOLS.contains(&tool) => {
                Self::FirecrawlNativeV1
            }
            ("aws-api", "call_aws") => Self::AwsApiV1,
            ("argocd", _) => Self::ArgoApiV1,
            ("okta", _) => Self::OktaApiV1,
            ("terraform", tool) if TERRAFORM_REVIEWED_TOOLS.contains(&tool) => Self::TerraformApiV1,
            ("grafana", tool) if GRAFANA_REVIEWED_TOOLS.contains(&tool) => Self::GrafanaApiV1,
            ("pagerduty", tool) if PAGERDUTY_REVIEWED_TOOLS.contains(&tool) => Self::PagerdutyApiV1,
            ("datadog", tool) if DATADOG_REVIEWED_TOOLS.contains(&tool) => Self::DatadogApiV1,
            ("google-workspace", tool) if GOOGLE_REVIEWED_TOOLS.contains(&tool) => {
                Self::GoogleWorkspaceReplicaV1
            }
            _ => Self::McpEnvelopeV1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedApiError {
    pub command: String,
    pub status: u16,
    pub code: String,
}

pub fn validate(call: &SolutionCall) -> anyhow::Result<()> {
    if call.allow_api_errors.is_empty() {
        return Ok(());
    }
    if ResultContract::for_tool(&call.endpoint, &call.tool) != ResultContract::AwsApiV1 {
        bail!("API error permissions require the reviewed aws-api.call_aws result contract");
    }
    let commands = match &call.arguments["cli_command"] {
        Value::String(command) => vec![command.as_str()],
        Value::Array(commands) => commands
            .iter()
            .map(|command| command.as_str().context("AWS commands must be strings"))
            .collect::<anyhow::Result<Vec<_>>>()?,
        _ => bail!("API error permissions require statically identified CLI commands"),
    };
    let mut distinct = BTreeSet::new();
    for permission in &call.allow_api_errors {
        if permission.command.trim().is_empty()
            || permission.command.contains("${")
            || !commands.contains(&permission.command.as_str())
            || !(400..600).contains(&permission.status)
            || permission.code.trim().is_empty()
            || permission.code.len() > 256
            || !distinct.insert(permission)
        {
            bail!(
                "API error permissions need distinct exact static commands, HTTP error statuses and nonempty SDK error codes"
            );
        }
    }
    Ok(())
}

pub fn registry(catalog: &Catalog, calls: &[SolutionCall]) -> anyhow::Result<Value> {
    catalog.validate()?;
    let profiles: BTreeMap<_, BTreeMap<&str, ResultContract>> = catalog
        .services
        .iter()
        .flat_map(|service| &service.endpoints)
        .map(|endpoint| {
            let tools = if endpoint.id == "aws-api" {
                BTreeMap::from([(
                    "call_aws",
                    ResultContract::for_tool(&endpoint.id, "call_aws"),
                )])
            } else if endpoint.id == "terraform" {
                TERRAFORM_REVIEWED_TOOLS
                    .iter()
                    .map(|tool| (*tool, ResultContract::TerraformApiV1))
                    .collect()
            } else if endpoint.id == "grafana" {
                GRAFANA_REVIEWED_TOOLS
                    .iter()
                    .map(|tool| (*tool, ResultContract::GrafanaApiV1))
                    .collect()
            } else if endpoint.id == "pagerduty" {
                PAGERDUTY_REVIEWED_TOOLS
                    .iter()
                    .map(|tool| (*tool, ResultContract::PagerdutyApiV1))
                    .collect()
            } else if endpoint.id == "datadog" {
                DATADOG_REVIEWED_TOOLS
                    .iter()
                    .map(|tool| (*tool, ResultContract::DatadogApiV1))
                    .collect()
            } else if endpoint.id == "google-workspace" {
                GOOGLE_REVIEWED_TOOLS
                    .iter()
                    .map(|tool| (*tool, ResultContract::GoogleWorkspaceReplicaV1))
                    .collect()
            } else if endpoint.id == "firecrawl" {
                FIRECRAWL_REVIEWED_TOOLS
                    .iter()
                    .map(|tool| (*tool, ResultContract::FirecrawlNativeV1))
                    .collect()
            } else {
                BTreeMap::new()
            };
            (endpoint.mcp_port.to_string(), tools)
        })
        .collect();
    for call in calls {
        validate(call)?;
    }
    let errors: BTreeSet<_> = calls
        .iter()
        .flat_map(|call| &call.allow_api_errors)
        .collect();
    let defaults: BTreeMap<_, _> = catalog
        .services
        .iter()
        .flat_map(|service| &service.endpoints)
        .filter(|endpoint| matches!(endpoint.id.as_str(), "argocd" | "okta"))
        .map(|endpoint| {
            (
                endpoint.mcp_port.to_string(),
                ResultContract::for_tool(&endpoint.id, ""),
            )
        })
        .collect();
    Ok(json!({
        "schema_version":1,
        "protocol":"oracle-tool-results-v1",
        "profiles":profiles,
        "default_profiles":defaults,
        "allowed_aws_api_errors":errors
    }))
}
