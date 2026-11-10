use axum::http::{HeaderMap, HeaderValue};
use mock_core::request::{CapturedBody, CapturedHeader, CapturedHeaderValue, HeaderRegistry};

#[test]
fn body_capture_preserves_json_whitespace_forms_and_non_utf8_bytes() {
    for bytes in [
        br#" { "count": 2 } "#.as_slice(),
        b"field=two+words&value=%00",
        &[0, 255, 128, 10],
    ] {
        let body = CapturedBody::capture(bytes);
        let decoded: CapturedBody =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(decoded.bytes().as_ref(), bytes);
    }
}

#[test]
fn unknown_credentials_and_custom_headers_are_hashed_not_recorded_as_plaintext() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer live-provider-secret"),
    );
    headers.insert(
        "cookie",
        HeaderValue::from_static("session=private-session"),
    );
    headers.insert(
        "x-private-context",
        HeaderValue::from_static("opaque-sensitive-value"),
    );
    let registry = HeaderRegistry::default();
    let captured = registry.capture("example", &headers);
    let encoded = serde_json::to_string(&captured).unwrap();
    assert!(
        !encoded.contains("live-provider-secret")
            && !encoded.contains("private-session")
            && !encoded.contains("opaque-sensitive-value")
    );
    assert!(
        captured
            .iter()
            .all(|header| matches!(header.value, CapturedHeaderValue::Redacted { .. }))
    );
    assert!(registry.restore("example", &captured).is_err());
}

#[test]
fn only_declared_synthetic_values_can_be_hydrated_in_their_service_and_header_scope() {
    let mut registry = HeaderRegistry::default();
    registry
        .insert_synthetic(
            "example",
            "authorization",
            "fixture/example/auth",
            "Bearer fake-token",
        )
        .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer fake-token"),
    );
    let captured = registry.capture("example", &headers);
    assert!(
        !serde_json::to_string(&captured)
            .unwrap()
            .contains("fake-token")
    );
    assert_eq!(
        registry.restore("example", &captured).unwrap()["authorization"],
        "Bearer fake-token"
    );
    assert!(registry.restore("other", &captured).is_err());
    let mut tampered = captured.clone();
    if let CapturedHeaderValue::Synthetic { sha256, .. } = &mut tampered[0].value {
        sha256.push('0');
    }
    assert!(registry.restore("example", &tampered).is_err());
    assert!(
        registry
            .insert_synthetic(
                "other",
                "authorization",
                "fixture/example/auth",
                "replacement"
            )
            .is_err()
    );
}

#[test]
fn public_headers_preserve_duplicate_values_and_transport_framing_is_not_replayed() {
    let mut headers = HeaderMap::new();
    headers.append("accept", HeaderValue::from_static("application/json"));
    headers.append("accept", HeaderValue::from_static("text/plain"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("host", HeaderValue::from_static("original.example"));
    headers.insert("content-length", HeaderValue::from_static("3"));
    headers.insert("connection", HeaderValue::from_static("keep-alive"));
    let registry = HeaderRegistry::default();
    let captured = registry.capture("example", &headers);
    assert_eq!(captured.len(), 3);
    let restored = registry.restore("example", &captured).unwrap();
    assert_eq!(
        restored
            .get_all("accept")
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>(),
        ["application/json", "text/plain"]
    );
    assert!(!restored.contains_key("host") && !restored.contains_key("content-length"));
}

#[test]
fn marking_an_arbitrary_credential_public_cannot_bypass_the_registry() {
    let forged = [CapturedHeader {
        name: "authorization".into(),
        value: CapturedHeaderValue::Public {
            value: "Bearer forged".into(),
        },
    }];
    assert!(
        HeaderRegistry::default()
            .restore("example", &forged)
            .is_err()
    );
}
