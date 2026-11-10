use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{grading::CheckKind, provenance::canonical_sha256, runner::sha256};

pub const REVIEW_PROTOCOL: &str = "tool-bench-content-review-v1";
const SIGNATURE_DOMAIN: &[u8] = b"tool-bench-content-review-signature-v1\0";
const MAX_REVIEWERS: usize = 32;
const MAX_ISSUES: usize = 256;
const MAX_REASON_BYTES: usize = 8192;
const MAX_CONTEXT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerKind {
    Human,
    ScriptedFixture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedReviewer {
    pub public_key: String,
    pub kind: ReviewerKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPolicy {
    pub schema_version: u32,
    pub protocol: String,
    pub reviewers: BTreeMap<String, ApprovedReviewer>,
}

fn unhex<const SIZE: usize>(value: &str) -> Result<[u8; SIZE], String> {
    if value.len() != SIZE * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected a fixed-length lowercase hexadecimal value".into());
    }
    let mut output = [0; SIZE];
    for (index, item) in output.iter_mut().enumerate() {
        *item = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid hexadecimal value")?;
    }
    Ok(output)
}

pub fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|value| unhex::<32>(value).is_ok())
}

fn reviewer_key(reviewer: &ApprovedReviewer) -> Result<VerifyingKey, String> {
    let bytes = unhex::<32>(&reviewer.public_key)?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid reviewer public key")?;
    if key.is_weak() || key.to_edwards().compress().to_bytes() != bytes {
        return Err("weak or noncanonical reviewer public keys are not admitted".into());
    }
    Ok(key)
}

impl ReviewPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.protocol != REVIEW_PROTOCOL {
            return Err("unsupported content-review policy".into());
        }
        if self.reviewers.len() < 2 || self.reviewers.len() > MAX_REVIEWERS {
            return Err(
                "content review requires a bounded quorum of at least two reviewers".into(),
            );
        }
        let mut keys = BTreeSet::new();
        let mut kind = None;
        for (name, reviewer) in &self.reviewers {
            if name.is_empty()
                || name.len() > 64
                || !name.as_bytes()[0].is_ascii_alphanumeric()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            {
                return Err("reviewer identifiers must be bounded safe identifiers".into());
            }
            let key = reviewer_key(reviewer)?;
            if !keys.insert(key.to_bytes()) {
                return Err("a reviewer public key cannot occupy multiple quorum slots".into());
            }
            if kind.is_some_and(|previous| previous != reviewer.kind) {
                return Err(
                    "human reviews and scripted fixtures are distinct review conditions".into(),
                );
            }
            kind = Some(reviewer.kind);
        }
        Ok(())
    }

    pub fn sha256(&self) -> Result<String, String> {
        self.validate()?;
        Ok(canonical_sha256(self))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewIssue {
    pub check: CheckKind,
    pub location: String,
    pub criterion: String,
    pub scope: Value,
    pub evidence: Vec<u64>,
    pub event_evidence: Vec<u64>,
}

impl ReviewIssue {
    pub fn id(&self) -> String {
        canonical_sha256(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewContext {
    pub initial_state: Value,
    pub final_state: Value,
    pub operations: Value,
    pub legacy_answer_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewCase {
    pub schema_version: u32,
    pub protocol: String,
    pub policy_sha256: String,
    pub bindings: BTreeMap<String, String>,
    pub instruction: String,
    pub context: ReviewContext,
    pub issues: BTreeMap<String, ReviewIssue>,
}

impl ReviewCase {
    pub fn validate(&self, policy: &ReviewPolicy) -> Result<(), String> {
        if self.schema_version != 1
            || self.protocol != REVIEW_PROTOCOL
            || self.policy_sha256 != policy.sha256()?
        {
            return Err("review case does not match its approved policy".into());
        }
        let mut required: BTreeSet<_> = [
            "assessment",
            "contract",
            "evidence",
            "instruction",
            "manifest",
            "review-implementation",
            "result",
            "runner-admission",
            "source",
            "task-context",
            "verification",
        ]
        .into_iter()
        .collect();
        if self.context.legacy_answer_file.is_some() {
            required.insert("answer");
        }
        if self
            .bindings
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != required
            || self.bindings.values().any(|value| !valid_digest(value))
            || self.bindings["instruction"] != sha256(self.instruction.as_bytes())
            || self
                .context
                .legacy_answer_file
                .as_ref()
                .is_some_and(|answer| {
                    self.bindings.get("answer") != Some(&sha256(answer.as_bytes()))
                })
        {
            return Err("review case lacks complete immutable source/evidence bindings".into());
        }
        if self.instruction.trim().is_empty()
            || self.instruction.len() > 64 * 1024
            || self.issues.is_empty()
            || self.issues.len() > MAX_ISSUES
            || serde_json::to_vec(self)
                .map_err(|_| "review case is not serializable")?
                .len()
                > MAX_CONTEXT_BYTES
        {
            return Err("review case is empty or exceeds review resource bounds".into());
        }
        for (identity, issue) in &self.issues {
            let declared_scope = if issue.scope["kind"] == "record_semantics" {
                issue.check == CheckKind::Records
                    && ((issue.scope["coverage"]
                        == "all_applicable_content_and_metric_requirements"
                        && issue.scope["phase"] == "final_semantics"
                        && issue.scope["declarations"]["metric_alarm"].is_object())
                        || (issue.scope["coverage"]
                            == "all_applicable_content_and_record_semantic_requirements"
                            && matches!(
                                issue.scope["phase"].as_str(),
                                Some("final_semantics" | "changed_committed_semantics")
                            )
                            && (issue.scope["declarations"]["label_matchers"].is_object()
                                || issue.scope["declarations"]["timestamp_fields"]
                                    .as_object()
                                    .is_some_and(|fields| !fields.is_empty()))))
            } else {
                issue.scope["coverage"] == "all_applicable_content_requirements"
                    && issue.scope["kind"]
                        == match issue.check {
                            CheckKind::State => "state_content",
                            CheckKind::Answer => "answer_content",
                            _ => "record_content",
                        }
                    && matches!(
                        issue.scope["phase"].as_str(),
                        Some("final_content" | "changed_nonempty_content_at_recorded_commits")
                    )
            };
            let answer_scope = issue.check != CheckKind::Answer
                || (issue.location == "answer"
                    && issue.scope["phase"] == "final_content"
                    && self.context.legacy_answer_file.is_some()
                    && (issue.scope["declarations"]["sentry_dsn"].is_object()
                        ^ issue.scope["declarations"]["oncall"].is_object()));
            if identity != &issue.id()
                || !matches!(
                    issue.check,
                    CheckKind::State | CheckKind::Records | CheckKind::Answer
                )
                || issue.location.trim().is_empty()
                || issue.location.len() > 2048
                || issue.criterion.trim().is_empty()
                || issue.criterion.len() > MAX_REASON_BYTES
                || !declared_scope
                || !answer_scope
                || !issue.scope["declarations"]
                    .as_object()
                    .is_some_and(|declarations| !declarations.is_empty())
            {
                return Err(
                    "review cases may resolve only identified unresolved content checks".into(),
                );
            }
        }
        Ok(())
    }

    pub fn sha256(&self, policy: &ReviewPolicy) -> Result<String, String> {
        self.validate(policy)?;
        Ok(canonical_sha256(self))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentDecision {
    Satisfied,
    Violated,
    Abstain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueDecision {
    pub decision: ContentDecision,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewStatement {
    pub schema_version: u32,
    pub protocol: String,
    pub case_sha256: String,
    pub policy_sha256: String,
    pub reviewer: String,
    pub decisions: BTreeMap<String, IssueDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedReview {
    pub statement: ReviewStatement,
    pub signature: String,
}

pub fn signing_bytes(statement: &ReviewStatement) -> Vec<u8> {
    let payload = canonical_sha256(statement);
    let mut message = SIGNATURE_DOMAIN.to_vec();
    message.extend_from_slice(payload.as_bytes());
    message
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewResolution {
    Satisfied,
    Violated,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedReviews {
    pub protocol: String,
    pub policy_sha256: String,
    pub case_sha256: String,
    pub reviewer_kind: ReviewerKind,
    pub reviewers: Vec<String>,
    pub decisions: BTreeMap<String, ReviewResolution>,
}

pub fn verify_reviews(
    policy: &ReviewPolicy,
    case: &ReviewCase,
    reviews: &[SignedReview],
) -> Result<VerifiedReviews, String> {
    let case_digest = case.sha256(policy)?;
    if reviews.len() > policy.reviewers.len() {
        return Err("review inventory exceeds the approved reviewer registry".into());
    }
    let mut seen = BTreeSet::new();
    let mut votes: BTreeMap<_, Vec<_>> = case
        .issues
        .keys()
        .map(|identity| (identity.clone(), Vec::new()))
        .collect();
    for review in reviews {
        let statement = &review.statement;
        if statement.schema_version != 1
            || statement.protocol != REVIEW_PROTOCOL
            || statement.case_sha256 != case_digest
            || statement.policy_sha256 != case.policy_sha256
            || !statement.decisions.keys().eq(case.issues.keys())
            || !seen.insert(statement.reviewer.clone())
        {
            return Err(
                "review is repeated, incomplete, unsupported or bound to another case/policy"
                    .into(),
            );
        }
        let reviewer = policy
            .reviewers
            .get(&statement.reviewer)
            .ok_or("reviewer is not approved by the prespecified policy")?;
        let signature = Signature::from_slice(&unhex::<64>(&review.signature)?)
            .map_err(|_| "malformed content-review signature")?;
        reviewer_key(reviewer)?
            .verify_strict(&signing_bytes(statement), &signature)
            .map_err(|_| "content-review signature authentication failed")?;
        for (identity, decision) in &statement.decisions {
            if decision.rationale.trim().is_empty() || decision.rationale.len() > MAX_REASON_BYTES {
                return Err("each review decision needs a bounded nonempty rationale".into());
            }
            votes
                .get_mut(identity)
                .ok_or("review names an unknown content check")?
                .push(decision.decision);
        }
    }
    let decisions = votes
        .into_iter()
        .map(|(identity, values)| {
            let decision = if values.len() < policy.reviewers.len() {
                ReviewResolution::Unresolved
            } else if values
                .iter()
                .all(|value| *value == ContentDecision::Satisfied)
            {
                ReviewResolution::Satisfied
            } else if values
                .iter()
                .all(|value| *value == ContentDecision::Violated)
            {
                ReviewResolution::Violated
            } else {
                ReviewResolution::Unresolved
            };
            (identity, decision)
        })
        .collect();
    Ok(VerifiedReviews {
        protocol: REVIEW_PROTOCOL.into(),
        policy_sha256: case.policy_sha256.clone(),
        case_sha256: case_digest,
        reviewer_kind: policy
            .reviewers
            .values()
            .next()
            .ok_or("missing reviewer registry")?
            .kind,
        reviewers: seen.into_iter().collect(),
        decisions,
    })
}
