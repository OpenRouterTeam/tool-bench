//! Resolved-surface audit (improvement plan §7): validates that every agent-visible channel
//! in a materialized task is free of evaluator-owned identifiers, admin access, and
//! solution artifacts.
//!
//! The audit operates on **resolved** content (the actual bytes a model receives), not
//! source templates. A passed lexical audit does not certify model unawareness—it
//! establishes that the surfaces inspected contain no *known* evaluator-owned markers.
//! Missing required surfaces make the audit unqualified, not passed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Surface definitions ──────────────────────────────────────────────────────

/// Every channel an agent can observe during a trial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    InitialSystemPrompt,
    InitialDeveloperPrompt,
    InitialUserPrompt,
    ProfileWorkspace,
    ProfileHome,
    WorkspaceSessionConfig,
    WorkspaceConnectionInventory,
    WorkspaceAccountIdentity,
    ToolDefinitions,
    ToolInitResponses,
    ToolResultBodies,
    ToolErrorBodies,
    AgentArgv,
    AgentEnvironment,
    AgentMounts,
    DnsNames,
    ServiceHealthRoutes,
    SessionMetadata,
}

impl Surface {
    pub const ALL: &'static [Surface] = &[
        Self::InitialSystemPrompt,
        Self::InitialDeveloperPrompt,
        Self::InitialUserPrompt,
        Self::ProfileWorkspace,
        Self::ProfileHome,
        Self::WorkspaceSessionConfig,
        Self::WorkspaceConnectionInventory,
        Self::WorkspaceAccountIdentity,
        Self::ToolDefinitions,
        Self::ToolInitResponses,
        Self::ToolResultBodies,
        Self::ToolErrorBodies,
        Self::AgentArgv,
        Self::AgentEnvironment,
        Self::AgentMounts,
        Self::DnsNames,
        Self::ServiceHealthRoutes,
        Self::SessionMetadata,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::InitialSystemPrompt => "initial_system_prompt",
            Self::InitialDeveloperPrompt => "initial_developer_prompt",
            Self::InitialUserPrompt => "initial_user_prompt",
            Self::ProfileWorkspace => "profile_workspace",
            Self::ProfileHome => "profile_home",
            Self::WorkspaceSessionConfig => "workspace_session_config",
            Self::WorkspaceConnectionInventory => "workspace_connection_inventory",
            Self::WorkspaceAccountIdentity => "workspace_account_identity",
            Self::ToolDefinitions => "tool_definitions",
            Self::ToolInitResponses => "tool_init_responses",
            Self::ToolResultBodies => "tool_result_bodies",
            Self::ToolErrorBodies => "tool_error_bodies",
            Self::AgentArgv => "agent_argv",
            Self::AgentEnvironment => "agent_environment",
            Self::AgentMounts => "agent_mounts",
            Self::DnsNames => "dns_names",
            Self::ServiceHealthRoutes => "service_health_routes",
            Self::SessionMetadata => "session_metadata",
        }
    }
}

// ── Evaluator-owned markers ──────────────────────────────────────────────────

/// Patterns that identify evaluator-owned content. The audit scans resolved bytes for
/// these; matches produce findings with redacted locations rather than exposing the
/// actual secret values.
const PRIVATE_MARKERS: &[(&str, &str)] = &[
    ("_bench", "evaluator_admin_route"),
    ("BENCH_ADMIN_TOKEN", "evaluator_credential"),
    ("bench-verify", "evaluator_binary"),
    ("bench_admin", "evaluator_admin_identifier"),
    ("tool-bench-oracle", "evaluator_oracle_identity"),
    ("oracle", "evaluator_oracle_reference"),
    ("scenario.toml", "evaluator_scenario_source"),
    ("solution/", "evaluator_solution_artifact"),
    ("result-contracts", "evaluator_result_contract"),
    ("expect.json", "evaluator_contract_artifact"),
    ("reward", "evaluator_scoring_signal"),
    ("evaluator", "evaluator_self_reference"),
    ("grading", "evaluator_grading_reference"),
];

/// Markers that indicate unresolved template content rather than evaluator ownership.
const TEMPLATE_MARKERS: &[&str] = &["{{", "}}"];

// ── Captured surface ─────────────────────────────────────────────────────────

/// A resolved surface captured by the trusted host before or during a trial.
#[derive(Debug, Clone)]
pub struct CapturedSurface {
    pub surface: Surface,
    pub content_sha256: String,
    pub byte_count: usize,
}

impl CapturedSurface {
    pub fn capture(surface: Surface, content: &[u8]) -> Self {
        Self {
            surface,
            content_sha256: sha256_hex(content),
            byte_count: content.len(),
        }
    }
}

// ── Findings ─────────────────────────────────────────────────────────────────

/// A single audit finding, always with a redacted location (byte offset range, not the
/// actual matched content) and an ownership reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub surface: Surface,
    pub surface_sha256: String,
    pub marker_kind: String,
    pub ownership_reason: String,
    /// Byte offset of the match start within the surface content.
    pub redacted_offset: usize,
    /// Length of the matched region in bytes.
    pub redacted_length: usize,
}

// ── Audit result ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// All required surfaces captured and no findings.
    Clean,
    /// At least one finding on a captured surface.
    FindingsPresent,
    /// One or more required surfaces were not captured; the audit is not qualified.
    Unqualified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    pub outcome: AuditOutcome,
    pub surfaces_required: Vec<Surface>,
    pub surfaces_captured: Vec<Surface>,
    pub surfaces_missing: Vec<Surface>,
    pub findings: Vec<Finding>,
    pub captured_hashes: BTreeMap<String, String>,
    pub limits: Vec<String>,
}

// ── Required surfaces per profile ────────────────────────────────────────────

/// Returns the surfaces that must be captured for a given workspace profile.
/// The minimal profile requires fewer surfaces (no workspace files).
pub fn required_surfaces(workspace_profile: &str) -> Vec<Surface> {
    let mut required = vec![
        Surface::InitialUserPrompt,
        Surface::ToolDefinitions,
        Surface::AgentArgv,
        Surface::AgentEnvironment,
        Surface::DnsNames,
        Surface::ServiceHealthRoutes,
        Surface::SessionMetadata,
    ];
    if workspace_profile != "minimal" {
        required.extend([
            Surface::ProfileWorkspace,
            Surface::ProfileHome,
            Surface::WorkspaceSessionConfig,
            Surface::WorkspaceConnectionInventory,
            Surface::WorkspaceAccountIdentity,
        ]);
    }
    required.sort();
    required
}

// ── Core audit logic ─────────────────────────────────────────────────────────

/// Runs the resolved-surface audit over captured surfaces. Every captured surface is
/// scanned for private markers and unresolved templates. Surfaces in `required` that
/// are not in `captured` make the audit unqualified.
///
/// Sensitive marker values are never placed in findings or logs; only redacted byte
/// offsets, lengths, and ownership reasons are recorded.
pub fn audit(required: &[Surface], captured: &[(CapturedSurface, &[u8])]) -> AuditReport {
    let captured_set: BTreeMap<Surface, (&CapturedSurface, &[u8])> = captured
        .iter()
        .map(|(cap, bytes)| (cap.surface, (cap, *bytes)))
        .collect();

    let surfaces_captured: Vec<Surface> = captured_set.keys().copied().collect();
    let surfaces_missing: Vec<Surface> = required
        .iter()
        .filter(|s| !captured_set.contains_key(s))
        .copied()
        .collect();

    let mut findings = Vec::new();
    let mut captured_hashes = BTreeMap::new();

    for (surface, (cap, bytes)) in &captured_set {
        captured_hashes.insert(surface.label().to_owned(), cap.content_sha256.clone());
        let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
        // Scan for evaluator-owned markers.
        for &(marker, reason) in PRIVATE_MARKERS {
            let lower_marker = marker.to_ascii_lowercase();
            let mut search_from = 0;
            while let Some(pos) = text[search_from..].find(&lower_marker) {
                let absolute = search_from + pos;
                findings.push(Finding {
                    surface: *surface,
                    surface_sha256: cap.content_sha256.clone(),
                    marker_kind: "evaluator_owned".into(),
                    ownership_reason: reason.into(),
                    redacted_offset: absolute,
                    redacted_length: marker.len(),
                });
                search_from = absolute + 1;
            }
        }
        // Scan for unresolved templates.
        for marker in TEMPLATE_MARKERS {
            let mut search_from = 0;
            while let Some(pos) = text[search_from..].find(marker) {
                let absolute = search_from + pos;
                findings.push(Finding {
                    surface: *surface,
                    surface_sha256: cap.content_sha256.clone(),
                    marker_kind: "unresolved_template".into(),
                    ownership_reason: "template_placeholder_in_resolved_content".into(),
                    redacted_offset: absolute,
                    redacted_length: marker.len(),
                });
                search_from = absolute + 1;
            }
        }
    }

    let outcome = if !surfaces_missing.is_empty() {
        AuditOutcome::Unqualified
    } else if findings.is_empty() {
        AuditOutcome::Clean
    } else {
        AuditOutcome::FindingsPresent
    };

    AuditReport {
        outcome,
        surfaces_required: required.to_vec(),
        surfaces_captured,
        surfaces_missing,
        findings,
        captured_hashes,
        limits: vec![
            "lexical_marker_scan_only".into(),
            "does_not_certify_model_unawareness".into(),
            "does_not_detect_semantic_leakage".into(),
            "missing_surfaces_are_unqualified_not_passed".into(),
            "sensitive_values_never_in_findings_or_logs".into(),
        ],
    }
}

// ── Trusted-host evidence interface ──────────────────────────────────────────

/// Narrow observer interface for trusted-host audit captures. Another child owns
/// native-inference attestation; this only exposes the evidence capture seam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedHostCapture {
    pub capture_id: String,
    pub surfaces: Vec<CapturedSurfaceRecord>,
    pub audit: AuditReport,
}

/// Serializable record of a captured surface (without the raw bytes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedSurfaceRecord {
    pub surface: Surface,
    pub content_sha256: String,
    pub byte_count: usize,
}

impl From<&CapturedSurface> for CapturedSurfaceRecord {
    fn from(cap: &CapturedSurface) -> Self {
        Self {
            surface: cap.surface,
            content_sha256: cap.content_sha256.clone(),
            byte_count: cap.byte_count,
        }
    }
}

/// Produce a trusted-host capture record from a set of captured surfaces.
/// The `capture_id` should be a unique session/trial identifier.
pub fn trusted_host_capture(
    capture_id: &str,
    required: &[Surface],
    captured: &[(CapturedSurface, &[u8])],
) -> TrustedHostCapture {
    let audit = audit(required, captured);
    TrustedHostCapture {
        capture_id: capture_id.to_owned(),
        surfaces: captured.iter().map(|(cap, _)| cap.into()).collect(),
        audit,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_surface_passes() {
        let content = b"Please file a bug report for the login issue.";
        let cap = CapturedSurface::capture(Surface::InitialUserPrompt, content);
        let report = audit(&[Surface::InitialUserPrompt], &[(cap, content)]);
        assert_eq!(report.outcome, AuditOutcome::Clean);
        assert!(report.findings.is_empty());
        assert!(report.surfaces_missing.is_empty());
    }

    #[test]
    fn evaluator_marker_produces_finding() {
        let content = b"Check the _bench/health endpoint for status.";
        let cap = CapturedSurface::capture(Surface::ServiceHealthRoutes, content);
        let report = audit(&[Surface::ServiceHealthRoutes], &[(cap, content)]);
        assert_eq!(report.outcome, AuditOutcome::FindingsPresent);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].marker_kind, "evaluator_owned");
        assert_eq!(report.findings[0].ownership_reason, "evaluator_admin_route");
        // The finding has a redacted offset, not the actual marker text.
        assert_eq!(report.findings[0].redacted_offset, 10);
        assert_eq!(report.findings[0].redacted_length, 6);
    }

    #[test]
    fn unresolved_template_produces_finding() {
        let content = b"Welcome {{USER}}, your task is ready.";
        let cap = CapturedSurface::capture(Surface::InitialUserPrompt, content);
        let report = audit(&[Surface::InitialUserPrompt], &[(cap, content)]);
        assert_eq!(report.outcome, AuditOutcome::FindingsPresent);
        let template_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.marker_kind == "unresolved_template")
            .collect();
        assert_eq!(template_findings.len(), 2); // {{ and }}
    }

    #[test]
    fn missing_required_surface_is_unqualified() {
        let content = b"Hello world";
        let cap = CapturedSurface::capture(Surface::InitialUserPrompt, content);
        let report = audit(
            &[Surface::InitialUserPrompt, Surface::ToolDefinitions],
            &[(cap, content)],
        );
        assert_eq!(report.outcome, AuditOutcome::Unqualified);
        assert_eq!(report.surfaces_missing, vec![Surface::ToolDefinitions]);
    }

    #[test]
    fn admin_token_marker_detected_case_insensitive() {
        let content = b"Set bench_admin_token=secret123 in environment.";
        let cap = CapturedSurface::capture(Surface::AgentEnvironment, content);
        let report = audit(&[Surface::AgentEnvironment], &[(cap, content)]);
        assert_eq!(report.outcome, AuditOutcome::FindingsPresent);
        let cred_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.ownership_reason == "evaluator_credential")
            .collect();
        assert!(!cred_findings.is_empty());
    }

    #[test]
    fn multiple_surfaces_all_clean() {
        let prompt = b"File the bug.";
        let tools = b"list_issues: Lists project issues.";
        let argv = b"/usr/bin/agent --mode interactive";
        let env = b"PATH=/usr/bin HOME=/root";
        let dns = b"api.linear.app tools";
        let health = b"http://tools:7101/mcp";
        let session = b"{\"session_id\": \"abc-123\"}";

        let surfaces: Vec<(CapturedSurface, &[u8])> = vec![
            (
                CapturedSurface::capture(Surface::InitialUserPrompt, prompt),
                prompt,
            ),
            (
                CapturedSurface::capture(Surface::ToolDefinitions, tools),
                tools,
            ),
            (CapturedSurface::capture(Surface::AgentArgv, argv), argv),
            (
                CapturedSurface::capture(Surface::AgentEnvironment, env),
                env,
            ),
            (CapturedSurface::capture(Surface::DnsNames, dns), dns),
            (
                CapturedSurface::capture(Surface::ServiceHealthRoutes, health),
                health,
            ),
            (
                CapturedSurface::capture(Surface::SessionMetadata, session),
                session,
            ),
        ];
        let required = required_surfaces("minimal");
        let report = audit(&required, &surfaces);
        assert_eq!(report.outcome, AuditOutcome::Clean);
        assert_eq!(report.captured_hashes.len(), 7);
    }

    #[test]
    fn trusted_host_capture_records_all_surfaces() {
        let content = b"Clean content here.";
        let cap = CapturedSurface::capture(Surface::InitialUserPrompt, content);
        let capture = trusted_host_capture(
            "trial-001",
            &[Surface::InitialUserPrompt],
            &[(cap, content)],
        );
        assert_eq!(capture.capture_id, "trial-001");
        assert_eq!(capture.surfaces.len(), 1);
        assert_eq!(capture.audit.outcome, AuditOutcome::Clean);
    }

    #[test]
    fn required_surfaces_differ_by_profile() {
        let minimal = required_surfaces("minimal");
        let support = required_surfaces("support");
        assert!(support.len() > minimal.len());
        assert!(support.contains(&Surface::ProfileWorkspace));
        assert!(!minimal.contains(&Surface::ProfileWorkspace));
    }

    #[test]
    fn findings_never_contain_secret_values() {
        let content = b"BENCH_ADMIN_TOKEN=supersecret123";
        let cap = CapturedSurface::capture(Surface::AgentEnvironment, content);
        let report = audit(&[Surface::AgentEnvironment], &[(cap, content)]);
        for finding in &report.findings {
            // Findings must not contain the actual secret value.
            let serialized = serde_json::to_string(finding).unwrap();
            assert!(!serialized.contains("supersecret123"));
        }
    }

    #[test]
    fn report_limits_are_always_present() {
        let report = audit(&[], &[]);
        assert!(!report.limits.is_empty());
        assert!(
            report
                .limits
                .iter()
                .any(|l| l == "does_not_certify_model_unawareness")
        );
    }
}
