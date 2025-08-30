use crate::error::{PhaetonError, Result};
use tracing::Level;

pub fn parse_log_level(level_str: &str) -> Result<Level> {
    match level_str.to_uppercase().as_str() {
        "TRACE" => Ok(Level::TRACE),
        "DEBUG" => Ok(Level::DEBUG),
        "INFO" => Ok(Level::INFO),
        "WARN" => Ok(Level::WARN),
        "ERROR" => Ok(Level::ERROR),
        _ => Err(PhaetonError::config(format!(
            "Invalid log level: {}",
            level_str
        ))),
    }
}

pub fn level_rank(level: Level) -> u8 {
    match level {
        Level::TRACE => 0,
        Level::DEBUG => 1,
        Level::INFO => 2,
        Level::WARN => 3,
        Level::ERROR => 4,
    }
}

pub fn min_level(a: Level, b: Level) -> Level {
    if level_rank(a) <= level_rank(b) { a } else { b }
}

pub fn parse_line_level(line: &str) -> Option<Level> {
    let line = strip_ansi_codes(line);
    if line.contains("\"level\":\"TRACE\"") {
        return Some(Level::TRACE);
    }
    if line.contains("\"level\":\"DEBUG\"") {
        return Some(Level::DEBUG);
    }
    if line.contains("\"level\":\"INFO\"") {
        return Some(Level::INFO);
    }
    if line.contains("\"level\":\"WARN\"") {
        return Some(Level::WARN);
    }
    if line.contains("\"level\":\"ERROR\"") {
        return Some(Level::ERROR);
    }
    if line.contains(" TRACE ") {
        return Some(Level::TRACE);
    }
    if line.contains(" DEBUG ") {
        return Some(Level::DEBUG);
    }
    if line.contains(" INFO ") {
        return Some(Level::INFO);
    }
    if line.contains(" WARN ") {
        return Some(Level::WARN);
    }
    if line.contains(" ERROR ") {
        return Some(Level::ERROR);
    }
    None
}

fn strip_ansi_codes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1B {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    if (b'@'..=b'~').contains(&c) {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            } else {
                continue;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

pub fn set_web_log_level_str(level_str: &str) -> Result<()> {
    let lvl = parse_log_level(level_str)?;
    super::state::set_web_log_level(lvl);
    Ok(())
}
