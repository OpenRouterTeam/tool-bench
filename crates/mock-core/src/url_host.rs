use serde_json::Value;

pub(crate) fn normalized(source: &str) -> Option<String> {
    let (scheme, remainder) = if let Some(remainder) = source.strip_prefix("https://") {
        ("https://", remainder)
    } else if let Some(remainder) = source.strip_prefix("http://") {
        ("http://", remainder)
    } else {
        ("", source)
    };
    let (host, path) = remainder.split_once('/')?;
    if host.is_empty()
        || !host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'*'))
        || remainder.contains("://")
    {
        return None;
    }
    Some(format!("{scheme}{}/{path}", host.to_ascii_lowercase()))
}

pub(crate) fn record(source: &Value, fields: &[String]) -> Option<Value> {
    let mut result = source.clone();
    for field in fields {
        let source = source.pointer(field)?.as_str()?;
        *result.pointer_mut(field)? = Value::String(normalized(source)?);
    }
    Some(result)
}
