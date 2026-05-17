//! Cross-cutting data model for one snapshot of the system's USB-C state.
//! Mirrors WhatCableCore's CableSnapshot/USBCPort/PowerSource/PDIdentity.

#![allow(dead_code)] // fields are surfaced through the JSON formatter / summary

use crate::pdvdo::{CableVdo, IdHeader, ProductVdo};

#[derive(Debug, Clone)]
pub struct PowerOption {
    pub voltage_mv: u32,
    pub max_current_ma: u32,
    pub max_power_mw: u32,
    pub kind: PowerOptionKind,
}

impl PowerOption {
    pub fn fixed(voltage_mv: u32, max_current_ma: u32) -> Self {
        let p = (voltage_mv as u64 * max_current_ma as u64 / 1000) as u32;
        PowerOption { voltage_mv, max_current_ma, max_power_mw: p, kind: PowerOptionKind::Fixed }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerOptionKind {
    Fixed,
    Variable,
    Battery,
    Pps,
}

impl PowerOptionKind {
    pub fn label(self) -> &'static str {
        match self {
            PowerOptionKind::Fixed => "fixed",
            PowerOptionKind::Variable => "variable",
            PowerOptionKind::Battery => "battery",
            PowerOptionKind::Pps => "PPS",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PowerSource {
    pub port_key: String,         // matches USBCPort.port_key
    pub name: String,             // e.g. "pd2"
    pub max_power_mw: u32,
    pub options: Vec<PowerOption>,
    pub winning: Option<PowerOption>,   // best-effort match against ucsi voltage_now/current_now
    pub negotiated_mv: Option<u32>,     // live-negotiated from ucsi-source-psy
    pub negotiated_ma: Option<u32>,
    pub usb_type: Option<String>,       // e.g. "C [PD] PD_PPS"
}

impl PowerSource {
    pub fn negotiated_mw(&self) -> Option<u32> {
        match (self.negotiated_mv, self.negotiated_ma) {
            (Some(v), Some(a)) => Some((v as u64 * a as u64 / 1000) as u32),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdEndpoint {
    Sop,        // partner
    SopPrime,   // near-end cable e-marker
}

impl PdEndpoint {
    pub fn as_str(self) -> &'static str {
        match self {
            PdEndpoint::Sop => "SOP",
            PdEndpoint::SopPrime => "SOP'",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PdIdentity {
    pub port_key: String,
    pub endpoint: PdEndpoint,
    pub vendor_id: u16,
    pub product_id: u16,
    pub id_header: Option<IdHeader>,
    pub product_vdo: Option<ProductVdo>,
    pub cable_vdo: Option<CableVdo>,    // only for SOP'
    pub vdos_hex: Vec<String>,          // raw hex strings, for --raw
}

#[derive(Debug, Clone)]
pub struct AltMode {
    pub svid: u16,
    pub vdo: u32,
    pub active: bool,
    pub description: Option<String>,
}

impl AltMode {
    pub fn is_displayport(&self) -> bool { self.svid == 0xff01 }
    pub fn is_thunderbolt(&self) -> bool { self.svid == 0x8087 }
}

#[derive(Debug, Clone)]
pub struct UsbcPort {
    /// Stable key like "port0", used to join PowerSources / PdIdentities.
    pub port_key: String,
    pub name: String,                       // display name
    pub data_role: Option<String>,          // host/device
    pub power_role: Option<String>,         // source/sink
    pub power_operation_mode: Option<String>, // usb_power_delivery / default / etc.
    pub typec_revision: Option<String>,
    pub pd_revision: Option<String>,
    pub partner_present: bool,
    pub partner_supports_pd: Option<bool>,
    pub partner_pd_revision: Option<String>,
    pub cable_present: bool,
    pub cable_type: Option<String>,         // active/passive
    pub alt_modes: Vec<AltMode>,            // local-port-side altmodes from sysfs (active hints)
    pub raw_attrs: Vec<(String, String)>,   // for --raw
}

impl UsbcPort {
    /// Best-effort: did we see anything plugged in?
    pub fn connected(&self) -> bool {
        self.partner_present || self.cable_present
    }

    pub fn has_active_dp(&self) -> bool {
        self.alt_modes.iter().any(|m| m.is_displayport() && m.active)
    }

    pub fn has_active_tbt(&self) -> bool {
        self.alt_modes.iter().any(|m| m.is_thunderbolt() && m.active)
    }
}

#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub bus: u32,
    pub devnum: u32,
    pub path: String,           // e.g. "3-1.2"
    pub speed_mbps: Option<u32>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

impl UsbDevice {
    pub fn speed_label(&self) -> String {
        match self.speed_mbps {
            Some(20000) => "USB4 (20 Gbps)".into(),
            Some(10000) => "SuperSpeed+ (10 Gbps)".into(),
            Some(5000) => "SuperSpeed (5 Gbps)".into(),
            Some(480) => "High-Speed (480 Mbps)".into(),
            Some(12) => "Full-Speed (12 Mbps)".into(),
            Some(s) if s >= 1000 => format!("{} Gbps", s / 1000),
            Some(s) => format!("{} Mbps", s),
            None => "unknown speed".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThunderboltDevice {
    pub path: String,           // e.g. "0-1"
    pub vendor_name: Option<String>,
    pub device_name: Option<String>,
    pub authorized: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct CableSnapshot {
    pub ports: Vec<UsbcPort>,
    pub power_sources: Vec<PowerSource>,
    pub identities: Vec<PdIdentity>,
    pub usb_devices: Vec<UsbDevice>,
    pub thunderbolt_devices: Vec<ThunderboltDevice>,
}

impl CableSnapshot {
    pub fn sources_for<'a>(&'a self, port: &UsbcPort) -> Vec<&'a PowerSource> {
        self.power_sources.iter().filter(|s| s.port_key == port.port_key).collect()
    }

    pub fn identities_for<'a>(&'a self, port: &UsbcPort) -> Vec<&'a PdIdentity> {
        self.identities.iter().filter(|i| i.port_key == port.port_key).collect()
    }
}
