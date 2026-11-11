use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::{
    MATERIALIZATION_SCHEMA, MaterializationManifest, MaterializedDataset, OrganizationBase,
    TaskOverlay, materialize, ordered_hash, snapshot, validate_path_component,
};
use crate::{
    error::{Error, Result},
    provenance::{DatasetSource, canonical_sha256, digest_valid},
    runner::sha256,
};

const MANIFEST: &str = "materialization/manifest.json";
const BASE: &str = "materialization/base.json";
const OVERLAY: &str = "materialization/overlay.json";
const DATASET: &str = "materialization/dataset.json";
const FILES: [&str; 4] = [BASE, DATASET, MANIFEST, OVERLAY];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterializationSpec {
    pub schema_version: u32,
    pub base: String,
    pub overlay: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Default)]
pub struct Bundle {
    pub files: BTreeMap<String, Vec<u8>>,
}

impl MaterializationManifest {
    pub fn validate(&self, scenario: &str) -> std::result::Result<(), String> {
        relative(scenario)?;
        if self.schema_version != MATERIALIZATION_SCHEMA
            || self.task_id != scenario
            || scenario.split('/').count() != 3
            || self.base_id.trim().is_empty()
            || self.base_revision == 0
            || [
                &self.base_sha256,
                &self.overlay_sha256,
                &self.materialized_sha256,
            ]
            .iter()
            .any(|digest| !digest_valid(digest))
            || self
                .source_files
                .keys()
                .map(String::as_str)
                .ne(["base", "overlay"])
        {
            return Err("materialization identity or source hashes are invalid".into());
        }
        for source in self.source_files.values() {
            relative(&source.path)?;
            if !source.path.ends_with(".json") || !digest_valid(&source.sha256) {
                return Err("materialization inputs must be hashed JSON sources".into());
            }
        }
        if !self.source_files["overlay"]
            .path
            .starts_with(&format!("{scenario}/"))
            || self.source_files["base"].path == self.source_files["overlay"].path
        {
            return Err("materialization overlay does not belong to its task".into());
        }
        for (service, collections) in &self.collection_hashes {
            validate_path_component(service).map_err(|error| error.to_string())?;
            for (collection, digest) in collections {
                validate_path_component(collection).map_err(|error| error.to_string())?;
                if !digest_valid(digest) {
                    return Err("materialized collection digest is invalid".into());
                }
            }
        }
        Ok(())
    }
}

fn relative(value: &str) -> std::result::Result<(), String> {
    if value.is_empty()
        || value.contains('\\')
        || value.contains('\0')
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | "..") || part.contains(':'))
    {
        return Err("materialization paths must be canonical relative paths".into());
    }
    Ok(())
}

fn checked_file(root: &Path, name: &str) -> std::result::Result<PathBuf, String> {
    relative(name)?;
    let root_metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("materialization source root must be an ordinary directory".into());
    }
    let mut current = root.to_owned();
    let components: Vec<_> = name.split('/').collect();
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink()
            || index + 1 < components.len() && !metadata.is_dir()
            || index + 1 == components.len() && !metadata.is_file()
        {
            return Err("materialization inputs cannot contain links or special files".into());
        }
    }
    Ok(current)
}

fn parse<Type: DeserializeOwned>(bytes: &[u8]) -> std::result::Result<Type, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let value = crate::json_value::parse(text).map_err(|error| error.to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn encoded(value: &impl Serialize) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).expect("materialization is JSON");
    bytes.push(b'\n');
    bytes
}

pub(crate) fn load(
    root: &Path,
    scenario: &str,
    spec: &MaterializationSpec,
) -> Result<(MaterializedDataset, Bundle)> {
    let fail = |message| Error::Dataset {
        path: root.to_owned(),
        message,
    };
    if spec.schema_version != MATERIALIZATION_SCHEMA {
        return Err(fail(
            "unsupported materialization declaration version".into(),
        ));
    }
    relative(scenario).map_err(fail)?;
    if scenario.split('/').count() != 3 {
        return Err(fail(
            "materialized scenarios require group/service/task identities".into(),
        ));
    }
    let source_root = root
        .ancestors()
        .nth(3)
        .ok_or_else(|| fail("materialized scenario is outside its source tree".into()))?;
    if root.strip_prefix(source_root).ok() != Some(Path::new(scenario)) {
        return Err(fail(
            "materialized scenario identity differs from its source path".into(),
        ));
    }
    checked_file(source_root, &format!("{scenario}/scenario.toml")).map_err(fail)?;
    relative(&spec.base).map_err(fail)?;
    relative(&spec.overlay).map_err(fail)?;
    let overlay_name = format!("{scenario}/{}", spec.overlay);
    let base_path = checked_file(source_root, &spec.base).map_err(fail)?;
    let overlay_path = checked_file(source_root, &overlay_name).map_err(fail)?;
    let read = |path: &Path| {
        fs::read(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })
    };
    let base_bytes = read(&base_path)?;
    let overlay_bytes = read(&overlay_path)?;
    let base: OrganizationBase = parse(&base_bytes).map_err(fail)?;
    let overlay: TaskOverlay = parse(&overlay_bytes).map_err(fail)?;
    if overlay.task_id != scenario {
        return Err(fail(
            "materialization overlay belongs to another task".into(),
        ));
    }
    let mut resolved = materialize(&base, &overlay).map_err(|error| fail(error.to_string()))?;
    resolved.manifest.source_files = BTreeMap::from([
        (
            "base".into(),
            SourceFile {
                path: spec.base.clone(),
                sha256: sha256(&base_bytes),
            },
        ),
        (
            "overlay".into(),
            SourceFile {
                path: overlay_name,
                sha256: sha256(&overlay_bytes),
            },
        ),
    ]);
    resolved.manifest.validate(scenario).map_err(fail)?;
    let bundle = Bundle {
        files: BTreeMap::from([
            (BASE.into(), base_bytes),
            (OVERLAY.into(), overlay_bytes),
            (MANIFEST.into(), encoded(&resolved.manifest)),
            (DATASET.into(), encoded(&snapshot(&resolved.stores))),
        ]),
    };
    Ok((resolved, bundle))
}

pub fn bundle_from_directory(
    root: &Path,
    expected: &BTreeMap<String, String>,
) -> std::result::Result<Bundle, String> {
    if expected.is_empty() {
        return Ok(Bundle::default());
    }
    if expected.keys().map(String::as_str).ne(FILES) {
        return Err("materialization requires its complete four-file verifier bundle".into());
    }
    let mut files = BTreeMap::new();
    for name in FILES {
        let path = checked_file(root, name)?;
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        if expected.get(name) != Some(&sha256(&bytes)) {
            return Err(format!("materialization artifact bytes changed: {name}"));
        }
        files.insert(name.into(), bytes);
    }
    Ok(Bundle { files })
}

pub fn validate_bundle(
    source: &DatasetSource,
    expected: &BTreeMap<String, String>,
    bundle: &Bundle,
) -> std::result::Result<(), String> {
    validate_context(source, expected)?;
    let Some(declared) = &source.materialization else {
        return if bundle.files.is_empty() {
            Ok(())
        } else {
            Err("standalone dataset cannot claim a materialization bundle".into())
        };
    };
    if expected.keys().map(String::as_str).ne(FILES)
        || bundle.files.keys().map(String::as_str).ne(FILES)
    {
        return Err("source-bound materialization bundle is incomplete".into());
    }
    for name in FILES {
        if expected.get(name) != Some(&sha256(&bundle.files[name])) {
            return Err("materialization artifact hash differs from its task context".into());
        }
    }
    let manifest: MaterializationManifest = parse(&bundle.files[MANIFEST])?;
    if manifest != *declared || canonical_sha256(&manifest) != source.dataset_sha256 {
        return Err("materialization manifest differs from the world dataset source".into());
    }
    if sha256(&bundle.files[BASE]) != manifest.source_files["base"].sha256
        || sha256(&bundle.files[OVERLAY]) != manifest.source_files["overlay"].sha256
    {
        return Err("materialization inputs differ from the authored source bytes".into());
    }
    let base: OrganizationBase = parse(&bundle.files[BASE])?;
    let overlay: TaskOverlay = parse(&bundle.files[OVERLAY])?;
    let mut replayed = materialize(&base, &overlay).map_err(|error| error.to_string())?;
    replayed.manifest.source_files = manifest.source_files;
    if replayed.manifest != *declared {
        return Err("materialization does not replay the declared source manifest".into());
    }
    let stored: Value = parse(&bundle.files[DATASET])?;
    if ordered_hash(&stored) != declared.materialized_sha256 || stored != snapshot(&replayed.stores)
    {
        return Err("materialized dataset artifact differs from its base and overlay".into());
    }
    Ok(())
}

pub fn validate_context(
    source: &DatasetSource,
    expected: &BTreeMap<String, String>,
) -> std::result::Result<(), String> {
    source.validate(&source.scenario)?;
    match &source.materialization {
        Some(_)
            if expected.keys().map(String::as_str).eq(FILES)
                && expected.values().all(|digest| digest_valid(digest)) =>
        {
            Ok(())
        }
        None if expected.is_empty() => Ok(()),
        _ => Err("dataset source and private materialization context differ".into()),
    }
}
