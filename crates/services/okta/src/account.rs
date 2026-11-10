use mock_core::world::ServiceState;
use serde_json::json;

pub const CLIENT_ID: &str = "0oa8wq4n2Yc6Ds7Rt0x7";
pub const PRINCIPAL_ID: &str = "00u8ops47Vb2Nc6Xt0x7";
pub const USER_TYPE_ID: &str = "oty8uc4n2Yd6Ws7Rt0x7";
pub const USER_SCHEMA_ID: &str = "osc8uv4n2Yd6Ws7Rt0x7";
pub const SESSION_ID: &str = "trs8Km4n2Yc6Ds7Rt0x7";
pub const SCOPES: &[&str] = &[
    "okta.users.read",
    "okta.users.manage",
    "okta.groups.read",
    "okta.groups.manage",
    "okta.apps.read",
    "okta.apps.manage",
    "okta.policies.read",
    "okta.policies.manage",
    "okta.logs.read",
    "okta.userTypes.read",
    "okta.deviceAssurance.read",
    "okta.deviceAssurance.manage",
    "okta.brands.read",
    "okta.brands.manage",
    "okta.domains.read",
    "okta.domains.manage",
    "okta.emailDomains.read",
    "okta.emailDomains.manage",
    "okta.templates.read",
    "okta.templates.manage",
];

pub fn initialize(service: &mut ServiceState, _now: i64) {
    crate::custom::initialize(service);
    let legacy_type = "oty1toolbench0000001";
    let users: Vec<_> = service
        .store
        .collection("users")
        .into_iter()
        .flat_map(|rows| rows.iter())
        .filter(|(_, user)| user["type"]["id"] == legacy_type)
        .map(|(identity, user)| (identity.to_owned(), user.clone()))
        .collect();
    for (identity, mut user) in users {
        user["type"]["id"] = json!(USER_TYPE_ID);
        service.store.collection_mut("users").insert(identity, user);
    }
    if let Some(mut user_type) = service
        .store
        .collection_mut("user_types")
        .remove(legacy_type)
    {
        user_type["id"] = json!(USER_TYPE_ID);
        service
            .store
            .collection_mut("user_types")
            .insert(USER_TYPE_ID, user_type);
    }
    let rules: Vec<_> = service
        .store
        .collection("policy_rules")
        .into_iter()
        .flat_map(|rows| rows.iter())
        .filter(|(_, rule)| rule["type"] == "PASSWORD")
        .map(|(identity, rule)| (identity.to_owned(), rule.clone()))
        .collect();
    for (identity, mut rule) in rules {
        crate::policies::normalize_password_conditions(&mut rule);
        service
            .store
            .collection_mut("policy_rules")
            .insert(identity, rule);
    }
}
