//! The sealed outcome of the trusted-host resolved-surface audit (improvement plan §7).
//!
//! The strict runner captures the bytes a trial actually exposed to the agent, scans them
//! on the host, keeps the full private report outside agent-writable mounts, and binds this
//! summary into the runner receipt. Scanning lives only in `deploy/runner/surface_audit.py`;
//! this module is the verifier/report reader.
//!
//! A clean status says the captured surfaces held none of the known evaluator-owned
//! markers. It is a lexical result about captured bytes and never a statement about what a
//! model inferred.

use serde::{Deserialize, Serialize};

pub const CLAIM: &str = "Lexical scan of host-captured agent-visible surfaces for known \
evaluator-owned markers. It does not establish that the model was unaware of being evaluated.";

/// Every surface the audit requires. A trial whose sealed record lacks one is unqualified.
pub const REQUIRED_SURFACES: [&str; 13] = [
    "resolved_instruction",
    "workspace_files",
    "home_files",
    "agent_environment",
    "agent_argv",
    "agent_mounts",
    "agent_hostnames",
    "mcp_initialize",
    "mcp_tool_definitions",
    "mcp_tool_results",
    "mcp_tool_errors",
    "harness_messages",
    "runtime_processes",
];

const REQUIRED_LIMITS: [&str; 3] = [
    "lexical_marker_scan_of_captured_bytes_only",
    "does_not_establish_model_unawareness",
    "uncaptured_surfaces_are_unqualified_not_passed",
];

/// A redacted finding: where a marker sat and why it is evaluator-owned, never its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceFinding {
    pub surface: String,
    /// Index of the item within its surface (file, variable, response).
    pub item: u64,
    /// Hash of the item; `None` where hashing could expose a low-entropy secret.
    pub item_sha256: Option<String>,
    pub offset: u64,
    pub length: u64,
    pub ownership_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DateCoherence {
    /// `coherent`, `contradiction`, or `not_supplied` (no agent-visible date context).
    pub status: String,
    pub reference_time: String,
    pub carriers_checked: u64,
    pub contradictions: u64,
}

/// What the tools container was actually seen to register, from its served responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedCatalog {
    pub registered_endpoints: u64,
    /// Endpoints whose `tools/list` response the agent received.
    pub listed_endpoints: u64,
    pub listed_tools: Option<u64>,
    /// Hash over the listed `(endpoint, tool, definition hash)` sequence.
    pub listed_catalog_sha256: Option<String>,
    /// Whether the listing matched the private exposure manifest; `None` without one.
    pub matches_exposure_manifest: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealedSurfaceAudit {
    pub schema_version: u32,
    /// `clean`, `findings_present`, or `unqualified`.
    pub outcome: String,
    /// Hash of the private host report this summary was sealed from.
    pub report_sha256: String,
    pub required: Vec<String>,
    pub captured: Vec<String>,
    pub missing: Vec<String>,
    pub finding_count: u64,
    /// At most the first findings; `finding_count` is the total.
    pub findings: Vec<SurfaceFinding>,
    pub date_coherence: DateCoherence,
    pub catalog: ObservedCatalog,
    pub limits: Vec<String>,
}

/// The status the verifier and report publish for one trial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceAuditStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub finding_count: Option<u64>,
    pub missing: Vec<String>,
    pub date_coherence: Option<String>,
    pub report_sha256: Option<String>,
    pub claim: String,
}

impl SurfaceAuditStatus {
    fn unqualified(reason: &str, sealed: Option<&SealedSurfaceAudit>) -> Self {
        Self {
            status: "unqualified".into(),
            reason: Some(reason.into()),
            finding_count: sealed.map(|value| value.finding_count),
            missing: sealed
                .map(|value| value.missing.clone())
                .unwrap_or_default(),
            date_coherence: sealed.map(|value| value.date_coherence.status.clone()),
            report_sha256: sealed.map(|value| value.report_sha256.clone()),
            claim: CLAIM.into(),
        }
    }

    pub fn is_clean(&self) -> bool {
        self.status == "clean"
    }
}

impl Default for SurfaceAuditStatus {
    fn default() -> Self {
        status(None)
    }
}

/// Status from a collected runner receipt. A receipt that was not admitted proves nothing.
pub fn status_from_receipt(receipt: Option<&[u8]>, admitted: bool) -> SurfaceAuditStatus {
    let Some(admission) = receipt
        .and_then(|bytes| serde_json::from_slice::<crate::runner::RunnerAdmission>(bytes).ok())
    else {
        return status(None);
    };
    if !admitted {
        return SurfaceAuditStatus::unqualified(
            "the runner receipt carrying the surface audit was not admitted",
            admission.surface_audit.as_ref(),
        );
    }
    status(admission.surface_audit.as_ref())
}

/// Derives the published status, failing closed: an absent, malformed, incomplete or
/// self-contradictory record is unqualified, never clean.
pub fn status(sealed: Option<&SealedSurfaceAudit>) -> SurfaceAuditStatus {
    let Some(record) = sealed else {
        return SurfaceAuditStatus::unqualified("no host surface capture was sealed", None);
    };
    let unqualified = |reason: &str| SurfaceAuditStatus::unqualified(reason, sealed);
    if record.schema_version != 1 || !crate::provenance::digest_valid(&record.report_sha256) {
        return unqualified("unsupported or unbound surface audit record");
    }
    if REQUIRED_LIMITS
        .iter()
        .any(|limit| !record.limits.iter().any(|value| value == limit))
    {
        return unqualified("surface audit record omits its claim limits");
    }
    let required_missing: Vec<String> = REQUIRED_SURFACES
        .iter()
        .filter(|surface| !record.captured.iter().any(|value| value == *surface))
        .map(|surface| (*surface).to_owned())
        .collect();
    if !required_missing.is_empty() || !record.missing.is_empty() {
        let mut status = unqualified("a required agent-visible surface was not captured");
        for surface in required_missing {
            if !status.missing.contains(&surface) {
                status.missing.push(surface);
            }
        }
        return status;
    }
    if record.findings.len() as u64 > record.finding_count
        || (record.finding_count > 0 && record.findings.is_empty())
        || !matches!(
            record.date_coherence.status.as_str(),
            "coherent" | "contradiction" | "not_supplied"
        )
        || (record.date_coherence.status == "contradiction")
            != (record.date_coherence.contradictions > 0)
        || record.catalog.listed_endpoints != record.catalog.registered_endpoints
        || record.catalog.listed_tools.is_none()
    {
        return unqualified("surface audit record is internally inconsistent");
    }
    let flagged = record.finding_count > 0
        || record.date_coherence.contradictions > 0
        || record.catalog.matches_exposure_manifest == Some(false);
    let expected = if flagged { "findings_present" } else { "clean" };
    if record.outcome != expected {
        return unqualified("surface audit outcome contradicts its own findings");
    }
    SurfaceAuditStatus {
        status: expected.into(),
        reason: None,
        finding_count: Some(record.finding_count),
        missing: Vec::new(),
        date_coherence: Some(record.date_coherence.status.clone()),
        report_sha256: Some(record.report_sha256.clone()),
        claim: CLAIM.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn clean() -> SealedSurfaceAudit {
        SealedSurfaceAudit {
            schema_version: 1,
            outcome: "clean".into(),
            report_sha256: format!("sha256:{}", "a".repeat(64)),
            required: REQUIRED_SURFACES.map(str::to_owned).to_vec(),
            captured: REQUIRED_SURFACES.map(str::to_owned).to_vec(),
            missing: Vec::new(),
            finding_count: 0,
            findings: Vec::new(),
            date_coherence: DateCoherence {
                status: "not_supplied".into(),
                reference_time: "2026-01-01T00:00:00Z".into(),
                carriers_checked: 0,
                contradictions: 0,
            },
            catalog: ObservedCatalog {
                registered_endpoints: 2,
                listed_endpoints: 2,
                listed_tools: Some(9),
                listed_catalog_sha256: Some(format!("sha256:{}", "b".repeat(64))),
                matches_exposure_manifest: None,
            },
            limits: REQUIRED_LIMITS.map(str::to_owned).to_vec(),
        }
    }

    #[test]
    fn absent_incomplete_or_contradictory_records_are_unqualified_never_clean() {
        assert_eq!(status(None).status, "unqualified");
        assert!(status(Some(&clean())).is_clean());
        assert!(status(Some(&clean())).claim.contains("does not establish"));

        let mut missing = clean();
        missing
            .captured
            .retain(|surface| surface != "mcp_tool_definitions");
        let result = status(Some(&missing));
        assert_eq!(result.status, "unqualified");
        assert_eq!(result.missing, ["mcp_tool_definitions"]);

        let mut declared_missing = clean();
        declared_missing.missing.push("home_files".into());
        assert_eq!(status(Some(&declared_missing)).status, "unqualified");

        let mut hidden = clean();
        hidden.finding_count = 2;
        assert_eq!(status(Some(&hidden)).status, "unqualified");

        let mut unlisted = clean();
        unlisted.catalog.listed_endpoints = 1;
        assert_eq!(status(Some(&unlisted)).status, "unqualified");

        let mut unlimited = clean();
        unlimited.limits.clear();
        assert_eq!(status(Some(&unlimited)).status, "unqualified");
    }

    #[test]
    fn findings_date_contradictions_and_registration_drift_are_reported_not_passed() {
        let mut found = clean();
        found.outcome = "findings_present".into();
        found.finding_count = 1;
        found.findings.push(SurfaceFinding {
            surface: "workspace_files".into(),
            item: 0,
            item_sha256: None,
            offset: 4,
            length: 12,
            ownership_reason: "evaluator_scenario_id".into(),
        });
        assert_eq!(status(Some(&found)).status, "findings_present");
        found.outcome = "clean".into();
        assert_eq!(status(Some(&found)).status, "unqualified");

        let mut dated = clean();
        dated.date_coherence.status = "contradiction".into();
        dated.date_coherence.contradictions = 1;
        dated.outcome = "findings_present".into();
        assert_eq!(status(Some(&dated)).status, "findings_present");

        let mut drift = clean();
        drift.catalog.matches_exposure_manifest = Some(false);
        assert_eq!(status(Some(&drift)).status, "unqualified");
        drift.outcome = "findings_present".into();
        assert_eq!(status(Some(&drift)).status, "findings_present");
    }
}
