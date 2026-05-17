//! Tiny sysfs read helpers. All reads are best-effort: missing files return
//! `None` rather than erroring, because per-laptop variability is huge.

use std::fs;
use std::path::Path;

pub fn read_trim<P: AsRef<Path>>(path: P) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Read a value like `20000mV` / `3250mA` / `5000` and strip a trailing unit.
pub fn read_mu_int<P: AsRef<Path>>(path: P) -> Option<i64> {
    let s = read_trim(path)?;
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
    digits.parse().ok()
}

/// Parse `0x00000001` or plain `1` as u32.
pub fn parse_hex_u32(s: &str) -> Option<u32> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u32::from_str_radix(s, 16).ok()
}

pub fn read_hex_u32<P: AsRef<Path>>(path: P) -> Option<u32> {
    parse_hex_u32(&read_trim(path)?)
}

/// Read a `[a] b c`-style sysfs enum and return the bracketed selection.
pub fn read_bracketed<P: AsRef<Path>>(path: P) -> Option<String> {
    let s = read_trim(path)?;
    if let (Some(start), Some(end)) = (s.find('['), s.find(']')) {
        if start < end {
            return Some(s[start + 1..end].to_string());
        }
    }
    Some(s)
}

pub fn list_dir<P: AsRef<Path>>(path: P) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    out
}


