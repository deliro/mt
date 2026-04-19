#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Duration;

use futures::future::FutureExt;
use futures::{SinkExt, StreamExt};
use mt::domain::ids::{BleAddress, ConfigId};
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::proto::meshtastic;
use mt::session::commands::Command;
use mt::session::{DeviceSession, Event};
use mt::transport::{BoxedTransport, ble};
use prost::Message;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing_subscriber::EnvFilter;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("mt=info,warn")),
        )
        .try_init();

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln_safe(&format!("failed to build tokio runtime: {e}"));
            return ExitCode::from(2);
        }
    };

    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map_or("", String::as_str);
    match cmd {
        "scan" => runtime.block_on(cmd_scan()),
        "connect" => arg_or_usage(args.get(2), |id| runtime.block_on(cmd_connect(id))),
        "pump" => arg_or_usage(args.get(2), |id| runtime.block_on(cmd_pump(id))),
        _ => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn arg_or_usage<F: FnOnce(&str) -> ExitCode>(arg: Option<&String>, run: F) -> ExitCode {
    arg.map_or_else(
        || {
            print_usage();
            ExitCode::from(2)
        },
        |s| run(s),
    )
}

fn duration_from_env(var: &str, default: Duration) -> Duration {
    env::var(var).ok().and_then(|v| v.parse::<u64>().ok()).map_or(default, Duration::from_secs)
}

fn print_usage() {
    eprintln_safe(
        "usage:\n  mt-cli scan\n  mt-cli connect <peripheral-id>\n  mt-cli pump <peripheral-id>\n\nenv:\n  MT_TIMEOUT_SECS (default 30)\n  MT_STREAM_SECS (pump/connect post-Connected window, default 10)\n  RUST_LOG (e.g. mt=debug,btleplug=debug)",
    );
}

async fn cmd_scan() -> ExitCode {
    let (tx, mut rx) = mpsc::unbounded_channel::<ble::Discovered>();
    let scanner = tokio::spawn(ble::scan_stream(Duration::from_secs(6), tx));
    let overall = duration_from_env("MT_TIMEOUT_SECS", Duration::from_secs(10));
    let deadline =
        tokio::time::Instant::now().checked_add(overall).unwrap_or_else(tokio::time::Instant::now);
    let mut found = 0usize;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Some(d)) => {
                let rssi = d.rssi_dbm.map_or_else(|| "? dBm".into(), |r| format!("{r} dBm"));
                let paired = if d.is_paired { "\tconnected" } else { "" };
                println_safe(&format!("{}\t{}\t{}{}", d.address.as_str(), d.name, rssi, paired));
                found = found.saturating_add(1);
            }
            Ok(None) | Err(_) => break,
        }
    }
    drop(rx);
    let _ = scanner.await;
    if found == 0 {
        println_safe("no Meshtastic devices found");
    }
    ExitCode::SUCCESS
}

async fn cmd_connect(id: &str) -> ExitCode {
    let overall = duration_from_env("MT_TIMEOUT_SECS", DEFAULT_TIMEOUT);
    let stream_window = duration_from_env("MT_STREAM_SECS", Duration::from_secs(10));
    let address = BleAddress::new(id);

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    let (ev_tx, mut ev_rx) = mpsc::channel::<Event>(256);
    let session = DeviceSession::new(Box::new(|profile: ConnectionProfile| {
        async move {
            match profile {
                ConnectionProfile::Ble { address, .. } => {
                    let t: BoxedTransport = ble::connect(&address).await?;
                    Ok((t, TransportKind::Ble))
                }
                ConnectionProfile::Tcp { .. } | ConnectionProfile::Serial { .. } => {
                    Err(mt::error::ConnectError::BleGatt("mt-cli only supports BLE".into()))
                }
            }
        }
        .boxed()
    }));
    let runner = tokio::spawn(session.run(cmd_rx, ev_tx));

    let profile = ConnectionProfile::Ble { name: "cli".into(), address };
    if cmd_tx.send(Command::Connect(profile)).is_err() {
        eprintln_safe("session channel closed");
        return ExitCode::FAILURE;
    }

    let outcome = drive_connect(&mut ev_rx, overall, stream_window).await;
    let _ = cmd_tx.send(Command::Disconnect);
    drop(cmd_tx);
    let _ = runner.await;
    outcome
}

async fn drive_connect(
    rx: &mut mpsc::Receiver<Event>,
    overall: Duration,
    stream_window: Duration,
) -> ExitCode {
    let ready_deadline =
        tokio::time::Instant::now().checked_add(overall).unwrap_or_else(tokio::time::Instant::now);
    let connected_snapshot: Option<Box<mt::domain::snapshot::DeviceSnapshot>>;
    loop {
        let remaining = ready_deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            eprintln_safe(&format!("timed out waiting for Connected after {overall:?}"));
            return ExitCode::FAILURE;
        }
        match timeout(remaining, rx.recv()).await {
            Ok(Some(Event::Connecting)) => println_safe("… Connecting"),
            Ok(Some(Event::Connected(snap))) => {
                println_safe(&format!(
                    "✓ Connected: node {} \"{}\" [{}] fw {}\n  nodes (initial): {}\n  channels (initial): {}",
                    snap.my_node.0,
                    snap.long_name,
                    snap.short_name,
                    snap.firmware_version,
                    snap.nodes.len(),
                    snap.channels.len(),
                ));
                connected_snapshot = Some(snap);
                break;
            }
            Ok(Some(Event::Error(msg))) => {
                eprintln_safe(&format!("error: {msg}"));
            }
            Ok(Some(Event::Disconnected)) => {
                eprintln_safe("disconnected before ready");
                return ExitCode::FAILURE;
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {
                eprintln_safe("event channel closed");
                return ExitCode::FAILURE;
            }
        }
    }

    let mut nodes = connected_snapshot.map_or(0, |s| s.nodes.len());
    let mut channels_count = 0usize;
    let post_deadline = tokio::time::Instant::now()
        .checked_add(stream_window)
        .unwrap_or_else(tokio::time::Instant::now);
    while let Ok(Some(ev)) =
        timeout(post_deadline.saturating_duration_since(tokio::time::Instant::now()), rx.recv())
            .await
    {
        match ev {
            Event::NodeUpdated(_) => {
                nodes = nodes.saturating_add(1);
            }
            Event::ChannelUpdated(_) => {
                channels_count = channels_count.saturating_add(1);
            }
            Event::Disconnected | Event::Error(_) => break,
            _ => {}
        }
    }
    println_safe(&format!(
        "post-ready stream ({stream_window:?}): +{nodes} node updates, +{channels_count} channel updates"
    ));
    ExitCode::SUCCESS
}

async fn cmd_pump(id: &str) -> ExitCode {
    let address = BleAddress::new(id);
    let overall = duration_from_env("MT_TIMEOUT_SECS", DEFAULT_TIMEOUT);
    let stream_window = duration_from_env("MT_STREAM_SECS", Duration::from_secs(6));

    match timeout(overall, pump_body(&address, stream_window)).await {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(e)) => {
            eprintln_safe(&format!("pump failed: {e}"));
            ExitCode::FAILURE
        }
        Err(_) => {
            eprintln_safe(&format!("pump overall timeout {overall:?}"));
            ExitCode::FAILURE
        }
    }
}

async fn pump_body(
    address: &BleAddress,
    stream_window: Duration,
) -> Result<(), mt::error::ConnectError> {
    let transport = ble::connect(address).await?;
    let (mut sink, stream) = transport.split();
    let mut stream = Box::pin(stream);

    let cfg = ConfigId::random().0;
    let want = meshtastic::ToRadio {
        payload_variant: Some(meshtastic::to_radio::PayloadVariant::WantConfigId(cfg)),
    };
    let mut buf = Vec::with_capacity(want.encoded_len());
    want.encode(&mut buf).map_err(mt::error::ConnectError::Encode)?;
    println_safe(&format!("sending want_config_id={cfg} (len={})", buf.len()));
    sink.send(buf).await.map_err(|e| mt::error::ConnectError::BleGatt(e.to_string()))?;

    let deadline = tokio::time::Instant::now()
        .checked_add(stream_window)
        .unwrap_or_else(tokio::time::Instant::now);
    let mut count: usize = 0;
    while let Ok(Some(item)) =
        timeout(deadline.saturating_duration_since(tokio::time::Instant::now()), stream.next())
            .await
    {
        let frame = item.map_err(|e| mt::error::ConnectError::BleGatt(e.to_string()))?;
        count = count.saturating_add(1);
        match meshtastic::FromRadio::decode(frame.as_slice()) {
            Ok(msg) => println_safe(&format!("  #{count} len={} {}", frame.len(), kind(&msg))),
            Err(e) => println_safe(&format!(
                "  #{count} len={} DECODE_ERR {e}: {}",
                frame.len(),
                hex_preview(&frame),
            )),
        }
    }
    println_safe(&format!("streamed {count} messages in {stream_window:?}"));
    Ok(())
}

fn kind(msg: &meshtastic::FromRadio) -> &'static str {
    use meshtastic::from_radio::PayloadVariant;
    match &msg.payload_variant {
        Some(PayloadVariant::Packet(_)) => "Packet",
        Some(PayloadVariant::MyInfo(_)) => "MyInfo",
        Some(PayloadVariant::NodeInfo(_)) => "NodeInfo",
        Some(PayloadVariant::Config(_)) => "Config",
        Some(PayloadVariant::ModuleConfig(_)) => "ModuleConfig",
        Some(PayloadVariant::Channel(_)) => "Channel",
        Some(PayloadVariant::ConfigCompleteId(_)) => "ConfigCompleteId",
        Some(PayloadVariant::Rebooted(_)) => "Rebooted",
        Some(PayloadVariant::QueueStatus(_)) => "QueueStatus",
        Some(PayloadVariant::XmodemPacket(_)) => "XmodemPacket",
        Some(PayloadVariant::Metadata(_)) => "Metadata",
        Some(PayloadVariant::FileInfo(_)) => "FileInfo",
        Some(PayloadVariant::LogRecord(_)) => "LogRecord",
        Some(PayloadVariant::MqttClientProxyMessage(_)) => "MqttClientProxyMessage",
        Some(PayloadVariant::ClientNotification(_)) => "ClientNotification",
        Some(PayloadVariant::DeviceuiConfig(_)) => "DeviceuiConfig",
        None => "<none>",
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let take = bytes.len().min(32);
    let mut out = String::with_capacity(take.saturating_mul(2));
    for b in bytes.iter().take(take) {
        let _ = write!(out, "{b:02x}");
    }
    if bytes.len() > take {
        out.push('…');
    }
    out
}

fn println_safe(s: &str) {
    let stdout = io::stdout();
    let mut h = stdout.lock();
    let _ = h.write_all(s.as_bytes());
    let _ = h.write_all(b"\n");
}

fn eprintln_safe(s: &str) {
    let stderr = io::stderr();
    let mut h = stderr.lock();
    let _ = h.write_all(s.as_bytes());
    let _ = h.write_all(b"\n");
}
