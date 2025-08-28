#[test]
fn web_schema_has_expected_sections_and_fields() {
    let schema = phaeton::web_schema::build_ui_schema();
    let sections = schema.get("sections").and_then(|v| v.as_object()).unwrap();
    // Spot-check a few top-level sections
    for key in [
        "modbus", "defaults", "controls", "logging", "tibber", "web", "updates",
    ] {
        assert!(sections.get(key).is_some(), "missing section: {}", key);
    }

    // Check a couple of field entries exist
    let modbus = sections.get("modbus").unwrap().get("fields").unwrap();
    assert!(modbus.get("ip").is_some());
    assert!(modbus.get("port").is_some());

    let logging = sections.get("logging").unwrap().get("fields").unwrap();
    assert!(logging.get("level").is_some());
    assert!(logging.get("file").is_some());
}
