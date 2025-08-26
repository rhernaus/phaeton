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

#[cfg(test)]
mod tests {
    use super::format_text_value;

    #[test]
    fn format_numbers_and_text() {
        assert_eq!(format_text_value(&serde_json::json!(1.2345)), "1.23");
        // Integers are formatted as floating point with 2 decimals
        assert_eq!(format_text_value(&serde_json::json!(2)), "2.00");
        assert_eq!(format_text_value(&serde_json::json!("abc")), "abc");
        assert_eq!(format_text_value(&serde_json::json!(true)), "true");
        // Objects/arrays fall back to to_string
        assert!(format_text_value(&serde_json::json!({"k":"v"})).contains("k"));
    }
}
