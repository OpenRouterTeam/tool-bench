use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
};

use anyhow::Context;
use mock_core::{
    dataset::Dataset,
    world::{WorldState, shared},
};
use world::{AccessPolicy, PortChoice, spawn_with_access};

const SCENARIO_ROOT: &str = "/opt/tool-bench/scenarios";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // SCENARIO_DIR wins; otherwise SCENARIO_ID under the image's scenario root.
    let dir = match (
        std::env::var_os("SCENARIO_DIR"),
        std::env::var("SCENARIO_ID"),
    ) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, Ok(id)) => PathBuf::from(SCENARIO_ROOT).join(id),
        (None, Err(_)) => anyhow::bail!("set SCENARIO_DIR or SCENARIO_ID"),
    };
    let dataset =
        Dataset::load(&dir).with_context(|| format!("loading scenario {}", dir.display()))?;
    let scenario = dataset.meta.id.clone();
    let world = shared(WorldState::from_dataset(dataset)?);

    let admin_token = std::env::var("BENCH_ADMIN_TOKEN")
        .ok()
        .filter(|token| !token.is_empty());
    let allow_insecure = std::env::var("BENCH_ALLOW_INSECURE_LOCAL")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true"));
    let admin_token = validate_admin_access(admin_token, allow_insecure)?;
    let tools_host = match std::env::var("BENCH_API_CALLER_HOST") {
        Ok(host) => (!host.is_empty()).then_some(host),
        Err(_) => admin_token
            .as_ref()
            .map(|_| world::grade::TOOLS_HOST.to_owned()),
    };
    let running = spawn_with_access(
        world,
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        PortChoice::Fixed,
        AccessPolicy {
            admin_token,
            tools_host,
        },
    )
    .await?;
    eprintln!("world: scenario {scenario}, admin on {}", running.admin);
    for (name, addr) in &running.services {
        eprintln!("world: {name} on {addr}");
    }
    running.wait().await?;
    Ok(())
}

fn validate_admin_access(
    token: Option<String>,
    allow_insecure: bool,
) -> anyhow::Result<Option<String>> {
    match token {
        Some(token) if !token.is_empty() && token.bytes().all(|byte| byte.is_ascii_graphic()) => {
            Ok(Some(token))
        }
        Some(_) => anyhow::bail!("BENCH_ADMIN_TOKEN must be nonempty visible ASCII without spaces"),
        None if allow_insecure => Ok(None),
        None => anyhow::bail!(
            "BENCH_ADMIN_TOKEN is required; insecure local development must explicitly set BENCH_ALLOW_INSECURE_LOCAL=1"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_admin_access;

    #[test]
    fn scored_startup_requires_an_admin_credential() {
        assert!(validate_admin_access(None, false).is_err());
        assert_eq!(
            validate_admin_access(Some("trial-secret".into()), false).unwrap(),
            Some("trial-secret".into())
        );
    }

    #[test]
    fn insecure_startup_is_explicit_and_invalid_credentials_are_not_downgraded() {
        assert_eq!(validate_admin_access(None, true).unwrap(), None);
        assert!(validate_admin_access(Some(" ".into()), true).is_err());
        assert!(validate_admin_access(Some("secret\n".into()), true).is_err());
    }
}
