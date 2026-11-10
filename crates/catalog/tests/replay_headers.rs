use catalog::Catalog;

fn catalog() -> Catalog {
    Catalog::parse(
        r#"
[[service]]
name = "example"
group = "search"
mode = "real"
world_port = 8101
[[service.endpoint]]
id = "example"
mcp_port = 7101
runtime = "npm"
env = { API_TOKEN = "synthetic-token" }
synthetic_replay_headers = { Authorization = ["Bearer {env.API_TOKEN}"] }
"#,
    )
    .unwrap()
}

#[test]
fn replay_header_templates_use_only_explicit_catalog_literals_and_normalize_names() {
    let catalog = catalog();
    catalog.validate().unwrap();
    let resolved = catalog.services[0].endpoints[0]
        .resolved_replay_headers()
        .unwrap();
    assert_eq!(resolved["authorization"], ["Bearer synthetic-token"]);
}

#[test]
fn undefined_nested_and_malformed_header_references_fail_authoring() {
    for value in [
        "Bearer {env.HOME}",
        "Bearer {env.API_TOKEN",
        "{unknown}",
        "",
    ] {
        let mut catalog = catalog();
        catalog.services[0].endpoints[0]
            .synthetic_replay_headers
            .insert("authorization".into(), vec![value.into()]);
        assert!(catalog.validate().is_err(), "{value}");
    }
    let mut catalog = catalog();
    catalog.services[0].endpoints[0]
        .env
        .insert("API_TOKEN".into(), "{env.OTHER}".into());
    assert!(catalog.validate().is_err());
}

#[test]
fn empty_duplicate_casing_and_invalid_header_declarations_are_not_silently_accepted() {
    for name in ["authorization", "bad\nheader", "host"] {
        let mut catalog = catalog();
        catalog.services[0].endpoints[0]
            .synthetic_replay_headers
            .insert(name.into(), vec!["literal".into()]);
        assert!(catalog.validate().is_err());
    }
    let mut catalog = catalog();
    catalog.services[0].endpoints[0]
        .synthetic_replay_headers
        .insert("Authorization".into(), Vec::new());
    assert!(catalog.validate().is_err());
}

#[test]
fn endpoint_configuration_rejects_unknown_fields_instead_of_ignoring_policy_typos() {
    let endpoint = serde_json::json!({
        "id": "example", "mcp_port": 7101, "runtime": "npm",
        "synthetic_replay_header": {"authorization": ["literal"]}
    });
    assert!(serde_json::from_value::<catalog::Endpoint>(endpoint).is_err());
}
