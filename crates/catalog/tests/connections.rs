use std::{collections::BTreeMap, path::Path};

use catalog::Catalog;

fn catalog() -> Catalog {
    Catalog::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../catalog/services.toml"))
        .unwrap()
}

#[test]
fn business_reference_time_overrides_only_declared_validation_clocks() {
    let catalog = catalog();
    let clock = "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW";
    let inherited = BTreeMap::from([
        (
            catalog::SCENARIO_NOW_ENV.into(),
            "2026-10-28T15:30:00Z".into(),
        ),
        (clock.into(), "2100-01-01T00:00:00Z".into()),
        ("SSL_CERT_FILE".into(), "/untrusted/ca.pem".into()),
        ("LD_PRELOAD".into(), "/untrusted/clock.so".into()),
        (
            "GRAFANA_SERVICE_ACCOUNT_TOKEN".into(),
            "unapproved-token".into(),
        ),
    ]);
    let launch = catalog
        .launch_with_environment("grafana", &inherited)
        .unwrap();
    assert_eq!(launch.env[clock], inherited[catalog::SCENARIO_NOW_ENV]);
    assert_eq!(
        launch.env["GRAFANA_SERVICE_ACCOUNT_TOKEN"],
        catalog.launch("grafana").unwrap().env["GRAFANA_SERVICE_ACCOUNT_TOKEN"]
    );
    assert!(!launch.env.contains_key("SSL_CERT_FILE"));
    assert!(!launch.env.contains_key("LD_PRELOAD"));
    let github = catalog
        .launch_with_environment("github", &inherited)
        .unwrap();
    assert!(!github.env.contains_key(clock));
}

#[test]
fn malformed_business_time_and_clock_declarations_fail_before_launch() {
    for value in ["", "not-a-time", "2026-02-30T12:00:00Z", "2026-10-28"] {
        let inherited = BTreeMap::from([(catalog::SCENARIO_NOW_ENV.into(), value.into())]);
        assert!(catalog().validate_runtime_environment(&inherited).is_err());
        assert!(
            catalog()
                .launch_with_environment("grafana", &inherited)
                .is_err()
        );
    }
    for variables in [
        vec!["MISSING".into()],
        vec!["GRAFANA_SERVICE_ACCOUNT_TOKEN".into()],
        vec![
            "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW".into(),
            "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW".into(),
        ],
    ] {
        let mut catalog = catalog();
        catalog
            .services
            .iter_mut()
            .find(|service| service.name == "grafana")
            .unwrap()
            .endpoints[0]
            .runtime_clock_env = variables;
        assert!(catalog.validate().is_err());
    }
}

#[test]
fn root_connections_without_a_trailing_slash_preserve_their_exact_authority_and_path() {
    let catalog = catalog();
    let origin = "https://grafana.octo-corp.com";
    assert_eq!(
        catalog.default_connection_origin("grafana").unwrap(),
        Some(origin.into())
    );
    let environment = catalog.connection_environment("grafana", origin).unwrap();
    assert_eq!(environment["GRAFANA_URL"], origin);
    for value in [
        "https://grafana.octo-corp.com/",
        "https://grafana.octo-corp.com/another-path",
        "http://grafana.octo-corp.com",
        "https://grafana.octo-corp.com:443",
        "https://user@grafana.octo-corp.com",
        "https://another.example",
        "https://grafana.octo-corp.com?query=1",
    ] {
        let inherited = BTreeMap::from([("GRAFANA_URL".into(), value.into())]);
        assert!(
            catalog
                .launch_with_environment("grafana", &inherited)
                .is_err(),
            "{value}"
        );
    }
}

#[test]
fn only_declared_account_origins_override_connection_variables_not_credentials_or_seed() {
    let catalog = catalog();
    let env = BTreeMap::from([
        (
            "GITLAB_API_URL".into(),
            "https://gitlab.lumen.internal/api/v4".into(),
        ),
        (
            "GITLAB_GRAPHQL_URL".into(),
            "https://gitlab.lumen.internal/api/graphql".into(),
        ),
        (
            "GITLAB_PERSONAL_ACCESS_TOKEN".into(),
            "unapproved-credential".into(),
        ),
        ("PYTHONHASHSEED".into(), "random".into()),
    ]);
    let launch = catalog.launch_with_environment("gitlab", &env).unwrap();
    assert_eq!(launch.env["GITLAB_API_URL"], env["GITLAB_API_URL"]);
    assert_eq!(launch.env["GITLAB_GRAPHQL_URL"], env["GITLAB_GRAPHQL_URL"]);
    assert_eq!(
        launch.env["GITLAB_PERSONAL_ACCESS_TOKEN"],
        "glpat-toolbench"
    );
    assert_eq!(launch.env["PYTHONHASHSEED"], "0");
}

#[test]
fn runtime_origins_cannot_change_scheme_host_identity_api_path_or_request_options() {
    let catalog = catalog();
    for value in [
        "http://world:8107/api/v4",
        "https://unlisted.example/api/v4",
        "https://user@gitlab.lumen.internal/api/v4",
        "https://gitlab.lumen.internal:444/api/v4",
        "https://gitlab.lumen.internal/api/admin",
        "https://gitlab.lumen.internal/api/v4?extra=1",
        "https://gitlab.lumen.internal/api/v4#fragment",
        "https://gitlab.lumen.internal\\elsewhere/api/v4",
        "https://gitlab.lumen.internal/api/v4\n",
    ] {
        let env = BTreeMap::from([("GITLAB_API_URL".into(), value.into())]);
        assert!(
            catalog.launch_with_environment("gitlab", &env).is_err(),
            "{value:?}"
        );
    }
}

#[test]
fn malformed_runtime_metadata_is_rejected_before_any_server_launch() {
    for variables in [
        vec!["MISSING".into()],
        vec!["GITLAB_PERSONAL_ACCESS_TOKEN".into()],
        vec!["GITLAB_API_URL".into(), "GITLAB_API_URL".into()],
    ] {
        let mut catalog = catalog();
        catalog
            .services
            .iter_mut()
            .find(|service| service.name == "gitlab")
            .unwrap()
            .endpoints[0]
            .runtime_origin_env = variables;
        assert!(catalog.validate().is_err());
    }
}

#[test]
fn partial_connection_overrides_cannot_mix_rest_and_graphql_accounts() {
    let catalog = catalog();
    let inherited = BTreeMap::from([(
        "GITLAB_API_URL".into(),
        "https://gitlab.lumen.internal/api/v4".into(),
    )]);
    assert!(
        catalog
            .launch_with_environment("gitlab", &inherited)
            .is_err()
    );
}

#[test]
fn sentry_uses_native_https_account_configuration_without_credential_overrides() {
    let catalog = catalog();
    let inherited = BTreeMap::from([
        (
            "SENTRY_URL".into(),
            "https://sentry.eu.lumenlabs.dev/".into(),
        ),
        ("SENTRY_ACCESS_TOKEN".into(), "unapproved-token".into()),
        ("OPENAI_API_KEY".into(), "unapproved-model-key".into()),
        ("EMBEDDED_AGENT_PROVIDER".into(), "openai".into()),
    ]);
    let launch = catalog
        .launch_with_environment("sentry", &inherited)
        .unwrap();
    assert_eq!(launch.args, ["--all-skills"]);
    assert_eq!(launch.env["SENTRY_URL"], inherited["SENTRY_URL"]);
    assert_eq!(launch.env["SENTRY_ACCESS_TOKEN"], "sntrys_toolbench");
    for variable in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENROUTER_API_KEY",
        "EMBEDDED_AGENT_PROVIDER",
        "SENTRY_DSN",
        "DEFAULT_SENTRY_DSN",
    ] {
        assert_eq!(launch.env[variable], "", "{variable}");
    }
    for origin in [
        "http://world:8113/",
        "https://unlisted.example/",
        "https://user@sentry.lumenlabs.dev/",
        "https://sentry.lumenlabs.dev/other",
        "https://sentry.lumenlabs.dev/?extra=1",
    ] {
        let inherited = BTreeMap::from([("SENTRY_URL".into(), origin.into())]);
        assert!(
            catalog
                .launch_with_environment("sentry", &inherited)
                .is_err(),
            "{origin}"
        );
    }
}

#[test]
fn default_grafana_clock_value_is_valid_rfc3339_and_survives_catalog_validation() {
    let catalog = catalog();
    catalog.validate().unwrap();
    let launch = catalog.launch("grafana").unwrap();
    let clock = "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW";
    let default = &launch.env[clock];
    time::OffsetDateTime::parse(default, &time::format_description::well_known::Rfc3339).unwrap();
}

#[test]
fn scenario_now_overrides_grafana_clock_and_ignores_direct_clock_variable_override() {
    let catalog = catalog();
    let clock = "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW";
    let scenario_now = "2026-09-01T08:00:00Z";
    let direct_override = "2099-12-31T23:59:59Z";
    let inherited = BTreeMap::from([
        (catalog::SCENARIO_NOW_ENV.into(), scenario_now.into()),
        (clock.into(), direct_override.into()),
    ]);
    let launch = catalog
        .launch_with_environment("grafana", &inherited)
        .unwrap();
    assert_eq!(launch.env[clock], scenario_now);
    assert_ne!(launch.env[clock], direct_override);
}

#[test]
fn absent_scenario_now_falls_back_to_catalog_default_clock() {
    let catalog = catalog();
    let clock = "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW";
    let default_launch = catalog.launch("grafana").unwrap();
    let inherited = BTreeMap::new();
    let launch = catalog
        .launch_with_environment("grafana", &inherited)
        .unwrap();
    assert_eq!(launch.env[clock], default_launch.env[clock]);
}

#[test]
fn non_clock_endpoints_never_receive_clock_variables() {
    let catalog = catalog();
    let clock = "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW";
    let inherited = BTreeMap::from([(
        catalog::SCENARIO_NOW_ENV.into(),
        "2026-10-28T15:30:00Z".into(),
    )]);
    for id in ["github", "gitlab", "sentry", "linear"] {
        if let Ok(launch) = catalog.launch_with_environment(id, &inherited) {
            assert!(
                !launch.env.contains_key(clock),
                "{id} should not have Grafana clock"
            );
        }
    }
}
