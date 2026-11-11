use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const SCHEDULE_VERSION: &str = "fault-schedule-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultRule {
    pub service: String,
    pub operation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    pub fault: FaultKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FaultKind {
    TransientError { status: u16, occurrences: Vec<u32> },
    ResponseLost { occurrence: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "ScheduleFile")]
pub struct FaultSchedule {
    version: String,
    rules: Vec<FaultRule>,
    #[serde(skip)]
    counters: BTreeMap<(String, String, Option<String>), u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleFile {
    version: String,
    rules: Vec<FaultRule>,
}

impl TryFrom<ScheduleFile> for FaultSchedule {
    type Error = FaultScheduleError;

    fn try_from(file: ScheduleFile) -> Result<Self, Self::Error> {
        let schedule = Self {
            version: file.version,
            rules: file.rules,
            counters: BTreeMap::new(),
        };
        schedule.validate()?;
        Ok(schedule)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultDecision {
    None,
    TransientError { status: u16 },
    ResponseLost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaultRecord {
    pub service: String,
    pub operation_id: String,
    pub resource: Option<String>,
    pub occurrence: u32,
    pub decision: FaultDecisionKind,
    pub request_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultDecisionKind {
    None,
    TransientError { status: u16 },
    ResponseLost,
}

pub static SUPPORTED_OPERATIONS: &[(&str, &str)] = &[
    ("stripe", "GetCharges"),
    ("stripe", "GetChargesCharge"),
    ("stripe", "GetRefunds"),
    ("stripe", "GetRefundsRefund"),
    ("stripe", "GetCustomers"),
    ("stripe", "GetCustomersCustomer"),
    ("stripe", "PostRefunds"),
    ("stripe", "PostCustomers"),
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FaultScheduleError {
    #[error("unknown schedule version {0:?} (expected {SCHEDULE_VERSION:?})")]
    UnknownVersion(String),
    #[error("unsupported fault operation: service={service:?} operation_id={operation_id:?}")]
    UnsupportedOperation {
        service: String,
        operation_id: String,
    },
    #[error("occurrence numbers must be positive")]
    ZeroOccurrence,
    #[error("transient status must be 429 or a server error")]
    InvalidStatus,
    #[error("occurrences must be nonempty and unique")]
    InvalidOccurrences,
    #[error("resource selectors must be nonempty")]
    EmptyResource,
    #[error("this operation has no stable pre-execution resource selector")]
    UnsupportedResourceSelector,
    #[error("fault rules overlap on an operation, resource and occurrence")]
    ConflictingRules,
    #[error("response loss requires a supported write operation")]
    ResponseLossOnRead,
    #[error("fault schedules must be configured before trial activity")]
    TrialAlreadyStarted,
}

impl FaultSchedule {
    pub fn new(rules: Vec<FaultRule>) -> Result<Self, FaultScheduleError> {
        ScheduleFile {
            version: SCHEDULE_VERSION.into(),
            rules,
        }
        .try_into()
    }

    pub fn validate(&self) -> Result<(), FaultScheduleError> {
        if self.version != SCHEDULE_VERSION {
            return Err(FaultScheduleError::UnknownVersion(self.version.clone()));
        }
        for (index, rule) in self.rules.iter().enumerate() {
            if !SUPPORTED_OPERATIONS.contains(&(rule.service.as_str(), rule.operation_id.as_str()))
            {
                return Err(FaultScheduleError::UnsupportedOperation {
                    service: rule.service.clone(),
                    operation_id: rule.operation_id.clone(),
                });
            }
            if rule
                .resource
                .as_ref()
                .is_some_and(|resource| resource.trim().is_empty())
            {
                return Err(FaultScheduleError::EmptyResource);
            }
            if rule.resource.is_some()
                && !matches!(
                    rule.operation_id.as_str(),
                    "GetChargesCharge"
                        | "GetRefundsRefund"
                        | "GetCustomersCustomer"
                        | "PostRefunds"
                )
            {
                return Err(FaultScheduleError::UnsupportedResourceSelector);
            }
            let selected = occurrences(&rule.fault);
            if selected.contains(&0) {
                return Err(FaultScheduleError::ZeroOccurrence);
            }
            match &rule.fault {
                FaultKind::TransientError {
                    status,
                    occurrences,
                } => {
                    if *status != 429 && !(500..600).contains(status) {
                        return Err(FaultScheduleError::InvalidStatus);
                    }
                    if occurrences.is_empty()
                        || occurrences.iter().collect::<BTreeSet<_>>().len() != occurrences.len()
                    {
                        return Err(FaultScheduleError::InvalidOccurrences);
                    }
                }
                FaultKind::ResponseLost { .. } if !rule.operation_id.starts_with("Post") => {
                    return Err(FaultScheduleError::ResponseLossOnRead);
                }
                _ => {}
            }
            for other in &self.rules[..index] {
                if rule.service == other.service
                    && rule.operation_id == other.operation_id
                    && (rule.resource.is_none()
                        || other.resource.is_none()
                        || rule.resource == other.resource)
                    && selected
                        .iter()
                        .any(|number| occurrences(&other.fault).contains(number))
                {
                    return Err(FaultScheduleError::ConflictingRules);
                }
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        self.counters.clear();
    }

    pub fn check(
        &mut self,
        service: &str,
        operation_id: &str,
        resource: Option<&str>,
    ) -> (FaultDecision, FaultRecord) {
        let key = (
            service.to_owned(),
            operation_id.to_owned(),
            resource.map(str::to_owned),
        );
        let counter = self.counters.entry(key).or_insert(0);
        *counter = counter.saturating_add(1);
        let occurrence = *counter;
        let mut decision = FaultDecision::None;
        for rule in &self.rules {
            if rule.service != service
                || rule.operation_id != operation_id
                || rule
                    .resource
                    .as_deref()
                    .is_some_and(|required| Some(required) != resource)
            {
                continue;
            }
            match &rule.fault {
                FaultKind::TransientError {
                    status,
                    occurrences,
                } if occurrences.contains(&occurrence) => {
                    decision = FaultDecision::TransientError { status: *status };
                    break;
                }
                FaultKind::ResponseLost { occurrence: target } if *target == occurrence => {
                    decision = FaultDecision::ResponseLost;
                    break;
                }
                _ => {}
            }
        }
        let record = FaultRecord {
            service: service.into(),
            operation_id: operation_id.into(),
            resource: resource.map(str::to_owned),
            occurrence,
            request_id: crate::events::current().map(|operation| operation.id),
            decision: match decision {
                FaultDecision::None => FaultDecisionKind::None,
                FaultDecision::TransientError { status } => {
                    FaultDecisionKind::TransientError { status }
                }
                FaultDecision::ResponseLost => FaultDecisionKind::ResponseLost,
            },
        };
        (decision, record)
    }

    pub fn rules(&self) -> &[FaultRule] {
        &self.rules
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

fn occurrences(fault: &FaultKind) -> Vec<u32> {
    match fault {
        FaultKind::TransientError { occurrences, .. } => occurrences.clone(),
        FaultKind::ResponseLost { occurrence } => vec![*occurrence],
    }
}

#[derive(Clone, Copy)]
pub struct ResponseDeliveryLost;

pub fn failed_response_body() -> axum::body::Body {
    axum::body::Body::from_stream(tokio_stream::once(Err::<axum::body::Bytes, _>(
        std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "response delivery failed",
        ),
    )))
}

pub fn validate_evidence(
    initial: &serde_json::Value,
    state: &serde_json::Value,
    records: &[FaultRecord],
    journal: &[crate::journal::JournalEntry],
    events: &[crate::events::OperationEvent],
) -> Result<(), String> {
    let selected: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.event {
            crate::events::EventKind::FaultSelected { record } => Some((event, record)),
            _ => None,
        })
        .collect();
    if selected
        .iter()
        .map(|(_, record)| *record)
        .ne(records.iter())
        || selected.iter().any(|(event, record)| {
            event.operation.as_ref().map(|operation| operation.id) != record.request_id
        })
    {
        return Err("fault records differ from their causal selection events".into());
    }
    if initial.get("fault_schedule") != state.get("fault_schedule") {
        return Err("fault schedule changed during execution".into());
    }
    let losses: Vec<_> = events
        .iter()
        .filter(|event| matches!(event.event, crate::events::EventKind::ResponseDeliveryLost))
        .map(|event| {
            event
                .operation
                .as_ref()
                .map(|operation| operation.id)
                .ok_or_else(|| "lost response has no request identity".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let Some(configuration) = initial.get("fault_schedule") else {
        return if records.is_empty() && losses.is_empty() {
            Ok(())
        } else {
            Err("fault evidence lacks a source-bound schedule".into())
        };
    };
    let mut schedule: FaultSchedule = serde_json::from_value(configuration.clone())
        .map_err(|error| format!("invalid source-bound fault schedule: {error}"))?;
    let mut seen = BTreeSet::new();
    let mut expected_losses = BTreeSet::new();
    for record in records {
        let identity = record
            .request_id
            .ok_or("fault selection has no request identity")?;
        let entry = journal
            .iter()
            .find(|entry| entry.operation_id == Some(identity))
            .ok_or("fault selection has no captured request")?;
        if !seen.insert(identity)
            || entry.service != record.service
            || !request_matches(initial, entry, record)
        {
            return Err("fault selector does not match its captured semantic request".into());
        }
        let selection_seq = selected
            .iter()
            .find(|(_, candidate)| candidate.request_id == Some(identity))
            .map(|(event, _)| event.seq)
            .ok_or("fault selector has no causal event")?;
        if events.iter().any(|event| {
            event
                .operation
                .as_ref()
                .is_some_and(|operation| operation.id == identity)
                && matches!(
                    event.event,
                    crate::events::EventKind::StateCommitted { .. }
                        | crate::events::EventKind::ResponseDeliveryLost
                )
                && event.seq <= selection_seq
        }) {
            return Err("fault selection was recorded after its effect".into());
        }
        let (_, mut expected) = schedule.check(
            &record.service,
            &record.operation_id,
            record.resource.as_deref(),
        );
        expected.request_id = Some(identity);
        if expected != *record {
            return Err("fault selections do not replay the declared schedule".into());
        }
        let successful = (200..300).contains(&entry.status)
            && entry
                .response_body
                .get("isError")
                .and_then(serde_json::Value::as_bool)
                != Some(true);
        match record.decision {
            FaultDecisionKind::TransientError { status } => {
                if entry.path == crate::mcp::TOOLS_CALL_PATH {
                    if entry.response_body["isError"] != true {
                        return Err("transient MCP fault did not return a tool error".into());
                    }
                } else if entry.status != status {
                    return Err("transient fault status differs from the delivered response".into());
                }
                if events.iter().any(|event| {
                    event
                        .operation
                        .as_ref()
                        .is_some_and(|operation| operation.id == identity)
                        && matches!(event.event, crate::events::EventKind::StateCommitted { .. })
                }) {
                    return Err("a pre-execution transient fault committed business state".into());
                }
            }
            FaultDecisionKind::ResponseLost if successful => {
                expected_losses.insert(identity);
            }
            _ => {}
        }
    }
    let actual_losses: BTreeSet<_> = losses.iter().copied().collect();
    if losses.len() != actual_losses.len() || actual_losses != expected_losses {
        return Err("response-loss delivery does not match the replayed schedule".into());
    }
    Ok(())
}

fn request_matches(
    initial: &serde_json::Value,
    entry: &crate::journal::JournalEntry,
    record: &FaultRecord,
) -> bool {
    let (operation, path_resource, parameters) = if entry.path == crate::mcp::TOOLS_CALL_PATH {
        let body = &entry.request_body;
        let arguments = &body["arguments"];
        let Some(operation) = arguments["stripe_api_operation_id"].as_str() else {
            return false;
        };
        if entry.method != "POST"
            || !matches!(
                body["name"].as_str(),
                Some("stripe_api_read" | "stripe_api_write")
            )
        {
            return false;
        }
        let parameters = arguments["parameters"].clone();
        let path_resource = match operation {
            "GetChargesCharge" => parameters.get("charge").or_else(|| parameters.get("id")),
            "GetRefundsRefund" => parameters.get("refund").or_else(|| parameters.get("id")),
            "GetCustomersCustomer" => parameters.get("customer").or_else(|| parameters.get("id")),
            _ => None,
        }
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
        (operation.to_owned(), path_resource, parameters)
    } else {
        let segments: Vec<_> = entry.path.trim_matches('/').split('/').collect();
        let (operation, path_resource) = match (entry.method.as_str(), segments.as_slice()) {
            ("GET", ["v1", "charges"]) => ("GetCharges", None),
            ("GET", ["v1", "charges", charge]) => ("GetChargesCharge", Some((*charge).to_owned())),
            ("GET", ["v1", "refunds"]) => ("GetRefunds", None),
            ("GET", ["v1", "refunds", refund]) => ("GetRefundsRefund", Some((*refund).to_owned())),
            ("GET", ["v1", "customers"]) => ("GetCustomers", None),
            ("GET", ["v1", "customers", customer]) => {
                ("GetCustomersCustomer", Some((*customer).to_owned()))
            }
            ("POST", ["v1", "refunds"]) => ("PostRefunds", None),
            ("POST", ["v1", "customers"]) => ("PostCustomers", None),
            _ => return false,
        };
        let mut parameters: serde_json::Map<String, serde_json::Value> =
            url::form_urlencoded::parse(entry.query.as_deref().unwrap_or_default().as_bytes())
                .map(|(key, value)| {
                    (
                        key.into_owned(),
                        serde_json::Value::String(value.into_owned()),
                    )
                })
                .collect();
        if let Some(object) = entry.request_body.as_object() {
            parameters.extend(object.clone());
        } else if let Some(body) = entry.request_body.as_str() {
            parameters.extend(
                url::form_urlencoded::parse(body.as_bytes()).map(|(key, value)| {
                    (
                        key.into_owned(),
                        serde_json::Value::String(value.into_owned()),
                    )
                }),
            );
        }
        (
            operation.to_owned(),
            path_resource.map(|resource| crate::http::form_component_decode(&resource)),
            serde_json::Value::Object(parameters),
        )
    };
    let resource = if operation == "PostRefunds" {
        parameters["charge"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| {
                let intent = parameters["payment_intent"].as_str()?;
                initial
                    .pointer("/services/stripe/charges")?
                    .as_object()?
                    .iter()
                    .find(|(_, charge)| charge["payment_intent"].as_str() == Some(intent))
                    .map(|(key, _)| key.clone())
            })
    } else {
        path_resource
    };
    operation == record.operation_id && resource == record.resource
}
