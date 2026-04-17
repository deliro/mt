# Meshtastic GUI client — MVP (Tier 1) design

Status: draft → pending user review
Date: 2026-04-18
Author: brainstorming session
Scope: Tier 1 MVP. Tier 2/3 will get their own specs later.

## 1. Goals and non-goals

### 1.1 Goals (Tier 1, MVP)

- Native desktop GUI client for Meshtastic, cross-platform: macOS, Linux, Windows.
- Connect to a single active device over **BLE**, **Serial (USB CDC)**, or **TCP** (firmware network API, port 4403).
- Perform the full PhoneAPI handshake: send `want_config_id`, drain `FromRadio` until `ConfigCompleteId`, build an initial snapshot (`MyNodeInfo`, nodes, channels, config, module config, metadata).
- Display live state:
  - Node list with name, role, battery, SNR/RSSI, hops away, last-heard, position (lat/lon/alt as numbers; map is Tier 3).
  - Channel list with current PSK/role metadata (read-only in MVP).
  - Primary-channel chat + per-channel tabs; DMs to individual nodes.
- Text messaging: send/receive on any channel, send DMs; show delivery state (Pending → Delivered → Failed) driven by routing acks.
- Connection profile management: add/edit/remove/select BLE, Serial, TCP profiles; persist to TOML.
- BLE scan with service UUID filter (`6ba1b218-15a8-461f-9fa8-5dcae273eafd`); paired-vs-new indication; OS-driven PIN entry on first pair.
- Keepalive heartbeat so firmware does not disconnect us.

### 1.2 Non-goals (deferred to later tiers)

- Tier 2: writing Device / Module / Channel config, traceroute, waypoints, admin actions (reboot/shutdown/factory reset), telemetry graphs, auto-reconnect, multi-device simultaneous sessions, programmatic BLE unpair, BlueZ Agent for in-app PIN entry on Linux.
- Tier 3: map with OSM tiles, file transfer, neighbor-info graph, OTA firmware update, localization, themes.
- Local SQLite persistence (nodedb/message history across restarts). MVP is in-memory only; only connection profiles persist.

## 2. Constraints and principles

- Rust, edition 2024. Deny warnings.
- Follow `/Users/tochkamac/.claude/custom/CLAUDE.md`: parse-don't-validate, functional core / imperative shell, newtypes for domain ids, sealed enums over stringly logic, `Result` over `Option` where errors are meaningful, exhaustive pattern matching, no global mutable state, prefer iterators and combinators, small composable functions, no comments unless WHY is non-obvious, no prefixes/suffixes on field names.
- Use `context7` MCP before calling into unfamiliar crate APIs (`btleplug`, `tokio-serial`, `prost`, `eframe/egui`).

## 3. Stack

| Concern | Choice | Reason |
| --- | --- | --- |
| GUI | `eframe` + `egui` | Only viable option for three-platform desktop targets; immediate-mode fits our reducer model. |
| Async runtime | `tokio` (multi-thread, minimal features) | De-facto standard; all chosen transport crates are tokio-native. |
| BLE | `btleplug` | Cross-platform (CoreBluetooth / WinRT / BlueZ) with a unified async API. |
| Serial | `tokio-serial` | Tokio-native port of `serialport` with `AsyncRead`/`AsyncWrite`. |
| TCP | `tokio::net::TcpStream` + `tokio_util::codec::Framed` | Stdlib, no extra dep. |
| Protobuf | `prost` + `prost-build` over `meshtastic/protobufs` as git submodule in `vendor/meshtastic-protobufs` | Full type control, strong types, no upstream API leakage. Pinned to a known-good firmware version. |
| Error | `thiserror` (lib-level) | Typed errors at boundaries. |
| Config paths | `directories` crate | XDG / macOS / Windows-correct config dirs. |
| Serde | `serde` + `toml` | Profiles only. |
| IDs | `rand` for random `ConfigId`/`PacketId` generation | Standard. |

The `meshtastic` crate from crates.io is deliberately not used — we own the transport layer and state machine. Only the `.proto` files are vendored.

## 4. Architecture

### 4.1 High-level shape

```
┌──────────────────────────────── process ────────────────────────────────┐
│                                                                          │
│  ┌─────────────┐    Commands (mpsc)      ┌──────────────────────────┐   │
│  │             │ ───────────────────────▶│                          │   │
│  │  UI thread  │                         │   Async runtime (tokio)  │   │
│  │  (egui,     │     Events (mpsc)       │                          │   │
│  │  eframe)    │ ◀───────────────────────│  ┌────────────────────┐  │   │
│  │             │                         │  │  Device session    │  │   │
│  │  holds      │   ctx.request_repaint() │  │  (state machine)   │  │   │
│  │  AppState   │ ◀───────────────────────│  └──────────┬─────────┘  │   │
│  └─────────────┘                         │             │            │   │
│                                          │   ┌─────────▼─────────┐  │   │
│                                          │   │  Transport trait  │  │   │
│                                          │   └────┬────┬────┬────┘  │   │
│                                          │    BLE │ SER│ TCP│       │   │
│                                          └────────┼────┼────┼───────┘   │
└───────────────────────────────────────────┬───────┼────┼────┼───────────┘
                                            ▼       ▼    ▼    ▼
                                          btleplug  tokio-serial
                                                        tokio::net::TcpStream
```

Principles baked in:

- UI knows nothing about transports — only `CommandTx` and `EventRx`.
- `AppState` in the UI is a projection. Updates happen by reducing `Event` into state — a pure function.
- One `DeviceSession` tokio-task owns the active transport, handshake, and heartbeat. Disconnect = task ends.
- A narrow `Transport` trait abstracts BLE / Serial / TCP behind `Sink<Frame> + Stream<Result<Frame>>`. A `MockTransport` exists for tests.
- The framing codec is a pure function, shared by all transports.

### 4.2 Module / file layout

```
Cargo.toml
build.rs                    # prost-build on vendor/meshtastic-protobufs/*.proto
vendor/
└── meshtastic-protobufs/   # git submodule pinned to a stable firmware tag
docs/
└── superpowers/specs/      # this file lives here
src/
├── main.rs                 # eframe entrypoint, wires channels
├── error.rs                # crate-level AppError (thiserror)
├── domain/                 # pure, no I/O
│   ├── mod.rs
│   ├── ids.rs              # newtypes: NodeId, ChannelIndex, PacketId, ConfigId, BleAddress
│   ├── node.rs             # Node, NodeRole, Position, DeviceMetrics
│   ├── channel.rs          # Channel, ChannelRole, PskMaterial
│   ├── message.rs          # TextMessage, Direction, DeliveryState, Recipient
│   ├── snapshot.rs         # DeviceSnapshot (post-handshake, invariants enforced)
│   ├── profile.rs          # ConnectionProfile, TransportKind
│   └── session.rs          # SessionState enum, reducer: (State, Event) -> State
├── proto/                  # generated + thin wrappers
│   ├── mod.rs              # re-exports; prost-build output
│   └── port.rs             # PortPayload sealed enum; PortNum+bytes -> typed
├── codec/                  # pure framing
│   ├── mod.rs
│   ├── frame.rs            # [0x94, 0xC3, len_hi, len_lo, bytes]
│   └── error.rs
├── transport/              # async I/O
│   ├── mod.rs              # trait Transport
│   ├── ble.rs              # btleplug adapter
│   ├── serial.rs           # tokio-serial adapter
│   ├── tcp.rs              # tokio TcpStream adapter
│   └── mock.rs             # test double, scripted frames
├── session/                # orchestration
│   ├── mod.rs              # DeviceSession task main loop
│   ├── handshake.rs        # want_config_id -> drain until ConfigCompleteId
│   └── commands.rs         # Command enum
├── persist/
│   ├── mod.rs
│   └── profiles.rs         # TOML profiles (load/save)
└── ui/
    ├── mod.rs              # App (eframe::App), top-level layout, event pump
    ├── connect.rs          # connect screen: profile list, scan, add dialog
    ├── nodes.rs            # node list pane
    ├── chat.rs             # channel tabs + message list + composer
    └── status.rs           # bottom status bar
```

### 4.3 Key types

Newtypes, all `Copy` where appropriate:

```rust
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NodeId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ChannelIndex(pub u8);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PacketId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ConfigId(pub u32);

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct BleAddress(pub String);  // platform-native form
```

Session state — invalid states unrepresentable:

```rust
pub enum SessionState {
    Disconnected,
    Connecting { transport: TransportKind, started: Instant },
    Handshake {
        transport: TransportKind,
        config_id: ConfigId,
        my_info: Option<MyNodeInfo>,
        nodes: HashMap<NodeId, Node>,
        channels: Vec<Channel>,
        config_complete: bool,
    },
    Ready(DeviceSnapshot),
    Failed(ConnectError),
}
```

`DeviceSnapshot` wraps all fields that are guaranteed present post-handshake, so `Ready` cannot be constructed without them:

```rust
pub struct DeviceSnapshot {
    pub my_info: MyNodeInfo,
    pub nodes: HashMap<NodeId, Node>,
    pub channels: Vec<Channel>,
    pub config: DeviceConfig,
    pub module_config: ModuleConfig,
    pub metadata: DeviceMetadata,
    pub messages: Vec<TextMessage>,
}
```

Connection profile and transport kind:

```rust
pub enum TransportKind { Ble, Serial, Tcp }

pub enum ConnectionProfile {
    Ble   { name: String, address: BleAddress },
    Serial{ name: String, path: PathBuf },
    Tcp   { name: String, host: String, port: u16 },
}
```

Port payload — sealed enum, no stringly logic:

```rust
pub enum PortPayload {
    Text(String),
    Position(Position),
    NodeInfo(NodeInfo),
    Telemetry(Telemetry),
    Routing(Routing),
    Admin(AdminMessage),
    Unknown { port: u32, bytes: Bytes },
}
```

`parse(port: PortNum, bytes: &[u8]) -> Result<PortPayload, ParseError>` lives in `proto::port`.

### 4.4 Channels between UI and session

```rust
// UI -> session
pub enum Command {
    Connect(ConnectionProfile),
    Disconnect,
    SendText { channel: ChannelIndex, to: Recipient, text: String, want_ack: bool },
}

pub enum Recipient { Broadcast, Node(NodeId) }

// session -> UI
pub enum Event {
    Connecting,
    Connected(DeviceSnapshot),
    NodeUpdated(Node),
    ChannelUpdated(Channel),
    MessageReceived(TextMessage),
    MessageStateChanged { id: PacketId, state: DeliveryState },
    Disconnected,
    Error(AppError),
}

// crate-level error, sum of everything that can surface to UI
pub enum AppError {
    Connect(ConnectError),
    Transport(TransportError),
    Persist(PersistError),
}
```

- `Command`: `tokio::sync::mpsc::unbounded_channel` — commands are rare.
- `Event`: `tokio::sync::mpsc::channel(256)` — bounded for handshake burst backpressure.
- Session-task clones the `egui::Context` at startup and calls `ctx.request_repaint()` after each event send.

## 5. Data flow

### 5.1 Connect and handshake

1. UI sends `Command::Connect(profile)`.
2. Session task picks a `Transport` impl per profile kind:
   - **BLE:** `btleplug::Manager::new()` → adapter → locate peripheral by `BleAddress` → `connect()` → `discover_services()` → find service `6ba1b218-15a8-461f-9fa8-5dcae273eafd` with characteristics `toRadio` (write), `fromRadio` (read), `fromNum` (notify). Subscribe to `fromNum`; each notify triggers a drain loop on `fromRadio` until it returns empty. Outgoing uses `write(toRadio, frame)`.
   - **Serial:** open `SerialStream` at 115200 8N1, no flow control; wrap in `tokio_util::codec::Framed` with our `FrameCodec`.
   - **TCP:** `TcpStream::connect(host:4403)` → `Framed<_, FrameCodec>`.
3. Session transitions `Disconnected → Connecting → Handshake`.
4. Send `ToRadio { want_config_id: rand::random() }`.
5. Read `FromRadio` messages in a loop, accumulating into the `Handshake` struct: `MyNodeInfo`, then `NodeInfo*`, `Channel*`, `Config*`, `ModuleConfig*`, `QueueStatus`, `Metadata`. Completion is signaled by `FromRadio::ConfigCompleteId(id)`; we verify `id == config_id`. Timeout = 10s → `ConnectError::HandshakeTimeout`.
6. Build `DeviceSnapshot`, transition to `Ready`, send `Event::Connected(snapshot)`.
7. Start heartbeat timer: every 300s send `ToRadio { heartbeat: Heartbeat {} }`.

### 5.2 Runtime (post-handshake)

- **Incoming:** each `FromRadio` is decoded. For `MeshPacket`, `PortPayload::parse` yields a typed payload, then mapped to an `Event` (e.g., `Text` → `MessageReceived`, `Position` → `NodeUpdated`, `Routing` → `MessageStateChanged`). Events flow to UI; UI reduces them into `AppState`.
- **Outgoing text:** UI emits `Command::SendText`. Session builds a `MeshPacket` (port `TEXT_MESSAGE_APP`; `to` = `BROADCAST_ADDR` for channel or `NodeId` for DM; `want_ack` as requested; fresh `PacketId`) wrapped in `ToRadio`, writes via transport. Session emits `MessageStateChanged { state: Pending }` locally so UI shows the message immediately. Firmware eventually returns a `Routing` response; session emits `MessageStateChanged { state: Delivered | Failed(reason) }`. Timeout for ack = 30s → `Failed(NoAck)`.
- **Heartbeat:** fire-and-forget; errors kill the session with `TransportError`.

### 5.3 Disconnect and errors

- UI `Command::Disconnect` → `CancellationToken` triggers → transport closed → session task exits → `Event::Disconnected`.
- Any transport error (in `Handshake` or `Ready`) → `Event::Error(AppError::Transport(..))` followed by `Event::Disconnected`. Session task exits. `SessionState::Failed` is used only for pre-`Ready` failures surfaced in the connect screen; once `Ready` is reached, runtime errors go through events and take the state back to `Disconnected`.
- **No auto-reconnect in MVP.** User clicks Connect again. Tier 2 will add backoff / reconnect.

## 6. BLE pairing

Meshtastic BLE characteristics require encryption (LE Secure Connections, Passkey Entry). The device shows a 6-digit PIN on its screen at first pair.

The PIN dialog is **always an OS-level dialog**. Our app cannot intercept the PIN entry on any of the three target platforms without substantial per-platform work (out of MVP scope).

### 6.1 Platform behavior

| Platform | Pair trigger | PIN entry | Bond location | Already-paired |
| --- | --- | --- | --- | --- |
| macOS (CoreBluetooth) | Any access to encrypted char | Native system dialog | System-wide | Transparent |
| Windows (WinRT) | Optional explicit `pair()` or first encrypted access | Native system dialog | System-wide | Transparent |
| Linux (BlueZ) | Explicit pair via BlueZ agent | `bluetoothctl` agent (MVP: external) | BlueZ system DB | Transparent once bonded |

### 6.2 MVP behavior in our app

- **Scan UI:** shows all advertisers exposing Meshtastic service UUID. Each entry carries a badge:
  - `Paired` when `peripheral.is_paired()` returns true.
  - `New` otherwise.
- **Connect to `Paired`:** direct, no dialog from us.
- **Connect to `New`:** before triggering first GATT op, we show a modal:
  > “Meshtastic will display a 6-digit PIN on the device screen. Your OS will open a system dialog asking for it — type it there. This is only needed the first time you pair.”
  
  After the user dismisses the modal, we run `connect()` → `discover_services()` → first `read(fromRadio)`. The OS takes over for the PIN step on macOS/Windows. On Linux, if no bond exists, this will fail with an authentication error (see below).
- **Linux pre-pair guidance (MVP):** if `ConnectError::BlePairingRequired` fires on Linux, we surface instructions with the exact `bluetoothctl` commands (`agent on; default-agent; pair <MAC>; trust <MAC>`). Registering an in-process BlueZ agent via `zbus` is deferred to Tier 2.
- **Forget device:** MVP shows a link to the OS Bluetooth settings. Programmatic unpair is Tier 2.

### 6.3 Error type

```rust
pub enum ConnectError {
    BleAdapterUnavailable,
    BleDeviceNotFound(BleAddress),
    BlePairingRequired { platform_hint: PairingHint },
    BlePairingFailed(String),
    BleGattFailed(String),
    Serial(tokio_serial::Error),
    Tcp(std::io::Error),
    HandshakeTimeout,
    Codec(codec::Error),
}

pub enum PairingHint { Macos, Windows, LinuxBluetoothctl }
```

UI maps each variant to a message with a concrete next step.

## 7. UI

### 7.1 Layout

Single `eframe` window. Before connect: connect screen. After connect: main screen.

Main screen:

```
┌──────────────────────────────────────────────────────────────┐
│  Status bar: transport ▓  my node ▓  battery ▓  ch ▓  ...    │
├───────────┬──────────────────────────────────────────────────┤
│  Sidebar  │  Tabs: Chat │ Nodes                              │
│  profiles │                                                  │
│   BLE …   │  Chat: channel tabs above, message list below,   │
│   Serial… │        composer at the bottom of the main pane.  │
│   TCP …   │  Nodes: sortable table.                          │
│  [+ Add]  │                                                  │
│ [Disconn] │                                                  │
└───────────┴──────────────────────────────────────────────────┘
```

### 7.2 Components

- **ConnectScreen** (`ui/connect.rs`): profile list on left, detail pane on right. Actions: Connect, Edit, Delete, Add (choose BLE/Serial/TCP → form), Scan BLE.
- **ScanDialog:** live list of discovered peripherals with service UUID filter. Each row: name, address, RSSI, `Paired`/`New` badge. Click → Connect via new transient profile; optionally Save as profile.
- **ChatView** (`ui/chat.rs`): top row of channel tabs (primary channel first, then others by index, then “DMs” group by NodeId). Main pane: scrolling message list, each message with sender name, channel, timestamp, text, delivery state icon for outgoing. Bottom: composer with channel selector and Send button.
- **NodesView** (`ui/nodes.rs`): sortable columns — long name, short name, role, battery %, SNR, RSSI, hops, last-heard (relative), position (“48.1°N, 17.1°E, 213m” or “—”).
- **StatusBar** (`ui/status.rs`): transport kind + endpoint, my node name, my battery, current firmware version, last error toast.

### 7.3 UX rules

- No blocking dialogs; pairing modal is informational only (user dismisses to proceed).
- Errors: toast + append to log panel. Never panic.
- `Ready` state transitions from `Connecting` are visualized with a progress indicator (percent based on handshake messages received vs expected, approximate).
- Localization is not wired in MVP; strings in English in code. Russian-first UX was discussed but postponed to Tier 3.

## 8. Persistence

Only connection profiles in MVP. Path:

- macOS: `~/Library/Application Support/mt/profiles.toml`
- Linux: `$XDG_CONFIG_HOME/mt/profiles.toml` (fallback `~/.config/mt/profiles.toml`)
- Windows: `%APPDATA%\mt\profiles.toml`

Resolution via `directories::ProjectDirs::from("dev", "", "mt")`.

File shape:

```toml
[[profile]]
kind = "ble"
name = "Rucksack"
address = "A1:B2:..."

[[profile]]
kind = "serial"
name = "Heltec on /dev/ttyUSB0"
path = "/dev/ttyUSB0"

[[profile]]
kind = "tcp"
name = "Home gateway"
host = "192.168.1.42"
port = 4403
```

Nodedb and message history do not persist in MVP.

## 9. Testing

### 9.1 Pure unit tests

- `codec::frame`: round-trip (encode then decode arbitrary payloads), boundary cases (empty, max-length), malformed input (bad magic, truncated length, truncated body), streaming partial reads.
- `proto::port::parse`: exhaustive PortNum coverage; `Unknown` fallback preserves raw bytes.
- `domain::session::reduce`: apply canned `Event` sequences, assert final `AppState` fields.

### 9.2 Integration tests

- `DeviceSession` with `MockTransport`: scripted frame stream exercising full handshake (want_config_id → MyNodeInfo → 3×NodeInfo → 2×Channel → Config → ModuleConfig → ConfigCompleteId). Assert emitted `Event` sequence and final snapshot.
- SendText round-trip with mock: command → outgoing `ToRadio` captured, inject `Routing` ack, assert `MessageStateChanged` progression.

### 9.3 Out of CI

- Real BLE/Serial/TCP against hardware: documented manual checklist in the repository’s README. One optional `#[ignore]`-gated integration test connects to a local `meshtasticd` TCP endpoint when present.
- UI: a headless smoke test instantiates `App::update` with fixture snapshots, asserting no panics across a small matrix of states. Full visual testing is out of scope.

## 10. Milestones for the implementation plan

(Feeds directly into the writing-plans step that follows.)

1. Project scaffolding: `Cargo.toml` deps, vendored protobuf submodule, `build.rs` wired, generated types re-exported.
2. `codec::frame` + unit tests. Pure, no async.
3. `domain` module: types, ids, reducer (with tests).
4. `proto::port` + tests.
5. `transport::Transport` trait + `MockTransport` + tests.
6. `session::handshake` driven by `MockTransport`.
7. `session::DeviceSession` task: command/event loop, heartbeat, ack timeout.
8. `transport::tcp` implementation (simplest real transport first).
9. `transport::serial` implementation.
10. `transport::ble` implementation, paired flow only; then pairing modal + error surfacing.
11. `persist::profiles` TOML load/save.
12. UI skeleton: `App`, top-level layout, event pump, `ctx.request_repaint()` wiring.
13. ConnectScreen + ScanDialog.
14. ChatView + composer; outgoing/ingoing wiring; delivery state.
15. NodesView.
16. StatusBar.
17. End-to-end manual test against a real device over TCP, then Serial, then BLE.

## 11. Open questions left for the plan

- Exact pinned commit of `meshtastic/protobufs` submodule (pick latest stable-firmware tag at plan-writing time).
- Whether `Framed<_, FrameCodec>` can sit on top of btleplug’s notify stream directly, or whether BLE needs a bespoke adapter that pieces characteristic reads into a byte stream (likely the latter — BLE reads are message-oriented, not byte-oriented). Resolve during step 10.
- Exact egui widget choice for the scrolling message list (probably `ScrollArea::vertical().stick_to_bottom(true)` around a `Vec`-backed list). Resolve during step 14.

These do not block spec approval; they are implementation details to settle inside the plan.
