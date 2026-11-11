use axum::Router;

use crate::world::{ServiceState, SharedWorld};

pub type ServiceInitializer = fn(&mut ServiceState, i64);

/// One mock API. The world binds `router()` on `port()` and journals every request under `name()`.
pub trait MockService: Send + Sync + 'static {
    /// Dataset directory name and journal service name, e.g. `"github"`.
    fn name(&self) -> &'static str;
    /// Fixed listen port inside the world container.
    fn port(&self) -> u16;
    /// Real hostnames this service answers for behind the TLS redirect proxy.
    fn hosts(&self) -> &'static [&'static str];
    fn initializer(&self) -> Option<ServiceInitializer> {
        None
    }
    fn router(&self) -> Router<SharedWorld>;
}
