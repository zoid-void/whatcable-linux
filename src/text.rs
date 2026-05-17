//! Human-readable formatter. Mirrors WhatCableCore/TextFormatter.swift.

use std::fmt::Write;

use crate::snapshot::{CableSnapshot, ThunderboltDevice, UsbDevice};
use crate::summary::{diagnose, summarize, Status};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GRAY: &str = "\x1b[90m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";

fn wrap(style: &str, body: &str) -> String {
    if std::env::var("NO_COLOR").is_ok() { return body.to_string(); }
    format!("{}{}{}", style, body, RESET)
}

fn status_color(s: Status) -> &'static str {
    match s {
        Status::Empty => GRAY,
        Status::Charging => YELLOW,
        Status::DataDevice => BLUE,
        Status::ThunderboltCable => MAGENTA,
        Status::DisplayCable => CYAN,
        Status::Unknown => YELLOW,
    }
}

pub fn render(snapshot: &CableSnapshot, show_raw: bool) -> String {
    if snapshot.ports.is_empty() {
        return "No USB-C / Type-C ports were found on this system.\n\
                (No /sys/class/typec entries — kernel typec class may not be populated.)\n".into();
    }

    let mut out = String::new();
    for (i, port) in snapshot.ports.iter().enumerate() {
        if i > 0 { out.push('\n'); }
        let sources = snapshot.sources_for(port);
        let identities = snapshot.identities_for(port);
        let summary = summarize(port, &sources, &identities);

        let header = format!("=== {} ===", port.name);
        let _ = writeln!(out, "{}", wrap(&format!("{}{}", BOLD, CYAN), &header));
        let _ = writeln!(out, "{}", wrap(&format!("{}{}", BOLD, status_color(summary.status)), &summary.headline));
        let _ = writeln!(out, "{}", wrap(DIM, &summary.subtitle));

        if let (Some(d), Some(p)) = (port.data_role.as_deref(), port.power_role.as_deref()) {
            let _ = writeln!(out, "{}", wrap(GRAY, &format!("  role: {} / {}", d, p)));
        }

        if !summary.bullets.is_empty() {
            out.push('\n');
            for b in &summary.bullets {
                let _ = writeln!(out, "  {} {}", wrap(GRAY, "•"), b);
            }
        }

        if let Some(diag) = diagnose(port, &sources, &identities) {
            let color = if diag.is_warning { YELLOW } else { GREEN };
            out.push('\n');
            let _ = writeln!(out, "{}{}", wrap(BOLD, "Charging: "), wrap(color, &diag.summary));
            let _ = writeln!(out, "  {}", wrap(DIM, &diag.detail));
        }

        if show_raw {
            out.push('\n');
            let _ = writeln!(out, "{}", wrap(BOLD, "Raw sysfs attributes:"));
            for (k, v) in &port.raw_attrs {
                let _ = writeln!(out, "  {} = {}", wrap(GRAY, k), v);
            }
            for id in &identities {
                let _ = writeln!(out, "  {} {}",
                    wrap(GRAY, &format!("identity[{}]", id.endpoint.as_str())),
                    id.vdos_hex.join(" "));
            }
            for src in &sources {
                let _ = writeln!(out, "  {} max_power_mW={} options={}",
                    wrap(GRAY, &format!("power_source[{}]", src.name)),
                    src.max_power_mw, src.options.len());
            }
        }
    }

    if !snapshot.usb_devices.is_empty() {
        out.push('\n');
        let _ = writeln!(out, "{}", wrap(&format!("{}{}", BOLD, CYAN), "=== Attached USB devices ==="));
        for dev in &snapshot.usb_devices {
            out.push_str(&format_usb_device(dev));
        }
    }

    if !snapshot.thunderbolt_devices.is_empty() {
        out.push('\n');
        let _ = writeln!(out, "{}", wrap(&format!("{}{}", BOLD, CYAN), "=== Thunderbolt devices ==="));
        for dev in &snapshot.thunderbolt_devices {
            out.push_str(&format_tb_device(dev));
        }
    }

    out
}

fn format_usb_device(dev: &UsbDevice) -> String {
    let vid = dev.vendor_id.map(|v| format!("{:04x}", v)).unwrap_or_else(|| "----".into());
    let pid = dev.product_id.map(|v| format!("{:04x}", v)).unwrap_or_else(|| "----".into());
    let label = match (dev.manufacturer.as_deref(), dev.product.as_deref()) {
        (Some(m), Some(p)) => format!("{} {}", m, p),
        (None, Some(p)) => p.to_string(),
        (Some(m), None) => m.to_string(),
        _ => format!("{}:{}", vid, pid),
    };
    let speed = dev.speed_label();
    format!("  {} bus {} dev {} [{}] {} — {}\n",
        wrap(GRAY, "•"),
        dev.bus, dev.devnum, dev.path,
        wrap(DIM, &format!("{}:{} · {}", vid, pid, speed)),
        label)
}

fn format_tb_device(dev: &ThunderboltDevice) -> String {
    let label = match (dev.vendor_name.as_deref(), dev.device_name.as_deref()) {
        (Some(v), Some(d)) => format!("{} {}", v, d),
        (None, Some(d)) => d.to_string(),
        (Some(v), None) => v.to_string(),
        _ => dev.path.clone(),
    };
    let auth = match dev.authorized {
        Some(true) => wrap(GREEN, "authorized"),
        Some(false) => wrap(RED, "unauthorized"),
        None => wrap(GRAY, "—"),
    };
    format!("  {} {} [{}] {}\n", wrap(GRAY, "•"), label, dev.path, auth)
}
