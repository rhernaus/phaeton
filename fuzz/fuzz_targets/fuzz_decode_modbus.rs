#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Interpret the input as u16 register stream in big-endian pairs
    let mut regs = Vec::new();
    let mut it = data.chunks_exact(2);
    for b in &mut it {
        regs.push(u16::from_be_bytes([b[0], b[1]]));
    }

    // Exercise the decoders under varying lengths
    let _ = phaeton::modbus::decode_32bit_float(&regs);
    let _ = phaeton::modbus::decode_64bit_float(&regs);
    let _ = phaeton::modbus::decode_string(&regs, Some(32));
});