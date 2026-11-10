use std::net::IpAddr;

use axum::http::StatusCode;
use tokio::net::lookup_host;

pub async fn require_caller(
    host: &str,
    source: Option<IpAddr>,
    allow_loopback: bool,
) -> Result<(), StatusCode> {
    let source = source.ok_or(StatusCode::FORBIDDEN)?;
    if allow_loopback && source.is_loopback() {
        return Ok(());
    }
    let mut addresses = lookup_host((host, 0))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if addresses.any(|address| address.ip() == source) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}
