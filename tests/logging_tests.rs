// New tests for logging parse_line_level and should_emit_to_web

#[test]
fn should_emit_filters_below_runtime_level() {
    use phaeton::logging::{set_web_log_level, should_emit_to_web};
    use tracing::Level;
    // Set runtime level to WARN, INFO lines should be filtered out, ERROR should pass
    set_web_log_level(Level::WARN);
    assert!(!should_emit_to_web(" INFO message"));
    assert!(should_emit_to_web(" ERROR something"));
}
