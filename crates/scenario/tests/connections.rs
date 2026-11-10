use std::{fs, path::Path};

use catalog::Catalog;
use mock_core::{dataset::Dataset, provenance::TaskContext, runner::sha256};
use scenario::{
    Scenario, connections,
    taskgen::{Images, write_task},
};
use serde_json::{Value, json};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn catalog() -> Catalog {
    Catalog::load(&root().join("catalog/services.toml")).unwrap()
}

#[test]
fn instance_accounts_control_connection_origins_without_task_specific_target_hints() {
    let catalog = catalog();
    for (scenario, origin) in [
        (
            "code/gitlab/file-circuit-breaker-issue",
            "https://gitlab.northwind.internal",
        ),
        (
            "code/multi/argocd-degraded-gitlab-commit",
            "https://gitlab.lumen.internal",
        ),
    ] {
        let dataset = Dataset::load(&root().join("scenarios").join(scenario)).unwrap();
        let connection = connections::materialize(&dataset, &catalog).unwrap();
        assert_eq!(connection.manifest.accounts["gitlab"].origin, origin);
        assert_eq!(
            connection.environment["GITLAB_API_URL"],
            format!("{origin}/api/v4")
        );
        assert_eq!(
            connection.environment["GITLAB_GRAPHQL_URL"],
            format!("{origin}/api/graphql")
        );
        assert_eq!(
            connection
                .environment
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "ARGOCD_BASE_URL",
                "GITLAB_API_URL",
                "GITLAB_GRAPHQL_URL",
                "GRAFANA_URL",
                "PAGERDUTY_API_HOST",
                "SENTRY_URL",
                "TFE_ADDRESS"
            ],
        );
        assert_eq!(
            connection.environment["TFE_ADDRESS"],
            "https://app.terraform.io/"
        );
        assert_eq!(
            connection.environment["ARGOCD_BASE_URL"],
            "https://argocd.lumen-retail.dev/"
        );
        assert_eq!(
            connection.environment["SENTRY_URL"],
            "https://sentry.lumenlabs.dev/"
        );
        assert_eq!(
            connection.environment["GRAFANA_URL"],
            "https://grafana.octo-corp.com"
        );
        assert_eq!(
            connection.environment["PAGERDUTY_API_HOST"],
            "https://api.pagerduty.com"
        );
        assert!(
            !connection
                .environment
                .contains_key("PAGERDUTY_USER_API_KEY")
        );
        assert!(
            !connection
                .environment
                .contains_key("GRAFANA_SERVICE_ACCOUNT_TOKEN")
        );
    }
}

#[test]
fn sentry_account_origins_do_not_confuse_ingestion_with_the_management_connection() {
    let catalog = catalog();
    for origin in [
        "https://sentry.lumenlabs.dev",
        "https://sentry.eu.lumenlabs.dev",
    ] {
        let mut dataset =
            Dataset::load(&root().join("scenarios/ops/sentry/create-notification-project"))
                .unwrap();
        dataset
            .services
            .get_mut("sentry")
            .unwrap()
            .collection_mut("instance")
            .insert(
                "instance",
                json!({
                    "public_origin":origin,"ingestion_origin":"https://ingest.example.net/sentry"
                }),
            );
        let connection = connections::materialize(&dataset, &catalog).unwrap();
        assert_eq!(connection.manifest.accounts["sentry"].origin, origin);
        assert_eq!(connection.environment["SENTRY_URL"], format!("{origin}/"));
        assert!(!connection.environment.contains_key("SENTRY_ACCESS_TOKEN"));
    }
}

#[test]
fn undeclared_or_malformed_account_origins_are_not_silently_replaced() {
    let catalog = catalog();
    for value in [
        json!("http://world:8107"),
        json!("https://unlisted.example"),
        json!("https://gitlab.lumen.internal/other"),
        json!(42),
    ] {
        let mut dataset =
            Dataset::load(&root().join("scenarios/code/gitlab/file-circuit-breaker-issue"))
                .unwrap();
        dataset
            .services
            .get_mut("gitlab")
            .unwrap()
            .collection_mut("instance")
            .get_mut("instance")
            .unwrap()["public_origin"] = value;
        assert!(connections::materialize(&dataset, &catalog).is_err());
    }
}

#[test]
fn private_connection_metadata_matches_tools_configuration_and_protected_context() {
    let scenario =
        Scenario::load(&root().join("scenarios/code/multi/argocd-degraded-gitlab-commit")).unwrap();
    let output = tempfile::tempdir().unwrap();
    write_task(
        &scenario,
        &catalog(),
        &Images {
            world: "world:fixture".into(),
            tools: "tools:fixture".into(),
        },
        output.path(),
    )
    .unwrap();
    let task = output.path().join(scenario.task_name());
    let context: TaskContext =
        serde_json::from_slice(&fs::read(task.join("tests/task-context.json")).unwrap()).unwrap();
    let connection: Value =
        serde_json::from_slice(&fs::read(task.join("tests/connections.json")).unwrap()).unwrap();
    assert_eq!(
        connection["accounts"]["gitlab"]["origin"],
        "https://gitlab.lumen.internal"
    );
    let compose = fs::read_to_string(task.join("environment/docker-compose.yaml")).unwrap();
    assert!(compose.contains("GITLAB_API_URL: \"https://gitlab.lumen.internal/api/v4\""));
    assert_eq!(
        context.connector_env_sha256["GITLAB_API_URL"],
        sha256(b"https://gitlab.lumen.internal/api/v4")
    );
    assert_eq!(
        fs::read_to_string(task.join("instruction.md")).unwrap(),
        scenario.instruction
    );
    assert!(!task.join("environment/connections.json").exists());
    assert!(
        !fs::read_to_string(task.join("environment/Dockerfile"))
            .unwrap()
            .contains("connections.json")
    );
}
