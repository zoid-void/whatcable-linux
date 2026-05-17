# whatcable (Linux)

> **What can this USB-C cable / port actually do?**

A small Linux CLI that tells you, in plain English, what each USB-C port on
your Linux machine is doing — what cable is plugged in, what e-marker (if
any) it advertises, what the charger can deliver, what's currently being
negotiated, and **why your laptop might be charging slowly**.

This is a Linux port of [Darryl Morley's WhatCable](https://github.com/darrylmorley/whatcable)
(macOS menu bar app + CLI). Same problem, same plain-English design, just
reading Linux sysfs instead of macOS IOKit.

## Install

Requires Rust (1.74+) and a recent Linux kernel (6.1+ recommended) with
the USB Type-C class populated.

```bash
cargo build --release
sudo install -m 755 target/release/whatcable /usr/local/bin/whatcable
```

## Usage

```bash
whatcable                # human-readable summary of every USB-C port
whatcable --json         # structured JSON, pipe into jq
whatcable --watch        # stream updates as cables come and go (Ctrl+C to exit)
whatcable --raw          # include underlying sysfs attributes
whatcable --version
whatcable --help
```

### Example from this computer

```
=== port0 ===
Thunderbolt / USB4 · 50W charger
Supports high-speed data, video.
  role: host / sink

  • Thunderbolt / USB4 alt mode active
  • USB Power Delivery negotiated
  • Cable does not advertise an e-marker (basic cable)
  • Charger advertises up to 50W
  • PDOs: 5V/2.00A, 9V/2.44A, 15V/2.00A, 20V/2.50A
  • Currently negotiated: 20V @ 2.50A (50.0W)
  • Power supply usb_type: C [PD] PD_PPS

Charging: Charging well at 50W
  Charger and cable are well-matched.

=== port1 ===
Thunderbolt / USB4
Supports high-speed data, video.
  role: host / source

  • Thunderbolt / USB4 alt mode active
  • USB Power Delivery negotiated
  • Cable does not advertise an e-marker (basic cable)
  • Power supply usb_type: C [PD] PD_PPS

=== Attached USB devices ===
  • bus 1 dev 1 [usb1] 1d6b:0002 · High-Speed (480 Mbps) — Linux 6.17.0-23-generic xhci-hcd xHCI Host Controller
  • bus 2 dev 1 [usb2] 1d6b:0003 · SuperSpeed+ (10 Gbps) — Linux 6.17.0-23-generic xhci-hcd xHCI Host Controller
  • bus 2 dev 2 [2-1] 2109:0822 · SuperSpeed+ (10 Gbps) — VIA Labs, Inc. USB3.1 Hub
  • bus 2 dev 3 [2-2] 0bda:9210 · SuperSpeed+ (10 Gbps) — Realtek RTL9210
  • bus 2 dev 4 [2-1.2] 152d:a580 · SuperSpeed+ (10 Gbps) — JMicron USB Mass Storage
  • bus 2 dev 5 [2-1.3] 05e3:0626 · SuperSpeed (5 Gbps) — GenesysLogic USB3.1 Hub
  • bus 2 dev 6 [2-1.3.4] 0b95:1790 · SuperSpeed (5 Gbps) — ASIX AX88179B
  • bus 3 dev 1 [usb3] 1d6b:0002 · High-Speed (480 Mbps) — Linux 6.17.0-23-generic xhci-hcd xHCI Host Controller
  • bus 3 dev 2 [3-1] 2109:2822 · High-Speed (480 Mbps) — VIA Labs, Inc. USB2.0 Hub
  • bus 3 dev 3 [3-2] 1a86:8095 · High-Speed (480 Mbps) — USB Hub
  • bus 3 dev 4 [3-6] 0bda:5532 · High-Speed (480 Mbps) — CKFIH56R29344008D4A0 Integrated_Webcam_HD
  • bus 3 dev 5 [3-1.3] 05e3:0610 · High-Speed (480 Mbps) — GenesysLogic USB2.1 Hub
  • bus 3 dev 6 [3-8] 0a5c:5842 · High-Speed (480 Mbps) — Broadcom Corp 58200
  • bus 3 dev 7 [3-1.5] 2109:8822 · High-Speed (480 Mbps) — VIA Labs, Inc. USB Billboard Device
  • bus 3 dev 8 [3-10] 8087:0026 · Full-Speed (12 Mbps) — 8087:0026
  • bus 4 dev 1 [usb4] 1d6b:0003 · SuperSpeed+ (10 Gbps) — Linux 6.17.0-23-generic xhci-hcd xHCI Host Controller

=== Thunderbolt devices ===
  • 0-0 [0-0] authorized
```

## What it shows

Per port, in plain English:

- **At-a-glance headline:** Thunderbolt/USB4, USB device, Charging only,
  Slow USB / charge-only cable, Nothing connected.
- **Charging diagnostic:** when something's plugged in, a banner identifies
  the bottleneck — *cable rated below the charger*, *Mac/Linux is asking
  for less than the charger can give*, or *charging well*.
- **Cable e-marker info:** when the kernel exposes it, the cable's
  speed (USB 2.0, 5 / 10 / 20 / 40 / 80 Gbps), current rating
  (3 A / 5 A up to 60W / 100W / 240W), and the chip's vendor.
- **Charger PDO list:** every voltage profile the charger advertises
  (5V / 9V / 12V / 15V / 20V…) with the live negotiated profile.
- **Connected device identity:** vendor name and product type, decoded
  from the PD Discover Identity response (when the kernel exposes it).
- **Active alt modes:** DisplayPort, Thunderbolt, vendor-specific.
- **Attached USB devices** with negotiated speed, listed at the end.
- **Thunderbolt devices** with their authorisation state.

## How it works

`whatcable` reads four families of Linux sysfs:

| Sysfs path | What it gives us |
| --- | --- |
| `/sys/class/typec/portN`, `portN-partner`, `portN-cable` | Per-port state: connection, data/power role, alt modes, e-marker presence, partner / cable identity |
| `/sys/class/usb_power_delivery/pdN/source-capabilities/*` | Full PDO list from the connected source — every voltage / current profile |
| `/sys/class/power_supply/ucsi-source-psy-USBC000:NNN` | Live negotiated voltage / current (the "winning" PDO) |
| `/sys/bus/usb/devices/*` | Attached USB devices (bus, devnum, speed, VID:PID, descriptors) |
| `/sys/bus/thunderbolt/devices/*` | Connected Thunderbolt / USB4 devices and authorisation state |

Cable e-marker decoding follows the USB Power Delivery 3.x spec
([`src/pdvdo.rs`](src/pdvdo.rs)).

No root, no kernel modules, no helper daemons.

## Caveats vs the macOS original

- **Cable identity is kernel-dependent.** Not every laptop's UCSI / PD
  driver populates `/sys/class/typec/portN-cable/identity/`. macOS gets
  this data from IOKit unconditionally; on Linux it shows up only when
  the kernel chooses to expose it.
- **No SOP'' (far-end cable e-marker).** Linux sysfs generally only
  exposes SOP and SOP'.
- **Per-port USB-device mapping** is not yet attempted. Linux does not
  guarantee a stable mapping from `/sys/class/typec/portN` to a
  `/sys/bus/usb/devices/...` subtree, so attached USB devices are
  listed in a global section instead of being grouped under each port.
  (Improving this is a future enhancement.)
- **Watch mode polls** at a 1-second interval rather than subscribing to
  netlink uevents. Good enough for "did you just plug something in?";
  upgrade to netlink later if you need sub-second response.
- **Vendor name lookup is bundled but not exhaustive.** Common cable,
  charger, hub, and dock vendors are recognised; everything else falls
  back to the hex VID.

## Architecture

```
src/
├── main.rs       # CLI entry + arg parsing + watch loop
├── snapshot.rs   # data model: CableSnapshot, UsbcPort, PowerSource, PdIdentity, …
├── pdvdo.rs      # PD VDO bit decoders (ID Header, Cable VDO)
├── vendor.rs     # VID -> vendor name lookup
├── sysfs.rs      # tiny read helpers
├── backend.rs    # the Linux backend: builds a CableSnapshot from sysfs
├── summary.rs    # plain-English PortSummary + ChargingDiagnostic
├── text.rs       # human formatter
└── json.rs       # JSON formatter
```

This mirrors the original WhatCable's split between `WhatCableCore`
(pure logic) and `WhatCableDarwinBackend` (IOKit watchers) — here the
backend is `backend.rs` and reads sysfs.

## Attribution

This project is based on [WhatCable](https://github.com/darrylmorley/whatcable),
the macOS app and CLI by [Darryl Morley](https://whatcable.uk). The original
project is licensed under the MIT License.

Original WhatCable copyright notice:

> Copyright (c) 2026 Darryl Morley

## License

MIT. See [`LICENSE`](LICENSE), which preserves the upstream WhatCable copyright
and MIT permission notice.
