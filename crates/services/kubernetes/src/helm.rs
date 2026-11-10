use std::io::Read;

use serde_json::Value;

use crate::util::b64_decode;

const MAX_DECODED: u64 = 16 * 1024 * 1024;

pub(super) fn release(secret: &Value) -> Result<Option<(String, Value)>, &'static str> {
    if secret["type"] != "helm.sh/release.v1" {
        return Ok(None);
    }
    let outer = secret["data"]["release"]
        .as_str()
        .ok_or("missing Helm release data")?;
    if outer.len() > MAX_DECODED as usize {
        return Err("Helm release exceeds the supported encoded size");
    }
    let inner = b64_decode(outer).ok_or("invalid Secret release encoding")?;
    let inner = std::str::from_utf8(&inner).map_err(|_| "invalid Helm storage encoding")?;
    let bytes = b64_decode(inner).ok_or("invalid Helm release encoding")?;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b, 8]) {
        let mut decoder = flate2::bufread::GzDecoder::new(bytes.as_slice());
        let mut decoded = Vec::new();
        decoder
            .by_ref()
            .take(MAX_DECODED + 1)
            .read_to_end(&mut decoded)
            .map_err(|_| "invalid compressed Helm release")?;
        if decoded.len() as u64 > MAX_DECODED || !decoder.into_inner().is_empty() {
            return Err("unsupported Helm release size or trailing compressed data");
        }
        decoded
    } else {
        bytes
    };
    let release: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid Helm release JSON")?;
    let name = release["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or("missing release name")?;
    let namespace = release["namespace"]
        .as_str()
        .filter(|namespace| !namespace.is_empty())
        .ok_or("missing release namespace")?;
    let version = release["version"]
        .as_i64()
        .filter(|version| *version > 0)
        .ok_or("invalid release revision")?;
    let expected = format!("sh.helm.release.v1.{name}.v{version}");
    if secret["metadata"]["name"] != expected || secret["metadata"]["namespace"] != namespace {
        return Err("release identity differs from its storage Secret");
    }
    Ok(Some((format!("{namespace}/{name}.v{version}"), release)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::b64_encode;
    use serde_json::json;
    use std::io::Write;

    fn secret(compressed: bool) -> Value {
        let data =
            json!({"name":"test","namespace":"perf","version":2,"info":{"status":"uninstalled"}});
        let bytes = if compressed {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            encoder.write_all(data.to_string().as_bytes()).unwrap();
            encoder.finish().unwrap()
        } else {
            data.to_string().into_bytes()
        };
        json!({"type":"helm.sh/release.v1","metadata":{"name":"sh.helm.release.v1.test.v2","namespace":"perf"},
            "data":{"release":b64_encode(b64_encode(&bytes).as_bytes())}})
    }

    #[test]
    fn real_gzip_and_legacy_storage_have_the_same_authoritative_release() {
        assert_eq!(
            release(&secret(true)).unwrap(),
            release(&secret(false)).unwrap()
        );
    }

    #[test]
    fn invalid_or_cross_identity_release_writes_cannot_hide_bookkeeping_state() {
        for (field, value) in [
            ("namespace", "other"),
            ("name", "sh.helm.release.v1.test.v3"),
        ] {
            let mut data = secret(true);
            data["metadata"][field] = json!(value);
            assert!(release(&data).is_err());
        }
        let mut data = secret(true);
        data["data"]["release"] = json!("bad");
        assert!(release(&data).is_err());
        assert_eq!(release(&json!({"type":"Opaque"})).unwrap(), None);
    }

    #[test]
    fn corrupt_or_trailing_compressed_bytes_do_not_become_authoritative_release_state() {
        let original = secret(true);
        let inner = b64_decode(original["data"]["release"].as_str().unwrap()).unwrap();
        let compressed = b64_decode(std::str::from_utf8(&inner).unwrap()).unwrap();
        for corrupt in [true, false] {
            let mut bytes = compressed.clone();
            if corrupt {
                *bytes.last_mut().unwrap() ^= 1;
            } else {
                bytes.extend_from_slice(b"unexpected trailing input");
            }
            let mut candidate = original.clone();
            candidate["data"]["release"] = json!(b64_encode(b64_encode(&bytes).as_bytes()));
            assert!(release(&candidate).is_err());
        }
    }
}
