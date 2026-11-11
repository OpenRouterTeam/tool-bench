//! Immutable organization bases with explicit task overlays, materialized into
//! complete, independently inspectable datasets with deterministic hashes.
//!
//! Design invariants:
//! - Base edits invalidate affected materialized hashes.
//! - Parallel materializations/trials cannot mutate each other or sources.
//! - Symlinks and path traversal are rejected.
//! - Ambiguous key collisions within a single collection are rejected.
//! - Standalone datasets and historical source digests remain usable (legacy compat).

use std::collections::{BTreeMap, BTreeSet};

use crate::store::Store;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod source;
pub use source::{Bundle, SourceFile, bundle_from_directory, validate_bundle, validate_context};
pub(crate) use source::{MaterializationSpec, load};

pub const MATERIALIZATION_SCHEMA: u32 = 1;

/// An immutable organization base: shared data that multiple tasks can reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OrganizationBase {
    pub schema_version: u32,
    pub id: String,
    pub revision: u32,
    pub description: String,
    /// service_name -> collection_name -> documents
    pub datasets: BTreeMap<String, BTreeMap<String, Vec<Value>>>,
}

/// A task-specific overlay applied on top of an organization base.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskOverlay {
    pub schema_version: u32,
    pub task_id: String,
    pub base_id: String,
    pub base_revision: u32,
    /// Documents to insert or replace in the materialized dataset.
    #[serde(default)]
    pub inserts: BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    /// Keys to remove from the materialized dataset.
    #[serde(default)]
    pub removes: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

/// The cryptographic identity of a materialized dataset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializationManifest {
    pub schema_version: u32,
    pub base_id: String,
    pub base_revision: u32,
    pub base_sha256: String,
    pub task_id: String,
    pub overlay_sha256: String,
    pub materialized_sha256: String,
    pub collection_hashes: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_files: BTreeMap<String, SourceFile>,
}

/// Result of materializing an organization base + task overlay.
#[derive(Debug)]
pub(crate) struct MaterializedDataset {
    pub stores: BTreeMap<String, Store>,
    pub manifest: MaterializationManifest,
}

#[derive(Debug, thiserror::Error)]
enum MaterializationError {
    #[error("path traversal or symlink rejected: {0}")]
    PathTraversal(String),
    #[error("ambiguous key collision in {service}/{collection}: key {key:?}")]
    KeyCollision {
        service: String,
        collection: String,
        key: String,
    },
    #[error(
        "overlay references base {expected_id} rev {expected_rev}, got {actual_id} rev {actual_rev}"
    )]
    BaseMismatch {
        expected_id: String,
        expected_rev: u32,
        actual_id: String,
        actual_rev: u32,
    },
    #[error("document missing key in {service}/{collection} at index {index}")]
    MissingDocumentKey {
        service: String,
        collection: String,
        index: usize,
    },
    #[error("remove target key {key:?} not found in {service}/{collection}")]
    RemoveTargetMissing {
        service: String,
        collection: String,
        key: String,
    },
    #[error("{0}")]
    Other(String),
}

/// Validates that a path component is safe (no traversal, no symlinks).
fn validate_path_component(component: &str) -> Result<(), MaterializationError> {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.contains('/')
        || component.contains('\\')
        || component.contains('\0')
    {
        return Err(MaterializationError::PathTraversal(component.to_owned()));
    }
    Ok(())
}

/// Extracts a document key: `_key` wins, then `id`.
fn document_key(doc: &mut Value) -> Option<String> {
    let obj = doc.as_object_mut()?;
    if let Some(key) = obj.shift_remove("_key") {
        return Some(match key {
            Value::String(s) => s,
            other => other.to_string(),
        });
    }
    match obj.get("id")? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Compute a deterministic hash of an organization base's content.
fn hash_base(base: &OrganizationBase) -> String {
    crate::provenance::canonical_sha256(base)
}

/// Compute a deterministic hash of a task overlay's content.
fn hash_overlay(overlay: &TaskOverlay) -> String {
    crate::provenance::canonical_sha256(overlay)
}

/// Materialize a complete dataset from an organization base + task overlay.
///
/// Returns an independent copy that callers can use for a trial without
/// mutating the source base or overlay.
fn materialize(
    base: &OrganizationBase,
    overlay: &TaskOverlay,
) -> Result<MaterializedDataset, MaterializationError> {
    if base.schema_version != MATERIALIZATION_SCHEMA
        || overlay.schema_version != MATERIALIZATION_SCHEMA
        || base.id.trim().is_empty()
        || base.revision == 0
        || overlay.task_id.trim().is_empty()
    {
        return Err(MaterializationError::Other(
            "unsupported or incomplete materialization identity".into(),
        ));
    }
    if overlay.base_id != base.id || overlay.base_revision != base.revision {
        return Err(MaterializationError::BaseMismatch {
            expected_id: overlay.base_id.clone(),
            expected_rev: overlay.base_revision,
            actual_id: base.id.clone(),
            actual_rev: base.revision,
        });
    }

    for service in base
        .datasets
        .keys()
        .chain(overlay.inserts.keys())
        .chain(overlay.removes.keys())
    {
        validate_path_component(service)?;
    }
    for collections in base.datasets.values().chain(overlay.inserts.values()) {
        for collection in collections.keys() {
            validate_path_component(collection)?;
        }
    }
    for collections in overlay.removes.values() {
        for collection in collections.keys() {
            validate_path_component(collection)?;
        }
    }

    let base_sha256 = hash_base(base);
    let overlay_sha256 = hash_overlay(overlay);

    let mut stores: BTreeMap<String, Store> = BTreeMap::new();
    let mut seen_keys: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();

    for (service, collections) in &base.datasets {
        let store = stores.entry(service.clone()).or_default();
        for (coll_name, docs) in collections {
            let collection = store.collection_mut(coll_name);
            let keys = seen_keys
                .entry(service.clone())
                .or_default()
                .entry(coll_name.clone())
                .or_default();
            for (index, doc) in docs.iter().enumerate() {
                let mut doc = doc.clone();
                let key = document_key(&mut doc).ok_or_else(|| {
                    MaterializationError::MissingDocumentKey {
                        service: service.clone(),
                        collection: coll_name.clone(),
                        index,
                    }
                })?;
                if !keys.insert(key.clone()) {
                    return Err(MaterializationError::KeyCollision {
                        service: service.clone(),
                        collection: coll_name.clone(),
                        key,
                    });
                }
                collection.insert(key, doc);
            }
        }
    }

    for (service, collections) in &overlay.removes {
        for (coll_name, keys_to_remove) in collections {
            let store = stores.entry(service.clone()).or_default();
            let collection = store.collection_mut(coll_name);
            for key in keys_to_remove {
                if collection.remove(key).is_none() {
                    return Err(MaterializationError::RemoveTargetMissing {
                        service: service.clone(),
                        collection: coll_name.clone(),
                        key: key.clone(),
                    });
                }
                if let Some(key_set) = seen_keys
                    .get_mut(service)
                    .and_then(|c| c.get_mut(coll_name.as_str()))
                {
                    key_set.remove(key);
                }
            }
        }
    }

    for (service, collections) in &overlay.inserts {
        let store = stores.entry(service.clone()).or_default();
        for (coll_name, docs) in collections {
            let mut overlay_keys = BTreeSet::new();
            let collection = store.collection_mut(coll_name);
            let keys = seen_keys
                .entry(service.clone())
                .or_default()
                .entry(coll_name.clone())
                .or_default();
            for (index, doc) in docs.iter().enumerate() {
                let mut doc = doc.clone();
                let key = document_key(&mut doc).ok_or_else(|| {
                    MaterializationError::MissingDocumentKey {
                        service: service.clone(),
                        collection: coll_name.clone(),
                        index,
                    }
                })?;
                if !overlay_keys.insert(key.clone()) {
                    return Err(MaterializationError::KeyCollision {
                        service: service.clone(),
                        collection: coll_name.clone(),
                        key,
                    });
                }
                keys.insert(key.clone());
                collection.insert(key, doc);
            }
        }
    }

    let mut collection_hashes: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (service, store) in &stores {
        let mut service_hashes = BTreeMap::new();
        for coll_name in store.collection_names() {
            if let Some(collection) = store.collection(coll_name) {
                let documents = Value::Object(
                    collection
                        .iter()
                        .map(|(key, value)| (key.to_owned(), value.clone()))
                        .collect(),
                );
                service_hashes.insert(coll_name.to_owned(), ordered_hash(&documents));
            }
        }
        if !service_hashes.is_empty() {
            collection_hashes.insert(service.clone(), service_hashes);
        }
    }

    let manifest = MaterializationManifest {
        schema_version: MATERIALIZATION_SCHEMA,
        base_id: base.id.clone(),
        base_revision: base.revision,
        base_sha256,
        task_id: overlay.task_id.clone(),
        overlay_sha256,
        materialized_sha256: ordered_hash(&snapshot(&stores)),
        collection_hashes,
        source_files: BTreeMap::new(),
    };

    Ok(MaterializedDataset { stores, manifest })
}

fn snapshot(stores: &BTreeMap<String, Store>) -> Value {
    Value::Object(
        stores
            .iter()
            .map(|(name, store)| (name.clone(), store.snapshot()))
            .collect(),
    )
}

fn ordered_hash(value: &Value) -> String {
    crate::runner::sha256(&serde_json::to_vec(value).expect("materialized stores are JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_base() -> OrganizationBase {
        OrganizationBase {
            schema_version: 1,
            id: "acme-corp".into(),
            revision: 1,
            description: "Synthetic test organization".into(),
            datasets: BTreeMap::from([
                (
                    "stripe".into(),
                    BTreeMap::from([(
                        "customers".into(),
                        vec![
                            json!({"id": "cus_A", "name": "Alice", "email": "alice@acme.example"}),
                            json!({"id": "cus_B", "name": "Bob", "email": "bob@acme.example"}),
                        ],
                    )]),
                ),
                (
                    "zendesk".into(),
                    BTreeMap::from([(
                        "tickets".into(),
                        vec![json!({"id": "101", "subject": "Billing question", "status": "open"})],
                    )]),
                ),
            ]),
        }
    }

    fn test_overlay(task_id: &str) -> TaskOverlay {
        TaskOverlay {
            schema_version: 1,
            task_id: task_id.into(),
            base_id: "acme-corp".into(),
            base_revision: 1,
            inserts: BTreeMap::from([(
                "stripe".into(),
                BTreeMap::from([(
                    "charges".into(),
                    vec![json!({"id": "ch_001", "amount": 5000, "customer": "cus_A"})],
                )]),
            )]),
            removes: BTreeMap::new(),
        }
    }

    #[test]
    fn materialize_produces_deterministic_hashes() {
        let base = test_base();
        let overlay = test_overlay("task-alpha");
        let m1 = materialize(&base, &overlay).unwrap();
        let m2 = materialize(&base, &overlay).unwrap();
        assert_eq!(
            m1.manifest.materialized_sha256,
            m2.manifest.materialized_sha256
        );
        assert_eq!(m1.manifest.base_sha256, m2.manifest.base_sha256);
        assert_eq!(m1.manifest.overlay_sha256, m2.manifest.overlay_sha256);
    }

    #[test]
    fn base_edit_invalidates_hash() {
        let mut base = test_base();
        let overlay = test_overlay("task-alpha");
        let m1 = materialize(&base, &overlay).unwrap();
        base.datasets
            .get_mut("stripe")
            .unwrap()
            .get_mut("customers")
            .unwrap()
            .push(json!({"id": "cus_C", "name": "Charlie"}));
        base.revision = 2;
        let overlay2 = TaskOverlay {
            base_revision: 2,
            ..test_overlay("task-alpha")
        };
        let m2 = materialize(&base, &overlay2).unwrap();
        assert_ne!(m1.manifest.base_sha256, m2.manifest.base_sha256);
        assert_ne!(
            m1.manifest.materialized_sha256,
            m2.manifest.materialized_sha256
        );
    }

    #[test]
    fn parallel_materializations_are_independent() {
        let base = test_base();
        let ov_a = test_overlay("task-alpha");
        let ov_b = TaskOverlay {
            task_id: "task-beta".into(),
            inserts: BTreeMap::from([(
                "zendesk".into(),
                BTreeMap::from([(
                    "tickets".into(),
                    vec![json!({"id": "102", "subject": "Refund", "status": "new"})],
                )]),
            )]),
            ..test_overlay("task-beta")
        };
        let ma = materialize(&base, &ov_a).unwrap();
        let mb = materialize(&base, &ov_b).unwrap();
        assert!(ma.stores["stripe"].collection("charges").is_some());
        assert_eq!(mb.stores["zendesk"].collection("tickets").unwrap().len(), 2);
        assert_eq!(ma.stores["zendesk"].collection("tickets").unwrap().len(), 1);
    }

    #[test]
    fn rejects_unsafe_collection_path_components() {
        assert!(validate_path_component("..").is_err());
        assert!(validate_path_component(".").is_err());
        assert!(validate_path_component("foo/bar").is_err());
        assert!(validate_path_component("").is_err());
    }

    #[test]
    fn rejects_duplicate_keys_in_base() {
        let mut base = test_base();
        base.datasets
            .get_mut("stripe")
            .unwrap()
            .get_mut("customers")
            .unwrap()
            .push(json!({"id": "cus_A", "name": "Alice duplicate"}));
        let overlay = test_overlay("task-alpha");
        let err = materialize(&base, &overlay).unwrap_err();
        assert!(matches!(err, MaterializationError::KeyCollision { .. }));
    }

    #[test]
    fn rejects_base_mismatch() {
        let base = test_base();
        let mut overlay = test_overlay("task-alpha");
        overlay.base_revision = 99;
        let err = materialize(&base, &overlay).unwrap_err();
        assert!(matches!(err, MaterializationError::BaseMismatch { .. }));
    }

    #[test]
    fn overlay_removes_work() {
        let base = test_base();
        let overlay = TaskOverlay {
            schema_version: 1,
            task_id: "task-remove".into(),
            base_id: "acme-corp".into(),
            base_revision: 1,
            inserts: BTreeMap::new(),
            removes: BTreeMap::from([(
                "stripe".into(),
                BTreeMap::from([("customers".into(), vec!["cus_B".into()])]),
            )]),
        };
        let m = materialize(&base, &overlay).unwrap();
        let customers = m.stores["stripe"].collection("customers").unwrap();
        assert!(customers.contains("cus_A"));
        assert!(!customers.contains("cus_B"));
    }

    #[test]
    fn rejects_remove_of_nonexistent_key() {
        let base = test_base();
        let overlay = TaskOverlay {
            schema_version: 1,
            task_id: "task-bad-remove".into(),
            base_id: "acme-corp".into(),
            base_revision: 1,
            inserts: BTreeMap::new(),
            removes: BTreeMap::from([(
                "stripe".into(),
                BTreeMap::from([("customers".into(), vec!["cus_NONEXISTENT".into()])]),
            )]),
        };
        let err = materialize(&base, &overlay).unwrap_err();
        assert!(matches!(
            err,
            MaterializationError::RemoveTargetMissing { .. }
        ));
    }

    #[test]
    fn overlay_insert_replaces_existing_key() {
        let base = test_base();
        let overlay = TaskOverlay {
            schema_version: 1,
            task_id: "task-replace".into(),
            base_id: "acme-corp".into(),
            base_revision: 1,
            inserts: BTreeMap::from([(
                "stripe".into(),
                BTreeMap::from([(
                    "customers".into(),
                    vec![json!({"id": "cus_A", "name": "Alice Updated"})],
                )]),
            )]),
            removes: BTreeMap::new(),
        };
        let m = materialize(&base, &overlay).unwrap();
        let doc = m.stores["stripe"]
            .collection("customers")
            .unwrap()
            .get("cus_A")
            .unwrap();
        assert_eq!(doc["name"], "Alice Updated");
    }

    #[test]
    fn collection_hashes_are_per_service() {
        let base = test_base();
        let overlay = test_overlay("task-alpha");
        let m = materialize(&base, &overlay).unwrap();
        assert!(m.manifest.collection_hashes.contains_key("stripe"));
        assert!(m.manifest.collection_hashes.contains_key("zendesk"));
        assert!(m.manifest.collection_hashes["stripe"].contains_key("customers"));
        assert!(m.manifest.collection_hashes["stripe"].contains_key("charges"));
    }
}
