use serde_json::Value;

/// Apply a JSON merge patch (RFC 7396) to a document
pub fn apply_merge_patch(doc: &mut Value, patch: &Value) {
    match (doc, patch) {
        (doc @ &mut Value::Object(_), Value::Object(patch_map)) => {
            let doc_map = doc.as_object_mut().unwrap();
            for (key, patch_value) in patch_map {
                if patch_value.is_null() {
                    doc_map.remove(key);
                } else if let Some(existing) = doc_map.get_mut(key) {
                    apply_merge_patch(existing, patch_value);
                } else {
                    doc_map.insert(key.clone(), patch_value.clone());
                }
            }
        }
        (doc, patch) => {
            *doc = patch.clone();
        }
    }
}
