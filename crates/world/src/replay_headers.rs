use std::sync::LazyLock;

use mock_core::request::HeaderRegistry;

static REGISTRY: LazyLock<Result<HeaderRegistry, String>> = LazyLock::new(|| {
    let catalog = catalog::Catalog::parse(include_str!("../../../catalog/services.toml"))
        .map_err(|error| error.to_string())?;
    catalog.validate().map_err(|error| error.to_string())?;
    let mut registry = HeaderRegistry::default();
    for service in &catalog.services {
        for endpoint in &service.endpoints {
            for (name, values) in endpoint
                .resolved_replay_headers()
                .map_err(|error| error.to_string())?
            {
                for (index, value) in values.iter().enumerate() {
                    registry.insert_synthetic(
                        &service.name,
                        &name,
                        &format!("{}/{name}/{index}", endpoint.id),
                        value,
                    )?;
                }
            }
        }
    }
    Ok(registry)
});

pub fn registry() -> Result<HeaderRegistry, String> {
    REGISTRY.clone()
}
