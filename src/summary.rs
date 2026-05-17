//! Plain-English interpretation of a port snapshot. Mirrors
//! WhatCableCore/PortSummary.swift and ChargingDiagnostic.swift.

use crate::pdvdo::CableType;
use crate::snapshot::{PdEndpoint, PdIdentity, PowerSource, UsbcPort};
use crate::vendor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Empty,
    Charging,
    DataDevice,
    ThunderboltCable,
    DisplayCable,
    Unknown,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Empty => "empty",
            Status::Charging => "charging",
            Status::DataDevice => "dataDevice",
            Status::ThunderboltCable => "thunderboltCable",
            Status::DisplayCable => "displayCable",
            Status::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PortSummary {
    pub status: Status,
    pub headline: String,
    pub subtitle: String,
    pub bullets: Vec<String>,
}

pub fn summarize(port: &UsbcPort, sources: &[&PowerSource], identities: &[&PdIdentity]) -> PortSummary {
    let connected = port.connected();

    let has_tb = port.has_active_tbt();
    let has_dp = port.has_active_dp();
    let pd_active = port.power_operation_mode.as_deref() == Some("usb_power_delivery");

    let cable_emarker = identities.iter().find(|i| i.endpoint == PdEndpoint::SopPrime);
    let partner = identities.iter().find(|i| i.endpoint == PdEndpoint::Sop);

    let chosen_source = preferred_charging_source(sources);
    let charger_w: Option<u32> = chosen_source
        .filter(|s| !s.options.is_empty())
        .map(|s| (s.max_power_mw + 500) / 1000)
        .filter(|w| *w > 0);

    if !connected {
        return PortSummary {
            status: Status::Empty,
            headline: "Nothing connected".into(),
            subtitle: format!("Plug a cable into {} to see what it can do.", port.name),
            bullets: vec![],
        };
    }

    let mut bullets: Vec<String> = Vec::new();

    if has_tb {
        bullets.push("Thunderbolt / USB4 alt mode active".into());
    } else if has_dp {
        bullets.push("Carrying DisplayPort video".into());
    }

    if pd_active {
        bullets.push("USB Power Delivery negotiated".into());
    }

    if cable_emarker.is_some() {
        bullets.push("Cable has an e-marker chip (advertises its capabilities)".into());
    } else if port.cable_present {
        bullets.push("Cable e-marker present (no identity exposed by kernel)".into());
    } else if connected {
        bullets.push("Cable does not advertise an e-marker (basic cable)".into());
    }

    if let Some(src) = chosen_source {
        if !src.options.is_empty() {
            if let Some(w) = charger_w {
                bullets.push(format!("Charger advertises up to {}W", w));
            }
            // List PDOs concisely.
            let mut profile_bits: Vec<String> = Vec::new();
            for o in &src.options {
                let v = o.voltage_mv as f64 / 1000.0;
                let a = o.max_current_ma as f64 / 1000.0;
                profile_bits.push(format!("{:.0}V/{:.2}A", v, a));
            }
            bullets.push(format!("PDOs: {}", profile_bits.join(", ")));
        }
        if let (Some(mv), Some(ma)) = (src.negotiated_mv, src.negotiated_ma) {
            let v = mv as f64 / 1000.0;
            let a = ma as f64 / 1000.0;
            let w = v * a;
            bullets.push(format!("Currently negotiated: {:.0}V @ {:.2}A ({:.1}W)", v, a, w));
        }
        if let Some(t) = &src.usb_type {
            bullets.push(format!("Power supply usb_type: {}", t));
        }
    }

    if let Some(cable) = cable_emarker {
        if let Some(cv) = cable.cable_vdo {
            bullets.push(format!("Cable speed: {}", cv.speed.label()));
            bullets.push(format!(
                "Cable rated for {} at up to {}V (~{}W)",
                cv.current.label(), cv.max_volts, cv.max_watts
            ));
            if cv.cable_type == CableType::Active {
                bullets.push("Active cable (contains signal-conditioning electronics)".into());
            }
        }
        if cable.vendor_id != 0 {
            bullets.push(format!("Cable made by {}", vendor::label_for(cable.vendor_id)));
        }
    } else if let Some(t) = &port.cable_type {
        bullets.push(format!("Cable type: {}", t));
    }

    if let Some(p) = partner {
        if let Some(h) = p.id_header {
            let kind = if matches!(h.ufp_product_type, crate::pdvdo::ProductType::Undefined) {
                h.dfp_product_type.label()
            } else {
                h.ufp_product_type.label()
            };
            bullets.push(format!("Connected device: {} — {}", kind, vendor::label_for(p.vendor_id)));
        } else if p.vendor_id != 0 {
            bullets.push(format!("Connected device vendor: {}", vendor::label_for(p.vendor_id)));
        }
    }

    let charger_suffix = charger_w.map(|w| format!(" · {}W charger", w)).unwrap_or_default();

    let (status, headline, subtitle) = if has_tb {
        (
            Status::ThunderboltCable,
            format!("Thunderbolt / USB4{}", charger_suffix),
            subtitle_for(true, has_dp, cable_emarker.is_some()),
        )
    } else if has_dp {
        (
            Status::DisplayCable,
            format!("Display connected{}", charger_suffix),
            "DisplayPort video over USB-C alt mode.".into(),
        )
    } else if pd_active && partner.is_some() {
        (
            Status::DataDevice,
            format!("USB device{}", charger_suffix),
            "PD-capable partner connected.".into(),
        )
    } else if chosen_source.is_some() {
        (
            Status::Charging,
            format!("Charging{}", charger_suffix),
            "Power is flowing.".into(),
        )
    } else if port.cable_present || port.partner_present {
        (
            Status::Unknown,
            "Connected".into(),
            "Cable/partner attached, but kernel did not expose detailed capabilities.".into(),
        )
    } else {
        (
            Status::Unknown,
            "Connected".into(),
            "Couldn't determine cable type from this port.".into(),
        )
    };

    PortSummary { status, headline, subtitle, bullets }
}

fn subtitle_for(usb3: bool, dp: bool, emarker: bool) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if usb3 { parts.push("high-speed data"); }
    if dp { parts.push("video"); }
    if emarker { parts.push("smart cable"); }
    if parts.is_empty() { return "Connected.".into(); }
    format!("Supports {}.", parts.join(", "))
}

pub fn preferred_charging_source<'a>(sources: &'a [&PowerSource]) -> Option<&'a PowerSource> {
    sources.iter().copied().max_by_key(|s| s.max_power_mw)
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // fields are kept for callers / future structured output
pub enum Bottleneck {
    NoCharger,
    ChargerLimit { charger_w: u32 },
    CableLimit { cable_w: u32, charger_w: u32 },
    MacLimit { negotiated_w: u32, charger_w: u32, cable_w: Option<u32> },
    Fine { negotiated_w: u32 },
}

impl Bottleneck {
    pub fn label(self) -> &'static str {
        match self {
            Bottleneck::NoCharger => "noCharger",
            Bottleneck::ChargerLimit { .. } => "chargerLimit",
            Bottleneck::CableLimit { .. } => "cableLimit",
            Bottleneck::MacLimit { .. } => "macLimit",
            Bottleneck::Fine { .. } => "fine",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChargingDiagnostic {
    pub bottleneck: Bottleneck,
    pub summary: String,
    pub detail: String,
    pub is_warning: bool,
}

pub fn diagnose(port: &UsbcPort, sources: &[&PowerSource], identities: &[&PdIdentity]) -> Option<ChargingDiagnostic> {
    let source = preferred_charging_source(sources)?;
    if !port.connected() { return None; }
    if source.max_power_mw == 0 && source.negotiated_mw().is_none() { return None; }

    let charger_w = ((source.max_power_mw + 500) / 1000).max(
        source.negotiated_mw().map(|w| (w + 500) / 1000).unwrap_or(0)
    );
    let negotiated_w = source.negotiated_mw().map(|w| (w + 500) / 1000);
    let cable_w = identities.iter()
        .find(|i| i.endpoint == PdEndpoint::SopPrime)
        .and_then(|i| i.cable_vdo.map(|cv| cv.max_watts));

    if let Some(cw) = cable_w {
        if cw < charger_w {
            return Some(ChargingDiagnostic {
                bottleneck: Bottleneck::CableLimit { cable_w: cw, charger_w },
                summary: "Cable is limiting charging speed".into(),
                detail: format!(
                    "Charger can deliver up to {}W, but this cable is only rated for {}W. Replace the cable to charge faster.",
                    charger_w, cw
                ),
                is_warning: true,
            });
        }
    }

    if let Some(n) = negotiated_w {
        let slack = std::cmp::max(5, charger_w / 10);
        let cable_ok = cable_w.map(|cw| n + std::cmp::max(5, cw / 10) < cw).unwrap_or(true);
        if charger_w > 0 && n + slack < charger_w && cable_ok {
            return Some(ChargingDiagnostic {
                bottleneck: Bottleneck::MacLimit { negotiated_w: n, charger_w, cable_w },
                summary: format!("Charging at {}W (charger can do up to {}W)", n, charger_w),
                detail: "Both the charger and cable can do more, but the system is currently asking for less. Normal once the battery is mostly full or the system is idle.".into(),
                is_warning: true,
            });
        }
        return Some(ChargingDiagnostic {
            bottleneck: Bottleneck::Fine { negotiated_w: n },
            summary: format!("Charging well at {}W", n),
            detail: "Charger and cable are well-matched.".into(),
            is_warning: false,
        });
    }

    Some(ChargingDiagnostic {
        bottleneck: Bottleneck::ChargerLimit { charger_w },
        summary: format!("Charger advertises up to {}W", charger_w),
        detail: "Negotiation hasn't completed yet.".into(),
        is_warning: false,
    })
}
