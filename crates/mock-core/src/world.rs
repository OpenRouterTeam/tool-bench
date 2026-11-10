use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
    sync::{Arc, LockResult, Mutex, MutexGuard, PoisonError},
};

use axum::http::HeaderMap;
use serde_json::{Map, Value, json};
use tokio::sync::watch;

use crate::{
    clock::LogicalClock,
    dataset::{Dataset, ScenarioMeta},
    error::Result,
    events::{self, EventLog, OperationContext},
    ids::IdGen,
    journal::{Journal, JournalEntry, ResponseRecord},
    lifecycle::{Lifecycle, LifecycleError},
    preservation::{business_state, record_changes},
    provenance::{DatasetSource, canonical_sha256},
    request::{CapturedBody, CapturedHeader, HeaderRegistry, RequestHead},
    service::ServiceInitializer,
    store::Store,
};

/// Mutable state of one mock service.
#[derive(Debug, Clone)]
pub struct ServiceState {
    pub store: Store,
    pub session: Store,
    pub ids: IdGen,
}

/// Everything a running world knows. Exactly one scenario per world.
#[derive(Debug, Clone)]
pub struct WorldState {
    pub meta: ScenarioMeta,
    pub clock: LogicalClock,
    pub journal: Journal,
    /// The agent's final answer, posted by the verifier before grading.
    pub answer: Option<String>,
    services: BTreeMap<String, ServiceState>,
    initial_meta: ScenarioMeta,
    initial_services: BTreeMap<String, ServiceState>,
    initialized_services: BTreeSet<String>,
    pub(crate) events: EventLog,
    pub(crate) epoch: u64,
    pub(crate) lifecycle: Lifecycle,
    replay_headers: HeaderRegistry,
    source: DatasetSource,
}

#[derive(Debug, Clone)]
pub struct SharedWorld(pub(crate) Arc<WorldShared>);

#[derive(Debug)]
pub(crate) struct WorldShared {
    pub(crate) state: Mutex<WorldState>,
    pub(crate) changed: watch::Sender<u64>,
}

pub struct WorldGuard<'world> {
    guard: MutexGuard<'world, WorldState>,
    initial: Option<Value>,
    epoch: u64,
    operation: Option<Box<OperationContext>>,
}

pub fn shared(state: WorldState) -> SharedWorld {
    SharedWorld(Arc::new(WorldShared {
        state: Mutex::new(state),
        changed: watch::channel(0).0,
    }))
}

impl SharedWorld {
    pub fn configure_replay_headers(
        &self,
        registry: HeaderRegistry,
    ) -> std::result::Result<(), String> {
        let mut world = self.0.state.lock().expect("world lock");
        if !world.events.entries().is_empty()
            || world.finalization().phase != crate::lifecycle::Phase::Open
        {
            return Err("replay headers must be configured before world activity".into());
        }
        world.replay_headers = registry;
        Ok(())
    }

    pub(crate) fn capture_headers(
        &self,
        service: &str,
        headers: &HeaderMap,
    ) -> Vec<CapturedHeader> {
        self.0
            .state
            .lock()
            .expect("world lock")
            .replay_headers
            .capture(service, headers)
    }

    pub(crate) fn record_request_head(&self, operation: &OperationContext, head: RequestHead) {
        let mut world = self.0.state.lock().expect("world lock");
        let time = world.clock.now_rfc3339();
        world.events.request_head(operation.clone(), time, head);
    }

    pub(crate) fn record_request_body(&self, operation: &OperationContext, body: CapturedBody) {
        let mut world = self.0.state.lock().expect("world lock");
        let time = world.clock.now_rfc3339();
        world.events.request_body(operation.clone(), time, body);
    }

    pub fn prepare_service(
        &self,
        name: &str,
        initializer: Option<ServiceInitializer>,
    ) -> std::result::Result<(), String> {
        let mut world = self.0.state.lock().expect("world lock");
        if world.initialized_services.contains(name) {
            return Ok(());
        }
        if let Some(initialize) = initializer {
            if !world.events.entries().is_empty() {
                return Err(format!(
                    "service {name} must be initialized before world activity"
                ));
            }
            let now = world.clock.now_unix();
            initialize(world.service_mut(name), now);
            let initial = world
                .services
                .get(name)
                .expect("initialized service")
                .clone();
            world.initial_services.insert(name.into(), initial);
        }
        world.initialized_services.insert(name.into());
        Ok(())
    }

    pub fn lock(&self) -> LockResult<WorldGuard<'_>> {
        match self.0.state.lock() {
            Ok(guard) => Ok(WorldGuard::new(guard)),
            Err(error) => Err(PoisonError::new(WorldGuard::new(error.into_inner()))),
        }
    }

    pub fn transport_clock_step(&self) -> Result<()> {
        let mut world = self.0.state.lock().expect("world lock");
        let mode = world.meta.clock_mode;
        world.clock.transport_step(mode)
    }

    pub fn fail_operation(&self, operation: &OperationContext, code: &str, reason: &str) {
        let mut world = self.0.state.lock().expect("world lock");
        world.fail_operation(operation, code, reason);
    }

    pub(crate) fn record_response(&self, operation: &OperationContext, record: ResponseRecord) {
        let mut world = self.0.state.lock().expect("world lock");
        let Some((head, body)) = world.events.request(operation.id) else {
            let time = world.clock.now_rfc3339();
            world.events.fail(
                operation.clone(),
                time,
                "request_evidence_missing".into(),
                "response observation has no complete pre-dispatch request".into(),
            );
            return;
        };
        let entry = JournalEntry {
            seq: world.journal.next_seq(),
            operation_id: Some(operation.id),
            replay: world.events.replay_context(operation.id),
            time: world.clock.now_rfc3339(),
            service: operation.service.clone(),
            source: operation.source.clone(),
            method: operation.method.clone(),
            path: operation.path.clone(),
            host: head.host.clone(),
            scheme: head.scheme.clone(),
            content_type: head.content_type.clone(),
            query: head.query.clone(),
            request_body: body.value(),
            status: record.status,
            response_body: record.response_body,
        };
        world.journal.push(entry);
    }
}

impl<'world> WorldGuard<'world> {
    fn new(guard: MutexGuard<'world, WorldState>) -> Self {
        Self {
            epoch: guard.epoch,
            guard,
            initial: None,
            operation: None,
        }
    }
}

impl Deref for WorldGuard<'_> {
    type Target = WorldState;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl DerefMut for WorldGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if self.initial.is_none() {
            self.operation = events::current().map(Box::new);
            self.initial = Some(
                business_state(&self.guard.snapshot()).expect("world snapshot has record maps"),
            );
        }
        &mut self.guard
    }
}

impl Drop for WorldGuard<'_> {
    fn drop(&mut self) {
        if self.epoch != self.guard.epoch {
            return;
        }
        if let Some(initial) = &self.initial {
            let final_state =
                business_state(&self.guard.snapshot()).expect("world snapshot has record maps");
            let records = record_changes(initial, &final_state);
            let time = self.guard.clock.now_rfc3339();
            let operation = self.operation.take().map(|operation| *operation);
            self.guard.events.commit(operation, time, records);
        }
    }
}

impl WorldState {
    pub fn from_dataset(dataset: Dataset) -> Result<Self> {
        let meta = dataset.meta.clone();
        let clock = LogicalClock::new(&meta.now, meta.clock_step_secs)?;
        let services: BTreeMap<_, _> = dataset
            .services
            .iter()
            .map(|(name, store)| {
                let state = ServiceState {
                    store: store.clone(),
                    session: Store::default(),
                    ids: IdGen::new(service_seed(&meta.seed, name)),
                };
                (name.clone(), state)
            })
            .collect();
        let mut source = dataset.source;
        source.contract_sha256 = canonical_sha256(&dataset.contract);
        Ok(Self {
            initial_meta: meta.clone(),
            meta,
            clock,
            journal: Journal::default(),
            answer: None,
            initial_services: services.clone(),
            services,
            initialized_services: BTreeSet::new(),
            events: EventLog::default(),
            epoch: 0,
            lifecycle: Lifecycle::default(),
            replay_headers: HeaderRegistry::default(),
            source,
        })
    }

    pub fn source(&self) -> &DatasetSource {
        &self.source
    }

    pub fn fail_operation(&mut self, operation: &OperationContext, code: &str, reason: &str) {
        self.events.fail(
            operation.clone(),
            self.clock.now_rfc3339(),
            code.to_owned(),
            reason.to_owned(),
        );
    }

    pub fn service(&self, name: &str) -> Option<&ServiceState> {
        self.services.get(name)
    }

    /// The named service's state, created empty if the dataset had none.
    pub fn service_mut(&mut self, name: &str) -> &mut ServiceState {
        let seed = service_seed(&self.meta.seed, name);
        self.services
            .entry(name.to_owned())
            .or_insert_with(|| ServiceState {
                store: Store::default(),
                session: Store::default(),
                ids: IdGen::new(seed),
            })
    }

    /// Back to the scenario's starting state: dataset, clock, IDs, and an empty journal.
    pub fn reset(&mut self) -> std::result::Result<(), LifecycleError> {
        self.ensure_resettable()?;
        self.meta = self.initial_meta.clone();
        self.clock = LogicalClock::new(&self.meta.now, self.meta.clock_step_secs)
            .expect("initial clock was valid");
        self.services = self.initial_services.clone();
        self.journal = Journal::default();
        self.events = EventLog::default();
        self.answer = None;
        self.epoch += 1;
        Ok(())
    }

    pub fn snapshot(&self) -> Value {
        json!({
            "scenario": self.meta.id,
            "now": self.clock.now_rfc3339(),
            "clock": {
                "mode": self.meta.clock_mode,
                "reference": self.initial_meta.now,
                "step_secs": self.meta.clock_step_secs,
            },
            "services": stores_snapshot(&self.services, false),
            "sessions": stores_snapshot(&self.services, true),
        })
    }

    pub fn initial_snapshot(&self) -> Value {
        json!({
            "scenario": self.initial_meta.id,
            "now": self.initial_meta.now,
            "clock": {
                "mode": self.initial_meta.clock_mode,
                "reference": self.initial_meta.now,
                "step_secs": self.initial_meta.clock_step_secs,
            },
            "services": stores_snapshot(&self.initial_services, false),
            "sessions": stores_snapshot(&self.initial_services, true),
        })
    }

    pub fn events(&self) -> &EventLog {
        &self.events
    }
}

fn stores_snapshot(services: &BTreeMap<String, ServiceState>, session: bool) -> Value {
    let stores = services
        .iter()
        .map(|(name, state)| {
            let store = if session {
                &state.session
            } else {
                &state.store
            };
            (name.clone(), store.snapshot())
        })
        .collect::<Map<_, _>>();
    Value::Object(stores)
}

fn service_seed(seed: &str, service: &str) -> String {
    format!("{seed}/{service}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::*;

    fn example() -> WorldState {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/example/github/basic");
        WorldState::from_dataset(Dataset::load(&dir).unwrap()).unwrap()
    }

    #[test]
    fn snapshot_reports_scenario_time_and_services() {
        let world = example();
        let snap = world.snapshot();
        assert_eq!(snap["scenario"], json!("example/github/basic"));
        assert_eq!(snap["now"], json!("2026-03-14T09:00:00Z"));
        assert_eq!(
            snap["services"]["github"]["repos"]["octo-org/app"]["id"],
            json!(1296269)
        );
    }

    #[test]
    fn service_mut_creates_missing_services_with_seeded_ids() {
        let mut a = example();
        let mut b = example();
        assert!(a.service("stripe").is_none());
        let id_a = a
            .service_mut("stripe")
            .ids
            .next_prefixed("customer", "cus_", 14);
        let id_b = b
            .service_mut("stripe")
            .ids
            .next_prefixed("customer", "cus_", 14);
        assert_eq!(id_a, id_b);
        let github_id = a
            .service_mut("github")
            .ids
            .next_prefixed("customer", "cus_", 14);
        assert_ne!(github_id, id_a, "services get independent ID streams");
    }

    #[test]
    fn reset_restores_dataset_clock_and_journal() {
        let mut world = example();
        world.clock.tick();
        world
            .service_mut("github")
            .store
            .collection_mut("issues")
            .remove("octo-org/app#1");
        world.reset().unwrap();
        assert_eq!(world.clock.now_rfc3339(), "2026-03-14T09:00:00Z");
        assert!(world.journal.entries().is_empty());
        assert!(
            world
                .service("github")
                .unwrap()
                .store
                .collection("issues")
                .unwrap()
                .contains("octo-org/app#1")
        );
    }
}
