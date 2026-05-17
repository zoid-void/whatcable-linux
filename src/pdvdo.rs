//! USB-PD VDO bit decoders. Mirrors WhatCableCore/PDVDO.swift, adapted to
//! the kernel-decoded VDOs that Linux exposes as hex text in
//! `/sys/class/typec/portN-{partner,cable}/identity/`.
//!
//! Spec: USB Power Delivery 3.x.

#![allow(dead_code)] // some decoded fields are kept for diagnostics / future formatters

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductType {
    Undefined,
    Hub,
    Peripheral,
    PassiveCable,
    ActiveCable,
    Amc,
    Ama,
    VConnPoweredAccessory,
    AlternateMode,
    Power,
    Other(u8),
}

impl ProductType {
    pub fn label(self) -> &'static str {
        match self {
            ProductType::Undefined => "Unknown",
            ProductType::Hub => "Hub",
            ProductType::Peripheral => "Peripheral",
            ProductType::PassiveCable => "Passive cable",
            ProductType::ActiveCable => "Active cable",
            ProductType::Amc => "Alternate Mode Controller",
            ProductType::Ama => "Alternate Mode Adapter",
            ProductType::VConnPoweredAccessory => "VConn-powered accessory",
            ProductType::AlternateMode => "Alternate-mode device",
            ProductType::Power => "Power adapter",
            ProductType::Other(_) => "Device",
        }
    }
}

fn ufp_type(v: u8) -> ProductType {
    // PD 3.x ID Header UFP product type.
    match v {
        0 => ProductType::Undefined,
        1 => ProductType::Hub,
        2 => ProductType::Peripheral,
        3 => ProductType::Power,
        5 => ProductType::AlternateMode,
        6 => ProductType::VConnPoweredAccessory,
        x => ProductType::Other(x),
    }
}

fn dfp_type(v: u8) -> ProductType {
    match v {
        0 => ProductType::Undefined,
        1 => ProductType::Hub,
        2 => ProductType::Peripheral,
        3 => ProductType::Power,
        x => ProductType::Other(x),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IdHeader {
    pub vendor_id: u16,
    pub usb_comm_host: bool,
    pub usb_comm_device: bool,
    pub modal_operation: bool,
    pub ufp_product_type: ProductType,
    pub dfp_product_type: ProductType,
}

impl IdHeader {
    pub fn decode(vdo: u32) -> Self {
        IdHeader {
            usb_comm_host: ((vdo >> 31) & 1) == 1,
            usb_comm_device: ((vdo >> 30) & 1) == 1,
            ufp_product_type: ufp_type(((vdo >> 27) & 0b111) as u8),
            modal_operation: ((vdo >> 26) & 1) == 1,
            dfp_product_type: dfp_type(((vdo >> 23) & 0b111) as u8),
            vendor_id: (vdo & 0xffff) as u16,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProductVdo {
    pub product_id: u16,
    pub bcd_device: u16,
}

impl ProductVdo {
    pub fn decode(vdo: u32) -> Self {
        ProductVdo {
            product_id: (vdo & 0xffff) as u16,
            bcd_device: ((vdo >> 16) & 0xffff) as u16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CableSpeed {
    Usb2,
    Usb3Gen1,    // 5 Gbps
    Usb3Gen2,    // 10 Gbps
    Usb4Gen3,    // 20 Gbps
    Usb4Gen4,    // 40 Gbps
    Usb4Gen480,  // 80 Gbps
    Reserved,
}

impl CableSpeed {
    pub fn from_bits(v: u8) -> Self {
        // PD 3.x Passive/Active Cable VDO1 USB Highest Speed (bits 2:0).
        match v {
            0 => CableSpeed::Usb2,
            1 => CableSpeed::Usb3Gen1,
            2 => CableSpeed::Usb3Gen2,
            3 => CableSpeed::Usb4Gen3,
            4 => CableSpeed::Usb4Gen4,
            5 => CableSpeed::Usb4Gen480,
            _ => CableSpeed::Reserved,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CableSpeed::Usb2 => "USB 2.0 (480 Mbps)",
            CableSpeed::Usb3Gen1 => "USB 3.2 Gen 1 (5 Gbps)",
            CableSpeed::Usb3Gen2 => "USB 3.2 Gen 2 (10 Gbps)",
            CableSpeed::Usb4Gen3 => "USB4 Gen 2 (20 Gbps)",
            CableSpeed::Usb4Gen4 => "USB4 Gen 3 (40 Gbps)",
            CableSpeed::Usb4Gen480 => "USB4 Gen 4 (80 Gbps)",
            CableSpeed::Reserved => "Reserved/Unknown speed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CableCurrent {
    A3,
    A5,
    Reserved,
}

impl CableCurrent {
    pub fn from_bits(v: u8) -> Self {
        // PD 3.x Cable VDO1 VBUS Current Handling Capability (bits 6:5).
        match v {
            1 => CableCurrent::A3,
            2 => CableCurrent::A5,
            _ => CableCurrent::Reserved,
        }
    }

    pub fn amps(self) -> u32 {
        match self {
            CableCurrent::A3 => 3,
            CableCurrent::A5 => 5,
            CableCurrent::Reserved => 0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CableCurrent::A3 => "3A",
            CableCurrent::A5 => "5A",
            CableCurrent::Reserved => "unknown current",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CableType {
    Passive,
    Active,
}

#[derive(Debug, Clone, Copy)]
pub struct CableVdo {
    pub speed: CableSpeed,
    pub current: CableCurrent,
    pub max_volts: u32,
    pub max_watts: u32,
    pub cable_type: CableType,
}

impl CableVdo {
    /// Decode a Cable (SOP') VDO1. `is_active` selects active vs passive
    /// labelling; the bit positions used here are common to both.
    pub fn decode(vdo: u32, is_active: bool) -> Self {
        let speed = CableSpeed::from_bits((vdo & 0b111) as u8);
        let current = CableCurrent::from_bits(((vdo >> 5) & 0b11) as u8);

        // Maximum VBUS voltage encoding (bits 10:9).
        // 0b00 = 20V, 0b01 = 30V, 0b10 = 40V, 0b11 = 50V (PD 3.1 EPR).
        let v_bits = (vdo >> 9) & 0b11;
        let max_volts = match v_bits {
            0 => 20,
            1 => 30,
            2 => 40,
            3 => 50,
            _ => 20,
        };
        let max_watts = max_volts * current.amps();

        CableVdo {
            speed,
            current,
            max_volts,
            max_watts,
            cable_type: if is_active { CableType::Active } else { CableType::Passive },
        }
    }
}
