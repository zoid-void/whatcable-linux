//! Structured JSON formatter. Mirrors WhatCableCore/JSONFormatter.swift.

use serde::Serialize;

use crate::pdvdo::CableType;
use crate::snapshot::{CableSnapshot, PdEndpoint, PowerSource, UsbcPort};
use crate::summary::{diagnose, summarize};
use crate::vendor;

#[derive(Serialize)]
struct Output<'a> {
    version: &'a str,
    ports: Vec<PortDto<'a>>,
    usb_devices: Vec<UsbDeviceDto<'a>>,
    thunderbolt_devices: Vec<TbDeviceDto<'a>>,
}

#[derive(Serialize)]
struct PortDto<'a> {
    name: &'a str,
    connected: bool,
    data_role: Option<&'a str>,
    power_role: Option<&'a str>,
    power_operation_mode: Option<&'a str>,
    typec_revision: Option<&'a str>,
    pd_revision: Option<&'a str>,
    status: &'a str,
    headline: String,
    subtitle: String,
    bullets: Vec<String>,
    alt_modes: Vec<AltModeDto>,
    power_sources: Vec<PowerSourceDto>,
    cable: Option<CableDto>,
    device: Option<DeviceDto>,
    charging: Option<ChargingDto>,
    raw: Option<Vec<(String, String)>>,
}

#[derive(Serialize)]
struct AltModeDto {
    svid_hex: String,
    svid: u16,
    vdo_hex: String,
    active: bool,
    description: Option<String>,
    name: &'static str,
}

#[derive(Serialize)]
struct PowerSourceDto {
    name: String,
    max_power_w: f64,
    options: Vec<PdoDto>,
    negotiated: Option<NegotiatedDto>,
    winning: Option<PdoDto>,
    usb_type: Option<String>,
}

#[derive(Serialize)]
struct PdoDto {
    kind: &'static str,
    voltage_v: f64,
    current_a: f64,
    power_w: f64,
}

#[derive(Serialize)]
struct NegotiatedDto {
    voltage_v: f64,
    current_a: f64,
    power_w: f64,
}

#[derive(Serialize)]
struct CableDto {
    endpoint: &'static str,
    vendor_id: u16,
    vendor_id_hex: String,
    vendor_name: Option<&'static str>,
    speed: Option<&'static str>,
    current_rating: Option<&'static str>,
    max_volts: Option<u32>,
    max_watts: Option<u32>,
    cable_type: Option<&'static str>,
}

#[derive(Serialize)]
struct DeviceDto {
    kind: Option<&'static str>,
    vendor_id: u16,
    vendor_id_hex: String,
    vendor_name: Option<&'static str>,
    product_id: u16,
}

#[derive(Serialize)]
struct ChargingDto {
    summary: String,
    detail: String,
    bottleneck: &'static str,
    is_warning: bool,
}

#[derive(Serialize)]
struct UsbDeviceDto<'a> {
    bus: u32,
    devnum: u32,
    path: &'a str,
    speed_mbps: Option<u32>,
    speed_label: String,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    manufacturer: Option<&'a str>,
    product: Option<&'a str>,
}

#[derive(Serialize)]
struct TbDeviceDto<'a> {
    path: &'a str,
    vendor_name: Option<&'a str>,
    device_name: Option<&'a str>,
    authorized: Option<bool>,
}

pub fn render(snapshot: &CableSnapshot, show_raw: bool) -> Result<String, serde_json::Error> {
    let ports: Vec<PortDto> = snapshot.ports.iter().map(|p| port_dto(snapshot, p, show_raw)).collect();
    let usb_devices = snapshot.usb_devices.iter().map(|d| UsbDeviceDto {
        bus: d.bus,
        devnum: d.devnum,
        path: &d.path,
        speed_mbps: d.speed_mbps,
        speed_label: d.speed_label(),
        vendor_id: d.vendor_id,
        product_id: d.product_id,
        manufacturer: d.manufacturer.as_deref(),
        product: d.product.as_deref(),
    }).collect();
    let thunderbolt_devices = snapshot.thunderbolt_devices.iter().map(|d| TbDeviceDto {
        path: &d.path,
        vendor_name: d.vendor_name.as_deref(),
        device_name: d.device_name.as_deref(),
        authorized: d.authorized,
    }).collect();

    let out = Output {
        version: env!("CARGO_PKG_VERSION"),
        ports,
        usb_devices,
        thunderbolt_devices,
    };
    serde_json::to_string_pretty(&out)
}

fn port_dto<'a>(snapshot: &'a CableSnapshot, port: &'a UsbcPort, show_raw: bool) -> PortDto<'a> {
    let sources = snapshot.sources_for(port);
    let identities = snapshot.identities_for(port);
    let summary = summarize(port, &sources, &identities);

    let alt_modes = port.alt_modes.iter().map(|m| AltModeDto {
        svid_hex: format!("0x{:04x}", m.svid),
        svid: m.svid,
        vdo_hex: format!("0x{:08x}", m.vdo),
        active: m.active,
        description: m.description.clone(),
        name: alt_mode_name(m.svid),
    }).collect();

    let power_sources = sources.iter().map(|s| power_source_dto(s)).collect();

    let cable_id = identities.iter().find(|i| i.endpoint == PdEndpoint::SopPrime).copied();
    let cable = cable_id.map(|c| CableDto {
        endpoint: c.endpoint.as_str(),
        vendor_id: c.vendor_id,
        vendor_id_hex: format!("0x{:04x}", c.vendor_id),
        vendor_name: vendor::name_for(c.vendor_id),
        speed: c.cable_vdo.map(|cv| cv.speed.label()),
        current_rating: c.cable_vdo.map(|cv| cv.current.label()),
        max_volts: c.cable_vdo.map(|cv| cv.max_volts),
        max_watts: c.cable_vdo.map(|cv| cv.max_watts),
        cable_type: c.cable_vdo.map(|cv| if cv.cable_type == CableType::Active { "active" } else { "passive" }),
    });

    let partner_id = identities.iter().find(|i| i.endpoint == PdEndpoint::Sop).copied();
    let device = partner_id.map(|p| DeviceDto {
        kind: p.id_header.map(|h| {
            if matches!(h.ufp_product_type, crate::pdvdo::ProductType::Undefined) {
                h.dfp_product_type.label()
            } else {
                h.ufp_product_type.label()
            }
        }),
        vendor_id: p.vendor_id,
        vendor_id_hex: format!("0x{:04x}", p.vendor_id),
        vendor_name: vendor::name_for(p.vendor_id),
        product_id: p.product_id,
    });

    let charging = diagnose(port, &sources, &identities).map(|d| ChargingDto {
        summary: d.summary,
        detail: d.detail,
        bottleneck: d.bottleneck.label(),
        is_warning: d.is_warning,
    });

    PortDto {
        name: &port.name,
        connected: port.connected(),
        data_role: port.data_role.as_deref(),
        power_role: port.power_role.as_deref(),
        power_operation_mode: port.power_operation_mode.as_deref(),
        typec_revision: port.typec_revision.as_deref(),
        pd_revision: port.pd_revision.as_deref(),
        status: summary.status.as_str(),
        headline: summary.headline,
        subtitle: summary.subtitle,
        bullets: summary.bullets,
        alt_modes,
        power_sources,
        cable,
        device,
        charging,
        raw: if show_raw { Some(port.raw_attrs.clone()) } else { None },
    }
}

fn power_source_dto(src: &PowerSource) -> PowerSourceDto {
    PowerSourceDto {
        name: src.name.clone(),
        max_power_w: src.max_power_mw as f64 / 1000.0,
        options: src.options.iter().map(|o| PdoDto {
            kind: o.kind.label(),
            voltage_v: o.voltage_mv as f64 / 1000.0,
            current_a: o.max_current_ma as f64 / 1000.0,
            power_w: o.max_power_mw as f64 / 1000.0,
        }).collect(),
        negotiated: match (src.negotiated_mv, src.negotiated_ma) {
            (Some(mv), Some(ma)) => Some(NegotiatedDto {
                voltage_v: mv as f64 / 1000.0,
                current_a: ma as f64 / 1000.0,
                power_w: (mv as f64 * ma as f64) / 1_000_000.0,
            }),
            _ => None,
        },
        winning: src.winning.as_ref().map(|o| PdoDto {
            kind: o.kind.label(),
            voltage_v: o.voltage_mv as f64 / 1000.0,
            current_a: o.max_current_ma as f64 / 1000.0,
            power_w: o.max_power_mw as f64 / 1000.0,
        }),
        usb_type: src.usb_type.clone(),
    }
}

fn alt_mode_name(svid: u16) -> &'static str {
    match svid {
        0xff01 => "DisplayPort",
        0x8087 => "Thunderbolt",
        0x413c => "Dell",
        _ => "Vendor",
    }
}
