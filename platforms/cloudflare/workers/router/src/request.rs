use serde_json::{Map, Value};
use worker::Request;

pub(crate) fn query_param(url: &worker::Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

pub(crate) fn decode_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

pub(crate) async fn read_json(req: &mut Request) -> Value {
    req.json::<Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({}))
}

pub(crate) async fn read_mastodon_body(req: &mut Request) -> Value {
    let content_type = request_content_type(req);
    if content_type.contains("application/json") {
        return read_json(req).await;
    }
    if content_type.contains("application/x-www-form-urlencoded") {
        let text = req.text().await.unwrap_or_default();
        let mut body = Map::new();
        for pair in text.split('&').filter(|part| !part.is_empty()) {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().map(decode_form_component).unwrap_or_default();
            if key.is_empty() {
                continue;
            }
            let value = parts.next().map(decode_form_component).unwrap_or_default();
            insert_repeating_body_value(&mut body, key, Value::String(value));
        }
        return Value::Object(body);
    }
    serde_json::json!({})
}

pub(crate) fn request_content_type(req: &Request) -> String {
    req.headers()
        .get("Content-Type")
        .ok()
        .flatten()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn decode_form_component(value: &str) -> String {
    decode_component(&value.replace('+', " "))
}

fn insert_repeating_body_value(body: &mut Map<String, Value>, key: String, value: Value) {
    match body.get_mut(&key) {
        Some(Value::Array(items)) => items.push(value),
        Some(existing) => {
            let previous = existing.clone();
            *existing = Value::Array(vec![previous, value]);
        }
        None => {
            body.insert(key, value);
        }
    }
}

pub(crate) fn required_body_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(number)) if number.as_i64().unwrap_or(1) != 0 => {
            Some(number.to_string())
        }
        Some(Value::Bool(true)) => Some("true".to_string()),
        _ => None,
    }
}

pub(crate) fn string_like_field(body: &Value, key: &str) -> Option<String> {
    body.get(key).map(|value| match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => value.to_string(),
    })
}

pub(crate) fn string_like_any(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| string_like_field(body, key))
}

pub(crate) fn optional_trimmed_body(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| body.get(*key).and_then(optional_body_string))
}

pub(crate) fn optional_body_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(number) if number.as_i64().unwrap_or(1) != 0 => Some(number.to_string()),
        Value::Bool(true) => Some("true".to_string()),
        _ => None,
    }
}

pub(crate) fn optional_url_field(
    body: &Value,
    key: &str,
    field: &str,
) -> std::result::Result<Option<String>, String> {
    let Some(value) = body.get(key).and_then(optional_body_string) else {
        return Ok(None);
    };
    let url =
        worker::Url::parse(&value).map_err(|_| format!("{field} must be an absolute https URL"))?;
    if url.scheme() != "https" {
        return Err(format!("{field} must be an absolute https URL"));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn body_string_helpers_trim_and_reject_empty_values() {
        assert_eq!(
            required_body_string(Some(&json!(" hello "))).as_deref(),
            Some("hello")
        );
        assert_eq!(required_body_string(Some(&json!("   "))), None);
        assert_eq!(
            required_body_string(Some(&json!(42))).as_deref(),
            Some("42")
        );
        assert_eq!(required_body_string(Some(&json!(0))), None);
        assert_eq!(
            required_body_string(Some(&json!(true))).as_deref(),
            Some("true")
        );
        assert_eq!(required_body_string(Some(&json!(false))), None);

        assert_eq!(
            optional_body_string(&json!(" value ")).as_deref(),
            Some("value")
        );
        assert_eq!(optional_body_string(&json!("")), None);
        assert_eq!(optional_body_string(&json!(0)), None);
    }

    #[test]
    fn string_like_any_preserves_first_matching_field() {
        let body = json!({
            "empty": null,
            "count": 3,
            "flag": true,
            "name": "Ada"
        });

        assert_eq!(
            string_like_any(&body, &["missing", "name"]).as_deref(),
            Some("Ada")
        );
        assert_eq!(string_like_any(&body, &["count"]).as_deref(), Some("3"));
        assert_eq!(string_like_any(&body, &["flag"]).as_deref(), Some("true"));
        assert_eq!(string_like_any(&body, &["empty"]).as_deref(), Some(""));
    }

    #[test]
    fn optional_url_field_accepts_only_absolute_https_urls() {
        let body = json!({
            "icon": "https://example.test/icon.png",
            "bad": "http://example.test/icon.png",
            "relative": "/icon.png"
        });

        assert_eq!(
            optional_url_field(&body, "icon", "icon")
                .unwrap()
                .as_deref(),
            Some("https://example.test/icon.png")
        );
        assert!(optional_url_field(&body, "missing", "missing")
            .unwrap()
            .is_none());
        assert_eq!(
            optional_url_field(&body, "bad", "bad").unwrap_err(),
            "bad must be an absolute https URL"
        );
        assert_eq!(
            optional_url_field(&body, "relative", "relative").unwrap_err(),
            "relative must be an absolute https URL"
        );
    }

    #[test]
    fn decode_component_handles_invalid_percent_sequences_losslessly() {
        assert_eq!(decode_component("Ada%20Lovelace"), "Ada Lovelace");
        assert_eq!(decode_component("bad%zz"), "bad%zz");
    }
}
