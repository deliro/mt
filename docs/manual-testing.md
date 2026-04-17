# Manual testing checklist

Run after any non-trivial change to transport or session code. Automated
tests cover domain logic, the framing codec, the handshake driver, and
MockTransport-driven session flow. Transports against real hardware are
covered here.

## Prerequisites

- `cargo build` succeeds.
- `cargo test` all green.
- For BLE on macOS: system Bluetooth is on and the binary has permission
  to use it (System Settings → Privacy & Security → Bluetooth).

## TCP

1. Start `meshtasticd` or point at a device that exposes TCP on port 4403.
2. `cargo run --release`.
3. Click **Add profile** → TCP → name `local`, host `127.0.0.1`, port
   `4403` → Save.
4. Click **Connect** on the `local` row.
5. Status bar must show `● Connected` and populated `fw …` within 10s.
6. Switch to **Nodes** tab. Expect at least our own node listed.
7. Switch back to **Chat**. Type text in the composer, press **Send**.
   The message appears with `…`, then flips to `✓` once the device acks.
   If there is no mesh around, expect `! no ack` after 30s.

## Serial

1. Plug in a Meshtastic device via USB. On macOS the device path is usually
   `/dev/cu.usbserial-*` or `/dev/cu.usbmodem*`.
2. Add a Serial profile with that path.
3. Same assertions as TCP.

## BLE (paired device)

Reference device: **fd1f_a910** — already paired at the OS level.

1. Launch `cargo run --release`.
2. Click **Scan BLE**. Wait ~3 seconds.
3. Expect `fd1f_a910` to appear in the list with a green `paired` badge.
4. Click **Connect**. No pairing modal should show (paired path).
5. Same assertions as TCP.

## BLE (new device, first pair)

1. Click **Scan BLE**. Pick a device with a yellow `new` badge.
2. Click **Connect**. The app shows an informational pairing modal.
3. Click **Continue**. The OS pairing dialog appears — type the 6-digit
   PIN from the device screen.
4. On success, the app proceeds through handshake and shows connected.
5. If the OS pairing fails or is cancelled, the app surfaces a typed
   error in the status bar.

### Linux note

btleplug on Linux relies on BlueZ. If BlueZ has no bond for the device,
the connect will fail with an authentication error. Pair once via
`bluetoothctl`:

```
bluetoothctl
> agent on
> default-agent
> pair <MAC>
> trust <MAC>
> exit
```

Then retry in the app.

## Disconnect & reconnect

1. While connected, click **Disconnect** in the sidebar.
2. Status bar must return to `○ Disconnected`.
3. Click **Connect** again on the same profile — expect a fresh
   handshake.

## Profile persistence

1. Add a profile, quit the app.
2. Config path:
   - macOS: `~/Library/Application Support/mt/profiles.toml`
   - Linux: `$XDG_CONFIG_HOME/mt/profiles.toml`
   - Windows: `%APPDATA%\mt\profiles.toml`
3. Open the file and confirm the profile is present.
4. Launch the app again — the profile reloads.
