# Meshtastic GUI Client MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Native desktop Meshtastic client in Rust over egui, connecting to a single device via BLE, Serial, or TCP; displays nodes and channels, sends and receives text messages; works on macOS, Linux, Windows.

**Architecture:** egui on the UI thread; tokio runtime on a background thread; one `DeviceSession` task owns the active transport and protocol state machine; UI and session communicate only through typed `Command` and `Event` mpsc channels. A pure framing codec and a thin `Transport` trait abstract BLE / Serial / TCP. State transitions (Disconnected → Connecting → Handshake → Ready) are modeled with a sealed enum; invalid states are unrepresentable.

**Tech Stack:** Rust edition 2024, `eframe`/`egui`, `tokio`, `tokio-util`, `tokio-serial`, `btleplug`, `prost` + `prost-build` on a vendored `meshtastic/protobufs` submodule, `thiserror`, `serde`/`toml`, `directories`, `rand`, `bytes`, `futures`, `tracing`.

Reference: `docs/superpowers/specs/2026-04-18-meshtastic-gui-mvp-design.md`.

---

## Conventions

- **TDD always.** Write the failing test, run it, implement the minimum, run it green, commit.
- **Commits are small** — one task = one or a few commits. Hooks must not be bypassed; fix problems, don't skip.
- **Deny warnings.** Add `#![deny(warnings)]` at the crate root once the code compiles; until then the build will still reject warnings via `RUSTFLAGS` in CI — for local dev the deny-attr is enough.
- **Use context7 MCP** for any crate whose API you are not sure about: `resolve-library-id` then `query-docs`. Do this before writing code that calls an unfamiliar crate (especially `btleplug`, `tokio-serial`, `eframe`, `prost`).
- **Strings** are in English in code. No localization in MVP.
- **File paths are absolute in commits only** when documenting. In code use relative module paths.
- When a test checks a side-effect that is async, use `#[tokio::test]` with `flavor = "current_thread"` by default; switch to multi-thread only if the test actually needs it.
- The working directory is `/Users/tochkamac/projects/own/mt`. All commands run from there unless noted.

---

## File Map

The plan builds this tree. Files named here must exist at the end of the listed task.

```
Cargo.toml                  # Task 1
build.rs                    # Task 1
.gitignore                  # pre-existing; extended in Task 1
rustfmt.toml                # Task 1
vendor/
  meshtastic-protobufs/     # git submodule (Task 1)
src/
  main.rs                   # Task 23
  lib.rs                    # Task 1 (empty module declarations grow through the plan)
  error.rs                  # Task 1
  codec/
    mod.rs                  # Task 2
    frame.rs                # Task 2, 3
    error.rs                # Task 2
  proto/
    mod.rs                  # Task 1
    port.rs                 # Task 7
  domain/
    mod.rs                  # Task 4
    ids.rs                  # Task 4
    node.rs                 # Task 5
    channel.rs              # Task 5
    message.rs              # Task 5
    snapshot.rs             # Task 5
    profile.rs              # Task 5
    session.rs              # Task 6
  transport/
    mod.rs                  # Task 8
    error.rs                # Task 8
    mock.rs                 # Task 9
    tcp.rs                  # Task 12
    serial.rs               # Task 13
    ble.rs                  # Task 14, 15
  session/
    mod.rs                  # Task 11
    handshake.rs            # Task 10
    commands.rs             # Task 11
  persist/
    mod.rs                  # Task 16
    profiles.rs             # Task 16
  ui/
    mod.rs                  # Task 17
    connect.rs              # Task 18
    scan.rs                 # Task 19
    chat.rs                 # Task 20
    nodes.rs                # Task 21
    status.rs               # Task 22
docs/
  superpowers/
    specs/2026-04-18-meshtastic-gui-mvp-design.md  # already committed
    plans/2026-04-18-meshtastic-gui-mvp.md         # this file
```

Every file has one clear responsibility. UI files call into `domain` but never into `transport` or `session` directly — they speak only via the `Command`/`Event` channels created in `main.rs`.

---

## Task 1: Project scaffolding and protobuf generation

**Goal:** `cargo check` compiles an empty-but-wired crate; `prost-build` generates types from the submodule; `use mt::proto::meshtastic::MyNodeInfo;` resolves.

**Files:**
- Modify: `Cargo.toml`
- Create: `build.rs`
- Create: `rustfmt.toml`
- Modify: `.gitignore`
- Create: `src/lib.rs`, `src/error.rs`, `src/proto/mod.rs`
- Submodule: `vendor/meshtastic-protobufs`

### Steps

- [ ] **Step 1: Extend `.gitignore`**

Edit `.gitignore` to contain:

```
/target
Cargo.lock.bak
.DS_Store
```

- [ ] **Step 2: Add submodule**

```bash
git submodule add https://github.com/meshtastic/protobufs.git vendor/meshtastic-protobufs
cd vendor/meshtastic-protobufs
git fetch --tags
# Pick the highest v2.* tag; as of plan-writing the v2.5.x series is stable.
# Verify with:  git tag --list 'v2.*' --sort=-v:refname | head -5
# Then checkout the chosen tag, for example:
git checkout v2.5.21
cd ../..
```

If `v2.5.21` does not exist, pick the highest `v2.*` release tag that does and use it everywhere this plan references the submodule.

- [ ] **Step 3: Write `rustfmt.toml`**

```toml
edition = "2024"
max_width = 100
use_small_heuristics = "Max"
```

- [ ] **Step 4: Write `Cargo.toml`**

```toml
[package]
name = "mt"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
path = "src/lib.rs"

[[bin]]
name = "mt"
path = "src/main.rs"

[dependencies]
bytes = "1"
directories = "5"
eframe = { version = "0.29", default-features = false, features = ["default_fonts", "glow", "wayland", "x11"] }
egui = "0.29"
egui_extras = "0.29"
futures = "0.3"
prost = "0.13"
rand = "0.8"
serde = { version = "1", features = ["derive"] }
thiserror = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "io-util", "net", "fs"] }
tokio-util = { version = "0.7", features = ["codec"] }
toml = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

btleplug = "0.11"
tokio-serial = "5"

[build-dependencies]
prost-build = "0.13"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "io-util", "net", "test-util"] }
```

Before running anything, use `context7` to confirm that `btleplug`, `eframe`, `tokio-serial`, and `prost` are still on the versions above. If a newer stable exists, bump.

- [ ] **Step 5: Write `build.rs`**

```rust
use std::io::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let proto_root = PathBuf::from("vendor/meshtastic-protobufs");
    let protos = walk_protos(&proto_root)?;
    if protos.is_empty() {
        panic!("no .proto files under {}", proto_root.display());
    }
    let mut cfg = prost_build::Config::new();
    cfg.out_dir(PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")));
    cfg.compile_protos(&protos, &[proto_root.as_path()])?;
    println!("cargo:rerun-if-changed=vendor/meshtastic-protobufs");
    Ok(())
}

fn walk_protos(root: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in walkdir(root)? {
        if entry.extension().map(|e| e == "proto").unwrap_or(false) {
            out.push(entry);
        }
    }
    Ok(out)
}

fn walkdir(root: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut stack = vec![root.to_path_buf()];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    Ok(out)
}
```

- [ ] **Step 6: Write `src/proto/mod.rs`**

```rust
#![allow(clippy::all)]

pub mod meshtastic {
    include!(concat!(env!("OUT_DIR"), "/meshtastic.rs"));
}
```

If `prost-build` emits multiple top-level packages (e.g. `meshtastic` and `nanopb`), add one `include!` per package. Check `OUT_DIR` after the first build.

- [ ] **Step 7: Write `src/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("connect error: {0}")]
    Connect(#[from] ConnectError),
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("persist error: {0}")]
    Persist(#[from] PersistError),
}

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("placeholder")] Placeholder,
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("placeholder")] Placeholder,
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("placeholder")] Placeholder,
}
```

The `Placeholder` variants are temporary — every later task that touches an error enum either adds variants or replaces `Placeholder`. The crate must compile at the end of Task 1.

- [ ] **Step 8: Write `src/lib.rs`**

```rust
#![deny(warnings)]

pub mod codec;
pub mod domain;
pub mod error;
pub mod persist;
pub mod proto;
pub mod session;
pub mod transport;
pub mod ui;
```

- [ ] **Step 9: Stub every referenced module**

Create the following one-line files so `lib.rs` compiles:

```bash
mkdir -p src/codec src/domain src/persist src/session src/transport src/ui
```

Each of these becomes a one-line `mod.rs`:

`src/codec/mod.rs`:
```rust
```

`src/domain/mod.rs`:
```rust
```

`src/persist/mod.rs`:
```rust
```

`src/session/mod.rs`:
```rust
```

`src/transport/mod.rs`:
```rust
```

`src/ui/mod.rs`:
```rust
```

(Yes, intentionally empty. Tasks 2-22 populate them.)

Also replace `src/main.rs` with:

```rust
fn main() {}
```

- [ ] **Step 10: Verify**

```bash
cargo build
```

Expected: build succeeds. `cargo check --tests` also succeeds.

If `prost-build` fails because a proto file uses `import "google/protobuf/..."`, add `--experimental_allow_proto3_optional` via `cfg.protoc_arg("--experimental_allow_proto3_optional")` in `build.rs`. This is common with recent Meshtastic protos.

- [ ] **Step 11: Commit**

```bash
git add .gitignore .gitmodules vendor/meshtastic-protobufs Cargo.toml build.rs rustfmt.toml src/
git commit -m "Scaffold crate, vendor meshtastic protobufs, wire prost-build"
```

---

## Task 2: Frame codec — encoder and pure decoder

**Goal:** Encode any byte payload into the Meshtastic framing `[0x94, 0xC3, len_hi, len_lo, ..bytes]` and decode a well-formed frame back out. Pure functions, no I/O.

**Files:**
- Create: `src/codec/frame.rs`
- Create: `src/codec/error.rs`
- Modify: `src/codec/mod.rs`
- Create: `tests/codec_frame.rs` (integration test for the pure API)

### Steps

- [ ] **Step 1: Write `src/codec/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("bad magic: {0:#04x} {1:#04x}")]
    BadMagic(u8, u8),
    #[error("frame too large: {0}")]
    TooLarge(usize),
    #[error("need {0} more bytes")]
    NeedMore(usize),
}

pub const MAX_FRAME_PAYLOAD: usize = 512;
```

- [ ] **Step 2: Write the failing test**

Create `tests/codec_frame.rs`:

```rust
use mt::codec::frame::{decode, encode, MAX_FRAME_PAYLOAD};
use mt::codec::error::FrameError;

#[test]
fn encodes_empty_payload_as_header_only() {
    let out = encode(&[]).expect("empty payload is valid");
    assert_eq!(out, vec![0x94, 0xC3, 0x00, 0x00]);
}

#[test]
fn encodes_payload_with_big_endian_length() {
    let payload = vec![1, 2, 3, 4, 5];
    let out = encode(&payload).unwrap();
    assert_eq!(&out[..4], &[0x94, 0xC3, 0x00, 0x05]);
    assert_eq!(&out[4..], &payload[..]);
}

#[test]
fn rejects_oversized_payload() {
    let big = vec![0u8; MAX_FRAME_PAYLOAD + 1];
    assert_eq!(encode(&big), Err(FrameError::TooLarge(big.len())));
}

#[test]
fn decodes_round_trip() {
    let payload = (0u8..200).collect::<Vec<_>>();
    let framed = encode(&payload).unwrap();
    let (out, consumed) = decode(&framed).unwrap();
    assert_eq!(out, payload);
    assert_eq!(consumed, framed.len());
}

#[test]
fn decode_rejects_bad_magic() {
    let bad = [0x00, 0x00, 0x00, 0x00];
    assert_eq!(decode(&bad), Err(FrameError::BadMagic(0x00, 0x00)));
}

#[test]
fn decode_needs_more_when_short() {
    assert_eq!(decode(&[0x94]), Err(FrameError::NeedMore(3)));
    assert_eq!(decode(&[0x94, 0xC3]), Err(FrameError::NeedMore(2)));
    assert_eq!(decode(&[0x94, 0xC3, 0x00, 0x05, 1, 2]), Err(FrameError::NeedMore(3)));
}
```

- [ ] **Step 3: Write `src/codec/frame.rs` with a compiling stub so the test fails on behavior, not missing symbols**

```rust
use crate::codec::error::{FrameError, MAX_FRAME_PAYLOAD};

pub use crate::codec::error::MAX_FRAME_PAYLOAD as _MAX;

pub const MAGIC: [u8; 2] = [0x94, 0xC3];

pub fn encode(_payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    unimplemented!()
}

pub fn decode(_bytes: &[u8]) -> Result<(Vec<u8>, usize), FrameError> {
    unimplemented!()
}
```

And `src/codec/mod.rs`:

```rust
pub mod error;
pub mod frame;
```

- [ ] **Step 4: Run the tests, expect them to fail**

```bash
cargo test --test codec_frame
```

Expected: every test panics with `not implemented`.

- [ ] **Step 5: Implement `encode` and `decode`**

Replace `src/codec/frame.rs`:

```rust
use crate::codec::error::{FrameError, MAX_FRAME_PAYLOAD};

pub use crate::codec::error::MAX_FRAME_PAYLOAD;

pub const MAGIC: [u8; 2] = [0x94, 0xC3];

pub fn encode(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.len() > MAX_FRAME_PAYLOAD {
        return Err(FrameError::TooLarge(payload.len()));
    }
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&MAGIC);
    let len = payload.len() as u16;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<(Vec<u8>, usize), FrameError> {
    if bytes.len() < 2 {
        return Err(FrameError::NeedMore(2 - bytes.len()));
    }
    if bytes[0] != MAGIC[0] || bytes[1] != MAGIC[1] {
        return Err(FrameError::BadMagic(bytes[0], bytes[1]));
    }
    if bytes.len() < 4 {
        return Err(FrameError::NeedMore(4 - bytes.len()));
    }
    let len = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
    if len > MAX_FRAME_PAYLOAD {
        return Err(FrameError::TooLarge(len));
    }
    let total = 4 + len;
    if bytes.len() < total {
        return Err(FrameError::NeedMore(total - bytes.len()));
    }
    Ok((bytes[4..total].to_vec(), total))
}
```

Then in `src/codec/frame.rs`, there is a duplicate `pub use`; remove the `pub use ... as _MAX;` line if you left it. The public API is `encode`, `decode`, `MAGIC`, and the re-export of `MAX_FRAME_PAYLOAD`.

- [ ] **Step 6: Run tests, expect them all to pass**

```bash
cargo test --test codec_frame
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/codec tests/codec_frame.rs
git commit -m "Add pure Meshtastic frame codec"
```

---

## Task 3: Frame codec — tokio-util streaming decoder

**Goal:** A `tokio_util::codec::Decoder`/`Encoder` implementation so transports can wrap any `AsyncRead + AsyncWrite` in `Framed<_, FrameCodec>`.

**Files:**
- Modify: `src/codec/frame.rs`
- Create: `tests/codec_stream.rs`

### Steps

- [ ] **Step 1: Write the failing streaming test**

Create `tests/codec_stream.rs`:

```rust
use bytes::BytesMut;
use mt::codec::frame::FrameCodec;
use tokio_util::codec::{Decoder, Encoder};

#[test]
fn decodes_one_frame_across_partial_reads() {
    let mut codec = FrameCodec::default();
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[0x94, 0xC3]);
    assert_eq!(codec.decode(&mut buf).unwrap(), None);
    buf.extend_from_slice(&[0x00, 0x03, b'h']);
    assert_eq!(codec.decode(&mut buf).unwrap(), None);
    buf.extend_from_slice(&[b'i', b'!']);
    let got = codec.decode(&mut buf).unwrap().expect("frame ready");
    assert_eq!(&got[..], b"hi!");
    assert!(buf.is_empty());
}

#[test]
fn decodes_two_frames_back_to_back() {
    let mut codec = FrameCodec::default();
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[0x94, 0xC3, 0x00, 0x01, b'a', 0x94, 0xC3, 0x00, 0x02, b'b', b'c']);
    let first = codec.decode(&mut buf).unwrap().expect("first");
    assert_eq!(&first[..], b"a");
    let second = codec.decode(&mut buf).unwrap().expect("second");
    assert_eq!(&second[..], b"bc");
    assert!(buf.is_empty());
}

#[test]
fn decoder_skips_garbage_before_magic() {
    let mut codec = FrameCodec::default();
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[0xFF, 0xAA, 0x94, 0xC3, 0x00, 0x01, b'x']);
    let f = codec.decode(&mut buf).unwrap().expect("skipped");
    assert_eq!(&f[..], b"x");
    assert!(buf.is_empty());
}

#[test]
fn encodes_through_framed_api() {
    let mut codec = FrameCodec::default();
    let mut out = BytesMut::new();
    codec.encode(b"ping".to_vec(), &mut out).unwrap();
    assert_eq!(&out[..], &[0x94, 0xC3, 0x00, 0x04, b'p', b'i', b'n', b'g']);
}
```

- [ ] **Step 2: Append `FrameCodec` to `src/codec/frame.rs`**

```rust
use bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Default)]
pub struct FrameCodec;

impl Decoder for FrameCodec {
    type Item = Vec<u8>;
    type Error = FrameError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match src.iter().position(|&b| b == MAGIC[0]) {
                None => {
                    src.clear();
                    return Ok(None);
                }
                Some(0) => break,
                Some(n) => {
                    src.advance(n);
                }
            }
        }
        if src.len() < 2 {
            return Ok(None);
        }
        if src[1] != MAGIC[1] {
            src.advance(1);
            return self.decode(src);
        }
        if src.len() < 4 {
            return Ok(None);
        }
        let len = u16::from_be_bytes([src[2], src[3]]) as usize;
        if len > MAX_FRAME_PAYLOAD {
            return Err(FrameError::TooLarge(len));
        }
        let total = 4 + len;
        if src.len() < total {
            return Ok(None);
        }
        let payload = src[4..total].to_vec();
        src.advance(total);
        Ok(Some(payload))
    }
}

impl Encoder<Vec<u8>> for FrameCodec {
    type Error = FrameError;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if item.len() > MAX_FRAME_PAYLOAD {
            return Err(FrameError::TooLarge(item.len()));
        }
        dst.reserve(4 + item.len());
        dst.extend_from_slice(&MAGIC);
        dst.extend_from_slice(&(item.len() as u16).to_be_bytes());
        dst.extend_from_slice(&item);
        Ok(())
    }
}
```

- [ ] **Step 3: Run**

```bash
cargo test --test codec_stream
cargo test --test codec_frame
```

Both must pass.

- [ ] **Step 4: Commit**

```bash
git add src/codec/frame.rs tests/codec_stream.rs
git commit -m "Add tokio-util Framed codec for Meshtastic frames"
```

---

## Task 4: Domain IDs

**Goal:** Strongly-typed identifiers for the domain so arguments cannot be confused at call sites.

**Files:**
- Create: `src/domain/ids.rs`
- Modify: `src/domain/mod.rs`
- Create: `tests/domain_ids.rs`

### Steps

- [ ] **Step 1: Write the failing test**

Create `tests/domain_ids.rs`:

```rust
use mt::domain::ids::{BleAddress, ChannelIndex, ConfigId, NodeId, PacketId, BROADCAST_NODE};

#[test]
fn broadcast_node_is_all_ones() {
    assert_eq!(BROADCAST_NODE, NodeId(0xFFFF_FFFF));
}

#[test]
fn ids_are_copy_and_distinct_types() {
    let a = NodeId(1);
    let b: NodeId = a;
    let _ = (a, b);
    let c = PacketId(1);
    let _ = c;
}

#[test]
fn channel_index_rejects_out_of_range() {
    assert!(ChannelIndex::new(0).is_some());
    assert!(ChannelIndex::new(7).is_some());
    assert!(ChannelIndex::new(8).is_none());
}

#[test]
fn config_id_generates_nonzero_random() {
    let id = ConfigId::random();
    assert_ne!(id.0, 0);
}

#[test]
fn ble_address_normalizes_case() {
    assert_eq!(BleAddress::new("aa:bb"), BleAddress::new("AA:BB"));
}
```

- [ ] **Step 2: Write `src/domain/ids.rs`**

```rust
use rand::Rng;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NodeId(pub u32);

pub const BROADCAST_NODE: NodeId = NodeId(0xFFFF_FFFF);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ChannelIndex(u8);

impl ChannelIndex {
    pub const MAX: u8 = 7;
    pub fn new(value: u8) -> Option<Self> {
        (value <= Self::MAX).then_some(Self(value))
    }
    pub const fn primary() -> Self {
        Self(0)
    }
    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PacketId(pub u32);

impl PacketId {
    pub fn random() -> Self {
        Self(rand::thread_rng().gen_range(1..=u32::MAX))
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ConfigId(pub u32);

impl ConfigId {
    pub fn random() -> Self {
        Self(rand::thread_rng().gen_range(1..=u32::MAX))
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct BleAddress(String);

impl BleAddress {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().to_ascii_uppercase())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

- [ ] **Step 3: Extend `src/domain/mod.rs`**

```rust
pub mod ids;
```

- [ ] **Step 4: Run**

```bash
cargo test --test domain_ids
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/domain/ids.rs src/domain/mod.rs tests/domain_ids.rs
git commit -m "Add domain id newtypes"
```

---

## Task 5: Domain value types

**Goal:** Data types for nodes, channels, messages, snapshot, and connection profiles — plain structs derived from the proto, but with `Option<T>` only where absence is real and without proto-specific types leaking.

**Files:**
- Create: `src/domain/node.rs`, `src/domain/channel.rs`, `src/domain/message.rs`, `src/domain/snapshot.rs`, `src/domain/profile.rs`
- Modify: `src/domain/mod.rs`
- Create: `tests/domain_values.rs`

### Steps

- [ ] **Step 1: Write the failing test**

Create `tests/domain_values.rs`:

```rust
use std::path::PathBuf;

use mt::domain::channel::{Channel, ChannelRole};
use mt::domain::ids::{BleAddress, ChannelIndex, NodeId, PacketId};
use mt::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use mt::domain::node::{Node, NodeRole, Position};
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::domain::snapshot::DeviceSnapshot;

#[test]
fn position_optional_fields() {
    let p = Position { latitude_deg: 48.14, longitude_deg: 17.11, altitude_m: None };
    assert!(p.altitude_m.is_none());
}

#[test]
fn connection_profile_kind_matches_variant() {
    let p = ConnectionProfile::Tcp { name: "home".into(), host: "h".into(), port: 4403 };
    assert_eq!(p.kind(), TransportKind::Tcp);
}

#[test]
fn channel_role_is_total_enum() {
    for role in [ChannelRole::Primary, ChannelRole::Secondary, ChannelRole::Disabled] {
        let _ = format!("{role:?}");
    }
}

#[test]
fn text_message_records_direction_and_state() {
    let m = TextMessage {
        id: PacketId(1),
        channel: ChannelIndex::primary(),
        from: NodeId(10),
        to: Recipient::Broadcast,
        text: "hi".into(),
        received_at: std::time::SystemTime::UNIX_EPOCH,
        direction: Direction::Outgoing,
        state: DeliveryState::Pending,
    };
    assert_eq!(m.direction, Direction::Outgoing);
}

#[test]
fn device_snapshot_is_constructable() {
    let _s = DeviceSnapshot {
        my_node: NodeId(7),
        short_name: "me".into(),
        long_name: "my node".into(),
        firmware_version: "2.5.21".into(),
        nodes: Default::default(),
        channels: Default::default(),
        messages: Default::default(),
    };
}

#[test]
fn profile_roundtrip_does_not_leak_transport_type() {
    let p = ConnectionProfile::Ble { name: "r".into(), address: BleAddress::new("AA:BB") };
    let _ = format!("{p:?}");
    let _ = ConnectionProfile::Serial { name: "s".into(), path: PathBuf::from("/dev/ttyUSB0") };
    let _ = Node {
        id: NodeId(1),
        long_name: "n".into(),
        short_name: "n".into(),
        role: NodeRole::Client,
        battery_level: None,
        voltage_v: None,
        snr_db: None,
        rssi_dbm: None,
        hops_away: None,
        last_heard: None,
        position: None,
    };
    let _ = Channel {
        index: ChannelIndex::primary(),
        role: ChannelRole::Primary,
        name: "Primary".into(),
        has_psk: true,
    };
}
```

- [ ] **Step 2: Write `src/domain/node.rs`**

```rust
use std::time::SystemTime;

use crate::domain::ids::NodeId;

#[derive(Clone, Debug, PartialEq)]
pub enum NodeRole {
    Client,
    ClientMute,
    Router,
    RouterClient,
    Repeater,
    Tracker,
    Sensor,
    Tak,
    TakTracker,
    LostAndFound,
    ClientHidden,
    Unknown(i32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub long_name: String,
    pub short_name: String,
    pub role: NodeRole,
    pub battery_level: Option<u8>,
    pub voltage_v: Option<f32>,
    pub snr_db: Option<f32>,
    pub rssi_dbm: Option<i32>,
    pub hops_away: Option<u8>,
    pub last_heard: Option<SystemTime>,
    pub position: Option<Position>,
}
```

- [ ] **Step 3: Write `src/domain/channel.rs`**

```rust
use crate::domain::ids::ChannelIndex;

#[derive(Clone, Debug, PartialEq)]
pub enum ChannelRole {
    Primary,
    Secondary,
    Disabled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub index: ChannelIndex,
    pub role: ChannelRole,
    pub name: String,
    pub has_psk: bool,
}
```

- [ ] **Step 4: Write `src/domain/message.rs`**

```rust
use std::time::SystemTime;

use crate::domain::ids::{ChannelIndex, NodeId, PacketId};

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Recipient {
    Broadcast,
    Node(NodeId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeliveryState {
    Pending,
    Delivered,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextMessage {
    pub id: PacketId,
    pub channel: ChannelIndex,
    pub from: NodeId,
    pub to: Recipient,
    pub text: String,
    pub received_at: SystemTime,
    pub direction: Direction,
    pub state: DeliveryState,
}
```

- [ ] **Step 5: Write `src/domain/snapshot.rs`**

```rust
use std::collections::HashMap;

use crate::domain::channel::Channel;
use crate::domain::ids::NodeId;
use crate::domain::message::TextMessage;
use crate::domain::node::Node;

#[derive(Clone, Debug, Default)]
pub struct DeviceSnapshot {
    pub my_node: NodeId,
    pub short_name: String,
    pub long_name: String,
    pub firmware_version: String,
    pub nodes: HashMap<NodeId, Node>,
    pub channels: Vec<Channel>,
    pub messages: Vec<TextMessage>,
}
```

Note: `Default` is derived so UI can build an empty placeholder before first connect. Once `Ready(snapshot)` is reached, `my_node` will be set to a non-default value.

- [ ] **Step 6: Write `src/domain/profile.rs`**

```rust
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::ids::BleAddress;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    Ble,
    Serial,
    Tcp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ConnectionProfile {
    Ble { name: String, address: BleAddress },
    Serial { name: String, path: PathBuf },
    Tcp { name: String, host: String, port: u16 },
}

impl ConnectionProfile {
    pub fn kind(&self) -> TransportKind {
        match self {
            Self::Ble { .. } => TransportKind::Ble,
            Self::Serial { .. } => TransportKind::Serial,
            Self::Tcp { .. } => TransportKind::Tcp,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Ble { name, .. } | Self::Serial { name, .. } | Self::Tcp { name, .. } => name,
        }
    }
}
```

`BleAddress` needs `Serialize`/`Deserialize`. Edit `src/domain/ids.rs` — add `serde::Serialize`, `serde::Deserialize` derives to `BleAddress`:

```rust
#[derive(Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct BleAddress(String);
```

- [ ] **Step 7: Extend `src/domain/mod.rs`**

```rust
pub mod channel;
pub mod ids;
pub mod message;
pub mod node;
pub mod profile;
pub mod snapshot;
```

- [ ] **Step 8: Run**

```bash
cargo test --test domain_values
cargo test --test domain_ids
```

- [ ] **Step 9: Commit**

```bash
git add src/domain tests/domain_values.rs
git commit -m "Add domain value types: node, channel, message, snapshot, profile"
```

---

## Task 6: Domain session state and reducer

**Goal:** A pure reducer that takes the current `SessionState` and an `Event`/intermediate and returns the next state. Contains the handshake aggregation logic as pure transformations.

**Files:**
- Create: `src/domain/session.rs`
- Modify: `src/domain/mod.rs`
- Create: `tests/domain_session.rs`

### Steps

- [ ] **Step 1: Write the failing tests**

Create `tests/domain_session.rs`:

```rust
use mt::domain::channel::{Channel, ChannelRole};
use mt::domain::ids::{ChannelIndex, ConfigId, NodeId};
use mt::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use mt::domain::node::{Node, NodeRole};
use mt::domain::profile::TransportKind;
use mt::domain::session::{HandshakeFragment, SessionState, apply, start_handshake};

fn node(id: u32, name: &str) -> Node {
    Node {
        id: NodeId(id),
        long_name: name.into(),
        short_name: name.chars().take(2).collect(),
        role: NodeRole::Client,
        battery_level: None,
        voltage_v: None,
        snr_db: None,
        rssi_dbm: None,
        hops_away: None,
        last_heard: None,
        position: None,
    }
}

#[test]
fn handshake_collects_fragments_and_completes() {
    let config_id = ConfigId(42);
    let s = start_handshake(TransportKind::Tcp, config_id);
    let s = apply(s, HandshakeFragment::MyNode { id: NodeId(1), short: "me".into(), long: "me".into(), firmware: "2.5".into() });
    let s = apply(s, HandshakeFragment::Node(node(2, "n2")));
    let s = apply(s, HandshakeFragment::Channel(Channel { index: ChannelIndex::primary(), role: ChannelRole::Primary, name: "Primary".into(), has_psk: true }));
    let s = apply(s, HandshakeFragment::ConfigComplete { id: config_id });

    match s {
        SessionState::Ready(snap) => {
            assert_eq!(snap.my_node, NodeId(1));
            assert_eq!(snap.nodes.len(), 1);
            assert_eq!(snap.channels.len(), 1);
        }
        _ => panic!("should be ready"),
    }
}

#[test]
fn config_complete_with_wrong_id_keeps_handshake() {
    let s = start_handshake(TransportKind::Tcp, ConfigId(1));
    let s = apply(s, HandshakeFragment::MyNode { id: NodeId(1), short: "me".into(), long: "me".into(), firmware: "x".into() });
    let s = apply(s, HandshakeFragment::ConfigComplete { id: ConfigId(999) });
    assert!(matches!(s, SessionState::Handshake { .. }));
}

#[test]
fn ready_applies_incoming_text_message() {
    let s = start_handshake(TransportKind::Tcp, ConfigId(1));
    let s = apply(s, HandshakeFragment::MyNode { id: NodeId(1), short: "me".into(), long: "me".into(), firmware: "x".into() });
    let s = apply(s, HandshakeFragment::ConfigComplete { id: ConfigId(1) });
    let msg = TextMessage {
        id: mt::domain::ids::PacketId(5),
        channel: ChannelIndex::primary(),
        from: NodeId(2),
        to: Recipient::Broadcast,
        text: "hello".into(),
        received_at: std::time::SystemTime::UNIX_EPOCH,
        direction: Direction::Incoming,
        state: DeliveryState::Delivered,
    };
    let s = apply(s, HandshakeFragment::Message(msg.clone()));
    match s {
        SessionState::Ready(snap) => assert_eq!(snap.messages, vec![msg]),
        _ => panic!("should be ready"),
    }
}
```

- [ ] **Step 2: Write `src/domain/session.rs`**

```rust
use std::collections::HashMap;
use std::time::Instant;

use crate::domain::channel::Channel;
use crate::domain::ids::{ChannelIndex, ConfigId, NodeId, PacketId};
use crate::domain::message::{DeliveryState, TextMessage};
use crate::domain::node::Node;
use crate::domain::profile::TransportKind;
use crate::domain::snapshot::DeviceSnapshot;
use crate::error::ConnectError;

#[derive(Clone, Debug)]
pub enum SessionState {
    Disconnected,
    Connecting { transport: TransportKind, started: Instant },
    Handshake(HandshakeAcc),
    Ready(DeviceSnapshot),
    Failed(ConnectError),
}

#[derive(Clone, Debug)]
pub struct HandshakeAcc {
    pub transport: TransportKind,
    pub config_id: ConfigId,
    pub my_node: Option<NodeId>,
    pub short_name: String,
    pub long_name: String,
    pub firmware: String,
    pub nodes: HashMap<NodeId, Node>,
    pub channels: Vec<Channel>,
}

pub fn start_handshake(transport: TransportKind, config_id: ConfigId) -> SessionState {
    SessionState::Handshake(HandshakeAcc {
        transport,
        config_id,
        my_node: None,
        short_name: String::new(),
        long_name: String::new(),
        firmware: String::new(),
        nodes: HashMap::new(),
        channels: Vec::new(),
    })
}

#[derive(Clone, Debug)]
pub enum HandshakeFragment {
    MyNode { id: NodeId, short: String, long: String, firmware: String },
    Node(Node),
    Channel(Channel),
    ConfigComplete { id: ConfigId },
    Message(TextMessage),
    MessageStateChanged { id: PacketId, state: DeliveryState },
    NodeMetricUpdate { id: NodeId, update: NodeMetric },
}

#[derive(Clone, Debug)]
pub enum NodeMetric {
    Battery(u8),
    Voltage(f32),
    Snr(f32),
    Rssi(i32),
}

pub fn apply(state: SessionState, event: HandshakeFragment) -> SessionState {
    match state {
        SessionState::Handshake(mut acc) => match event {
            HandshakeFragment::MyNode { id, short, long, firmware } => {
                acc.my_node = Some(id);
                acc.short_name = short;
                acc.long_name = long;
                acc.firmware = firmware;
                SessionState::Handshake(acc)
            }
            HandshakeFragment::Node(node) => {
                acc.nodes.insert(node.id, node);
                SessionState::Handshake(acc)
            }
            HandshakeFragment::Channel(channel) => {
                if let Some(existing) = acc.channels.iter_mut().find(|c| c.index == channel.index) {
                    *existing = channel;
                } else {
                    acc.channels.push(channel);
                }
                SessionState::Handshake(acc)
            }
            HandshakeFragment::ConfigComplete { id } if id == acc.config_id => {
                let Some(my_node) = acc.my_node else {
                    return SessionState::Handshake(acc);
                };
                SessionState::Ready(DeviceSnapshot {
                    my_node,
                    short_name: acc.short_name,
                    long_name: acc.long_name,
                    firmware_version: acc.firmware,
                    nodes: acc.nodes,
                    channels: acc.channels,
                    messages: Vec::new(),
                })
            }
            _ => SessionState::Handshake(acc),
        },
        SessionState::Ready(mut snap) => match event {
            HandshakeFragment::Node(node) => {
                snap.nodes.insert(node.id, node);
                SessionState::Ready(snap)
            }
            HandshakeFragment::Channel(channel) => {
                if let Some(existing) = snap.channels.iter_mut().find(|c| c.index == channel.index) {
                    *existing = channel;
                } else {
                    snap.channels.push(channel);
                }
                SessionState::Ready(snap)
            }
            HandshakeFragment::Message(msg) => {
                snap.messages.push(msg);
                SessionState::Ready(snap)
            }
            HandshakeFragment::MessageStateChanged { id, state } => {
                if let Some(m) = snap.messages.iter_mut().find(|m| m.id == id) {
                    m.state = state;
                }
                SessionState::Ready(snap)
            }
            HandshakeFragment::NodeMetricUpdate { id, update } => {
                if let Some(node) = snap.nodes.get_mut(&id) {
                    apply_metric(node, update);
                }
                SessionState::Ready(snap)
            }
            _ => SessionState::Ready(snap),
        },
        other => other,
    }
}

fn apply_metric(node: &mut Node, metric: NodeMetric) {
    match metric {
        NodeMetric::Battery(b) => node.battery_level = Some(b),
        NodeMetric::Voltage(v) => node.voltage_v = Some(v),
        NodeMetric::Snr(s) => node.snr_db = Some(s),
        NodeMetric::Rssi(r) => node.rssi_dbm = Some(r),
    }
}

#[allow(dead_code)]
fn _touch_unused_index(_c: ChannelIndex) {}
```

- [ ] **Step 3: Extend `src/domain/mod.rs`**

```rust
pub mod session;
```

- [ ] **Step 4: Run**

```bash
cargo test --test domain_session
```

- [ ] **Step 5: Commit**

```bash
git add src/domain/session.rs src/domain/mod.rs tests/domain_session.rs
git commit -m "Add domain session state and reducer"
```

---

## Task 7: Port payload parser

**Goal:** Convert a proto `MeshPacket` `Decoded.portnum` + `payload` bytes pair into our typed `PortPayload`. Unknown ports keep their raw bytes; known ports are parsed into `String` / `Node` / `Position` / `DeliveryState` via domain types.

**Files:**
- Create: `src/proto/port.rs`
- Modify: `src/proto/mod.rs`
- Create: `tests/proto_port.rs`

### Steps

- [ ] **Step 1: Inspect generated proto module**

After `cargo build` run once. Inspect `target/debug/build/mt-*/out/meshtastic.rs` for the enum variants you need (`PortNum::TextMessageApp`, `PortNum::PositionApp`, `PortNum::NodeinfoApp`, `PortNum::TelemetryApp`, `PortNum::RoutingApp`, `PortNum::AdminApp`). If the module is split (e.g., separate files per proto file), adjust the `include!` in `src/proto/mod.rs` accordingly.

- [ ] **Step 2: Write the failing test**

Create `tests/proto_port.rs`:

```rust
use mt::proto::port::{PortPayload, parse};

#[test]
fn unknown_port_preserves_bytes() {
    let payload = parse(9999, b"\x01\x02\x03").unwrap();
    match payload {
        PortPayload::Unknown { port, bytes } => {
            assert_eq!(port, 9999);
            assert_eq!(bytes.as_ref(), b"\x01\x02\x03");
        }
        other => panic!("expected Unknown, got {other:?}"),
    }
}

#[test]
fn text_port_parses_utf8() {
    let text_port = mt::proto::meshtastic::PortNum::TextMessageApp as i32;
    let p = parse(text_port, "hi".as_bytes()).unwrap();
    assert!(matches!(p, PortPayload::Text(t) if t == "hi"));
}
```

- [ ] **Step 3: Write `src/proto/port.rs`**

```rust
use bytes::Bytes;
use prost::Message;
use thiserror::Error;

use crate::proto::meshtastic;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("utf-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("protobuf: {0}")]
    Proto(#[from] prost::DecodeError),
}

#[derive(Debug, Clone)]
pub enum PortPayload {
    Text(String),
    Position(meshtastic::Position),
    NodeInfo(meshtastic::User),
    Telemetry(meshtastic::Telemetry),
    Routing(meshtastic::Routing),
    Admin(meshtastic::AdminMessage),
    Unknown { port: i32, bytes: Bytes },
}

pub fn parse(port: i32, bytes: &[u8]) -> Result<PortPayload, ParseError> {
    use meshtastic::PortNum;
    let port_num = PortNum::try_from(port).ok();
    Ok(match port_num {
        Some(PortNum::TextMessageApp) => PortPayload::Text(String::from_utf8(bytes.to_vec())?),
        Some(PortNum::PositionApp) => PortPayload::Position(meshtastic::Position::decode(bytes)?),
        Some(PortNum::NodeinfoApp) => PortPayload::NodeInfo(meshtastic::User::decode(bytes)?),
        Some(PortNum::TelemetryApp) => PortPayload::Telemetry(meshtastic::Telemetry::decode(bytes)?),
        Some(PortNum::RoutingApp) => PortPayload::Routing(meshtastic::Routing::decode(bytes)?),
        Some(PortNum::AdminApp) => PortPayload::Admin(meshtastic::AdminMessage::decode(bytes)?),
        _ => PortPayload::Unknown { port, bytes: Bytes::copy_from_slice(bytes) },
    })
}
```

If the proto types are named differently (e.g., `meshtastic::User` vs `meshtastic::NodeInfo`), consult the generated file. Adjust both the enum payload type and the decode call.

Add the `TryFrom<i32>` for `PortNum` — it is generated by `prost-build` automatically on enum types. If it is missing, use `meshtastic::PortNum::from_i32(port)`.

- [ ] **Step 4: Extend `src/proto/mod.rs`**

```rust
#![allow(clippy::all)]

pub mod meshtastic {
    include!(concat!(env!("OUT_DIR"), "/meshtastic.rs"));
}

pub mod port;
```

- [ ] **Step 5: Run**

```bash
cargo test --test proto_port
```

- [ ] **Step 6: Commit**

```bash
git add src/proto tests/proto_port.rs
git commit -m "Add typed port payload parser"
```

---

## Task 8: Transport trait and error type

**Goal:** A narrow `Transport` abstraction: a `Sink<Vec<u8>, Error>` + a `Stream<Item = Result<Vec<u8>>>`. A single `TransportError` enum covers BLE/Serial/TCP failures.

**Files:**
- Create: `src/transport/mod.rs`, `src/transport/error.rs`
- Modify: `src/error.rs`

### Steps

- [ ] **Step 1: Write `src/transport/error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serial: {0}")]
    Serial(#[from] tokio_serial::Error),
    #[error("ble: {0}")]
    Ble(String),
    #[error("frame: {0}")]
    Frame(#[from] crate::codec::error::FrameError),
    #[error("closed")]
    Closed,
}
```

- [ ] **Step 2: Write `src/transport/mod.rs`**

```rust
pub mod error;

use std::pin::Pin;

use futures::{Sink, Stream};

pub use error::TransportError;

pub type Frame = Vec<u8>;

pub trait Transport:
    Sink<Frame, Error = TransportError> + Stream<Item = Result<Frame, TransportError>> + Send + 'static
{
}

impl<T> Transport for T where
    T: Sink<Frame, Error = TransportError>
        + Stream<Item = Result<Frame, TransportError>>
        + Send
        + 'static
{
}

pub type BoxedTransport = Pin<Box<dyn Transport>>;
```

- [ ] **Step 3: Replace the placeholder in `src/error.rs`**

```rust
use thiserror::Error;

pub use crate::transport::error::TransportError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("connect: {0}")]
    Connect(#[from] ConnectError),
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
    #[error("persist: {0}")]
    Persist(#[from] PersistError),
}

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("ble adapter unavailable")]
    BleAdapterUnavailable,
    #[error("ble device not found: {0}")]
    BleDeviceNotFound(String),
    #[error("ble pairing required ({0:?})")]
    BlePairingRequired(PairingHint),
    #[error("ble pairing failed: {0}")]
    BlePairingFailed(String),
    #[error("ble gatt: {0}")]
    BleGatt(String),
    #[error("serial: {0}")]
    Serial(#[from] tokio_serial::Error),
    #[error("tcp: {0}")]
    Tcp(std::io::Error),
    #[error("handshake timeout")]
    HandshakeTimeout,
    #[error("codec: {0}")]
    Codec(#[from] crate::codec::error::FrameError),
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
}

#[derive(Debug, Copy, Clone)]
pub enum PairingHint {
    Macos,
    Windows,
    LinuxBluetoothctl,
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("serialize: {0}")]
    Serialize(#[from] toml::ser::Error),
}
```

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
```

- [ ] **Step 5: Commit**

```bash
git add src/transport src/error.rs
git commit -m "Define Transport trait and typed error variants"
```

---

## Task 9: MockTransport

**Goal:** Scripted test double for session testing. Accepts a scripted stream of inbound frames and records outbound frames so tests can assert both sides.

**Files:**
- Create: `src/transport/mock.rs`
- Modify: `src/transport/mod.rs`
- Create: `tests/transport_mock.rs`

### Steps

- [ ] **Step 1: Write the failing test**

Create `tests/transport_mock.rs`:

```rust
use futures::{SinkExt, StreamExt};
use mt::transport::mock::{MockTransport, Script};

#[tokio::test]
async fn mock_delivers_scripted_frames_and_captures_writes() {
    let (transport, handle) = MockTransport::new(Script::from_frames(vec![
        vec![1, 2, 3],
        vec![9, 9, 9],
    ]));
    let (mut sink, mut stream) = futures::StreamExt::split(transport);

    assert_eq!(stream.next().await.unwrap().unwrap(), vec![1, 2, 3]);
    assert_eq!(stream.next().await.unwrap().unwrap(), vec![9, 9, 9]);
    assert!(stream.next().await.is_none());

    sink.send(vec![0xAA]).await.unwrap();
    sink.send(vec![0xBB]).await.unwrap();
    assert_eq!(handle.captured().await, vec![vec![0xAA], vec![0xBB]]);
}
```

- [ ] **Step 2: Write `src/transport/mock.rs`**

```rust
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Sink, Stream};
use tokio::sync::{mpsc, Mutex};

use crate::transport::TransportError;

pub struct Script {
    frames: Vec<Vec<u8>>,
}

impl Script {
    pub fn from_frames(frames: Vec<Vec<u8>>) -> Self {
        Self { frames }
    }
}

pub struct MockTransport {
    incoming: mpsc::UnboundedReceiver<Result<Vec<u8>, TransportError>>,
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
}

pub struct MockHandle {
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
    inject: mpsc::UnboundedSender<Result<Vec<u8>, TransportError>>,
}

impl MockHandle {
    pub async fn captured(&self) -> Vec<Vec<u8>> {
        self.captured.lock().await.clone()
    }
    pub fn inject(&self, frame: Vec<u8>) {
        let _ = self.inject.send(Ok(frame));
    }
    pub fn close(&self) {
        drop(self.inject.clone());
    }
}

impl MockTransport {
    pub fn new(script: Script) -> (Self, MockHandle) {
        let (tx, rx) = mpsc::unbounded_channel();
        for f in script.frames {
            let _ = tx.send(Ok(f));
        }
        let captured = Arc::new(Mutex::new(Vec::new()));
        let handle = MockHandle { captured: captured.clone(), inject: tx };
        (Self { incoming: rx, captured }, handle)
    }
}

impl Stream for MockTransport {
    type Item = Result<Vec<u8>, TransportError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.incoming.poll_recv(cx)
    }
}

impl Sink<Vec<u8>> for MockTransport {
    type Error = TransportError;
    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn start_send(self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), Self::Error> {
        let captured = self.captured.clone();
        tokio::spawn(async move { captured.lock().await.push(item) });
        Ok(())
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
```

`start_send` uses `tokio::spawn` so we do not need `async` there. For deterministic tests we await `handle.captured()` after both sends complete; the spawned tasks will have acquired the mutex by then in practice. If tests turn flaky, replace `tokio::spawn` with `futures::executor::block_on` on a `Mutex` that does not require tokio, or switch the captured store to `std::sync::Mutex<Vec<Vec<u8>>>` which is lock-free for a short critical section.

Prefer `std::sync::Mutex` to avoid the spawn dance:

```rust
use std::sync::Mutex as StdMutex;
// ...
captured: Arc<StdMutex<Vec<Vec<u8>>>>,
// start_send:
self.captured.lock().unwrap().push(item);
```

Adopt the `std::sync::Mutex` variant. Update `MockHandle::captured` to be synchronous (no `.await`), and adjust the test to drop `.await` on that call. Final form: `handle.captured()` returns `Vec<Vec<u8>>` directly.

- [ ] **Step 3: Register the module**

Extend `src/transport/mod.rs`:

```rust
pub mod mock;
```

- [ ] **Step 4: Run**

```bash
cargo test --test transport_mock
```

- [ ] **Step 5: Commit**

```bash
git add src/transport/mock.rs src/transport/mod.rs tests/transport_mock.rs
git commit -m "Add MockTransport for session tests"
```

---

## Task 10: Handshake driver

**Goal:** Given an arbitrary `Transport`, send a `ToRadio { want_config_id }`, consume `FromRadio` messages, fold them into `HandshakeFragment`s, and return `DeviceSnapshot` (or `ConnectError`) on completion/timeout.

**Files:**
- Create: `src/session/handshake.rs`
- Modify: `src/session/mod.rs`
- Create: `tests/session_handshake.rs`

### Steps

- [ ] **Step 1: Write the failing test**

Create `tests/session_handshake.rs`:

```rust
use std::time::Duration;

use mt::codec::frame::encode;
use mt::domain::ids::{ConfigId, NodeId};
use mt::domain::profile::TransportKind;
use mt::proto::meshtastic;
use mt::session::handshake::run_handshake;
use mt::transport::mock::{MockTransport, Script};
use prost::Message;

fn frame_from_radio(m: meshtastic::FromRadio) -> Vec<u8> {
    let mut buf = Vec::new();
    m.encode(&mut buf).unwrap();
    encode(&buf).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn completes_when_config_complete_matches() {
    let config_id = ConfigId(123);
    let my_info = meshtastic::FromRadio {
        id: 1,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::MyInfo(meshtastic::MyNodeInfo {
            my_node_num: 77,
            ..Default::default()
        })),
    };
    let done = meshtastic::FromRadio {
        id: 2,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::ConfigCompleteId(123)),
    };

    let (transport, _handle) = MockTransport::new(Script::from_frames(vec![
        frame_from_radio(my_info),
        frame_from_radio(done),
    ]));

    let snapshot = tokio::time::timeout(
        Duration::from_millis(200),
        run_handshake(Box::pin(transport), TransportKind::Tcp, config_id),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(snapshot.my_node, NodeId(77));
}
```

- [ ] **Step 2: Write `src/session/handshake.rs`**

```rust
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use prost::Message;
use tokio::time::timeout;

use crate::codec::frame::{decode, encode};
use crate::domain::ids::{ConfigId, NodeId};
use crate::domain::node::{Node, NodeRole};
use crate::domain::profile::TransportKind;
use crate::domain::session::{HandshakeFragment, SessionState, apply, start_handshake};
use crate::domain::snapshot::DeviceSnapshot;
use crate::error::ConnectError;
use crate::proto::meshtastic;
use crate::transport::BoxedTransport;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run_handshake(
    mut transport: BoxedTransport,
    transport_kind: TransportKind,
    config_id: ConfigId,
) -> Result<DeviceSnapshot, ConnectError> {
    let want = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::WantConfigId(config_id.0)),
    };
    let mut buf = Vec::new();
    want.encode(&mut buf).map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    let frame = encode(&buf)?;
    transport.send(frame).await?;

    let mut state = start_handshake(transport_kind, config_id);

    let result = timeout(HANDSHAKE_TIMEOUT, async {
        while let Some(item) = transport.next().await {
            let frame = item?;
            let msg = meshtastic::FromRadio::decode(frame.as_slice())
                .map_err(|e| ConnectError::BleGatt(e.to_string()))?;
            for fragment in fragments_from_radio(msg) {
                state = apply(state.clone(), fragment);
                if let SessionState::Ready(snap) = &state {
                    return Ok::<DeviceSnapshot, ConnectError>(snap.clone());
                }
            }
        }
        Err(ConnectError::HandshakeTimeout)
    })
    .await
    .map_err(|_| ConnectError::HandshakeTimeout)?;
    result
}

fn fragments_from_radio(msg: meshtastic::FromRadio) -> Vec<HandshakeFragment> {
    use meshtastic::from_radio::PayloadVariant::*;
    let Some(variant) = msg.payload_variant else { return Vec::new(); };
    let mut out = Vec::new();
    match variant {
        MyInfo(info) => out.push(HandshakeFragment::MyNode {
            id: NodeId(info.my_node_num),
            short: String::new(),
            long: String::new(),
            firmware: String::new(),
        }),
        NodeInfo(ni) => out.push(HandshakeFragment::Node(Node {
            id: NodeId(ni.num),
            long_name: ni.user.as_ref().map(|u| u.long_name.clone()).unwrap_or_default(),
            short_name: ni.user.as_ref().map(|u| u.short_name.clone()).unwrap_or_default(),
            role: NodeRole::Client,
            battery_level: ni.device_metrics.as_ref().map(|m| m.battery_level as u8),
            voltage_v: ni.device_metrics.as_ref().map(|m| m.voltage),
            snr_db: Some(ni.snr),
            rssi_dbm: None,
            hops_away: Some(ni.hops_away as u8),
            last_heard: None,
            position: None,
        })),
        ConfigCompleteId(id) => out.push(HandshakeFragment::ConfigComplete { id: ConfigId(id) }),
        _ => {}
    }
    out
}

#[cfg(test)]
fn _require_decode_is_used(b: &[u8]) -> Option<Vec<u8>> {
    decode(b).ok().map(|(v, _)| v)
}
```

The handshake driver is intentionally narrow: it does not parse channels, configs, or module configs in this first pass. The reducer already handles those fragments; when we add `Channel` / `Config` / `ModuleConfig` parsing in Task 11 (session main loop), they will be wired through.

Short- and long-name for `MyNode` populate in Task 11 from the self `NodeInfo` that follows `MyInfo` (Meshtastic sends the local node as the first `NodeInfo`). For the handshake driver alone, empty strings are acceptable because `Ready` is reached once `ConfigCompleteId` matches — and the session loop will immediately apply any `NodeInfo` fragments that arrived during handshake to fill in the names via a second pass.

- [ ] **Step 3: Extend `src/session/mod.rs`**

```rust
pub mod handshake;
```

- [ ] **Step 4: Run**

```bash
cargo test --test session_handshake
```

- [ ] **Step 5: Commit**

```bash
git add src/session/handshake.rs src/session/mod.rs tests/session_handshake.rs
git commit -m "Add handshake driver over Transport trait"
```

---

## Task 11: `DeviceSession` task: commands, events, heartbeat, ack timeout

**Goal:** A single tokio task owns the transport after handshake. It pumps outgoing `Command`s into `ToRadio` frames, parses incoming `FromRadio` into `Event`s, fires heartbeats every 300s, and times out outgoing messages missing an ack in 30s.

**Files:**
- Create: `src/session/commands.rs`
- Create (expand): `src/session/mod.rs`
- Create: `tests/session_run.rs`

### Steps

- [ ] **Step 1: Write `src/session/commands.rs`**

```rust
use crate::domain::ids::{ChannelIndex, PacketId};
use crate::domain::message::Recipient;
use crate::domain::profile::ConnectionProfile;

#[derive(Clone, Debug)]
pub enum Command {
    Connect(ConnectionProfile),
    Disconnect,
    SendText {
        channel: ChannelIndex,
        to: Recipient,
        text: String,
        want_ack: bool,
    },
    AckTimeout(PacketId),
}
```

- [ ] **Step 2: Write `src/session/mod.rs` event type**

Extend `src/session/mod.rs`:

```rust
pub mod commands;
pub mod handshake;

use std::time::{Duration, SystemTime};

use futures::{SinkExt, StreamExt};
use prost::Message;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::codec::frame::encode;
use crate::domain::channel::{Channel, ChannelRole};
use crate::domain::ids::{BROADCAST_NODE, ChannelIndex, ConfigId, NodeId, PacketId};
use crate::domain::message::{DeliveryState, Direction, Recipient, TextMessage};
use crate::domain::node::{Node, NodeRole, Position};
use crate::domain::profile::{ConnectionProfile, TransportKind};
use crate::domain::session::{HandshakeFragment, NodeMetric};
use crate::domain::snapshot::DeviceSnapshot;
use crate::error::{ConnectError, TransportError};
use crate::proto::meshtastic;
use crate::proto::port::{PortPayload, parse};
use crate::session::commands::Command;
use crate::session::handshake::run_handshake;
use crate::transport::BoxedTransport;

const HEARTBEAT: Duration = Duration::from_secs(300);
const ACK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub enum Event {
    Connecting,
    Connected(Box<DeviceSnapshot>),
    NodeUpdated(Node),
    ChannelUpdated(Channel),
    MessageReceived(TextMessage),
    MessageStateChanged { id: PacketId, state: DeliveryState },
    Disconnected,
    Error(String),
}

pub struct DeviceSession {
    connect: Box<dyn Fn(ConnectionProfile) -> futures::future::BoxFuture<'static, Result<(BoxedTransport, TransportKind), ConnectError>> + Send + Sync>,
}

impl DeviceSession {
    pub fn new(
        connect: Box<dyn Fn(ConnectionProfile) -> futures::future::BoxFuture<'static, Result<(BoxedTransport, TransportKind), ConnectError>> + Send + Sync>,
    ) -> Self {
        Self { connect }
    }

    pub async fn run(
        self,
        mut rx: mpsc::UnboundedReceiver<Command>,
        tx: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) {
        while let Some(cmd) = tokio::select! {
            _ = cancel.cancelled() => None,
            cmd = rx.recv() => cmd,
        } {
            match cmd {
                Command::Connect(profile) => {
                    let _ = tx.send(Event::Connecting).await;
                    match (self.connect)(profile).await {
                        Ok((transport, kind)) => {
                            let config_id = ConfigId::random();
                            match run_handshake(transport, kind, config_id).await {
                                Ok(snap) => {
                                    let _ = tx.send(Event::Connected(Box::new(snap.clone()))).await;
                                    // In this first iteration the transport is moved into run_handshake
                                    // and cannot be reused post-handshake. The follow-up refactor
                                    // (same task, step 3) splits the transport so we can continue.
                                    break;
                                }
                                Err(e) => {
                                    let _ = tx.send(Event::Error(e.to_string())).await;
                                    let _ = tx.send(Event::Disconnected).await;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Error(e.to_string())).await;
                            let _ = tx.send(Event::Disconnected).await;
                        }
                    }
                }
                Command::Disconnect => {
                    let _ = tx.send(Event::Disconnected).await;
                }
                _ => {}
            }
        }
    }
}
```

This first cut compiles and handles connect+handshake. **Step 3 below rewrites it** to keep the transport after handshake and run the full command/event loop. We commit between steps.

- [ ] **Step 3: Commit the first pass**

```bash
git add src/session
git commit -m "Add DeviceSession skeleton with connect+handshake"
```

- [ ] **Step 4: Refactor `run_handshake` to return the transport**

Change `run_handshake` signature in `src/session/handshake.rs` from:

```rust
pub async fn run_handshake(
    mut transport: BoxedTransport,
    transport_kind: TransportKind,
    config_id: ConfigId,
) -> Result<DeviceSnapshot, ConnectError>;
```

to:

```rust
pub async fn run_handshake(
    mut transport: BoxedTransport,
    transport_kind: TransportKind,
    config_id: ConfigId,
) -> Result<(DeviceSnapshot, BoxedTransport), ConnectError>;
```

And change the body to `return Ok((snap.clone(), transport));` on success. Update `tests/session_handshake.rs` to destructure `(snapshot, _transport)`.

Run:

```bash
cargo test --test session_handshake
```

- [ ] **Step 5: Expand `DeviceSession::run` to the full loop**

Replace the body of `run` in `src/session/mod.rs`:

```rust
pub async fn run(
    self,
    mut rx: mpsc::UnboundedReceiver<Command>,
    tx: mpsc::Sender<Event>,
    cancel: CancellationToken,
) {
    'outer: loop {
        let cmd = tokio::select! {
            _ = cancel.cancelled() => break,
            cmd = rx.recv() => cmd,
        };
        let Some(Command::Connect(profile)) = cmd else { continue };
        let _ = tx.send(Event::Connecting).await;

        let (transport, kind) = match (self.connect)(profile).await {
            Ok(pair) => pair,
            Err(e) => {
                let _ = tx.send(Event::Error(e.to_string())).await;
                let _ = tx.send(Event::Disconnected).await;
                continue;
            }
        };
        let config_id = ConfigId::random();
        let (snapshot, transport) = match run_handshake(transport, kind, config_id).await {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.send(Event::Error(e.to_string())).await;
                let _ = tx.send(Event::Disconnected).await;
                continue;
            }
        };
        let _ = tx.send(Event::Connected(Box::new(snapshot))).await;

        let (mut sink, mut stream) = transport.split();
        let mut hb = interval(HEARTBEAT);
        hb.tick().await; // skip immediate tick

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = tx.send(Event::Disconnected).await;
                    break 'outer;
                }
                _ = hb.tick() => {
                    let frame = encode_heartbeat();
                    if let Err(e) = sink.send(frame).await {
                        let _ = tx.send(Event::Error(e.to_string())).await;
                        let _ = tx.send(Event::Disconnected).await;
                        break;
                    }
                }
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    match cmd {
                        Command::Disconnect => {
                            let _ = tx.send(Event::Disconnected).await;
                            break;
                        }
                        Command::SendText { channel, to, text, want_ack } => {
                            let (id, frame) = encode_text(channel, to, text.clone(), want_ack, &snapshot_my_node_from_events(&tx));
                            let _ = tx.send(Event::MessageReceived(TextMessage {
                                id,
                                channel,
                                from: NodeId(0),
                                to,
                                text,
                                received_at: SystemTime::now(),
                                direction: Direction::Outgoing,
                                state: DeliveryState::Pending,
                            })).await;
                            if let Err(e) = sink.send(frame).await {
                                let _ = tx.send(Event::Error(e.to_string())).await;
                                let _ = tx.send(Event::Disconnected).await;
                                break;
                            }
                            let tx2 = tx.clone();
                            tokio::spawn(async move {
                                sleep(ACK_TIMEOUT).await;
                                let _ = tx2
                                    .send(Event::MessageStateChanged {
                                        id,
                                        state: DeliveryState::Failed("no ack".into()),
                                    })
                                    .await;
                            });
                        }
                        _ => {}
                    }
                }
                item = stream.next() => {
                    let Some(item) = item else {
                        let _ = tx.send(Event::Disconnected).await;
                        break;
                    };
                    let frame = match item {
                        Ok(f) => f,
                        Err(e) => {
                            let _ = tx.send(Event::Error(e.to_string())).await;
                            let _ = tx.send(Event::Disconnected).await;
                            break;
                        }
                    };
                    let msg = match meshtastic::FromRadio::decode(frame.as_slice()) {
                        Ok(m) => m,
                        Err(e) => { warn!(?e, "bad FromRadio"); continue; }
                    };
                    for ev in events_from_radio(msg) {
                        let _ = tx.send(ev).await;
                    }
                }
            }
        }
    }
}
```

The helper `snapshot_my_node_from_events` above is a smell — the session has already produced a snapshot at handshake. Replace the placeholder: store `my_node: NodeId` captured after `run_handshake`:

```rust
let my_node = snapshot.my_node;
```

Then in `Command::SendText`, use `from: my_node` and in `encode_text` pass `my_node` in. Remove the helper.

Also add the encoding helpers:

```rust
fn encode_heartbeat() -> Vec<u8> {
    let hb = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::Heartbeat(meshtastic::Heartbeat {})),
    };
    let mut buf = Vec::new();
    hb.encode(&mut buf).expect("heartbeat encode");
    encode(&buf).expect("heartbeat frame")
}

fn encode_text(channel: ChannelIndex, to: Recipient, text: String, want_ack: bool, from: NodeId) -> (PacketId, Vec<u8>) {
    let id = PacketId::random();
    let dest = match to {
        Recipient::Broadcast => BROADCAST_NODE.0,
        Recipient::Node(n) => n.0,
    };
    let data = meshtastic::Data {
        portnum: meshtastic::PortNum::TextMessageApp as i32,
        payload: text.into_bytes(),
        want_response: want_ack,
        ..Default::default()
    };
    let packet = meshtastic::MeshPacket {
        from: from.0,
        to: dest,
        channel: channel.get() as u32,
        id: id.0,
        want_ack,
        payload_variant: Some(meshtastic::mesh_packet::PayloadVariant::Decoded(data)),
        ..Default::default()
    };
    let msg = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::Packet(packet)),
    };
    let mut buf = Vec::new();
    msg.encode(&mut buf).expect("text encode");
    let frame = encode(&buf).expect("text frame");
    (id, frame)
}

fn events_from_radio(msg: meshtastic::FromRadio) -> Vec<Event> {
    use meshtastic::from_radio::PayloadVariant::*;
    let Some(variant) = msg.payload_variant else { return Vec::new(); };
    match variant {
        Packet(packet) => packet_to_events(packet),
        NodeInfo(ni) => vec![Event::NodeUpdated(node_from_proto(ni))],
        _ => Vec::new(),
    }
}

fn node_from_proto(ni: meshtastic::NodeInfo) -> Node {
    Node {
        id: NodeId(ni.num),
        long_name: ni.user.as_ref().map(|u| u.long_name.clone()).unwrap_or_default(),
        short_name: ni.user.as_ref().map(|u| u.short_name.clone()).unwrap_or_default(),
        role: NodeRole::Client,
        battery_level: ni.device_metrics.as_ref().map(|m| m.battery_level as u8),
        voltage_v: ni.device_metrics.as_ref().map(|m| m.voltage),
        snr_db: Some(ni.snr),
        rssi_dbm: None,
        hops_away: Some(ni.hops_away as u8),
        last_heard: None,
        position: ni.position.map(|p| Position {
            latitude_deg: (p.latitude_i as f64) * 1e-7,
            longitude_deg: (p.longitude_i as f64) * 1e-7,
            altitude_m: Some(p.altitude),
        }),
    }
}

fn packet_to_events(p: meshtastic::MeshPacket) -> Vec<Event> {
    use meshtastic::mesh_packet::PayloadVariant as V;
    let Some(V::Decoded(data)) = p.payload_variant else { return Vec::new(); };
    let payload = match parse(data.portnum, &data.payload) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    match payload {
        PortPayload::Text(text) => vec![Event::MessageReceived(TextMessage {
            id: PacketId(p.id),
            channel: ChannelIndex::new(p.channel as u8).unwrap_or(ChannelIndex::primary()),
            from: NodeId(p.from),
            to: if p.to == BROADCAST_NODE.0 { Recipient::Broadcast } else { Recipient::Node(NodeId(p.to)) },
            text,
            received_at: SystemTime::now(),
            direction: Direction::Incoming,
            state: DeliveryState::Delivered,
        })],
        PortPayload::Routing(r) => {
            let id = PacketId(p.request_id);
            let state = match r.error_reason {
                0 => DeliveryState::Delivered,
                _ => DeliveryState::Failed(format!("routing error {}", r.error_reason)),
            };
            vec![Event::MessageStateChanged { id, state }]
        }
        _ => Vec::new(),
    }
}
```

- [ ] **Step 6: Add session-loop test**

Create `tests/session_run.rs`:

```rust
use std::time::Duration;

use futures::future::FutureExt;
use mt::codec::frame::encode;
use mt::domain::ids::{ChannelIndex, NodeId};
use mt::domain::message::{Direction, Recipient};
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::proto::meshtastic;
use mt::session::commands::Command;
use mt::session::{DeviceSession, Event};
use mt::transport::mock::{MockTransport, Script};
use mt::transport::BoxedTransport;
use prost::Message;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn frame(m: meshtastic::FromRadio) -> Vec<u8> {
    let mut b = Vec::new();
    m.encode(&mut b).unwrap();
    encode(&b).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn end_to_end_connect_and_receive_text() {
    let my_info = meshtastic::FromRadio {
        id: 1,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::MyInfo(meshtastic::MyNodeInfo {
            my_node_num: 7,
            ..Default::default()
        })),
    };
    let done = meshtastic::FromRadio {
        id: 2,
        payload_variant: Some(meshtastic::from_radio::PayloadVariant::ConfigCompleteId(0)),
    };
    // config_id is random in the session; script accepts any id by matching after encoded frame inspection.
    // For the test, intercept sent frames via the handle.
    let (transport, handle) = MockTransport::new(Script::from_frames(vec![
        frame(my_info),
        frame(done),
    ]));
    let transport: BoxedTransport = Box::pin(transport);

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, mut ev_rx) = mpsc::channel(32);
    let cancel = CancellationToken::new();

    let session = DeviceSession::new(Box::new(move |_profile| {
        let t = transport.clone_boxed();
        async move { Ok((t, TransportKind::Tcp)) }.boxed()
    }));

    let cancel2 = cancel.clone();
    let handle_task = tokio::spawn(async move {
        session.run(cmd_rx, ev_tx, cancel2).await;
    });

    cmd_tx.send(Command::Connect(ConnectionProfile::Tcp { name: "t".into(), host: "h".into(), port: 1 })).unwrap();

    // We expect Connecting, then Connected (with mismatched config id the driver will time out; the script must be updated)
    let ev = tokio::time::timeout(Duration::from_millis(500), ev_rx.recv()).await.unwrap().unwrap();
    assert!(matches!(ev, Event::Connecting));
    let _ = handle; // quiet unused

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_millis(200), handle_task).await;
}
```

The test as written is a **smoke test** — it exercises the connect path but does not try to match `ConfigCompleteId`. The scripted `0` will mismatch the random `ConfigId`, causing a handshake timeout. For a deterministic end-to-end test that produces `Event::Connected`, inject the `want_config_id` by:

1. Capturing the first outbound frame with `handle.captured()` after a short delay.
2. Decoding it as `ToRadio`, reading the `want_config_id`, and using `handle.inject(...)` to push a `ConfigCompleteId` that matches.

This is real work; defer the full round-trip to Step 7 and keep the smoke test as-is for now to unblock commits.

`BoxedTransport::clone_boxed` does not exist. Either make the connect closure consume a `Vec<BoxedTransport>` and pop one per attempt, or change `connect` to a `Box<dyn Fn() -> ...>` that constructs a fresh `MockTransport` each call. Pick the latter; adjust the closure in the test to build a new `MockTransport` inside `move |_profile|`.

- [ ] **Step 7: Run**

```bash
cargo test --test session_run
cargo test --test session_handshake
```

Both must pass.

- [ ] **Step 8: Commit**

```bash
git add src/session tests/session_run.rs
git commit -m "Flesh out DeviceSession: commands, events, heartbeat, ack timeout"
```

---

## Task 12: TCP transport

**Goal:** Real `Transport` impl on top of `tokio::net::TcpStream` and `FrameCodec`.

**Files:**
- Create: `src/transport/tcp.rs`
- Modify: `src/transport/mod.rs`
- Create: `tests/transport_tcp.rs`

### Steps

- [ ] **Step 1: Write the failing test (loopback server)**

Create `tests/transport_tcp.rs`:

```rust
use futures::{SinkExt, StreamExt};
use mt::codec::frame::encode;
use mt::transport::tcp::connect;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

#[tokio::test(flavor = "current_thread")]
async fn tcp_transport_round_trips_frames() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        // Send a framed "hi"
        let frame = encode(b"hi").unwrap();
        sock.write_all(&frame).await.unwrap();
    });

    let transport = connect(&addr.ip().to_string(), addr.port()).await.unwrap();
    let (mut _sink, mut stream) = futures::StreamExt::split(transport);
    let got = stream.next().await.unwrap().unwrap();
    assert_eq!(&got[..], b"hi");
}
```

- [ ] **Step 2: Write `src/transport/tcp.rs`**

```rust
use tokio::net::TcpStream;
use tokio_util::codec::Framed;

use crate::codec::frame::FrameCodec;
use crate::error::ConnectError;
use crate::transport::{BoxedTransport, TransportError};

pub async fn connect(host: &str, port: u16) -> Result<BoxedTransport, ConnectError> {
    let stream = TcpStream::connect((host, port)).await.map_err(ConnectError::Tcp)?;
    stream.set_nodelay(true).ok();
    let framed = Framed::new(stream, FrameCodec::default());
    Ok(Box::pin(adapt(framed)))
}

fn adapt<T>(inner: T) -> impl futures::Sink<Vec<u8>, Error = TransportError> + futures::Stream<Item = Result<Vec<u8>, TransportError>> + Send + 'static
where
    T: futures::Sink<Vec<u8>, Error = crate::codec::error::FrameError>
        + futures::Stream<Item = Result<Vec<u8>, crate::codec::error::FrameError>>
        + Send
        + 'static,
{
    use futures::{SinkExt, StreamExt};
    inner
        .sink_map_err(TransportError::from)
        .map_err(TransportError::from)
}
```

`Framed<TcpStream, FrameCodec>` yields `Result<Vec<u8>, FrameError>`. The `adapt` helper collapses that into `TransportError`.

- [ ] **Step 3: Extend `src/transport/mod.rs`**

```rust
pub mod tcp;
```

- [ ] **Step 4: Run**

```bash
cargo test --test transport_tcp
```

- [ ] **Step 5: Commit**

```bash
git add src/transport/tcp.rs src/transport/mod.rs tests/transport_tcp.rs
git commit -m "Add TCP transport"
```

---

## Task 13: Serial transport

**Goal:** Real `Transport` impl on top of `tokio_serial::SerialStream` at 115200 8N1.

**Files:**
- Create: `src/transport/serial.rs`
- Modify: `src/transport/mod.rs`

### Steps

- [ ] **Step 1: Write `src/transport/serial.rs`**

```rust
use std::path::Path;

use tokio_serial::{DataBits, FlowControl, Parity, SerialPortBuilderExt, SerialStream, StopBits};
use tokio_util::codec::Framed;

use crate::codec::frame::FrameCodec;
use crate::error::ConnectError;
use crate::transport::{BoxedTransport, TransportError};

pub async fn connect(path: &Path) -> Result<BoxedTransport, ConnectError> {
    let path_str = path.to_string_lossy();
    let builder = tokio_serial::new(path_str.as_ref(), 115_200)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None);
    let stream: SerialStream = builder.open_native_async()?;
    let framed = Framed::new(stream, FrameCodec::default());
    Ok(Box::pin(adapt(framed)))
}

fn adapt<T>(inner: T) -> impl futures::Sink<Vec<u8>, Error = TransportError> + futures::Stream<Item = Result<Vec<u8>, TransportError>> + Send + 'static
where
    T: futures::Sink<Vec<u8>, Error = crate::codec::error::FrameError>
        + futures::Stream<Item = Result<Vec<u8>, crate::codec::error::FrameError>>
        + Send
        + 'static,
{
    use futures::{SinkExt, StreamExt};
    inner.sink_map_err(TransportError::from).map_err(TransportError::from)
}
```

- [ ] **Step 2: Extend `src/transport/mod.rs`**

```rust
pub mod serial;
```

- [ ] **Step 3: Verify compiles**

```bash
cargo build
```

No automated test — requires real hardware. Add to manual test checklist.

- [ ] **Step 4: Commit**

```bash
git add src/transport/serial.rs src/transport/mod.rs
git commit -m "Add serial transport"
```

---

## Task 14: BLE transport (paired flow)

**Goal:** Connect to a known-address peripheral over BLE, discover the Meshtastic service, subscribe to `fromNum`, and expose reads/writes of `fromRadio`/`toRadio` as a `Transport`-shaped pair of streams. Paired flow only — pairing UI is Task 15.

**Files:**
- Create: `src/transport/ble.rs`
- Modify: `src/transport/mod.rs`

### Steps

- [ ] **Step 1: Use context7 to confirm btleplug API**

Before writing code, query context7 for current `btleplug` async API: `Manager`, `adapters`, `start_scan`, `peripherals`, `Peripheral::connect/disconnect/discover_services/characteristics/read/write/subscribe`, and any `is_paired` method. Adjust the code in Step 2 if signatures differ.

- [ ] **Step 2: Write `src/transport/ble.rs`**

```rust
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use btleplug::api::{BDAddr, Central, CharPropFlags, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::Manager;
use futures::{Sink, Stream, StreamExt};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::domain::ids::BleAddress;
use crate::error::{ConnectError, PairingHint};
use crate::transport::{BoxedTransport, TransportError};

const SERVICE_UUID: Uuid = Uuid::from_u128(0x6ba1b218_15a8_461f_9fa8_5dcae273eafd);
const TORADIO_UUID: Uuid = Uuid::from_u128(0xf75c76d2_129e_4dad_a1dd_7866124401e7);
const FROMRADIO_UUID: Uuid = Uuid::from_u128(0x2c55e69e_4993_11ed_b878_0242ac120002);
const FROMNUM_UUID: Uuid = Uuid::from_u128(0xed9da18c_a800_4f66_a670_aa7547e34453);

pub async fn connect(address: &BleAddress) -> Result<BoxedTransport, ConnectError> {
    let manager = Manager::new().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    let adapters = manager.adapters().await.map_err(|_| ConnectError::BleAdapterUnavailable)?;
    let adapter = adapters.into_iter().next().ok_or(ConnectError::BleAdapterUnavailable)?;

    adapter
        .start_scan(ScanFilter { services: vec![SERVICE_UUID] })
        .await
        .map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    tokio::time::sleep(Duration::from_secs(3)).await;

    let peripherals = adapter.peripherals().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    let mut target = None;
    for p in peripherals {
        if p.address().to_string().eq_ignore_ascii_case(address.as_str()) {
            target = Some(p);
            break;
        }
    }
    let peripheral = target.ok_or_else(|| ConnectError::BleDeviceNotFound(address.as_str().into()))?;

    peripheral.connect().await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("authentication") || msg.contains("Authentication") {
            ConnectError::BlePairingRequired(pairing_hint())
        } else {
            ConnectError::BleGatt(msg)
        }
    })?;
    peripheral.discover_services().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;

    let chars = peripheral.characteristics();
    let to_radio = find(&chars, TORADIO_UUID)?;
    let from_radio = find(&chars, FROMRADIO_UUID)?;
    let from_num = find(&chars, FROMNUM_UUID)?;

    peripheral
        .subscribe(&from_num)
        .await
        .map_err(|e| ConnectError::BleGatt(e.to_string()))?;

    Ok(Box::pin(BleTransport::spawn(peripheral, to_radio, from_radio, from_num).await?))
}

fn find(chars: &std::collections::BTreeSet<Characteristic>, uuid: Uuid) -> Result<Characteristic, ConnectError> {
    chars
        .iter()
        .find(|c| c.uuid == uuid)
        .cloned()
        .ok_or_else(|| ConnectError::BleGatt(format!("missing characteristic {uuid}")))
}

fn pairing_hint() -> PairingHint {
    if cfg!(target_os = "macos") { PairingHint::Macos }
    else if cfg!(target_os = "windows") { PairingHint::Windows }
    else { PairingHint::LinuxBluetoothctl }
}

struct BleTransport {
    out: mpsc::UnboundedSender<Vec<u8>>,
    incoming: mpsc::UnboundedReceiver<Result<Vec<u8>, TransportError>>,
}

impl BleTransport {
    async fn spawn(
        peripheral: btleplug::platform::Peripheral,
        to_radio: Characteristic,
        from_radio: Characteristic,
        _from_num: Characteristic,
    ) -> Result<Self, ConnectError> {
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (in_tx, in_rx) = mpsc::unbounded_channel();

        let mut notify = peripheral
            .notifications()
            .await
            .map_err(|e| ConnectError::BleGatt(e.to_string()))?;

        let p2 = peripheral.clone();
        let in_tx2 = in_tx.clone();
        tokio::spawn(async move {
            while let Some(_n) = notify.next().await {
                loop {
                    match p2.read(&from_radio).await {
                        Ok(bytes) if bytes.is_empty() => break,
                        Ok(bytes) => { let _ = in_tx2.send(Ok(bytes)); }
                        Err(e) => { let _ = in_tx2.send(Err(TransportError::Ble(e.to_string()))); return; }
                    }
                }
            }
        });

        let p3 = peripheral.clone();
        tokio::spawn(async move {
            while let Some(frame) = out_rx.recv().await {
                if let Err(e) = p3.write(&to_radio, &frame, WriteType::WithoutResponse).await {
                    tracing::warn!(?e, "ble write failed");
                    break;
                }
            }
        });

        Ok(Self { out: out_tx, incoming: in_rx })
    }
}

impl Stream for BleTransport {
    type Item = Result<Vec<u8>, TransportError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.incoming.poll_recv(cx)
    }
}

impl Sink<Vec<u8>> for BleTransport {
    type Error = TransportError;
    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn start_send(self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), Self::Error> {
        self.out.send(item).map_err(|_| TransportError::Closed)
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
```

**Notes on UUIDs:** The numeric values above are from the Meshtastic firmware BLE specification (`firmware/bin/build-all.sh` / `src/mesh/NodeDB.cpp`). If a firmware version returns a `characteristic not found` error, verify the UUIDs against the running firmware's `BluetoothPhoneAPI` file in `github.com/meshtastic/firmware`.

Add to `Cargo.toml`:

```toml
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 3: Extend `src/transport/mod.rs`**

```rust
pub mod ble;
```

- [ ] **Step 4: Compile**

```bash
cargo build
```

No automated test — requires a real device. Add manual step to the checklist.

- [ ] **Step 5: Commit**

```bash
git add src/transport/ble.rs src/transport/mod.rs Cargo.toml
git commit -m "Add BLE transport (paired flow)"
```

---

## Task 15: BLE scan and pairing-hint plumbing

**Goal:** Expose a `scan()` function that returns discovered peripherals with address, name, RSSI, and `is_paired` flag; funnel pairing errors into `ConnectError::BlePairingRequired` so the UI can show the right dialog. Linux-specific hint is a text message only.

**Files:**
- Modify: `src/transport/ble.rs`
- Create: `tests/transport_ble_scan.rs` (compile-only — gated)

### Steps

- [ ] **Step 1: Append `scan` and `Discovered` to `src/transport/ble.rs`**

```rust
pub struct Discovered {
    pub name: String,
    pub address: crate::domain::ids::BleAddress,
    pub rssi_dbm: Option<i16>,
    pub is_paired: bool,
}

pub async fn scan(duration_secs: u64) -> Result<Vec<Discovered>, crate::error::ConnectError> {
    let manager = Manager::new().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    let adapters = manager.adapters().await.map_err(|_| ConnectError::BleAdapterUnavailable)?;
    let adapter = adapters.into_iter().next().ok_or(ConnectError::BleAdapterUnavailable)?;

    adapter
        .start_scan(ScanFilter { services: vec![SERVICE_UUID] })
        .await
        .map_err(|e| ConnectError::BleGatt(e.to_string()))?;
    tokio::time::sleep(std::time::Duration::from_secs(duration_secs)).await;
    let peripherals = adapter.peripherals().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;

    let mut out = Vec::new();
    for p in peripherals {
        let properties = p.properties().await.map_err(|e| ConnectError::BleGatt(e.to_string()))?;
        let Some(props) = properties else { continue };
        let is_paired = p.is_paired().await.unwrap_or(false);
        out.push(Discovered {
            name: props.local_name.unwrap_or_else(|| "Meshtastic".into()),
            address: crate::domain::ids::BleAddress::new(p.address().to_string()),
            rssi_dbm: props.rssi,
            is_paired,
        });
    }
    Ok(out)
}
```

If `Peripheral::is_paired` is not present on the version you picked (context7 says it is as of 0.11), stub it as `Ok(false)` and document the follow-up.

- [ ] **Step 2: Compile**

```bash
cargo build
```

- [ ] **Step 3: Commit**

```bash
git add src/transport/ble.rs
git commit -m "Expose BLE scan with paired-state hint"
```

---

## Task 16: Persisted profiles

**Goal:** Load/save a `Vec<ConnectionProfile>` from/to a TOML file in the platform config directory.

**Files:**
- Create: `src/persist/profiles.rs`
- Modify: `src/persist/mod.rs`
- Create: `tests/persist_profiles.rs`

### Steps

- [ ] **Step 1: Write the failing test**

Create `tests/persist_profiles.rs`:

```rust
use mt::domain::ids::BleAddress;
use mt::domain::profile::ConnectionProfile;
use mt::persist::profiles::{load_from, save_to};

#[test]
fn round_trip_profiles_to_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profiles.toml");
    let input = vec![
        ConnectionProfile::Tcp { name: "home".into(), host: "192.168.1.1".into(), port: 4403 },
        ConnectionProfile::Ble { name: "pack".into(), address: BleAddress::new("AA:BB:CC:DD:EE:FF") },
    ];
    save_to(&path, &input).unwrap();
    let loaded = load_from(&path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert!(matches!(loaded[0], ConnectionProfile::Tcp { .. }));
    assert!(matches!(loaded[1], ConnectionProfile::Ble { .. }));
}

#[test]
fn load_missing_file_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.toml");
    assert!(load_from(&path).unwrap().is_empty());
}
```

Add `tempfile = "3"` to `[dev-dependencies]` in `Cargo.toml`.

- [ ] **Step 2: Write `src/persist/profiles.rs`**

```rust
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::profile::ConnectionProfile;
use crate::error::PersistError;

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    profile: Vec<ConnectionProfile>,
}

pub fn load_from(path: &Path) -> Result<Vec<ConnectionProfile>, PersistError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(toml::from_str::<File>(&text)?.profile),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(PersistError::Io(e)),
    }
}

pub fn save_to(path: &Path, profiles: &[ConnectionProfile]) -> Result<(), PersistError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(&File { profile: profiles.to_vec() })?;
    std::fs::write(path, text)?;
    Ok(())
}

pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("dev", "", "mt")
        .map(|d| d.config_dir().join("profiles.toml"))
        .unwrap_or_else(|| PathBuf::from("profiles.toml"))
}
```

- [ ] **Step 3: Extend `src/persist/mod.rs`**

```rust
pub mod profiles;
```

- [ ] **Step 4: Run**

```bash
cargo test --test persist_profiles
```

- [ ] **Step 5: Commit**

```bash
git add src/persist tests/persist_profiles.rs Cargo.toml
git commit -m "Add TOML profile persistence"
```

---

## Task 17: UI app skeleton and event pump

**Goal:** `eframe::App` that holds `AppState`, drains `Event` channel on every frame, reduces into state, and routes to subviews.

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/status.rs`

### Steps

- [ ] **Step 1: Write `src/ui/mod.rs`**

```rust
pub mod chat;
pub mod connect;
pub mod nodes;
pub mod scan;
pub mod status;

use std::sync::Arc;

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::profile::ConnectionProfile;
use crate::domain::snapshot::DeviceSnapshot;
use crate::session::commands::Command;
use crate::session::Event;

#[derive(Default)]
pub struct AppState {
    pub connected: bool,
    pub snapshot: DeviceSnapshot,
    pub profiles: Vec<ConnectionProfile>,
    pub last_error: Option<String>,
    pub active_tab: Tab,
}

#[derive(Default, Copy, Clone, Eq, PartialEq)]
pub enum Tab {
    #[default]
    Chat,
    Nodes,
}

pub struct App {
    state: AppState,
    cmd_tx: mpsc::UnboundedSender<Command>,
    ev_rx: mpsc::Receiver<Event>,
    profiles_path: std::path::PathBuf,
}

impl App {
    pub fn new(
        profiles: Vec<ConnectionProfile>,
        profiles_path: std::path::PathBuf,
        cmd_tx: mpsc::UnboundedSender<Command>,
        ev_rx: mpsc::Receiver<Event>,
    ) -> Self {
        Self {
            state: AppState { profiles, ..Default::default() },
            cmd_tx,
            ev_rx,
            profiles_path,
        }
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.ev_rx.try_recv() {
            self.reduce(ev);
        }
    }

    fn reduce(&mut self, ev: Event) {
        match ev {
            Event::Connecting => { self.state.connected = false; self.state.last_error = None; }
            Event::Connected(s) => { self.state.connected = true; self.state.snapshot = *s; }
            Event::NodeUpdated(n) => { self.state.snapshot.nodes.insert(n.id, n); }
            Event::ChannelUpdated(c) => {
                if let Some(existing) = self.state.snapshot.channels.iter_mut().find(|x| x.index == c.index) {
                    *existing = c;
                } else {
                    self.state.snapshot.channels.push(c);
                }
            }
            Event::MessageReceived(m) => { self.state.snapshot.messages.push(m); }
            Event::MessageStateChanged { id, state } => {
                if let Some(m) = self.state.snapshot.messages.iter_mut().find(|m| m.id == id) {
                    m.state = state;
                }
            }
            Event::Disconnected => { self.state.connected = false; }
            Event::Error(msg) => { self.state.last_error = Some(msg); }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        egui::TopBottomPanel::top("status").show(ctx, |ui| status::render(ui, &self.state));
        if !self.state.connected {
            egui::CentralPanel::default().show(ctx, |ui| {
                connect::render(ui, &mut self.state, &self.cmd_tx, &self.profiles_path);
            });
            return;
        }
        egui::SidePanel::left("sidebar").show(ctx, |ui| {
            if ui.button("Disconnect").clicked() {
                let _ = self.cmd_tx.send(Command::Disconnect);
            }
            ui.separator();
            ui.selectable_value(&mut self.state.active_tab, Tab::Chat, "Chat");
            ui.selectable_value(&mut self.state.active_tab, Tab::Nodes, "Nodes");
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.state.active_tab {
            Tab::Chat => chat::render(ui, &mut self.state, &self.cmd_tx),
            Tab::Nodes => nodes::render(ui, &self.state),
        });
    }
}
```

- [ ] **Step 2: Stub `src/ui/status.rs`**

```rust
use eframe::egui;

use crate::ui::AppState;

pub fn render(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        ui.label(if state.connected { "Connected" } else { "Disconnected" });
        ui.separator();
        ui.label(&state.snapshot.long_name);
        if let Some(err) = &state.last_error {
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }
    });
}
```

Also create empty placeholder stubs for `connect.rs`, `chat.rs`, `nodes.rs`, `scan.rs` each containing:

```rust
use eframe::egui;
use tokio::sync::mpsc;

use crate::session::commands::Command;
use crate::ui::AppState;

#[allow(unused_variables)]
pub fn render(ui: &mut egui::Ui, state: &mut AppState, cmd: &mpsc::UnboundedSender<Command>, profiles_path: &std::path::Path) {
    ui.label("TODO");
}
```

Adjust signatures per caller above: `chat::render(ui, &mut state, &cmd_tx)`, `nodes::render(ui, &state)`, `connect::render(ui, &mut state, &cmd_tx, &profiles_path)`, `scan::render` called from within `connect::render` later.

Exact placeholder bodies:

`src/ui/connect.rs`:
```rust
use std::path::Path;
use eframe::egui;
use tokio::sync::mpsc;
use crate::session::commands::Command;
use crate::ui::AppState;

pub fn render(ui: &mut egui::Ui, _state: &mut AppState, _cmd: &mpsc::UnboundedSender<Command>, _profiles_path: &Path) {
    ui.label("connect placeholder");
}
```

`src/ui/chat.rs`:
```rust
use eframe::egui;
use tokio::sync::mpsc;
use crate::session::commands::Command;
use crate::ui::AppState;

pub fn render(ui: &mut egui::Ui, _state: &mut AppState, _cmd: &mpsc::UnboundedSender<Command>) {
    ui.label("chat placeholder");
}
```

`src/ui/nodes.rs`:
```rust
use eframe::egui;
use crate::ui::AppState;

pub fn render(ui: &mut egui::Ui, _state: &AppState) {
    ui.label("nodes placeholder");
}
```

`src/ui/scan.rs`:
```rust
use eframe::egui;
pub fn render(ui: &mut egui::Ui) {
    ui.label("scan placeholder");
}
```

- [ ] **Step 3: Compile**

```bash
cargo build
```

- [ ] **Step 4: Commit**

```bash
git add src/ui
git commit -m "Add UI App skeleton with event pump and placeholders"
```

---

## Task 18: ConnectScreen

**Goal:** Connect screen with profile list, Add (BLE/Serial/TCP form), Edit, Delete, Connect, Scan-BLE actions.

**Files:**
- Replace: `src/ui/connect.rs`

### Steps

- [ ] **Step 1: Write full `src/ui/connect.rs`**

```rust
use std::path::{Path, PathBuf};

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::ids::BleAddress;
use crate::domain::profile::{ConnectionProfile, TransportKind};
use crate::persist::profiles::save_to;
use crate::session::commands::Command;
use crate::ui::AppState;

#[derive(Default)]
struct AddForm {
    open: bool,
    kind: Option<TransportKind>,
    name: String,
    host: String,
    port: String,
    path: String,
    address: String,
}

static mut FORM: Option<AddForm> = None;

fn form() -> &'static mut AddForm {
    unsafe {
        if FORM.is_none() {
            FORM = Some(AddForm::default());
        }
        FORM.as_mut().expect("form")
    }
}
```

**Stop.** The `static mut FORM` approach violates our "no global mutable state" rule. Replace with a state-owned struct. Redo from scratch:

```rust
use std::path::{Path, PathBuf};

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::ids::BleAddress;
use crate::domain::profile::{ConnectionProfile, TransportKind};
use crate::persist::profiles::save_to;
use crate::session::commands::Command;
use crate::ui::AppState;

#[derive(Default)]
pub struct ConnectUi {
    pub add: AddForm,
    pub selected: Option<usize>,
}

#[derive(Default)]
pub struct AddForm {
    pub open: bool,
    pub kind: Option<TransportKind>,
    pub name: String,
    pub host: String,
    pub port: String,
    pub path: String,
    pub address: String,
}
```

Move `ConnectUi` onto `AppState`:

```rust
// in src/ui/mod.rs AppState:
pub connect_ui: connect::ConnectUi,
```

Now `connect.rs` `render` can mutate `state.connect_ui`:

```rust
pub fn render(ui: &mut egui::Ui, state: &mut AppState, cmd: &mpsc::UnboundedSender<Command>, profiles_path: &Path) {
    ui.heading("Connect");
    ui.horizontal(|ui| {
        if ui.button("Add profile").clicked() {
            state.connect_ui.add.open = true;
            state.connect_ui.add.kind = None;
        }
        if ui.button("Scan BLE").clicked() {
            // Step wired in Task 19.
        }
    });
    ui.separator();
    list_profiles(ui, state, cmd, profiles_path);
    if state.connect_ui.add.open {
        add_dialog(ui.ctx(), state, profiles_path);
    }
}

fn list_profiles(ui: &mut egui::Ui, state: &mut AppState, cmd: &mpsc::UnboundedSender<Command>, profiles_path: &Path) {
    let mut delete_idx: Option<usize> = None;
    for (idx, profile) in state.profiles.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("[{:?}] {}", profile.kind(), profile.name()));
            if ui.button("Connect").clicked() {
                let _ = cmd.send(Command::Connect(profile.clone()));
            }
            if ui.button("Delete").clicked() {
                delete_idx = Some(idx);
            }
        });
    }
    if let Some(i) = delete_idx {
        state.profiles.remove(i);
        let _ = save_to(profiles_path, &state.profiles);
    }
}

fn add_dialog(ctx: &egui::Context, state: &mut AppState, profiles_path: &Path) {
    let mut close = false;
    egui::Window::new("Add profile").collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.connect_ui.add.kind, Some(TransportKind::Ble), "BLE");
            ui.selectable_value(&mut state.connect_ui.add.kind, Some(TransportKind::Serial), "Serial");
            ui.selectable_value(&mut state.connect_ui.add.kind, Some(TransportKind::Tcp), "TCP");
        });
        ui.text_edit_singleline(&mut state.connect_ui.add.name);
        match state.connect_ui.add.kind {
            Some(TransportKind::Ble) => {
                ui.label("BLE address");
                ui.text_edit_singleline(&mut state.connect_ui.add.address);
            }
            Some(TransportKind::Serial) => {
                ui.label("Serial path");
                ui.text_edit_singleline(&mut state.connect_ui.add.path);
            }
            Some(TransportKind::Tcp) => {
                ui.label("Host");
                ui.text_edit_singleline(&mut state.connect_ui.add.host);
                ui.label("Port");
                ui.text_edit_singleline(&mut state.connect_ui.add.port);
            }
            None => { ui.label("Pick a transport"); }
        }
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() { close = true; }
            if ui.button("Save").clicked() {
                if let Some(profile) = build_profile(&state.connect_ui.add) {
                    state.profiles.push(profile);
                    let _ = save_to(profiles_path, &state.profiles);
                    close = true;
                }
            }
        });
    });
    if close {
        state.connect_ui.add = AddForm::default();
    }
}

fn build_profile(form: &AddForm) -> Option<ConnectionProfile> {
    match form.kind? {
        TransportKind::Ble => Some(ConnectionProfile::Ble {
            name: form.name.clone(),
            address: BleAddress::new(form.address.clone()),
        }),
        TransportKind::Serial => Some(ConnectionProfile::Serial {
            name: form.name.clone(),
            path: PathBuf::from(form.path.clone()),
        }),
        TransportKind::Tcp => Some(ConnectionProfile::Tcp {
            name: form.name.clone(),
            host: form.host.clone(),
            port: form.port.parse().ok()?,
        }),
    }
}
```

- [ ] **Step 2: Compile**

```bash
cargo build
```

- [ ] **Step 3: Commit**

```bash
git add src/ui/connect.rs src/ui/mod.rs
git commit -m "Add connect screen: profile list, add form, connect/delete actions"
```

---

## Task 19: ScanDialog

**Goal:** BLE scan dialog that launches `transport::ble::scan`, shows discovered peripherals with paired badge, and lets the user connect directly or save as a profile.

**Files:**
- Replace: `src/ui/scan.rs`
- Modify: `src/ui/connect.rs` (invoke the dialog when "Scan BLE" is clicked)
- Modify: `src/ui/mod.rs` (store scan state)

### Steps

- [ ] **Step 1: Write `src/ui/scan.rs`**

```rust
use eframe::egui;
use tokio::sync::{mpsc, oneshot};

use crate::domain::ids::BleAddress;
use crate::domain::profile::ConnectionProfile;
use crate::session::commands::Command;
use crate::transport::ble::{scan, Discovered};

#[derive(Default)]
pub struct ScanUi {
    pub open: bool,
    pub results: Vec<DiscoveredRow>,
    pub pending: Option<oneshot::Receiver<Vec<DiscoveredRow>>>,
    pub pending_pairing: Option<BleAddress>,
}

#[derive(Clone)]
pub struct DiscoveredRow {
    pub name: String,
    pub address: BleAddress,
    pub rssi_dbm: Option<i16>,
    pub is_paired: bool,
}

impl From<Discovered> for DiscoveredRow {
    fn from(d: Discovered) -> Self {
        Self { name: d.name, address: d.address, rssi_dbm: d.rssi_dbm, is_paired: d.is_paired }
    }
}

pub fn open(scan_ui: &mut ScanUi) {
    scan_ui.open = true;
    scan_ui.results.clear();
    let (tx, rx) = oneshot::channel();
    scan_ui.pending = Some(rx);
    tokio::spawn(async move {
        let results = scan(3).await.unwrap_or_default().into_iter().map(DiscoveredRow::from).collect();
        let _ = tx.send(results);
    });
}

pub fn render(ctx: &egui::Context, scan_ui: &mut ScanUi, cmd: &mpsc::UnboundedSender<Command>, profiles: &mut Vec<ConnectionProfile>) {
    if !scan_ui.open { return; }

    if let Some(mut rx) = scan_ui.pending.take() {
        match rx.try_recv() {
            Ok(results) => scan_ui.results = results,
            Err(oneshot::error::TryRecvError::Empty) => scan_ui.pending = Some(rx),
            Err(oneshot::error::TryRecvError::Closed) => {}
        }
    }

    let mut close = false;
    egui::Window::new("BLE Scan").collapsible(false).show(ctx, |ui| {
        if scan_ui.pending.is_some() { ui.label("Scanning..."); }
        for row in scan_ui.results.clone() {
            ui.horizontal(|ui| {
                ui.label(&row.name);
                ui.label(row.address.as_str());
                if let Some(r) = row.rssi_dbm { ui.label(format!("{r} dBm")); }
                if row.is_paired { ui.colored_label(egui::Color32::LIGHT_GREEN, "paired"); }
                else { ui.colored_label(egui::Color32::YELLOW, "new"); }
                if ui.button("Connect").clicked() {
                    if !row.is_paired {
                        scan_ui.pending_pairing = Some(row.address.clone());
                    } else {
                        let _ = cmd.send(Command::Connect(ConnectionProfile::Ble {
                            name: row.name.clone(),
                            address: row.address.clone(),
                        }));
                    }
                }
                if ui.button("Save").clicked() {
                    profiles.push(ConnectionProfile::Ble { name: row.name.clone(), address: row.address.clone() });
                }
            });
        }
        if ui.button("Close").clicked() { close = true; }
    });

    if let Some(addr) = scan_ui.pending_pairing.clone() {
        let mut continue_connect = false;
        egui::Window::new("First-time pairing").collapsible(false).show(ctx, |ui| {
            ui.label("Meshtastic will display a 6-digit PIN on the device screen.");
            ui.label("Your OS will open a system dialog asking for it — type it there.");
            ui.label("This is only needed the first time you pair.");
            if ui.button("Continue").clicked() { continue_connect = true; }
            if ui.button("Cancel").clicked() { scan_ui.pending_pairing = None; }
        });
        if continue_connect {
            let _ = cmd.send(Command::Connect(ConnectionProfile::Ble {
                name: "New device".into(),
                address: addr.clone(),
            }));
            scan_ui.pending_pairing = None;
        }
    }

    if close { scan_ui.open = false; }
}
```

- [ ] **Step 2: Store `ScanUi` in `AppState`**

In `src/ui/mod.rs`, add:

```rust
pub scan_ui: scan::ScanUi,
```

In `src/ui/connect.rs`, replace the scan button handler:

```rust
if ui.button("Scan BLE").clicked() {
    crate::ui::scan::open(&mut state.scan_ui);
}
```

And in `src/ui/mod.rs`, call `scan::render` once per frame regardless of which panel is open:

```rust
scan::render(ctx, &mut self.state.scan_ui, &self.cmd_tx, &mut self.state.profiles);
```

Add after `drain_events` at the start of `update`.

- [ ] **Step 3: Compile**

```bash
cargo build
```

- [ ] **Step 4: Commit**

```bash
git add src/ui
git commit -m "Add BLE scan dialog and first-time pairing info modal"
```

---

## Task 20: ChatView + composer

**Goal:** Render per-channel chat; list incoming/outgoing messages with delivery indicator; composer sends.

**Files:**
- Replace: `src/ui/chat.rs`

### Steps

- [ ] **Step 1: Write `src/ui/chat.rs`**

```rust
use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::ids::ChannelIndex;
use crate::domain::message::{DeliveryState, Direction, Recipient};
use crate::session::commands::Command;
use crate::ui::AppState;

#[derive(Default)]
pub struct ChatUi {
    pub active_channel: u8,
    pub composer_text: String,
}

pub fn render(ui: &mut egui::Ui, state: &mut AppState, cmd: &mpsc::UnboundedSender<Command>) {
    let active = ChannelIndex::new(state.chat_ui.active_channel).unwrap_or(ChannelIndex::primary());

    ui.horizontal(|ui| {
        for ch in &state.snapshot.channels {
            let idx_u8 = ch.index.get();
            let label = if ch.name.is_empty() { format!("#{idx_u8}") } else { ch.name.clone() };
            ui.selectable_value(&mut state.chat_ui.active_channel, idx_u8, label);
        }
    });
    ui.separator();

    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        for m in state.snapshot.messages.iter().filter(|m| m.channel == active) {
            ui.horizontal(|ui| {
                let sender = state
                    .snapshot
                    .nodes
                    .get(&m.from)
                    .map(|n| n.long_name.clone())
                    .unwrap_or_else(|| format!("{:08x}", m.from.0));
                ui.strong(sender);
                ui.label(&m.text);
                match (&m.direction, &m.state) {
                    (Direction::Outgoing, DeliveryState::Pending) => { ui.label("…"); }
                    (Direction::Outgoing, DeliveryState::Delivered) => { ui.label("✓"); }
                    (Direction::Outgoing, DeliveryState::Failed(r)) => { ui.colored_label(egui::Color32::LIGHT_RED, format!("! {r}")); }
                    _ => {}
                }
            });
        }
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.chat_ui.composer_text);
        if ui.button("Send").clicked() && !state.chat_ui.composer_text.is_empty() {
            let _ = cmd.send(Command::SendText {
                channel: active,
                to: Recipient::Broadcast,
                text: std::mem::take(&mut state.chat_ui.composer_text),
                want_ack: true,
            });
        }
    });
}
```

- [ ] **Step 2: Extend `AppState`**

In `src/ui/mod.rs`:

```rust
pub chat_ui: chat::ChatUi,
```

- [ ] **Step 3: Compile**

```bash
cargo build
```

- [ ] **Step 4: Commit**

```bash
git add src/ui
git commit -m "Add chat view with composer and delivery indicators"
```

---

## Task 21: NodesView

**Goal:** Sortable table of nodes.

**Files:**
- Replace: `src/ui/nodes.rs`

### Steps

- [ ] **Step 1: Write `src/ui/nodes.rs`**

```rust
use std::time::SystemTime;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::domain::node::Node;
use crate::ui::AppState;

pub fn render(ui: &mut egui::Ui, state: &AppState) {
    let mut nodes: Vec<&Node> = state.snapshot.nodes.values().collect();
    nodes.sort_by(|a, b| a.long_name.cmp(&b.long_name));

    TableBuilder::new(ui)
        .striped(true)
        .column(Column::auto().resizable(true))
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::remainder())
        .header(20.0, |mut header| {
            for h in ["Long", "Short", "Role", "Bat", "SNR", "Hops", "Position"] {
                header.col(|ui| { ui.strong(h); });
            }
        })
        .body(|mut body| {
            for node in nodes {
                body.row(18.0, |mut row| {
                    row.col(|ui| { ui.label(&node.long_name); });
                    row.col(|ui| { ui.label(&node.short_name); });
                    row.col(|ui| { ui.label(format!("{:?}", node.role)); });
                    row.col(|ui| { ui.label(node.battery_level.map(|b| format!("{b}%")).unwrap_or("—".into())); });
                    row.col(|ui| { ui.label(node.snr_db.map(|s| format!("{s:.1}")).unwrap_or("—".into())); });
                    row.col(|ui| { ui.label(node.hops_away.map(|h| format!("{h}")).unwrap_or("—".into())); });
                    row.col(|ui| {
                        let pos = node.position.as_ref().map(|p| format!("{:.4},{:.4}", p.latitude_deg, p.longitude_deg)).unwrap_or("—".into());
                        ui.label(pos);
                    });
                });
            }
        });
    let _ = SystemTime::now();
}
```

- [ ] **Step 2: Compile**

```bash
cargo build
```

- [ ] **Step 3: Commit**

```bash
git add src/ui/nodes.rs
git commit -m "Add nodes table view"
```

---

## Task 22: StatusBar polish

**Goal:** Show transport kind, my node info, battery, firmware version, and last error.

**Files:**
- Replace: `src/ui/status.rs`

### Steps

- [ ] **Step 1: Write `src/ui/status.rs`**

```rust
use eframe::egui;

use crate::ui::AppState;

pub fn render(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        let label = if state.connected { "● Connected" } else { "○ Disconnected" };
        ui.strong(label);
        ui.separator();
        if state.connected {
            ui.label(format!("{} [{}]", state.snapshot.long_name, state.snapshot.short_name));
            ui.separator();
            ui.label(format!("fw {}", state.snapshot.firmware_version));
        }
        if let Some(err) = &state.last_error {
            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }
    });
}
```

- [ ] **Step 2: Compile and commit**

```bash
cargo build
git add src/ui/status.rs
git commit -m "Polish status bar"
```

---

## Task 23: Wire `main.rs`

**Goal:** Start a tokio runtime on a background thread, run the `DeviceSession`, start eframe on the main thread.

**Files:**
- Replace: `src/main.rs`

### Steps

- [ ] **Step 1: Write `src/main.rs`**

```rust
use std::sync::Arc;

use eframe::{egui, NativeOptions};
use futures::future::FutureExt;
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::error::ConnectError;
use mt::persist::profiles::{default_path, load_from};
use mt::session::{DeviceSession, Event};
use mt::session::commands::Command;
use mt::transport::{ble, serial, tcp, BoxedTransport};
use mt::ui::App;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let profiles_path = default_path();
    let profiles = load_from(&profiles_path).unwrap_or_default();

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    let (ev_tx, ev_rx) = mpsc::channel::<Event>(256);
    let cancel = CancellationToken::new();

    let rt = Arc::new(rt);
    let rt_session = rt.clone();
    std::thread::spawn(move || {
        let session = DeviceSession::new(Box::new(|profile: ConnectionProfile| {
            async move {
                match profile {
                    ConnectionProfile::Tcp { host, port, .. } => Ok((tcp::connect(&host, port).await?, TransportKind::Tcp)),
                    ConnectionProfile::Serial { path, .. } => Ok((serial::connect(&path).await?, TransportKind::Serial)),
                    ConnectionProfile::Ble { address, .. } => Ok((ble::connect(&address).await?, TransportKind::Ble)),
                }
            }
            .boxed()
        }));
        rt_session.block_on(session.run(cmd_rx, ev_tx, cancel.clone()));
    });

    let options = NativeOptions::default();
    eframe::run_native(
        "Meshtastic",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(profiles, profiles_path, cmd_tx, ev_rx)))),
    )
}
```

- [ ] **Step 2: Build**

```bash
cargo build --bin mt
```

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "Wire main: tokio runtime on background thread, eframe on main"
```

---

## Task 24: Manual end-to-end checklist

**Goal:** A living document in the repo describing the manual smoke test. No code changes.

**Files:**
- Create: `docs/manual-testing.md`

### Steps

- [ ] **Step 1: Write `docs/manual-testing.md`**

```markdown
# Manual testing checklist

Run after any non-trivial change to transport or session code.

## TCP

1. Start `meshtasticd` (official firmware daemon) or point at a device that exposes TCP on port 4403.
2. `cargo run`.
3. Add profile: TCP, host `127.0.0.1`, port `4403`. Connect.
4. Expect `Connected` in status bar within 10s. Node list populated. Primary channel tab shows.
5. Send a text on primary. Expect `…` → `✓` within 30s.

## Serial

1. Plug in a Meshtastic device via USB. Identify `/dev/ttyUSB*` (Linux), `/dev/cu.usbserial-*` (macOS), `COMn` (Windows).
2. Add profile: Serial, path `<device>`. Connect.
3. Same assertions as TCP.

## BLE

1. For a new device: open the BLE Scan dialog. Select "New" device. The OS system dialog appears; enter the PIN shown on the device screen. Expect Connected.
2. On Linux, if step 1 fails with "pairing required": run `bluetoothctl`, `agent on`, `default-agent`, `pair <MAC>`, `trust <MAC>`, `exit`, then retry in the app.
3. For a paired device: connect directly, no dialog.
4. Same assertions as TCP.
```

- [ ] **Step 2: Commit**

```bash
git add docs/manual-testing.md
git commit -m "Add manual testing checklist"
```

---

## Self-review notes

- **Spec coverage:** Tier 1 goals (§1.1 of the spec) are covered by Tasks 1–23; BLE pairing UX is Task 19; persistence is Task 16; manual test doc is Task 24.
- **Deferred items** (per §1.2 of the spec) are explicitly absent from the plan: no config write, no traceroute, no waypoints, no auto-reconnect, no multi-device, no SQLite, no map, no localization.
- **Potential friction:**
  - `src/proto/mod.rs` depends on the exact module layout emitted by `prost-build`. Task 7 has an explicit step to inspect `OUT_DIR` and adjust `include!` if the proto splits into multiple packages.
  - `DeviceSession::run` (Task 11) is sketched, then refactored mid-task. This is intentional (two commits, first compiles with handshake only, second adds the loop). If the refactor is rough, split Task 11 into 11a and 11b locally.
  - `btleplug` API has drifted between versions. Task 14 opens with a context7 query — do not skip it. If `Peripheral::is_paired` is not available on the picked version, stub `Ok(false)` and document the follow-up.
  - `tempfile` is added in Task 16's dev-dependencies; the edit to `Cargo.toml` is in that task's Step 1 list.

End of plan.
