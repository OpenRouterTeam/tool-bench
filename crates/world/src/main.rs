use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    time::Duration,
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
    let bootstrap_bind = if admin_token.is_some() {
        let host = std::env::var("BENCH_BOOTSTRAP_BIND_HOST")
            .context("protected startup requires BENCH_BOOTSTRAP_BIND_HOST")?;
        Some(resolve_bootstrap_bind(&host).await?)
    } else {
        None
    };
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
            bootstrap_bind,
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

async fn resolve_bootstrap_bind(host: &str) -> anyhow::Result<IpAddr> {
    anyhow::ensure!(
        !host.is_empty() && !host.chars().any(char::is_whitespace),
        "bootstrap binding requires one private interface hostname"
    );
    let addresses = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::lookup_host((host, world::BOOTSTRAP_PORT)),
    )
    .await
    .context("bootstrap interface resolution timed out")??;
    let addresses: BTreeSet<Ipv4Addr> = addresses
        .filter_map(|address| match address.ip() {
            IpAddr::V4(address)
                if !address.is_unspecified()
                    && !address.is_multicast()
                    && !address.is_broadcast() =>
            {
                Some(address)
            }
            _ => None,
        })
        .collect();
    anyhow::ensure!(
        addresses.len() == 1,
        "bootstrap hostname must resolve to exactly one concrete IPv4 interface"
    );
    Ok(IpAddr::V4(*addresses.first().expect("one checked address")))
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
    use super::{resolve_bootstrap_bind, validate_admin_access};

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

    #[tokio::test]
    async fn bootstrap_resolution_requires_one_concrete_interface() {
        assert_eq!(
            resolve_bootstrap_bind("127.0.0.1").await.unwrap(),
            "127.0.0.1".parse::<std::net::IpAddr>().unwrap(),
        );
        for invalid in ["", " ", "127.0.0.1 invalid", "0.0.0.0", "255.255.255.255"] {
            assert!(resolve_bootstrap_bind(invalid).await.is_err());
        }
    }
}
