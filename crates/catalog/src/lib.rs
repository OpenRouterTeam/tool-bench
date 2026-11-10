//! Parser and validator for `catalog/services.toml`, the source of truth for every service:
//! its mock API port, TLS hostnames, and the MCP endpoints the agent connects to.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, bail};
use mcp_tools::launch::LaunchSpec;
use serde::Deserialize;

/// Hostname of the world container on the trial's compose network.
pub const WORLD_HOST: &str = "world";
/// Directory holding per-service config files inside the tools container.
pub const CONFIG_ROOT: &str = "/etc/tool-bench";
pub const SCENARIO_NOW_ENV: &str = "TOOL_BENCH_SCENARIO_NOW";
/// Env every server gets: fixed Python hash seed, so set/dict-derived text is stable.
pub const DETERMINISM_ENV: [(&str, &str); 1] = [("PYTHONHASHSEED", "0")];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Real,
    RealTls,
    Replica,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Runtime {
    Npm,
    Pypi,
    Go,
    Cargo,
    GitNpm,
    GitPypi,
    Replica,
}

impl Runtime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pypi => "pypi",
            Self::Go => "go",
            Self::Cargo => "cargo",
            Self::GitNpm => "git-npm",
            Self::GitPypi => "git-pypi",
            Self::Replica => "replica",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub id: String,
    pub mcp_port: u16,
    pub runtime: Runtime,
    pub package: Option<String>,
    pub version: Option<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub runtime_origin_env: Vec<String>,
    #[serde(default)]
    pub runtime_clock_env: Vec<String>,
    #[serde(default)]
    pub synthetic_replay_headers: BTreeMap<String, Vec<String>>,
    pub snapshot: Option<String>,
    pub source: Option<String>,
    /// The server lists tools in a nondeterministic order (e.g. from a hash map); the bridge
    /// returns its `tools/list` sorted by name. Definitions are never changed.
    #[serde(default)]
    pub sort_tools: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Service {
    pub name: String,
    pub group: String,
    pub mode: Mode,
    pub world_port: u16,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(rename = "endpoint", default)]
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    #[serde(rename = "service")]
    pub services: Vec<Service>,
}

impl Endpoint {
    pub fn resolved_replay_headers(&self) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
        let mut resolved = BTreeMap::new();
        for (name, templates) in &self.synthetic_replay_headers {
            let header_name = axum::http::HeaderName::from_bytes(name.as_bytes())?;
            if matches!(
                header_name.as_str(),
                "host"
                    | "content-length"
                    | "connection"
                    | "keep-alive"
                    | "proxy-connection"
                    | "transfer-encoding"
                    | "te"
                    | "trailer"
                    | "upgrade"
            ) {
                bail!("{} replay header {name} is transport-owned", self.id);
            }
            if templates.is_empty() {
                bail!("{}.{} has no synthetic replay header values", self.id, name);
            }
            let mut values = Vec::new();
            for template in templates {
                let mut value = template.clone();
                while let Some(start) = value.find("{env.") {
                    let end = value[start..]
                        .find('}')
                        .context("unclosed replay header reference")?
                        + start;
                    let variable = &value[start + 5..end];
                    let replacement = self.env.get(variable).with_context(|| {
                        format!(
                            "{} replay header {name} references unknown env {variable}",
                            self.id
                        )
                    })?;
                    if replacement.contains(['{', '}']) {
                        bail!(
                            "{} replay header {name} has a nonliteral fixture reference",
                            self.id
                        );
                    }
                    value.replace_range(start..=end, replacement);
                }
                if value.is_empty() || value.contains(['{', '}']) {
                    bail!(
                        "{} replay header {name} has an invalid fixture template",
                        self.id
                    );
                }
                axum::http::HeaderValue::from_str(&value)?;
                values.push(value);
            }
            if resolved
                .insert(header_name.as_str().into(), values)
                .is_some()
            {
                bail!(
                    "{} repeats replay header {name} with different casing",
                    self.id
                );
            }
        }
        Ok(resolved)
    }
}

impl Catalog {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// Every problem found, joined into one error.
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut problems = Vec::new();
        let mut seen = |kind: &str, value: String, set: &mut BTreeSet<String>| {
            if !set.insert(value.clone()) {
                problems.push(format!("{kind} {value} used twice"));
            }
        };
        let (mut names, mut world_ports, mut ids, mut mcp_ports, mut hosts) = Default::default();
        for s in &self.services {
            seen("service name", s.name.clone(), &mut names);
            seen("world_port", s.world_port.to_string(), &mut world_ports);
            for h in &s.hosts {
                seen("host", h.clone(), &mut hosts);
            }
            for e in &s.endpoints {
                seen("endpoint id", e.id.clone(), &mut ids);
                seen("mcp_port", e.mcp_port.to_string(), &mut mcp_ports);
            }
        }
        for s in &self.services {
            if s.endpoints.is_empty() {
                problems.push(format!("{}: no endpoints", s.name));
            }
            if let Err(error) = self.default_connection_origin(&s.name) {
                problems.push(error.to_string());
            }
            for e in &s.endpoints {
                if let Err(error) = e.resolved_replay_headers() {
                    problems.push(error.to_string());
                }
                let mut overrides = BTreeSet::new();
                for variable in &e.runtime_origin_env {
                    if !overrides.insert(variable) {
                        problems.push(format!(
                            "{} repeats runtime connection variable {variable}",
                            e.id
                        ));
                    }
                    match e.env.get(variable) {
                        Some(value) => {
                            if let Err(error) = https_connection(value, s) {
                                problems.push(format!("{} connection {variable}: {error}", e.id));
                            }
                        }
                        None => problems.push(format!(
                            "{} connection variable {variable} has no default",
                            e.id
                        )),
                    }
                }
                let mut clocks = BTreeSet::new();
                for variable in &e.runtime_clock_env {
                    if variable.is_empty() || !clocks.insert(variable) {
                        problems.push(format!(
                            "{} repeats or empties a runtime clock variable",
                            e.id
                        ));
                    }
                    match e.env.get(variable) {
                        Some(value) => {
                            if let Err(error) = validation_clock(value) {
                                problems.push(format!("{} clock {variable}: {error}", e.id));
                            }
                        }
                        None => problems
                            .push(format!("{} clock variable {variable} has no default", e.id)),
                    }
                }
                for text in e.args.iter().chain(e.env.values()) {
                    if let Err(e) = substitute(text, s) {
                        problems.push(format!("{}: {e}", s.name));
                    }
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            bail!("{}", problems.join("; "))
        }
    }

    pub fn endpoint(&self, id: &str) -> Option<(&Service, &Endpoint)> {
        self.services
            .iter()
            .flat_map(|s| s.endpoints.iter().map(move |e| (s, e)))
            .find(|(_, e)| e.id == id)
    }

    /// Stdio launch spec for a non-replica endpoint, placeholders substituted.
    pub fn launch(&self, id: &str) -> anyhow::Result<LaunchSpec> {
        self.launch_with_environment(id, &BTreeMap::new())
    }

    pub fn launch_with_environment(
        &self,
        id: &str,
        inherited: &BTreeMap<String, String>,
    ) -> anyhow::Result<LaunchSpec> {
        self.validate_runtime_environment(inherited)?;
        let (service, endpoint) = self
            .endpoint(id)
            .with_context(|| format!("no endpoint {id}"))?;
        let command = endpoint
            .command
            .clone()
            .with_context(|| format!("{id} is a replica and has no launch spec"))?;
        let mut launch = LaunchSpec {
            command,
            args: endpoint
                .args
                .iter()
                .map(|a| substitute(a, service))
                .collect::<anyhow::Result<_>>()?,
            env: DETERMINISM_ENV
                .iter()
                .map(|(k, v)| Ok(((*k).to_owned(), (*v).to_owned())))
                .chain(
                    endpoint
                        .env
                        .iter()
                        .map(|(k, v)| Ok((k.clone(), substitute(v, service)?))),
                )
                .collect::<anyhow::Result<_>>()?,
        };
        for variable in &endpoint.runtime_origin_env {
            if let Some(value) = inherited.get(variable) {
                let default = endpoint
                    .env
                    .get(variable)
                    .context("runtime connection lacks a default")?;
                let (_, default_path) = https_connection(default, service)?;
                let (_, path) = https_connection(value, service)?;
                if path != default_path {
                    bail!("runtime connection {variable} changes its declared API path");
                }
                launch.env.insert(variable.clone(), value.clone());
            }
        }
        for variable in &endpoint.runtime_clock_env {
            let value = inherited
                .get(SCENARIO_NOW_ENV)
                .or_else(|| endpoint.env.get(variable))
                .context("runtime validation clock lacks a default")?;
            validation_clock(value)?;
            launch.env.insert(variable.clone(), value.clone());
        }
        let mut origin = None;
        for variable in &endpoint.runtime_origin_env {
            let (host, _) = https_connection(
                launch
                    .env
                    .get(variable)
                    .context("resolved connection is absent")?,
                service,
            )?;
            if origin.as_ref().is_some_and(|previous| previous != &host) {
                bail!("resolved connection variables for {id} name different account origins");
            }
            origin = Some(host);
        }
        Ok(launch)
    }

    pub fn validate_runtime_environment(
        &self,
        inherited: &BTreeMap<String, String>,
    ) -> anyhow::Result<()> {
        if let Some(value) = inherited.get(SCENARIO_NOW_ENV)
            && self
                .services
                .iter()
                .flat_map(|service| &service.endpoints)
                .any(|endpoint| !endpoint.runtime_clock_env.is_empty())
        {
            validation_clock(value)?;
        }
        Ok(())
    }

    pub fn default_connection_origin(&self, service_name: &str) -> anyhow::Result<Option<String>> {
        let service = self
            .services
            .iter()
            .find(|service| service.name == service_name)
            .context("unknown connection service")?;
        let mut origin = None;
        for endpoint in &service.endpoints {
            for variable in &endpoint.runtime_origin_env {
                let value = endpoint
                    .env
                    .get(variable)
                    .context("connection default missing")?;
                let (host, _) = https_connection(value, service)?;
                let current = format!("https://{host}");
                if origin.as_ref().is_some_and(|previous| previous != &current) {
                    bail!("connection defaults for {service_name} name different account origins");
                }
                origin = Some(current);
            }
        }
        Ok(origin)
    }

    pub fn connection_environment(
        &self,
        service_name: &str,
        origin: &str,
    ) -> anyhow::Result<BTreeMap<String, String>> {
        let service = self
            .services
            .iter()
            .find(|service| service.name == service_name)
            .context("unknown connection service")?;
        if !service
            .hosts
            .iter()
            .any(|host| origin == format!("https://{host}"))
        {
            bail!("connection origin must be an exact declared HTTPS account origin");
        }
        let mut values = BTreeMap::new();
        for endpoint in &service.endpoints {
            for variable in &endpoint.runtime_origin_env {
                let default = endpoint
                    .env
                    .get(variable)
                    .context("connection default missing")?;
                let (_, path) = https_connection(default, service)?;
                let value = format!("{origin}{path}");
                if values
                    .insert(variable.clone(), value.clone())
                    .is_some_and(|previous| previous != value)
                {
                    bail!("connection variable {variable} is ambiguous");
                }
            }
        }
        Ok(values)
    }

    /// Network aliases the world container needs: every TLS hostname, sorted.
    pub fn world_aliases(&self) -> Vec<String> {
        let mut hosts: Vec<String> = self.services.iter().flat_map(|s| s.hosts.clone()).collect();
        hosts.sort();
        hosts
    }
}

fn validation_clock(value: &str) -> anyhow::Result<()> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .context("runtime business reference time must be an RFC 3339 instant")?;
    Ok(())
}

fn https_connection<'a>(value: &'a str, service: &Service) -> anyhow::Result<(&'a str, &'a str)> {
    if value
        .bytes()
        .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || value.contains(['?', '#', '\\'])
    {
        bail!("connection URL contains unsupported characters or routing options");
    }
    let authority = value
        .strip_prefix("https://")
        .context("connection URL must use HTTPS")?;
    let split = authority.find('/').unwrap_or(authority.len());
    let host = &authority[..split];
    let path = &authority[split..];
    if !service.hosts.iter().any(|allowed| allowed == host) {
        bail!("connection URL host is not a declared service alias");
    }
    Ok((host, path))
}

/// Replaces `{world}`, `{world_host}` and `{config}`; any other `{...}` is an error.
fn substitute(text: &str, service: &Service) -> anyhow::Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find('}')
            .map(|e| start + e)
            .context("unclosed {")?;
        let value = match &rest[start + 1..end] {
            "world" => format!("http://{WORLD_HOST}:{}", service.world_port),
            "world_host" => format!("{WORLD_HOST}:{}", service.world_port),
            "config" => format!("{CONFIG_ROOT}/{}", service.name),
            other => bail!("unknown placeholder {{{other}}}"),
        };
        out.push_str(&value);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}
