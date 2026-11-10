//! The real `tools-supervisor` binary registers exactly what a task's exposure directive
//! names and keeps a passive transcript of what it served.

use std::{
    fs,
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use mcp_tools::{
    client::McpClient,
    exposure::{
        EXPOSURE_ENV, EndpointExposure, ExposureDirective, definition_sha256, sequence_sha256,
    },
    http_client::HttpClient,
    snapshot::{Snapshot, capture},
};
use serde_json::{Value, json};

struct Supervisor(Child);

impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn tools(prefix: &str) -> Vec<Value> {
    ["read", "write", "search"]
        .iter()
        .map(|verb| {
            json!({"name": format!("{prefix}_{verb}"), "description": format!("{verb} a record."),
            "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}}}})
        })
        .collect()
}

struct Fixture {
    directory: tempfile::TempDir,
    ports: [u16; 2],
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let ports = [free_port(), free_port()];
    let mut catalog = String::new();
    for (index, name) in ["acme", "globex"].iter().enumerate() {
        catalog.push_str(&format!(
            "[[service]]\nname = \"{name}\"\ngroup = \"ops\"\nmode = \"replica\"\nworld_port = {}\n\
             [[service.endpoint]]\nid = \"{name}\"\nmcp_port = {}\nruntime = \"replica\"\n\
             snapshot = \"{name}.json\"\nsource = \"fixture\"\n\n",
            8100 + index,
            ports[index]
        ));
        let snapshot = Snapshot::new(
            name,
            json!({"name": name, "version": "1"}),
            Value::Null,
            tools(name),
        );
        fs::write(
            directory.path().join(format!("{name}.json")),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
    }
    fs::write(directory.path().join("services.toml"), catalog).unwrap();
    Fixture { directory, ports }
}

fn start(fixture: &Fixture, directive: Option<&ExposureDirective>) -> Supervisor {
    let root = fixture.directory.path();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tools-supervisor"));
    command
        .arg("--catalog")
        .arg(root.join("services.toml"))
        .arg("--snapshots")
        .arg(root)
        .arg("--surface-log")
        .arg(root.join("surface"))
        .env_remove(EXPOSURE_ENV)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(directive) = directive {
        command.env(EXPOSURE_ENV, serde_json::to_string(directive).unwrap());
    }
    Supervisor(command.spawn().unwrap())
}

async fn listening(port: u16) -> bool {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

fn exposure(endpoint: &str, names: &[&str], pinned: &[Value]) -> EndpointExposure {
    let hashes: Vec<(String, String)> = names
        .iter()
        .map(|name| {
            let definition = pinned.iter().find(|tool| tool["name"] == *name).unwrap();
            ((*name).to_owned(), definition_sha256(definition))
        })
        .collect();
    EndpointExposure {
        id: endpoint.into(),
        tools: Some(names.iter().map(|name| (*name).to_owned()).collect()),
        sha256: sequence_sha256(
            hashes
                .iter()
                .map(|(name, hash)| (name.as_str(), hash.as_str())),
        ),
    }
}

fn transcript(root: &Path, endpoint: &str) -> Vec<Value> {
    fs::read_to_string(root.join("surface").join(format!("{endpoint}.jsonl")))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn directive_decides_registered_endpoints_tools_and_order_and_the_transcript_records_them() {
    let fixture = fixture();
    let directive = ExposureDirective {
        schema_version: 1,
        endpoints: vec![exposure(
            "globex",
            &["globex_search", "globex_read"],
            &tools("globex"),
        )],
    };
    let _supervisor = start(&fixture, Some(&directive));
    assert!(
        listening(fixture.ports[1]).await,
        "registered endpoint is served"
    );
    // The unregistered endpoint is not served at all.
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", fixture.ports[0]))
            .await
            .is_err()
    );

    let mut client = McpClient::Http(HttpClient::new(format!(
        "http://127.0.0.1:{}/mcp",
        fixture.ports[1]
    )));
    let listed = capture(&mut client, "globex").await.unwrap();
    let names: Vec<&str> = listed
        .tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["globex_search", "globex_read"]);
    // Served definitions are the pinned ones, untouched.
    assert_eq!(listed.tools[1], tools("globex")[0]);
    let hidden = client
        .request(
            "tools/call",
            json!({"name": "globex_write", "arguments": {}}),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(hidden.contains("Unknown tool"), "{hidden}");

    let records = transcript(fixture.directory.path(), "globex");
    assert_eq!(
        records[0],
        json!({"endpoint": "globex", "registered": true})
    );
    let methods: Vec<&str> = records[1..]
        .iter()
        .map(|record| record["method"].as_str().unwrap())
        .collect();
    assert_eq!(methods, ["initialize", "tools/list", "tools/call"]);
    assert_eq!(
        records[2]["tools"],
        json!([
            ["globex_search", definition_sha256(&tools("globex")[2])],
            ["globex_read", definition_sha256(&tools("globex")[0])]
        ])
    );
    assert!(
        records[3]["body"]
            .as_str()
            .unwrap()
            .contains("Unknown tool")
    );
    assert!(!fixture.directory.path().join("surface/acme.jsonl").exists());
}

#[tokio::test]
async fn a_definition_that_differs_from_its_pin_is_never_registered() {
    let fixture = fixture();
    let mut drifted = exposure("acme", &["acme_read"], &tools("acme"));
    drifted.sha256 = format!("sha256:{}", "0".repeat(64));
    let _supervisor = start(
        &fixture,
        Some(&ExposureDirective {
            schema_version: 1,
            endpoints: vec![drifted],
        }),
    );
    assert!(listening(fixture.ports[0]).await);
    let mut client = McpClient::Http(HttpClient::new(format!(
        "http://127.0.0.1:{}/mcp",
        fixture.ports[0]
    )));
    let error = capture(&mut client, "acme").await.unwrap_err();
    assert!(
        format!("{error:#}").contains("pinned definitions"),
        "{error:#}"
    );
}

#[tokio::test]
async fn without_a_directive_every_endpoint_serves_its_full_pinned_list_and_is_transcribed() {
    let fixture = fixture();
    let _supervisor = start(&fixture, None);
    for (index, name) in ["acme", "globex"].iter().enumerate() {
        assert!(listening(fixture.ports[index]).await);
        let mut client = McpClient::Http(HttpClient::new(format!(
            "http://127.0.0.1:{}/mcp",
            fixture.ports[index]
        )));
        assert_eq!(capture(&mut client, name).await.unwrap().tools, tools(name));
        assert_eq!(transcript(fixture.directory.path(), name).len(), 3);
    }
}

#[test]
fn a_directive_naming_an_unknown_endpoint_or_malformed_json_stops_the_supervisor() {
    let fixture = fixture();
    for value in [
        r#"{"schema_version":1,"endpoints":[{"id":"initech","tools":null,"sha256":"sha256:00"}]}"#,
        r#"{"schema_version":1,"endpoints":[],"variant":"relevant_only"}"#,
        "not json",
    ] {
        let status = Command::new(env!("CARGO_BIN_EXE_tools-supervisor"))
            .arg("--catalog")
            .arg(fixture.directory.path().join("services.toml"))
            .arg("--snapshots")
            .arg(fixture.directory.path())
            .arg("--surface-log")
            .arg(fixture.directory.path().join("surface"))
            .env(EXPOSURE_ENV, value)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "{value}");
    }
}
