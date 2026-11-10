use mock_core::world::{ServiceState, WorldState};
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::NAME;

pub const HOSTS: &[&str] = &["sentry.lumenlabs.dev", "sentry.eu.lumenlabs.dev"];
pub const PUBLIC_ORIGIN: &str = "https://sentry.lumenlabs.dev";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Instance {
    public_origin: String,
    ingestion_origin: String,
}

pub fn initialize(service: &mut ServiceState, _now: i64) {
    let declared = service
        .store
        .collection("instance")
        .and_then(|instances| instances.get("instance"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "public_origin":PUBLIC_ORIGIN,"ingestion_origin":PUBLIC_ORIGIN
            })
        });
    let instance: Instance =
        serde_json::from_value(declared).expect("unsupported Sentry instance fixture shape");
    assert!(
        HOSTS
            .iter()
            .any(|host| instance.public_origin == format!("https://{host}")),
        "Sentry public origin must be a declared HTTPS account origin"
    );
    let ingestion = Url::parse(&instance.ingestion_origin)
        .expect("Sentry ingestion origin must be a valid HTTPS origin/path");
    assert!(
        ingestion.scheme() == "https"
            && ingestion.has_host()
            && ingestion.username().is_empty()
            && ingestion.password().is_none()
            && ingestion.query().is_none()
            && ingestion.fragment().is_none(),
        "Sentry ingestion origin must be a credential-free HTTPS origin/path"
    );
    service.store.collection_mut("instance").insert(
        "instance",
        serde_json::to_value(instance).expect("Sentry instance serializes"),
    );
}

pub fn ingestion_url(world: &WorldState) -> Url {
    let origin = world
        .service(NAME)
        .and_then(|service| service.store.collection("instance"))
        .and_then(|instances| instances.get("instance"))
        .and_then(|instance| instance["ingestion_origin"].as_str())
        .expect("Sentry ingestion is initialized before requests");
    Url::parse(origin).expect("initialized Sentry ingestion origin parses")
}
