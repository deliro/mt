# mt — native Meshtastic desktop client

A cross-platform desktop client for [Meshtastic](https://meshtastic.org) radios,
written in Rust with `egui`. Connects over **BLE** (primary), **USB serial**, or
**TCP** to a Meshtastic device and provides feature parity with the official
web / mobile clients for day-to-day operation.

## Features

- **Transports:** BLE (macOS / Linux / Windows via `btleplug`), USB serial, TCP.
- **Chat:** per-channel broadcast, direct messages, ack / nack with visual
  delivery state (`○ queued · ◐ sent · ✓ acked · ✗ failed`).
- **Nodes:** live + cached roster, last-heard, SNR / RSSI / battery,
  favorites, ignore-list, traceroute with per-hop SNR.
- **Channels:** 8 slots, role / name / PSK controls (AES256, AES128, 1-byte
  default preset, clear), MQTT uplink / downlink, position precision,
  share URL import / export (compatible with the `meshtastic.org/e/#` scheme
  used by web and mobile clients).
- **Settings:** Owner / LoRa / Device / Position / Power / Network / Display /
  Bluetooth / MQTT / Telemetry / Neighbor Info / Store-and-Forward module
  configs, with tooltips on every field.
- **Admin actions:** reboot / shutdown / reboot-to-OTA / factory reset
  (device / config only) / NodeDB reset, gated by destructive-action
  confirmation modals.
- **Local persistence:** node roster and chat history survive disconnects
  (SQLite under the platform data dir). Messages scoped per-device.
- **Status bar:** connection state, link health, battery, channel utilization,
  airtime, relay count.
- **Resilience:** receive-side watchdog + metadata probe reconnect cleanly
  after the OS suspends the radio (e.g. laptop lid close / wake).

## Install

### From source

```sh
git clone <this repo>
cd mt
cargo build --release
```

Binary ends up at `target/release/mt`.

A minimal headless CLI for BLE connectivity testing is also built:
`target/release/mt-cli`.

### Runtime requirements

- **macOS:** first launch over BLE prompts for Bluetooth permission under
  *System Settings → Privacy & Security → Bluetooth*. Grant it, restart the
  app.
- **Linux:** requires BlueZ ≥ 5.56. The user must be able to access
  `org.bluez` over D-Bus (`bluetooth` group or a polkit rule). USB serial
  usually needs the user to be in the `dialout` or `uucp` group.
- **Windows:** BLE works out of the box on Windows 10 21H2+. USB serial
  needs the vendor CP210x / CH340 driver for the board.

## Running

```sh
cargo run --release
```

On first launch you'll see the connection screen. Scan for BLE devices,
enter a TCP `host:port`, or select a serial port, then hit *Connect*.
The app remembers profiles in
`~/Library/Application Support/mt/profiles.toml` (macOS) /
`~/.config/mt/profiles.toml` (Linux) /
`%APPDATA%\mt\profiles.toml` (Windows).

Message history and the node roster are persisted in `mt.sqlite` alongside
the profiles file.

## Development

- `cargo clippy --all-targets -- -D warnings` must pass clean. The crate
  enables `clippy::pedantic`, `clippy::nursery`, and denies `unwrap_used`,
  `expect_used`, `panic`, `indexing_slicing`, `integer_division`, and
  friends in production code.
- `cargo test` runs the unit / integration test suite.
- Protobuf definitions are vendored under `vendor/meshtastic-protobufs`
  and regenerated at build time via `build.rs`.

See `RELEASING.md` for how to cut and ship a release build.

## License

Dual-licensed under Apache 2.0 or MIT, at your option. The bundled DejaVu
Sans font is redistributed under its own permissive license — see
`assets/DejaVuSans-LICENSE.txt`.
