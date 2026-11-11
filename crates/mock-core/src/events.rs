use std::{collections::BTreeMap, future::Future};

use serde::{Deserialize, Serialize};

use crate::preservation::StateChange;
use crate::request::{CapturedBody, ReplayContext, RequestHead};

mod audit;
pub(crate) use audit::audit_with_observer;
pub use audit::{
    EffectCheck, HistoryError, HistoryFailure, HistoryFailureKind, OrderingCheck, audit,
};

tokio::task_local! {
    static ACTIVE_OPERATION: Option<OperationContext>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationContext {
    pub id: u64,
    pub generation: u64,
    pub parent_id: Option<u64>,
    pub scenario: String,
    pub service: String,
    pub source: String,
    pub method: String,
    pub path: String,
    pub kind: OperationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Http,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationEvent {
    pub seq: u64,
    pub time: String,
    pub operation: Option<OperationContext>,
    #[serde(flatten)]
    pub event: EventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    RequestStarted,
    RequestHeadReceived {
        head: RequestHead,
    },
    RequestBodyReceived {
        body: CapturedBody,
    },
    StateCommitted {
        records: Vec<StateChange>,
    },
    ResponseCompleted {
        status: u16,
        body_complete: bool,
    },
    ResponseDeliveryLost,
    FaultSelected {
        record: crate::faults::FaultRecord,
    },
    UserExchanged {
        record: crate::scripted_user::UserExchange,
    },
    EnvironmentFailed {
        code: String,
        reason: String,
    },
    OperationAborted,
    ExecutionRetained {
        execution_id: u64,
    },
    ExecutionReleased {
        execution_id: u64,
    },
}

/// Refusal codes with a call site that can refuse after part of the request was already
/// written. A trial that recovers from one of these cannot be told from a trial the
/// refusal left half-changed, so they void the trial like any other environment failure.
/// Remove a code from this list only once every one of its call sites refuses before its
/// first write.
const REFUSALS_AFTER_A_WRITE: &[&str] = &[
    // Argo CD: create and update store the Application and its event, then reconcile. An
    // automated sync the mock cannot run is refused with those writes already made.
    "unsupported_argocd_operation",
    // Supabase: a branch merge or push replays the missing migrations one statement at a
    // time, and each is committed before the next is tried.
    "unsupported_sql_statement",
    // Terraform: a run created with auto-apply is written, then removed again when its
    // apply is refused. Nothing remains, but the refusal does not come before the write.
    "unsupported_terraform_execution",
    // Vercel: environment variables sent as a batch are written one at a time, so an
    // unsupported upsert late in the batch is refused after the earlier ones were stored.
    "unsupported_vercel_operation",
];

/// Whether an environment-failure code records a refusal: the agent asked for an
/// operation the mock does not model, and the call was refused before it changed
/// anything. Such codes contain `unsupported`. Every other code is an internal error
/// (the clock, caller resolution, a panic, missing request evidence).
///
/// Grading voids a trial for an internal error whatever else happened, and for a refusal
/// only when the trial does not otherwise pass.
pub fn is_unsupported_refusal(code: &str) -> bool {
    code.contains("unsupported") && !REFUSALS_AFTER_A_WRITE.contains(&code)
}

#[derive(Debug, Clone, Default)]
pub struct EventLog {
    entries: Vec<OperationEvent>,
    requests: BTreeMap<u64, (Option<usize>, Option<usize>)>,
}

impl EventLog {
    pub fn entries(&self) -> &[OperationEvent] {
        &self.entries
    }

    pub(crate) fn begin(
        &mut self,
        mut context: OperationContext,
        time: String,
    ) -> OperationContext {
        context.id = self.entries.len() as u64 + 1;
        self.requests.insert(context.id, (None, None));
        self.push(Some(context.clone()), time, EventKind::RequestStarted);
        context
    }

    pub(crate) fn request_head(
        &mut self,
        operation: OperationContext,
        time: String,
        head: RequestHead,
    ) {
        self.requests.entry(operation.id).or_default().0 = Some(self.entries.len());
        self.push(
            Some(operation),
            time,
            EventKind::RequestHeadReceived { head },
        );
    }

    pub(crate) fn request_body(
        &mut self,
        operation: OperationContext,
        time: String,
        body: CapturedBody,
    ) {
        self.requests.entry(operation.id).or_default().1 = Some(self.entries.len());
        self.push(
            Some(operation),
            time,
            EventKind::RequestBodyReceived { body },
        );
    }

    pub(crate) fn request(&self, operation_id: u64) -> Option<(&RequestHead, &CapturedBody)> {
        let (head_index, body_index) = self.requests.get(&operation_id)?;
        match (
            &self.entries.get((*head_index)?)?.event,
            &self.entries.get((*body_index)?)?.event,
        ) {
            (EventKind::RequestHeadReceived { head }, EventKind::RequestBodyReceived { body }) => {
                Some((head, body))
            }
            _ => None,
        }
    }

    pub(crate) fn replay_context(&self, operation_id: u64) -> Option<ReplayContext> {
        let (head, body) = self.request(operation_id)?;
        let head_index = self.requests.get(&operation_id)?.0?;
        Some(ReplayContext {
            parent_operation_id: self.entries.get(head_index)?.operation.as_ref()?.parent_id,
            headers: head.headers.clone(),
            body: body.clone(),
        })
    }

    pub(crate) fn commit(
        &mut self,
        operation: Option<OperationContext>,
        time: String,
        records: Vec<StateChange>,
    ) {
        if !records.is_empty() {
            self.push(operation, time, EventKind::StateCommitted { records });
        }
    }

    pub(crate) fn finish(
        &mut self,
        operation: OperationContext,
        time: String,
        status: u16,
        body_complete: bool,
    ) {
        self.push(
            Some(operation),
            time,
            EventKind::ResponseCompleted {
                status,
                body_complete,
            },
        );
    }

    pub(crate) fn fail(
        &mut self,
        operation: OperationContext,
        time: String,
        code: String,
        reason: String,
    ) {
        self.push(
            Some(operation),
            time,
            EventKind::EnvironmentFailed { code, reason },
        );
    }

    pub(crate) fn abort(&mut self, operation: OperationContext, time: String) {
        self.push(Some(operation), time, EventKind::OperationAborted);
    }

    pub(crate) fn lose_response(&mut self, operation: OperationContext, time: String) {
        self.push(Some(operation), time, EventKind::ResponseDeliveryLost);
    }

    pub(crate) fn select_fault(
        &mut self,
        operation: OperationContext,
        time: String,
        record: crate::faults::FaultRecord,
    ) {
        self.push(Some(operation), time, EventKind::FaultSelected { record });
    }

    pub(crate) fn user_exchange(
        &mut self,
        operation: OperationContext,
        time: String,
        record: crate::scripted_user::UserExchange,
    ) {
        self.push(Some(operation), time, EventKind::UserExchanged { record });
    }

    pub(crate) fn retain(&mut self, operation: OperationContext, time: String) -> u64 {
        let execution_id = self.entries.len() as u64 + 1;
        self.push(
            Some(operation),
            time,
            EventKind::ExecutionRetained { execution_id },
        );
        execution_id
    }

    pub(crate) fn release(&mut self, operation: OperationContext, time: String, execution_id: u64) {
        self.push(
            Some(operation),
            time,
            EventKind::ExecutionReleased { execution_id },
        );
    }

    fn push(&mut self, operation: Option<OperationContext>, time: String, event: EventKind) {
        self.entries.push(OperationEvent {
            seq: self.entries.len() as u64 + 1,
            time,
            operation,
            event,
        });
    }
}

pub fn current() -> Option<OperationContext> {
    ACTIVE_OPERATION.try_with(Clone::clone).ok().flatten()
}

pub async fn scope<Output>(
    operation: OperationContext,
    future: impl Future<Output = Output>,
) -> Output {
    ACTIVE_OPERATION.scope(Some(operation), future).await
}

pub fn sync_scope<Output>(
    operation: Option<OperationContext>,
    operation_fn: impl FnOnce() -> Output,
) -> Output {
    ACTIVE_OPERATION.sync_scope(operation, operation_fn)
}
