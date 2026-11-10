use std::collections::BTreeMap;

use anyhow::{Context, bail};
use catalog::Catalog;
use mock_core::dataset::Dataset;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountConnection {
    pub origin: String,
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionManifest {
    pub schema_version: u32,
    pub accounts: BTreeMap<String, AccountConnection>,
}

pub struct Connections {
    pub environment: BTreeMap<String, String>,
    pub manifest: ConnectionManifest,
}

pub fn materialize(dataset: &Dataset, catalog: &Catalog) -> anyhow::Result<Connections> {
    let mut environment = BTreeMap::new();
    let mut accounts = BTreeMap::new();
    for service in &catalog.services {
        let Some(default) = catalog.default_connection_origin(&service.name)? else {
            continue;
        };
        let declared = dataset
            .services
            .get(&service.name)
            .and_then(|store| store.collection("instance"))
            .and_then(|instances| instances.get("instance"))
            .and_then(|instance| instance.get("public_origin"));
        let origin = match declared {
            Some(value) => value
                .as_str()
                .context("instance public_origin must be a declared HTTPS origin")?
                .to_owned(),
            None => default,
        };
        let values = catalog.connection_environment(&service.name, &origin)?;
        for (key, value) in &values {
            if environment
                .insert(key.clone(), value.clone())
                .is_some_and(|previous| previous != *value)
            {
                bail!("connected accounts require conflicting values for {key}");
            }
        }
        accounts.insert(
            service.name.clone(),
            AccountConnection {
                origin,
                environment: values,
            },
        );
    }
    Ok(Connections {
        environment,
        manifest: ConnectionManifest {
            schema_version: 1,
            accounts,
        },
    })
}
