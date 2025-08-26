pub(crate) fn format_text_value(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                format!("{:.2}", f)
            } else {
                n.to_string()
            }
        }
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => val.to_string(),
    }
}
