//! Tiny VID -> vendor name lookup. Only common cable / charger / hub
//! vendors are listed; everything else falls back to the hex VID.

pub fn name_for(vid: u16) -> Option<&'static str> {
    match vid {
        0x05ac => Some("Apple"),
        0x046d => Some("Logitech"),
        0x05e3 => Some("Genesys Logic"),
        0x0bda => Some("Realtek"),
        0x174c => Some("ASMedia"),
        0x2188 => Some("Anker"),
        0x291a => Some("Anker"),
        0x2109 => Some("VIA Labs"),
        0x152d => Some("JMicron"),
        0x0781 => Some("SanDisk"),
        0x0951 => Some("Kingston"),
        0x1058 => Some("Western Digital"),
        0x04e8 => Some("Samsung"),
        0x8087 => Some("Intel"),
        0x18d1 => Some("Google"),
        0x413c => Some("Dell"),
        0x17ef => Some("Lenovo"),
        0x03f0 => Some("HP"),
        0x0bb4 => Some("HTC"),
        0x2c7c => Some("Quectel"),
        0x1d6b => Some("Linux Foundation"),
        0x32ac => Some("Framework"),
        0x0451 => Some("Texas Instruments"),
        0x0b95 => Some("ASIX"),
        _ => None,
    }
}

pub fn label_for(vid: u16) -> String {
    match name_for(vid) {
        Some(name) => name.to_string(),
        None => format!("Vendor 0x{:04X}", vid),
    }
}
