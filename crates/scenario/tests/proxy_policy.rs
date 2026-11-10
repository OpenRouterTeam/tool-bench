//! The generated runner policy approves exactly the provider-proxy capture the runner emits.

use std::{fs, path::Path};

use serde_json::Value;

#[test]
fn generated_policy_pins_the_proxy_capture_identity_the_runner_stamps() {
    let approved = scenario::proxy_policy::adapter();
    assert_eq!(approved.name, mock_core::transport::PROXY_ADAPTER_NAME);
    // This document was sealed by deploy/runner/proxy_capture.py from real proxy output,
    // so its adapter digest is the runner's own computation over the same three files.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mock-core/tests/fixtures/proxy-capture/completed.json");
    let sealed: Value = serde_json::from_slice(&fs::read(fixture).unwrap()).unwrap();
    assert_eq!(
        sealed["adapter"]["code_sha256"], approved.code_sha256,
        "deploy/arm-proxy.ts, deploy/runner/proxy_capture.py or deploy/runner/inference.py \
         changed: regenerate with `UPDATE_PROXY_FIXTURES=1 python -m unittest \
         deploy.runner.test_proxy_capture`"
    );
    assert_eq!(sealed["adapter"]["name"], approved.name);
}
