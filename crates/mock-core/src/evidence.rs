use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    events::OperationEvent,
    grading::{EVALUATOR_VERSION, Expect, Grade, GradeInput, InterfacePolicy, grade},
    journal::JournalEntry,
    lifecycle::Finalization,
    provenance::{DatasetSource, canonical_sha256},
    startup::StartupCall,
    world::WorldState,
};

pub const EVIDENCE_SCHEMA_VERSION: u32 = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldEvidence {
    pub schema_version: u32,
    pub evaluator_version: u32,
    pub scenario: String,
    pub source: DatasetSource,
    pub initial_state_sha256: String,
    pub initial_state: Value,
    pub state: Value,
    pub journal: Vec<JournalEntry>,
    pub events: Vec<OperationEvent>,
    pub startup: Vec<StartupCall>,
    pub tools_ips: Vec<String>,
    pub origin_error: Option<String>,
    pub answer: Option<String>,
    pub finalization: Finalization,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fault_records: Vec<crate::faults::FaultRecord>,
}

impl WorldEvidence {
    pub fn capture(
        world: &WorldState,
        startup: &[StartupCall],
        origin: Result<Vec<String>, String>,
    ) -> Self {
        let (tools_ips, origin_error) = match origin {
            Ok(addresses) => (addresses, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        Self {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            evaluator_version: EVALUATOR_VERSION,
            scenario: world.meta.id.clone(),
            source: world.source().clone(),
            initial_state_sha256: canonical_sha256(&world.initial_snapshot()),
            initial_state: world.initial_snapshot(),
            state: world.snapshot(),
            journal: world.journal.entries().to_vec(),
            events: world.events().entries().to_vec(),
            startup: startup.to_vec(),
            tools_ips,
            origin_error,
            answer: world.answer.clone(),
            finalization: world.finalization(),
            fault_records: world.fault_records().to_vec(),
        }
    }

    pub fn assess(&self, contract: &Expect, answer: Option<&str>, context_overflow: bool) -> Grade {
        let mut assessment = grade(&GradeInput {
            expect: contract,
            state: self.state.clone(),
            journal: &self.journal,
            answer,
            tools_ips: self.tools_ips.clone(),
            context_overflow,
            startup: &self.startup,
            initial_state: Some(&self.initial_state),
            events: Some(&self.events),
        });
        if self.schema_version != EVIDENCE_SCHEMA_VERSION
            || self.evaluator_version != EVALUATOR_VERSION
        {
            assessment.invalidate_environment(
                "evidence/version",
                "world evidence and evaluator revisions are incompatible",
            );
        }
        if let Err(reason) = self.source.validate(&self.scenario) {
            assessment.invalidate_environment("evidence/source", reason);
        }
        if self.source.contract_sha256 != canonical_sha256(contract) {
            assessment.invalidate_environment(
                "evidence/contract-source",
                "the verifier contract does not match the contract loaded by the world",
            );
        }
        if self.initial_state_sha256 != canonical_sha256(&self.initial_state) {
            assessment.invalidate_environment(
                "evidence/initial-source",
                "initial-state provenance does not match the captured bytes",
            );
        }
        if self.initial_state["scenario"].as_str() != Some(self.scenario.as_str())
            || self.state["scenario"].as_str() != Some(self.scenario.as_str())
        {
            assessment.invalidate_environment(
                "evidence/scenario",
                "world evidence contains inconsistent scenario identities",
            );
        }
        if contract.interfaces == InterfacePolicy::McpOnly
            && (self.origin_error.is_some() || self.tools_ips.is_empty())
        {
            assessment.invalidate_environment(
                "evidence/tools_ips",
                "the producer could not resolve the trusted tools origin",
            );
        }
        if let Err(reason) = self.finalization.validate(&self.scenario, &self.events) {
            assessment.invalidate_environment("evidence/finalization", reason);
        }
        if let Err(reason) = crate::clock::validate_evidence(&self.initial_state, &self.state) {
            assessment.invalidate_environment("evidence/clock", reason);
        }
        if let Err(reason) = crate::faults::validate_evidence(
            &self.initial_state,
            &self.state,
            &self.fault_records,
            &self.journal,
            &self.events,
        ) {
            assessment.invalidate_environment("evidence/fault-schedule", reason);
        }
        if let Err(reason) = crate::request::validate_journal(&self.events, &self.journal)
            && !assessment
                .failures
                .iter()
                .any(|failure| failure.location == "evidence/requests" && failure.reason == reason)
        {
            assessment.invalidate_environment("evidence/requests", reason);
        }
        assessment
    }
}
