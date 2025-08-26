use phaeton::error::PhaetonError;

#[test]
fn error_constructors_group_1() {
    assert!(matches!(
        PhaetonError::config("x"),
        PhaetonError::Config { .. }
    ));
    assert!(matches!(
        PhaetonError::modbus("x"),
        PhaetonError::Modbus { .. }
    ));
    assert!(matches!(PhaetonError::dbus("x"), PhaetonError::DBus { .. }));
    assert!(matches!(PhaetonError::web("x"), PhaetonError::Web { .. }));
}

#[test]
fn error_constructors_group_2() {
    let ser = PhaetonError::Serialization {
        message: "s".into(),
    };
    assert!(matches!(ser, PhaetonError::Serialization { .. }));
    assert!(matches!(PhaetonError::io("x"), PhaetonError::Io { .. }));
    assert!(matches!(
        PhaetonError::network("x"),
        PhaetonError::Network { .. }
    ));
    assert!(matches!(PhaetonError::api("x"), PhaetonError::Api { .. }));
}

#[test]
fn error_constructors_group_3() {
    assert!(matches!(PhaetonError::auth("x"), PhaetonError::Auth { .. }));
    assert!(matches!(
        PhaetonError::validation("f", "m"),
        PhaetonError::Validation { .. }
    ));
    assert!(matches!(
        PhaetonError::timeout("x"),
        PhaetonError::Timeout { .. }
    ));
    assert!(matches!(
        PhaetonError::update("x"),
        PhaetonError::Update { .. }
    ));
    assert!(matches!(
        PhaetonError::generic("x"),
        PhaetonError::Generic { .. }
    ));
}

#[test]
fn display_messages() {
    let e = PhaetonError::validation("field", "bad");
    let s = format!("{}", e);
    assert!(s.contains("Validation error"));
}
