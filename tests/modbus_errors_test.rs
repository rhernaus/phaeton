use phaeton::modbus::{decode_32bit_float, decode_64bit_float, decode_string, encode_32bit_float};

#[test]
fn decode_32bit_float_insufficient_registers() {
    let regs = [0x3F80u16];
    assert!(decode_32bit_float(&regs).is_err());
}

#[test]
fn decode_64bit_float_insufficient_registers() {
    let regs = [0x3FF0u16, 0x0000u16, 0x0000u16];
    assert!(decode_64bit_float(&regs).is_err());
}

#[test]
fn decode_string_truncates_and_trims() {
    // Registers for "AB C" with trailing spaces and nulls
    let regs = [0x0041u16, 0x0042u16, 0x0020u16, 0x0043u16, 0x0000u16];
    let s_full = decode_string(&regs, None).unwrap();
    assert_eq!(s_full, "AB C");

    let s_trunc = decode_string(&regs, Some(2)).unwrap();
    assert_eq!(s_trunc, "AB");
}

#[test]
fn decode_string_invalid_utf8_errors() {
    // 0xFF and 0xFE are invalid in UTF-8
    let regs = [0x00FFu16, 0x00FEu16];
    assert!(decode_string(&regs, None).is_err());
}

#[test]
fn encode_32bit_float_values() {
    // 1.0f32 -> 0x3F800000 => [0x3F80, 0x0000]
    assert_eq!(encode_32bit_float(1.0), [0x3F80, 0x0000]);
    // 2.5f32 -> 0x40200000 => [0x4020, 0x0000]
    assert_eq!(encode_32bit_float(2.5), [0x4020, 0x0000]);
}
