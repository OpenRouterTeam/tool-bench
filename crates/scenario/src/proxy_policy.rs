//! Approved identity of the host provider-proxy inference capture.
//!
//! The digest covers the proxy, its sealer and the evidence schema module, in the same
//! framing `deploy/runner/inference.py::_proxy_adapter_sha256` uses.

use mock_core::inference::AdapterIdentity;
use sha2::{Digest, Sha256};

pub fn adapter() -> AdapterIdentity {
    let mut digest = Sha256::new();
    // Sorted by file name, as the runner sorts them.
    for (name, bytes) in [
        (
            "arm-proxy.ts",
            include_bytes!("../../../deploy/arm-proxy.ts").as_slice(),
        ),
        (
            "inference.py",
            include_bytes!("../../../deploy/runner/inference.py").as_slice(),
        ),
        (
            "proxy_capture.py",
            include_bytes!("../../../deploy/runner/proxy_capture.py").as_slice(),
        ),
    ] {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    AdapterIdentity {
        name: mock_core::transport::PROXY_ADAPTER_NAME.into(),
        code_sha256: format!(
            "sha256:{}",
            digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ),
    }
}
