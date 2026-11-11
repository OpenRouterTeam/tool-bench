//! Hosts every mock service plus the admin API for one scenario.

pub mod grade;
pub mod replay;
pub mod replay_headers;
pub mod tls;

use std::{
    collections::BTreeMap,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use axum::{
    Router,
    extract::{ConnectInfo, Request},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use axum_server::tls_rustls::RustlsConfig;
use mock_core::{
    access::require_caller,
    admin::admin_app,
    serve::{serve, service_app_with_caller},
    service::MockService,
    world::SharedWorld,
};
use tokio::{net::TcpListener, task::JoinSet};

pub const ADMIN_PORT: u16 = 9001;
pub const BOOTSTRAP_PORT: u16 = 9000;
pub const TLS_PORT: u16 = 443;

/// Every mock service compiled into the world.
pub fn services() -> Vec<Box<dyn MockService>> {
    vec![
        Box::new(mock_argocd::Argocd),
        Box::new(mock_aws::Aws),
        Box::new(mock_brave::Brave),
        Box::new(mock_cloudflare::Cloudflare),
        Box::new(mock_datadog::Datadog),
        Box::new(mock_elasticsearch::Elasticsearch),
        Box::new(mock_exa::Exa),
        Box::new(mock_firecrawl::Firecrawl),
        Box::new(mock_github::GitHub),
        Box::new(mock_gitlab::Gitlab),
        Box::new(mock_google::Google),
        Box::new(mock_grafana::Grafana),
        Box::new(mock_hubspot::Hubspot),
        Box::new(mock_jira::Jira),
        Box::new(mock_kubernetes::Kubernetes),
        Box::new(mock_linear::Linear),
        Box::new(mock_mongodb::Mongodb),
        Box::new(mock_notion::Notion),
        Box::new(mock_okta::Okta),
        Box::new(mock_pagerduty::Pagerduty),
        Box::new(mock_salesforce::Salesforce),
        Box::new(mock_sentry::Sentry),
        Box::new(mock_shopify::Shopify),
        Box::new(mock_slack::Slack),
        Box::new(mock_stripe::Stripe),
        Box::new(mock_supabase::Supabase),
        Box::new(mock_tavily::Tavily),
        Box::new(mock_terraform::Terraform),
        Box::new(mock_vercel::Vercel),
        Box::new(mock_zendesk::Zendesk),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortChoice {
    /// Each service on its own fixed port, admin on [`ADMIN_PORT`] (container use).
    Fixed,
    /// OS-assigned ports (tests).
    Ephemeral,
}

#[derive(Debug, Clone, Default)]
pub struct AccessPolicy {
    pub admin_token: Option<String>,
    pub tools_host: Option<String>,
    /// Concrete tools-only interface; required for protected fixed-port runs.
    pub bootstrap_bind: Option<IpAddr>,
}

pub struct Running {
    pub admin: SocketAddr,
    pub bootstrap: SocketAddr,
    /// HTTPS listener that dispatches by `Host` to the service owning that hostname.
    pub tls: SocketAddr,
    /// PEM of the CA that signed the TLS certificate (also served at `/_bench/ca.pem`).
    pub ca_pem: String,
    pub services: BTreeMap<&'static str, SocketAddr>,
    tasks: JoinSet<io::Result<()>>,
}

impl Running {
    /// Runs until any listener stops, returning its error.
    pub async fn wait(mut self) -> io::Result<()> {
        while let Some(result) = self.tasks.join_next().await {
            result.map_err(io::Error::other)??;
        }
        Ok(())
    }
}

/// Health and CA routes available without the admin token.
const OPEN_ADMIN_PATHS: [&str; 2] = ["/_bench/health", "/_bench/ca.pem"];

/// Binds the admin API and every service, then serves them in the background.
pub async fn spawn(world: SharedWorld, ip: IpAddr, ports: PortChoice) -> io::Result<Running> {
    spawn_guarded(world, ip, ports, None).await
}

/// [`spawn`], with protected control routes on loopback when a token is set.
/// Tools receive only the separate health/CA bootstrap listener.
/// Protected fixed-port runs use [`spawn_with_access`] with an explicit bootstrap interface.
pub async fn spawn_guarded(
    world: SharedWorld,
    ip: IpAddr,
    ports: PortChoice,
    admin_token: Option<String>,
) -> io::Result<Running> {
    spawn_with_access(
        world,
        ip,
        ports,
        AccessPolicy {
            admin_token,
            tools_host: None,
            bootstrap_bind: None,
        },
    )
    .await
}

pub async fn spawn_with_access(
    world: SharedWorld,
    ip: IpAddr,
    ports: PortChoice,
    access: AccessPolicy,
) -> io::Result<Running> {
    if access.admin_token.is_some() && ports == PortChoice::Fixed && access.bootstrap_bind.is_none()
        || access
            .bootstrap_bind
            .is_some_and(|address| address.is_unspecified())
        || access.bootstrap_bind.is_some() && access.admin_token.is_none()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "protected fixed-port runs require a concrete bootstrap interface; explicit bootstrap binds require protected control",
        ));
    }
    world
        .configure_replay_headers(replay_headers::registry().map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    let port = |fixed: u16| match ports {
        PortChoice::Fixed => fixed,
        PortChoice::Ephemeral => 0,
    };
    let mut tasks = JoinSet::new();
    let services = services();
    for service in &services {
        world
            .prepare_service(service.name(), service.initializer())
            .map_err(io::Error::other)?;
    }
    let ca = tls::LocalCa::generate(&tls::all_hosts(&services)).map_err(io::Error::other)?;

    let protected = access.admin_token.is_some();
    let admin_ip = if protected {
        match ip {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        }
    } else {
        ip
    };
    let admin_listener = TcpListener::bind((
        admin_ip,
        port(if protected {
            ADMIN_PORT
        } else {
            BOOTSTRAP_PORT
        }),
    ))
    .await?;
    let admin = admin_listener.local_addr()?;
    let bootstrap = if protected {
        let listener =
            TcpListener::bind((access.bootstrap_bind.unwrap_or(ip), port(BOOTSTRAP_PORT))).await?;
        let address = listener.local_addr()?;
        let certificate = ca.ca_pem.clone();
        let app = Router::new()
            .route(
                "/_bench/health",
                get(|| async { axum::Json(serde_json::json!({"status":"ok"})) }),
            )
            .route(
                "/_bench/ca.pem",
                get(move || {
                    let certificate = certificate.clone();
                    async move { certificate }
                }),
            );
        tasks.spawn(serve(listener, app));
        address
    } else {
        admin
    };
    let ca_pem = ca.ca_pem.clone();
    let admin_routes = admin_app(world.clone())
        .merge(grade::routes(world.clone()))
        .merge(mock_core::scripted_user::routes(world.clone()))
        .merge(Router::new().route(
            "/_bench/ca.pem",
            get(move || {
                let pem = ca_pem.clone();
                async move { pem }
            }),
        ));
    let admin_routes = match access.admin_token {
        Some(token) => admin_routes.layer(middleware::from_fn(move |request, next| {
            require_admin_token(token.clone(), request, next)
        })),
        None => admin_routes,
    };
    let admin_routes = match access.tools_host.clone() {
        Some(host) => admin_routes.layer(middleware::from_fn(move |request, next| {
            require_admin_caller(host.clone(), request, next)
        })),
        None => admin_routes,
    };
    tasks.spawn(serve(admin_listener, admin_routes));

    let tls_listener = std::net::TcpListener::bind((ip, port(TLS_PORT)))?;
    tls_listener.set_nonblocking(true)?;
    let tls = tls_listener.local_addr()?;
    let config = RustlsConfig::from_pem(
        ca.leaf_cert_pem.clone().into_bytes(),
        ca.leaf_key_pem.clone().into_bytes(),
    )
    .await?;
    let tls_app = tls::host_router_with_caller(world.clone(), &services, access.tools_host.clone());
    tasks.spawn(async move {
        axum_server::from_tcp_rustls(tls_listener, config)?
            .serve(tls_app.into_make_service_with_connect_info::<SocketAddr>())
            .await
    });

    let mut bound = BTreeMap::new();
    for service in &services {
        let listener = TcpListener::bind((ip, port(service.port()))).await?;
        bound.insert(service.name(), listener.local_addr()?);
        tasks.spawn(serve(
            listener,
            service_app_with_caller(world.clone(), service.as_ref(), access.tools_host.clone()),
        ));
    }

    Ok(Running {
        admin,
        bootstrap,
        tls,
        ca_pem: ca.ca_pem,
        services: bound,
        tasks,
    })
}

async fn require_admin_caller(host: String, request: Request, next: Next) -> Response {
    let source = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip());
    match require_caller(&host, source, true).await {
        Ok(()) => next.run(request).await,
        Err(status) => status.into_response(),
    }
}

async fn require_admin_token(token: String, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let authorized = OPEN_ADMIN_PATHS.contains(&path)
        || request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|presented| presented == token);
    if authorized {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
