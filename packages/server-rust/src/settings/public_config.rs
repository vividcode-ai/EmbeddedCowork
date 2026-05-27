use serde_json::Value;

/// Sanitize config owner values for public publishing (redact sensitive fields)
pub fn sanitize_config_owner(_owner: &str, value: Value) -> Value {
    // Redact sensitive fields like api keys
    if let Value::Object(map) = &value {
        let mut result = map.clone();
        let sensitive_keys = ["apiKey", "api_key", "password", "token", "secret"];
        for key in sensitive_keys {
            if result.contains_key(key) {
                result.insert(key.to_string(), Value::String("[REDACTED]".to_string()));
            }
        }
        Value::Object(result)
    } else {
        value
    }
}
