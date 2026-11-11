use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Deref,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::time::{Instant, timeout_at};

use crate::{
    events::{self, EventKind, OperationContext, OperationEvent, OperationKind},
    world::{SharedWorld, WorldState},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Open,
    Draining,
    Sealed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingOperation {
    pub operation: OperationContext,
    pub executions: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finalization {
    pub phase: Phase,
    pub generation: u64,
    pub pending: Vec<PendingOperation>,
    pub failure: Option<String>,
    pub event_count: usize,
    pub drain_timeout_ms: Option<u64>,
    pub drain_elapsed_ms: Option<u64>,
}

impl Finalization {
    pub fn validate(&self, scenario: &str, events: &[OperationEvent]) -> Result<(), String> {
        if self.phase != Phase::Sealed || !self.pending.is_empty() || self.failure.is_some() {
            return Err(self.failure.clone().unwrap_or_else(|| {
                "world evidence was not sealed after all admitted work finished".into()
            }));
        }
        if self.event_count != events.len() {
            return Err("finalization does not cover the complete event stream".into());
        }
        match (self.drain_timeout_ms, self.drain_elapsed_ms) {
            (Some(budget), Some(elapsed)) if elapsed <= budget => {}
            (None, None) => {}
            _ => return Err("finalization timing evidence is inconsistent".into()),
        }
        let mut unfinished = BTreeSet::new();
        let mut operations = BTreeMap::new();
        let mut executions = BTreeMap::new();
        for (index, event) in events.iter().enumerate() {
            if event.seq != index as u64 + 1 {
                return Err("finalization event sequence is inconsistent".into());
            }
            let context = event
                .operation
                .as_ref()
                .ok_or("an operation event has no causal identity")?;
            let active = |operation_id: u64| {
                unfinished.contains(&operation_id)
                    || executions
                        .values()
                        .any(|retained| *retained == operation_id)
            };
            match &event.event {
                EventKind::RequestStarted => {
                    if context.id != event.seq
                        || context.generation != self.generation
                        || context.scenario != scenario
                        || operations.contains_key(&context.id)
                        || context.parent_id.is_some_and(|parent| !active(parent))
                    {
                        return Err("operation admission evidence is inconsistent".into());
                    }
                    unfinished.insert(context.id);
                    operations.insert(context.id, context);
                }
                EventKind::ResponseCompleted {
                    status,
                    body_complete,
                } => {
                    if !(100..600).contains(status) || !body_complete {
                        return Err("response evidence was not completely captured".into());
                    }
                    if operations.get(&context.id) != Some(&context)
                        || !unfinished.remove(&context.id)
                    {
                        return Err("operation completion evidence is inconsistent".into());
                    }
                }
                EventKind::OperationAborted => {
                    if operations.get(&context.id) != Some(&context)
                        || !unfinished.remove(&context.id)
                    {
                        return Err("operation completion evidence is inconsistent".into());
                    }
                }
                EventKind::ExecutionRetained { execution_id } => {
                    if operations.get(&context.id) != Some(&context)
                        || !active(context.id)
                        || *execution_id != event.seq
                        || executions.insert(*execution_id, context.id).is_some()
                    {
                        return Err("retained execution evidence is inconsistent".into());
                    }
                }
                EventKind::ExecutionReleased { execution_id } => {
                    if operations.get(&context.id) != Some(&context)
                        || executions.remove(execution_id) != Some(context.id)
                    {
                        return Err("released execution evidence is inconsistent".into());
                    }
                }
                _ => {
                    if operations.get(&context.id) != Some(&context) || !active(context.id) {
                        return Err("operation effects outlived their admitted execution".into());
                    }
                }
            }
        }
        if !unfinished.is_empty() || !executions.is_empty() {
            return Err("sealed evidence contains operations without terminal evidence".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Lifecycle {
    pub(crate) phase: Phase,
    active: BTreeMap<u64, PendingOperation>,
    failure: Option<String>,
    drain_started: Option<Instant>,
    drain_timeout: Option<Duration>,
    drain_elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleError {
    #[error("the world is closing and cannot admit this operation")]
    Closed,
    #[error("the world still has admitted work in progress")]
    Busy,
    #[error("world finalization has already failed")]
    Failed,
}

pub struct Operation {
    world: SharedWorld,
    context: OperationContext,
    epoch: u64,
    finished: bool,
}

pub struct Execution {
    world: SharedWorld,
    context: OperationContext,
    epoch: u64,
    id: u64,
}

impl Operation {
    pub fn capture_headers(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> Vec<crate::request::CapturedHeader> {
        self.world.capture_headers(&self.context.service, headers)
    }

    pub fn record_head(&self, head: crate::request::RequestHead) {
        self.world.record_request_head(&self.context, head);
    }

    pub fn record_body(&self, body: crate::request::CapturedBody) {
        self.world.record_request_body(&self.context, body);
    }

    pub fn record_response(&self, record: crate::journal::ResponseRecord) {
        self.world.record_response(&self.context, record);
    }

    pub fn record_delivery_loss(&self) {
        let mut world = self.world.0.state.lock().expect("world lock");
        if world.epoch == self.epoch {
            let time = world.clock.now_rfc3339();
            world.events.lose_response(self.context.clone(), time);
        }
    }

    pub fn context(&self) -> &OperationContext {
        &self.context
    }

    pub fn finish(mut self, status: u16, body_complete: bool) {
        let mut world = self.world.0.state.lock().expect("world lock");
        if world.epoch == self.epoch {
            let time = world.clock.now_rfc3339();
            world
                .events
                .finish(self.context.clone(), time, status, body_complete);
            release(&mut world.lifecycle, self.context.id);
            self.world
                .0
                .changed
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
        self.finished = true;
    }
}

impl Deref for Operation {
    type Target = OperationContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let mut world = self
            .world
            .0
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if world.epoch == self.epoch {
            let time = world.clock.now_rfc3339();
            if std::thread::panicking() {
                world.events.fail(
                    self.context.clone(),
                    time.clone(),
                    "operation_panicked".into(),
                    "an admitted operation panicked before completing".into(),
                );
            }
            world.events.abort(self.context.clone(), time);
            release(&mut world.lifecycle, self.context.id);
            self.world
                .0
                .changed
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
    }
}

impl Execution {
    pub fn run<Output>(self, execute: impl FnOnce() -> Output) -> Output {
        events::sync_scope(Some(self.context.clone()), execute)
    }
}

impl Drop for Execution {
    fn drop(&mut self) {
        let mut world = self
            .world
            .0
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if world.epoch == self.epoch {
            let time = world.clock.now_rfc3339();
            if std::thread::panicking() {
                world.events.fail(
                    self.context.clone(),
                    time.clone(),
                    "execution_panicked".into(),
                    "a retained execution panicked before completing".into(),
                );
            }
            world.events.release(self.context.clone(), time, self.id);
            release(&mut world.lifecycle, self.context.id);
            self.world
                .0
                .changed
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
    }
}

impl SharedWorld {
    pub fn begin_operation(
        &self,
        kind: OperationKind,
        service: &str,
        source: &str,
        method: &str,
        path: &str,
    ) -> Result<Operation, LifecycleError> {
        let mut world = self.0.state.lock().expect("world lock");
        let parent = (kind == OperationKind::Internal)
            .then(events::current)
            .flatten();
        let admitted_parent = parent.as_ref().is_some_and(|context| {
            world
                .lifecycle
                .active
                .get(&context.id)
                .is_some_and(|pending| pending.operation == *context)
        });
        if parent.is_some() && !admitted_parent {
            return Err(LifecycleError::Closed);
        }
        if world.lifecycle.phase != Phase::Open
            && !(world.lifecycle.phase == Phase::Draining && admitted_parent)
        {
            return Err(LifecycleError::Closed);
        }
        let context = OperationContext {
            id: 0,
            generation: world.epoch,
            parent_id: parent.map(|context| context.id),
            scenario: world.meta.id.clone(),
            service: service.into(),
            source: source.into(),
            method: method.into(),
            path: path.into(),
            kind,
        };
        let time = world.clock.now_rfc3339();
        let context = world.events.begin(context, time);
        world.lifecycle.active.insert(
            context.id,
            PendingOperation {
                operation: context.clone(),
                executions: 1,
            },
        );
        Ok(Operation {
            world: self.clone(),
            context,
            epoch: world.epoch,
            finished: false,
        })
    }

    pub fn retain_execution(
        &self,
        context: &OperationContext,
    ) -> Result<Execution, LifecycleError> {
        let mut world = self.0.state.lock().expect("world lock");
        let pending = world
            .lifecycle
            .active
            .get_mut(&context.id)
            .filter(|pending| pending.operation == *context)
            .ok_or(LifecycleError::Closed)?;
        pending.executions += 1;
        let time = world.clock.now_rfc3339();
        let id = world.events.retain(context.clone(), time);
        Ok(Execution {
            world: self.clone(),
            context: context.clone(),
            epoch: world.epoch,
            id,
        })
    }

    pub async fn finalize(&self, timeout: Duration) {
        let mut changed = self.0.changed.subscribe();
        let deadline = {
            let mut world = self.0.state.lock().expect("world lock");
            if matches!(world.lifecycle.phase, Phase::Sealed | Phase::Failed) {
                return;
            }
            let started = *world
                .lifecycle
                .drain_started
                .get_or_insert_with(Instant::now);
            let budget = *world.lifecycle.drain_timeout.get_or_insert(timeout);
            world.lifecycle.phase = Phase::Draining;
            started + budget
        };
        loop {
            {
                let mut world = self.0.state.lock().expect("world lock");
                if world.seal() != Err(LifecycleError::Busy) {
                    return;
                }
            }
            if timeout_at(deadline, changed.changed()).await.is_err() {
                let mut world = self.0.state.lock().expect("world lock");
                if world.seal() == Err(LifecycleError::Busy) {
                    fail_drain(&mut world.lifecycle, Instant::now());
                    self.0
                        .changed
                        .send_modify(|revision| *revision = revision.wrapping_add(1));
                }
                return;
            }
        }
    }
}

impl WorldState {
    pub fn finalization(&self) -> Finalization {
        Finalization {
            phase: self.lifecycle.phase,
            generation: self.epoch,
            pending: self.lifecycle.active.values().cloned().collect(),
            failure: self.lifecycle.failure.clone(),
            event_count: self.events.entries().len(),
            drain_timeout_ms: self.lifecycle.drain_timeout.map(millis),
            drain_elapsed_ms: self.lifecycle.drain_elapsed_ms,
        }
    }

    pub fn seal(&mut self) -> Result<(), LifecycleError> {
        if self.lifecycle.phase == Phase::Failed {
            return Err(LifecycleError::Failed);
        }
        if self.lifecycle.phase == Phase::Sealed {
            return Ok(());
        }
        if !self.lifecycle.active.is_empty() {
            self.lifecycle.phase = Phase::Draining;
            return Err(LifecycleError::Busy);
        }
        self.lifecycle.phase = Phase::Sealed;
        if self.lifecycle.drain_started.is_some() && self.lifecycle.drain_elapsed_ms.is_none() {
            self.lifecycle.drain_elapsed_ms = Some(0);
        }
        Ok(())
    }

    pub(crate) fn ensure_resettable(&self) -> Result<(), LifecycleError> {
        if !self.lifecycle.active.is_empty() {
            Err(LifecycleError::Busy)
        } else if self.lifecycle.phase != Phase::Open {
            Err(LifecycleError::Closed)
        } else {
            Ok(())
        }
    }
}

fn release(lifecycle: &mut Lifecycle, id: u64) {
    let pending = lifecycle.active.get_mut(&id).expect("registered operation");
    pending.executions -= 1;
    if pending.executions == 0 {
        lifecycle.active.remove(&id);
    }
    if lifecycle.phase == Phase::Draining {
        let now = Instant::now();
        if let Some((started, budget)) = lifecycle.drain_started.zip(lifecycle.drain_timeout) {
            if now > started + budget {
                fail_drain(lifecycle, now);
            } else if lifecycle.active.is_empty() {
                lifecycle.drain_elapsed_ms = Some(millis(now.duration_since(started)));
            }
        }
    }
}

fn fail_drain(lifecycle: &mut Lifecycle, now: Instant) {
    if lifecycle.phase == Phase::Failed {
        return;
    }
    lifecycle.phase = Phase::Failed;
    lifecycle.failure =
        Some("world finalization timed out while admitted work was still running".into());
    lifecycle.drain_elapsed_ms = lifecycle
        .drain_started
        .map(|started| millis(now.duration_since(started)));
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}
