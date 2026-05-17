//! whatcable-linux — Linux CLI that reports what each USB-C port can do.
//!
//! Mirrors the macOS WhatCable CLI in spirit and flag set.

mod backend;
mod json;
mod pdvdo;
mod snapshot;
mod summary;
mod sysfs;
mod text;
mod vendor;

use std::collections::HashSet;
use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

const HELP: &str = "\
whatcable-linux — what can this USB-C cable / port actually do? (Linux)

Usage: whatcable-linux [options]

Options:
  --watch           Continuously monitor for changes (Ctrl+C to exit)
  --json            Output as JSON instead of human-readable text
  --raw             Include raw sysfs attributes for each port
  --interval SECS   Polling interval for --watch (default: 1)
  --version         Print version and exit
  -h, --help        Show this help and exit

Reads /sys/class/typec, /sys/class/usb_power_delivery,
/sys/class/power_supply/ucsi-source-psy-*, /sys/bus/usb/devices,
and /sys/bus/thunderbolt/devices.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{}", HELP);
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let known: HashSet<&str> = [
        "--watch", "--json", "--raw", "--interval", "--version", "--help", "-h",
    ].into_iter().collect();

    let mut as_json = false;
    let mut show_raw = false;
    let mut watch = false;
    let mut interval_secs: u64 = 1;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--watch" => watch = true,
            "--json" => as_json = true,
            "--raw" => show_raw = true,
            "--interval" => {
                i += 1;
                let v = args.get(i).cloned().unwrap_or_default();
                interval_secs = v.parse().unwrap_or_else(|_| {
                    eprintln!("whatcable-linux: bad --interval value: {}", v);
                    1
                });
            }
            other if other.starts_with("--") || other.starts_with('-') => {
                if !known.contains(other) {
                    eprintln!("whatcable-linux: unknown option {}", other);
                    eprint!("{}", HELP);
                    return ExitCode::from(2);
                }
            }
            _ => {
                eprintln!("whatcable-linux: unexpected argument {}", a);
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    if watch {
        return run_watch(as_json, show_raw, interval_secs);
    }

    let snapshot = backend::build_snapshot();
    if as_json {
        match json::render(&snapshot, show_raw) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("whatcable-linux: json encoding failed: {}", e);
                return ExitCode::FAILURE;
            }
        }
    } else {
        print!("{}", text::render(&snapshot, show_raw));
    }
    ExitCode::SUCCESS
}

fn run_watch(as_json: bool, show_raw: bool, interval_secs: u64) -> ExitCode {
    let mut last_output: Option<String> = None;
    let stdout = std::io::stdout();
    loop {
        let snapshot = backend::build_snapshot();
        let output = if as_json {
            match json::render(&snapshot, show_raw) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("whatcable-linux: json encoding failed: {}", e);
                    std::thread::sleep(Duration::from_secs(interval_secs));
                    continue;
                }
            }
        } else {
            text::render(&snapshot, show_raw)
        };

        if last_output.as_deref() != Some(output.as_str()) {
            let mut handle = stdout.lock();
            if as_json {
                let _ = writeln!(handle, "{}", output);
            } else {
                // Clear screen + home cursor, then redraw with timestamp.
                let _ = write!(handle, "\x1b[2J\x1b[H");
                let _ = writeln!(handle, "whatcable-linux --watch · {}\n", current_timestamp());
                let _ = write!(handle, "{}", output);
            }
            let _ = handle.flush();
            last_output = Some(output);
        }
        std::thread::sleep(Duration::from_secs(interval_secs));
    }
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Rough UTC formatter; avoids pulling in chrono just for a header.
    let days = secs / 86_400;
    let day_secs = secs % 86_400;
    let h = day_secs / 3600;
    let m = (day_secs / 60) % 60;
    let s = day_secs % 60;
    let (year, month, day) = days_to_ymd(days as i64);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", year, month, day, h, m, s)
}

fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719_468; // shift epoch to 0000-03-01
    let era = if days >= 0 { days / 146_097 } else { (days - 146_096) / 146_097 };
    let doe = (days - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = (y + (if m <= 2 { 1 } else { 0 })) as i32;
    (year, m, d)
}
