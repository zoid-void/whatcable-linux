//! Linux sysfs backend. Builds a `CableSnapshot` from `/sys/class/typec`,
//! `/sys/class/usb_power_delivery`, `/sys/class/power_supply`,
//! `/sys/bus/usb/devices`, and `/sys/bus/thunderbolt/devices`.

use std::path::{Path, PathBuf};

use crate::pdvdo;
use crate::snapshot::*;
use crate::sysfs;

const TYPEC_ROOT: &str = "/sys/class/typec";
const POWER_SUPPLY_ROOT: &str = "/sys/class/power_supply";
const USB_DEVICES_ROOT: &str = "/sys/bus/usb/devices";
const TB_DEVICES_ROOT: &str = "/sys/bus/thunderbolt/devices";

pub fn build_snapshot() -> CableSnapshot {
    let ports = read_ports();
    let identities = read_identities(&ports);
    let power_sources = read_power_sources(&ports);
    let usb_devices = read_usb_devices();
    let thunderbolt_devices = read_thunderbolt_devices();

    CableSnapshot {
        ports,
        power_sources,
        identities,
        usb_devices,
        thunderbolt_devices,
    }
}

fn read_ports() -> Vec<UsbcPort> {
    let mut ports = Vec::new();
    for name in sysfs::list_dir(TYPEC_ROOT) {
        // Only top-level "portN" entries. Ignore "portN-partner", "portN-cable", "portN-plugX".
        if !name.starts_with("port") || name.contains('-') {
            continue;
        }
        let port_path = PathBuf::from(TYPEC_ROOT).join(&name);
        ports.push(read_one_port(&name, &port_path));
    }
    ports.sort_by(|a, b| a.port_key.cmp(&b.port_key));
    ports
}

fn read_one_port(key: &str, path: &Path) -> UsbcPort {
    let partner_path = path.with_file_name(format!("{}-partner", key));
    let cable_path = path.with_file_name(format!("{}-cable", key));

    let partner_present = partner_path.exists();
    let cable_present = cable_path.exists();

    let alt_modes = read_altmodes(path);

    let mut raw: Vec<(String, String)> = Vec::new();
    for f in [
        "data_role", "power_role", "port_type", "vconn_source",
        "power_operation_mode", "preferred_role", "orientation",
        "usb_power_delivery_revision", "usb_typec_revision",
        "supported_accessory_modes", "usb_capability",
    ] {
        if let Some(v) = sysfs::read_trim(path.join(f)) {
            raw.push((f.into(), v));
        }
    }
    if partner_present {
        for f in [
            "supports_usb_power_delivery", "accessory_mode", "usb_power_delivery_revision",
            "number_of_alternate_modes", "type", "usb_mode",
        ] {
            if let Some(v) = sysfs::read_trim(partner_path.join(f)) {
                raw.push((format!("partner/{}", f), v));
            }
        }
    }
    if cable_present {
        for f in ["type", "plug_type", "usb_power_delivery_revision"] {
            if let Some(v) = sysfs::read_trim(cable_path.join(f)) {
                raw.push((format!("cable/{}", f), v));
            }
        }
    }

    UsbcPort {
        port_key: key.to_string(),
        name: key.to_string(),
        data_role: sysfs::read_bracketed(path.join("data_role")),
        power_role: sysfs::read_bracketed(path.join("power_role")),
        power_operation_mode: sysfs::read_trim(path.join("power_operation_mode")),
        typec_revision: sysfs::read_trim(path.join("usb_typec_revision")),
        pd_revision: sysfs::read_trim(path.join("usb_power_delivery_revision")),
        partner_present,
        partner_supports_pd: sysfs::read_trim(partner_path.join("supports_usb_power_delivery"))
            .map(|s| s == "yes" || s == "1"),
        partner_pd_revision: sysfs::read_trim(partner_path.join("usb_power_delivery_revision")),
        cable_present,
        cable_type: sysfs::read_trim(cable_path.join("type")),
        alt_modes,
        raw_attrs: raw,
    }
}

fn read_altmodes(port_path: &Path) -> Vec<AltMode> {
    let mut modes = Vec::new();
    for entry in sysfs::list_dir(port_path) {
        if !entry.starts_with("port") || !entry.contains('.') {
            continue; // altmode subdirs look like "port0.0"
        }
        let alt_path = port_path.join(&entry);
        let svid = match sysfs::read_hex_u32(alt_path.join("svid")) {
            Some(v) => v as u16,
            None => continue,
        };
        // Find the active mode (mode1, mode2, …) — pick the first `active = yes`.
        let mut chosen_vdo: u32 = 0;
        let mut chosen_active = false;
        let mut chosen_desc: Option<String> = None;
        for sub in sysfs::list_dir(&alt_path) {
            if !sub.starts_with("mode") { continue; }
            let mode_path = alt_path.join(&sub);
            let active = sysfs::read_trim(mode_path.join("active"))
                .map(|s| s == "yes")
                .unwrap_or(false);
            let vdo = sysfs::read_hex_u32(mode_path.join("vdo")).unwrap_or(0);
            if active || !chosen_active {
                chosen_vdo = vdo;
                chosen_active = active;
                chosen_desc = sysfs::read_trim(mode_path.join("description"));
                if active { break; }
            }
        }
        modes.push(AltMode {
            svid,
            vdo: chosen_vdo,
            active: chosen_active,
            description: chosen_desc,
        });
    }
    modes
}

fn read_identities(ports: &[UsbcPort]) -> Vec<PdIdentity> {
    let mut out = Vec::new();
    for port in ports {
        let partner = PathBuf::from(TYPEC_ROOT).join(format!("{}-partner", port.port_key));
        if let Some(id) = read_identity(&partner.join("identity"), &port.port_key, PdEndpoint::Sop, false) {
            out.push(id);
        }
        let cable = PathBuf::from(TYPEC_ROOT).join(format!("{}-cable", port.port_key));
        let is_active = sysfs::read_trim(cable.join("type")).map(|s| s == "active").unwrap_or(false);
        if let Some(id) = read_identity(&cable.join("identity"), &port.port_key, PdEndpoint::SopPrime, is_active) {
            out.push(id);
        }
    }
    out
}

fn read_identity(dir: &Path, port_key: &str, endpoint: PdEndpoint, is_active_cable: bool) -> Option<PdIdentity> {
    if !dir.exists() { return None; }

    let id_header_raw = sysfs::read_trim(dir.join("id_header"));
    let cert_stat_raw = sysfs::read_trim(dir.join("cert_stat"));
    let product_raw = sysfs::read_trim(dir.join("product"));
    let pt1 = sysfs::read_trim(dir.join("product_type_vdo1"));
    let pt2 = sysfs::read_trim(dir.join("product_type_vdo2"));
    let pt3 = sysfs::read_trim(dir.join("product_type_vdo3"));

    let id_header = id_header_raw.as_deref().and_then(sysfs::parse_hex_u32).map(pdvdo::IdHeader::decode);
    let product_vdo = product_raw.as_deref().and_then(sysfs::parse_hex_u32).map(pdvdo::ProductVdo::decode);

    let cable_vdo = if endpoint == PdEndpoint::SopPrime {
        pt1.as_deref()
            .and_then(sysfs::parse_hex_u32)
            .map(|v| pdvdo::CableVdo::decode(v, is_active_cable))
    } else {
        None
    };

    // Nothing useful at all? Skip.
    if id_header.is_none() && product_vdo.is_none() && cable_vdo.is_none() {
        return None;
    }

    let mut vdos_hex: Vec<String> = Vec::new();
    for (label, opt) in [
        ("id_header", id_header_raw.as_deref()),
        ("cert_stat", cert_stat_raw.as_deref()),
        ("product", product_raw.as_deref()),
        ("product_type_vdo1", pt1.as_deref()),
        ("product_type_vdo2", pt2.as_deref()),
        ("product_type_vdo3", pt3.as_deref()),
    ] {
        if let Some(v) = opt { vdos_hex.push(format!("{}={}", label, v)); }
    }

    Some(PdIdentity {
        port_key: port_key.to_string(),
        endpoint,
        vendor_id: id_header.map(|h| h.vendor_id).unwrap_or(0),
        product_id: product_vdo.map(|p| p.product_id).unwrap_or(0),
        id_header,
        product_vdo,
        cable_vdo,
        vdos_hex,
    })
}

fn read_power_sources(ports: &[UsbcPort]) -> Vec<PowerSource> {
    let mut out = Vec::new();
    for port in ports {
        // Find the partner's pd link, e.g. /sys/class/typec/port0-partner/usb_power_delivery
        let partner = PathBuf::from(TYPEC_ROOT).join(format!("{}-partner", port.port_key));
        let pd_link = partner.join("usb_power_delivery");
        if !pd_link.exists() { continue; }

        let real = match std::fs::canonicalize(&pd_link) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let pd_name = real.file_name().and_then(|s| s.to_str()).unwrap_or("pd").to_string();

        let mut options: Vec<PowerOption> = Vec::new();
        let src_caps = real.join("source-capabilities");
        if src_caps.exists() {
            let mut entries = sysfs::list_dir(&src_caps);
            entries.sort_by_key(|s| {
                s.split(':').next().unwrap_or("0").parse::<u32>().unwrap_or(0)
            });
            for entry in entries {
                if !entry.contains(':') { continue; }
                let kind_str = entry.splitn(2, ':').nth(1).unwrap_or("");
                let dir = src_caps.join(&entry);
                let kind = match kind_str {
                    "fixed_supply" => PowerOptionKind::Fixed,
                    "variable_supply" => PowerOptionKind::Variable,
                    "battery" => PowerOptionKind::Battery,
                    "programmable_supply" => PowerOptionKind::Pps,
                    _ => continue,
                };
                if let Some(opt) = parse_pdo(&dir, kind) {
                    options.push(opt);
                }
            }
        }

        let max_power_mw = options.iter().map(|o| o.max_power_mw).max().unwrap_or(0);

        // Live negotiation comes from /sys/class/power_supply/ucsi-source-psy-...
        let (negotiated_mv, negotiated_ma, usb_type, online) =
            read_ucsi_for_port(&port.port_key);

        // Best-effort "winning PDO": closest fixed option to the negotiated voltage.
        let winning = match (negotiated_mv, online) {
            (Some(mv), Some(true)) => options.iter()
                .filter(|o| matches!(o.kind, PowerOptionKind::Fixed))
                .min_by_key(|o| (o.voltage_mv as i64 - mv as i64).abs())
                .cloned(),
            _ => None,
        };

        out.push(PowerSource {
            port_key: port.port_key.clone(),
            name: pd_name,
            max_power_mw,
            options,
            winning,
            negotiated_mv: negotiated_mv.filter(|_| online == Some(true)),
            negotiated_ma: negotiated_ma.filter(|_| online == Some(true)),
            usb_type,
        });
    }
    out
}

fn parse_pdo(dir: &Path, kind: PowerOptionKind) -> Option<PowerOption> {
    let voltage = sysfs::read_mu_int(dir.join("voltage"))
        .or_else(|| sysfs::read_mu_int(dir.join("maximum_voltage")));
    let current = sysfs::read_mu_int(dir.join("maximum_current"))
        .or_else(|| sysfs::read_mu_int(dir.join("operational_current")));
    let voltage_mv = voltage? as u32;
    let current_ma = current.unwrap_or(0).max(0) as u32;
    let mut o = PowerOption::fixed(voltage_mv, current_ma);
    o.kind = kind;
    if !matches!(kind, PowerOptionKind::Fixed) {
        if let Some(p) = sysfs::read_mu_int(dir.join("maximum_power")).map(|v| v as u32) {
            o.max_power_mw = p;
        }
    }
    Some(o)
}

/// Map a `portN` key onto a `ucsi-source-psy-USBC000:NNN` directory.
/// UCSI connector indexes start at 1, so port0 -> :001, port1 -> :002, …
fn read_ucsi_for_port(port_key: &str) -> (Option<u32>, Option<u32>, Option<String>, Option<bool>) {
    let port_idx: u32 = port_key.trim_start_matches("port").parse().unwrap_or(0);
    let target_suffix = format!(":{:03}", port_idx + 1);

    for entry in sysfs::list_dir(POWER_SUPPLY_ROOT) {
        if !entry.starts_with("ucsi-source-psy") { continue; }
        if !entry.ends_with(&target_suffix) { continue; }
        let p = PathBuf::from(POWER_SUPPLY_ROOT).join(&entry);
        let online = sysfs::read_trim(p.join("online")).map(|s| s == "1");
        // sysfs reports voltage/current in microvolts/microamps for power_supply class.
        let mv = sysfs::read_trim(p.join("voltage_now"))
            .and_then(|s| s.parse::<u64>().ok())
            .map(|uv| (uv / 1000) as u32);
        let ma = sysfs::read_trim(p.join("current_now"))
            .and_then(|s| s.parse::<u64>().ok())
            .map(|ua| (ua / 1000) as u32);
        let usb_type = sysfs::read_trim(p.join("usb_type"));
        return (mv, ma, usb_type, online);
    }
    (None, None, None, None)
}

fn read_usb_devices() -> Vec<UsbDevice> {
    let mut out = Vec::new();
    let root = Path::new(USB_DEVICES_ROOT);
    if !root.exists() { return out; }
    for entry in sysfs::list_dir(root) {
        // Skip interface directories (they contain ':') and platform-specific entries.
        if entry.contains(':') { continue; }
        let dir = root.join(&entry);
        // A USB device dir has busnum / devnum / idVendor.
        let busnum = match sysfs::read_trim(dir.join("busnum")).and_then(|s| s.parse().ok()) {
            Some(b) => b,
            None => continue,
        };
        let devnum = sysfs::read_trim(dir.join("devnum"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let speed_mbps = sysfs::read_trim(dir.join("speed")).and_then(|s| s.parse().ok());
        let id_vendor = sysfs::read_hex_u32(dir.join("idVendor")).map(|v| v as u16);
        let id_product = sysfs::read_hex_u32(dir.join("idProduct")).map(|v| v as u16);
        let manufacturer = sysfs::read_trim(dir.join("manufacturer"));
        let product = sysfs::read_trim(dir.join("product"));

        out.push(UsbDevice {
            bus: busnum,
            devnum,
            path: entry,
            speed_mbps,
            vendor_id: id_vendor,
            product_id: id_product,
            manufacturer,
            product,
        });
    }
    out.sort_by(|a, b| (a.bus, a.devnum).cmp(&(b.bus, b.devnum)));
    out
}

fn read_thunderbolt_devices() -> Vec<ThunderboltDevice> {
    let mut out = Vec::new();
    let root = Path::new(TB_DEVICES_ROOT);
    if !root.exists() { return out; }
    for entry in sysfs::list_dir(root) {
        if entry.starts_with("domain") { continue; }
        let dir = root.join(&entry);
        out.push(ThunderboltDevice {
            path: entry,
            vendor_name: sysfs::read_trim(dir.join("vendor_name")),
            device_name: sysfs::read_trim(dir.join("device_name")),
            authorized: sysfs::read_trim(dir.join("authorized")).map(|s| s == "1"),
        });
    }
    out
}
