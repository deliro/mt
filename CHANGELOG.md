# Changelog

All notable user-visible changes are listed here. Dates are UTC.

## v0.1.0 — unreleased

First tagged build. Feature complete for day-to-day Meshtastic use
against firmware 2.5+.

### Features

- BLE / USB-serial / TCP transports.
- Chat with per-channel broadcast, direct messages, delivery state
  (`queued / sent / acked / failed`) and ack-race / broadcast-Sent fixes.
- Node roster with live + cached split, favorites, ignore list, SNR /
  RSSI / battery, traceroute with per-hop SNR on forward and reverse
  paths.
- Full channel editing: 8 slots, roles, name, PSK (AES256 / AES128 /
  1-byte preset / clear), MQTT uplink-downlink, position precision,
  share URL import / export (`meshtastic.org/e/#` scheme).
- Admin actions with destructive-confirm modals: reboot, shutdown,
  reboot-to-OTA, factory reset (device / config-only), NodeDB reset.
- Module configs: MQTT, Telemetry, Neighbor Info, Store-and-Forward.
- Config sections: Owner, LoRa, Device, Position (with Fixed Position),
  Power, Network, Display, Bluetooth — every field has a tooltip.
- Local persistence: per-device SQLite of nodes and messages, survives
  restarts, clearable from the Settings tab.
- BLE suspend/resume recovery: receive-side watchdog + heartbeat
  metadata probe so a stale BLE handle after laptop wake is detected
  and surfaced as a disconnect within ~7 min.
- Bundled DejaVu Sans font for broad Unicode coverage in user-chosen
  node names.
