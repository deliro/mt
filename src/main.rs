#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, OnceLock};

use eframe::NativeOptions;
use futures::FutureExt;
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::persist::history::{HistoryStore, default_path as history_path};
use mt::session::commands::Command;
use mt::session::{DeviceSession, Event};
use mt::transport::{BoxedTransport, ble, serial, tcp};
use mt::ui::App;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    // Must run before any other thread is spawned: on Linux the localtime
    // probe is only sound in a single-threaded process.
    let _ = mt::ui::chat::local_offset();

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,mt=info")),
        )
        .try_init();

    let store = match HistoryStore::open(&history_path()) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!(%e, "failed to open history db; running without persistence");
            None
        }
    };

    let profiles = store.as_ref().and_then(|s| s.load_profiles().ok()).unwrap_or_default();
    let last_active_key = store.as_ref().and_then(|s| s.load_last_active().ok()).flatten();
    let nodes_sort = store
        .as_ref()
        .and_then(|s| s.load_nodes_sort_json().ok().flatten())
        .and_then(|blob| serde_json::from_str(&blob).ok())
        .unwrap_or_default();
    let alerts = store
        .as_ref()
        .and_then(|s| s.load_alerts_json().ok().flatten())
        .and_then(|blob| serde_json::from_str(&blob).ok())
        .unwrap_or_default();

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            tracing::error!(%e, "failed to build tokio runtime");
            return Err(eframe::Error::AppCreation(Box::new(e)));
        }
    };

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    // Session writes events to `raw_ev_tx`; the forwarder below pushes them to
    // `ev_tx` (read by the App) and wakes the egui run-loop. This way the UI
    // sleeps until something actually happens — no idle 10Hz repaint.
    let (raw_ev_tx, mut raw_ev_rx) = mpsc::channel::<Event>(256);
    let (ev_tx, ev_rx) = mpsc::channel::<Event>(256);
    let ctx_holder: Arc<OnceLock<eframe::egui::Context>> = Arc::new(OnceLock::new());

    {
        let ctx_holder = ctx_holder.clone();
        runtime.spawn(async move {
            while let Some(ev) = raw_ev_rx.recv().await {
                if ev_tx.send(ev).await.is_err() {
                    break;
                }
                if let Some(ctx) = ctx_holder.get() {
                    ctx.request_repaint();
                }
            }
        });
    }

    let rt_for_session = runtime.clone();
    std::thread::spawn(move || {
        let session = DeviceSession::new(Box::new(|profile: ConnectionProfile| {
            async move {
                match profile {
                    ConnectionProfile::Tcp { host, port, .. } => {
                        let transport: BoxedTransport = tcp::connect(&host, port).await?;
                        Ok((transport, TransportKind::Tcp))
                    }
                    ConnectionProfile::Serial { path, .. } => {
                        let transport: BoxedTransport = serial::connect(&path)?;
                        Ok((transport, TransportKind::Serial))
                    }
                    ConnectionProfile::Ble { address, .. } => {
                        let transport: BoxedTransport = ble::connect(&address).await?;
                        Ok((transport, TransportKind::Ble))
                    }
                }
            }
            .boxed()
        }));
        rt_for_session.block_on(session.run(cmd_rx, raw_ev_tx));
    });

    let _guard = runtime.enter();
    eframe::run_native(
        "Meshtastic",
        NativeOptions::default(),
        Box::new(move |cc| {
            mt::ui::install_fonts(&cc.egui_ctx);
            let _ = ctx_holder.set(cc.egui_ctx.clone());
            Ok(Box::new(App::new(
                profiles,
                last_active_key,
                nodes_sort,
                alerts,
                cmd_tx,
                ev_rx,
                store,
            )))
        }),
    )
}
