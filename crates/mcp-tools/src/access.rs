//! Reports what reaches an MCP endpoint to the world, which seals the reports with its
//! evidence (`mock_core::tool_access`). The world sees an API request only after a tool
//! server makes one, always from the tools container; who asked the tool server is known
//! only here.
//!
//! Reporting is passive. A report that cannot be delivered is dropped after a few tries
//! and never delays or changes the answer to the client.

use std::{sync::Arc, time::Duration};

use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::front::{Access, Observer};

/// Path of the world's report endpoint, beside the CA it serves to the tools container.
pub const REPORT_PATH: &str = "/_bench/tool-access";

/// The report endpoint that sits beside `ca_url` (`.../_bench/ca.pem`).
pub fn report_url(ca_url: &str) -> Option<String> {
    let origin = ca_url.strip_suffix("/_bench/ca.pem")?;
    Some(format!("{origin}{REPORT_PATH}"))
}

/// Starts the task that delivers reports to `url` in the order they were made.
pub fn deliver(url: String) -> UnboundedSender<Value> {
    let (sender, mut receiver) = unbounded_channel::<Value>();
    tokio::spawn(async move {
        let http = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client");
        while let Some(report) = receiver.recv().await {
            for attempt in 0..3 {
                match http.post(&url).json(&report).send().await {
                    Ok(_) => break,
                    Err(error) if attempt == 2 => {
                        eprintln!("tools: a tool-access report was not delivered: {error}");
                    }
                    Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
                }
            }
        }
    });
    sender
}

/// Sends one endpoint's requests and failed tool calls to the delivery task.
pub struct AccessReporter {
    endpoint: String,
    sender: UnboundedSender<Value>,
}

impl AccessReporter {
    /// Announces the endpoint, so the world can tell "nothing arrived" from "nothing
    /// was watching".
    pub fn new(endpoint: &str, sender: UnboundedSender<Value>) -> Self {
        let reporter = Self {
            endpoint: endpoint.to_owned(),
            sender,
        };
        reporter.send(json!({"endpoint": endpoint, "event": "registered"}));
        reporter
    }

    fn send(&self, report: Value) {
        // The receiver lives as long as the process; a closed channel loses the report.
        let _ = self.sender.send(report);
    }
}

/// The report of one request, in the shape the world stores.
pub fn request_report(endpoint: &str, access: &Access) -> Value {
    let mut report = json!({"endpoint": endpoint, "event": "request", "method": access.method});
    for (name, value) in [
        ("peer", &access.peer),
        ("user_agent", &access.user_agent),
        ("session", &access.session),
        ("client", &access.client),
        ("tool", &access.tool),
    ] {
        if let Some(value) = value {
            report[name] = json!(value);
        }
    }
    report
}

/// Whether a served `tools/call` body is an error: a JSON-RPC error or `isError`.
pub fn tool_call_failed(body: &str) -> bool {
    serde_json::from_str::<Value>(body).is_ok_and(|reply| {
        reply.get("error").is_some_and(|error| !error.is_null())
            || reply["result"]["isError"] == true
    })
}

impl Observer for AccessReporter {
    fn served(&self, method: &str, body: &str) {
        if method == "tools/call" && tool_call_failed(body) {
            self.send(json!({"endpoint": self.endpoint, "event": "tool_error"}));
        }
    }

    fn accessed(&self, access: &Access) {
        self.send(request_report(&self.endpoint, access));
    }
}

/// Several observers behind one.
pub struct Observers(pub Vec<Arc<dyn Observer>>);

impl Observer for Observers {
    fn served(&self, method: &str, body: &str) {
        for observer in &self.0 {
            observer.served(method, body);
        }
    }

    fn accessed(&self, access: &Access) {
        for observer in &self.0 {
            observer.accessed(access);
        }
    }
}
