//! Shared JSON HTTP helpers for LiveHttp adapters.
//! Prefer force-direct (no system proxy) like Python `http_util`.

use crate::types::AdapterError;
use serde_json::Value;

/// Build a reqwest client; when `force_direct`, disable system proxy.
pub fn build_client(force_direct: bool) -> Result<reqwest::Client, AdapterError> {
    let mut b = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("checkin-suite/1.0");
    if force_direct {
        b = b.no_proxy();
    }
    b.build()
        .map_err(|e| AdapterError::Network(format!("client_build: {e}")))
}

/// POST JSON body; returns (http_status, parsed_json).
/// Non-2xx with JSON body is still returned (WB may use HTTP 400 + code=10001).
pub async fn post_json(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, String)],
    body: &Value,
) -> Result<(u16, Value), AdapterError> {
    let mut req = client.post(url).json(body);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| AdapterError::Network(format!("send: {e}")))?;
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .await
        .map_err(|e| AdapterError::Network(format!("read_body: {e}")))?;
    let parsed = safe_json(&text);
    Ok((status, parsed))
}

fn safe_json(raw: &str) -> Value {
    if raw.trim().is_empty() {
        return Value::Object(serde_json::Map::new());
    }
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(m)) => Value::Object(m),
        Ok(other) => serde_json::json!({ "_data": other }),
        Err(_) => serde_json::json!({ "_raw": raw.chars().take(500).collect::<String>() }),
    }
}

/// Extract business code from common response shapes.
pub fn extract_code(parsed: &Value) -> Option<i64> {
    for key in ["code", "err_code", "errno", "status_code"] {
        if let Some(c) = as_i64(parsed.get(key)) {
            return Some(c);
        }
    }
    if let Some(data) = parsed.get("data").filter(|d| d.is_object()) {
        for key in ["code", "err_code"] {
            if let Some(c) = as_i64(data.get(key)) {
                return Some(c);
            }
        }
    }
    None
}

/// Pick int from `data` then optional fallback object.
pub fn pick_int(data: &Value, key: &str, fallback: Option<&Value>) -> Option<i64> {
    if let Some(v) = as_i64(data.get(key)) {
        return Some(v);
    }
    if let Some(fb) = fallback {
        if let Some(v) = as_i64(fb.get(key)) {
            return Some(v);
        }
    }
    None
}

fn as_i64(v: Option<&Value>) -> Option<i64> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    if let Some(i) = v.as_i64() {
        return Some(i);
    }
    if let Some(u) = v.as_u64() {
        return i64::try_from(u).ok();
    }
    if let Some(f) = v.as_f64() {
        return Some(f as i64);
    }
    if let Some(s) = v.as_str() {
        return s.parse().ok();
    }
    None
}

/// Nested `data` object or the whole parsed body.
pub fn data_object(parsed: &Value) -> &Value {
    match parsed.get("data") {
        Some(d) if d.is_object() => d,
        _ => parsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_code_top_level() {
        let v = serde_json::json!({"code": 10001, "message": "x"});
        assert_eq!(extract_code(&v), Some(10001));
    }

    #[test]
    fn pick_int_fallback() {
        let data = serde_json::json!({});
        let fb = serde_json::json!({"streak_days": 5});
        assert_eq!(pick_int(&data, "streak_days", Some(&fb)), Some(5));
    }
}
