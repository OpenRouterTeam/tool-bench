use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, bail};
use ed25519_dalek::{Signer, SigningKey};
use mock_core::review::{
    IssueDecision, REVIEW_PROTOCOL, ReviewCase, ReviewPolicy, ReviewResolution, ReviewStatement,
    ReviewerKind, SignedReview, hex, signing_bytes, verify_reviews,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Outcome, Trial,
    provenance::load_verified,
    review::{ReviewedAssessment, implementation_sha256, prepare, prepare_verified},
};

const MAX_JSON_BYTES: u64 = 32 * 1024 * 1024;
const MAX_REVIEW_BYTES: u64 = 4 * 1024 * 1024;

pub fn read_bytes(path: &Path, limit: u64) -> anyhow::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("reading review input {}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        bail!(
            "review input must be a bounded regular file: {}",
            path.display()
        );
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("review input grew beyond its resource bound");
    }
    Ok(bytes)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    serde_json::from_slice(&read_bytes(path, MAX_JSON_BYTES)?)
        .map_err(|_| anyhow::anyhow!("malformed review JSON in {}", path.display()))
}

pub fn read_policy(path: &Path) -> anyhow::Result<ReviewPolicy> {
    let policy: ReviewPolicy = serde_json::from_slice(&read_bytes(path, 64 * 1024)?)
        .map_err(|_| anyhow::anyhow!("malformed review policy"))?;
    policy.validate().map_err(anyhow::Error::msg)?;
    Ok(policy)
}

fn output_path(path: &Path) -> anyhow::Result<PathBuf> {
    let name = path
        .file_name()
        .context("review output needs a file name")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    Ok(parent.join(name))
}

pub fn require_external(path: &Path, trial_root: &Path) -> anyhow::Result<()> {
    let path = if path.exists() {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            bail!("review paths cannot be symbolic links");
        }
        path.canonicalize()?
    } else {
        output_path(path)?
    };
    if path.starts_with(trial_root.canonicalize()?) {
        bail!("review outputs and stores must be outside collected trial/run trees");
    }
    Ok(())
}

pub fn write_new(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let path = output_path(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .with_context(|| format!("creating a new review output {}", path.display()))?;
    let written = (|| -> anyhow::Result<()> {
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    })();
    if written.is_err() {
        let _ = fs::remove_file(&path);
    }
    written
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateReviewKey {
    schema_version: u32,
    algorithm: String,
    seed: String,
}

#[derive(Serialize)]
pub struct PublicReviewKey {
    pub schema_version: u32,
    pub algorithm: String,
    pub public_key: String,
}

#[cfg(unix)]
pub fn generate_key(private_path: &Path, public_path: &Path) -> anyhow::Result<PublicReviewKey> {
    use std::os::unix::fs::PermissionsExt;
    let private_path = output_path(private_path)?;
    let public_path = output_path(public_path)?;
    if private_path == public_path || private_path.exists() || public_path.exists() {
        bail!("key outputs must be distinct new files");
    }
    if fs::metadata(private_path.parent().context("private key parent")?)?
        .permissions()
        .mode()
        & 0o077
        != 0
    {
        bail!("private review keys require an operator-owned private directory");
    }
    let key = SigningKey::generate(&mut rand_core::OsRng);
    let public = PublicReviewKey {
        schema_version: 1,
        algorithm: "ed25519".into(),
        public_key: hex(&key.verifying_key().to_bytes()),
    };
    write_new(
        &private_path,
        &PrivateReviewKey {
            schema_version: 1,
            algorithm: "ed25519".into(),
            seed: hex(&key.to_bytes()),
        },
    )?;
    if let Err(error) = write_new(&public_path, &public) {
        let _ = fs::remove_file(&private_path);
        return Err(error);
    }
    Ok(public)
}

#[cfg(not(unix))]
pub fn generate_key(_private_path: &Path, _public_path: &Path) -> anyhow::Result<PublicReviewKey> {
    bail!("private-key file handling is not validated on this host platform")
}

fn private_key(path: &Path) -> anyhow::Result<SigningKey> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::symlink_metadata(path)?.permissions().mode() & 0o077 != 0 {
            bail!("private review key permissions must exclude group and other access");
        }
    }
    #[cfg(not(unix))]
    bail!("private-key file handling is not validated on this host platform");
    let key: PrivateReviewKey = serde_json::from_slice(&read_bytes(path, 4096)?)
        .map_err(|_| anyhow::anyhow!("malformed private review key"))?;
    if key.schema_version != 1
        || key.algorithm != "ed25519"
        || key.seed.len() != 64
        || !key
            .seed
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("unsupported or malformed private review key");
    }
    let mut seed = [0; 32];
    for (index, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&key.seed[index * 2..index * 2 + 2], 16)
            .map_err(|_| anyhow::anyhow!("malformed private review key"))?;
    }
    Ok(SigningKey::from_bytes(&seed))
}

pub fn sign(
    case: &ReviewCase,
    policy: &ReviewPolicy,
    reviewer: &str,
    key_path: &Path,
    decisions: BTreeMap<String, IssueDecision>,
) -> anyhow::Result<SignedReview> {
    case.validate(policy).map_err(anyhow::Error::msg)?;
    let approved = policy
        .reviewers
        .get(reviewer)
        .context("reviewer is not approved")?;
    let key = private_key(key_path)?;
    if hex(&key.verifying_key().to_bytes()) != approved.public_key {
        bail!("private key does not match the predeclared reviewer");
    }
    let statement = ReviewStatement {
        schema_version: 1,
        protocol: REVIEW_PROTOCOL.into(),
        case_sha256: case.sha256(policy).map_err(anyhow::Error::msg)?,
        policy_sha256: policy.sha256().map_err(anyhow::Error::msg)?,
        reviewer: reviewer.into(),
        decisions,
    };
    let signed = SignedReview {
        signature: hex(&key.sign(&signing_bytes(&statement)).to_bytes()),
        statement,
    };
    verify_reviews(policy, case, std::slice::from_ref(&signed)).map_err(anyhow::Error::msg)?;
    Ok(signed)
}

pub fn read_reviews(
    root: &Path,
    case: &ReviewCase,
    policy: &ReviewPolicy,
) -> anyhow::Result<Vec<SignedReview>> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("review store must be an external regular directory");
    }
    let digest = case.sha256(policy).map_err(anyhow::Error::msg)?;
    let directory = root
        .canonicalize()?
        .join(digest.strip_prefix("sha256:").context("case digest")?);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("review case store cannot be a link or non-directory");
    }
    let mut reviews = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .context("invalid reviewer file name")?;
        if path.extension().and_then(|extension| extension.to_str()) != Some("json")
            || !policy.reviewers.contains_key(name)
        {
            bail!("review store contains an unapproved entry");
        }
        let signed: SignedReview = serde_json::from_slice(&read_bytes(&path, MAX_REVIEW_BYTES)?)
            .map_err(|_| anyhow::anyhow!("malformed signed review"))?;
        if signed.statement.reviewer != name {
            bail!("review file identity differs from its signed reviewer");
        }
        reviews.push(signed);
    }
    reviews.sort_by(|left, right| left.statement.reviewer.cmp(&right.statement.reviewer));
    Ok(reviews)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewAudit {
    pub task: String,
    pub session_id: String,
    pub protocol: String,
    pub policy_sha256: String,
    pub reviewer_kind: ReviewerKind,
    pub implementation_sha256: String,
    pub raw_outcome: Outcome,
    pub raw_reward: f64,
    pub effective_outcome: Outcome,
    pub effective_reward: f64,
    pub case_sha256: Option<String>,
    pub reviewers: Vec<String>,
    pub decisions: BTreeMap<String, ReviewResolution>,
    pub signed_review_sha256: BTreeMap<String, String>,
}

impl ReviewAudit {
    pub fn pending(&self) -> bool {
        self.decisions
            .values()
            .any(|decision| *decision == ReviewResolution::Unresolved)
    }
}

pub fn verify(
    trial: &Path,
    instruction: &str,
    policy: &ReviewPolicy,
    store: &Path,
) -> anyhow::Result<ReviewedAssessment> {
    require_external(store, trial)?;
    let prepared = prepare(trial, instruction, policy)?;
    let reviews = read_reviews(store, prepared.case(), policy)?;
    prepared.resolve(policy, &reviews)
}

pub fn load_trials(
    root: &Path,
    policy: &ReviewPolicy,
    store: &Path,
    instructions: &Path,
) -> anyhow::Result<Vec<Trial>> {
    policy.validate().map_err(anyhow::Error::msg)?;
    require_external(store, root)?;
    let store_metadata = fs::symlink_metadata(store)?;
    if !store_metadata.is_dir() || store_metadata.file_type().is_symlink() {
        bail!("review reporting requires an external regular review-store directory");
    }
    let mut files = Vec::new();
    crate::walk(root, &mut files)?;
    files.sort();
    let mut trials = Vec::new();
    for file in files {
        let bytes = read_bytes(&file, MAX_JSON_BYTES)?;
        let result: serde_json::Value = serde_json::from_slice(&bytes)?;
        if !result["exception_info"].is_null() {
            bail!("failed trials cannot be promoted by review");
        }
        let directory = file.parent().context("trial directory")?;
        let verified = load_verified(directory, &result)?
            .context("review reporting requires checked source and execution provenance")?;
        let provenance = verified.provenance.clone();
        let policy_sha256 = policy.sha256().map_err(anyhow::Error::msg)?;
        if provenance.manifest.context.review_policy_sha256.as_ref() != Some(&policy_sha256) {
            bail!("every compared trial must bind the exact predeclared review policy");
        }
        let task = provenance.source.scenario.replace('/', "__");
        if Path::new(&task)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
            || task.is_empty()
        {
            bail!("unsafe task identity for instruction lookup");
        }
        let raw = verified.assessment.clone();
        let raw_outcome = Outcome::from_code(Some(raw.outcome.code()));
        let mut audit = ReviewAudit {
            task: task.clone(),
            session_id: provenance.manifest.session_id.clone(),
            protocol: REVIEW_PROTOCOL.into(),
            policy_sha256,
            reviewer_kind: policy
                .reviewers
                .values()
                .next()
                .context("reviewer registry")?
                .kind,
            implementation_sha256: implementation_sha256(),
            raw_outcome,
            raw_reward: raw.reward,
            effective_outcome: raw_outcome,
            effective_reward: raw.reward,
            case_sha256: None,
            reviewers: Vec::new(),
            decisions: BTreeMap::new(),
            signed_review_sha256: BTreeMap::new(),
        };
        if raw.adjudication_needed {
            let instruction = String::from_utf8(read_bytes(
                &instructions.join(&task).join("instruction.md"),
                64 * 1024,
            )?)?;
            let prepared = prepare_verified(verified, &bytes, &instruction, policy)?;
            let reviews = read_reviews(store, prepared.case(), policy)?;
            let reviewed = prepared.resolve(policy, &reviews)?;
            audit.effective_outcome = Outcome::from_code(Some(reviewed.effective.outcome.code()));
            audit.effective_reward = reviewed.effective.reward;
            audit.case_sha256 = Some(reviewed.reviews.case_sha256);
            audit.reviewers = reviewed.reviews.reviewers;
            audit.decisions = reviewed.reviews.decisions;
            audit.signed_review_sha256 = reviewed.signed_review_sha256;
        }
        trials.push(Trial {
            task,
            reward: audit.effective_reward,
            outcome: audit.effective_outcome,
            provenance: Some(provenance),
            review: Some(audit),
        });
    }
    Ok(trials)
}
