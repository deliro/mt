#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use eframe::NativeOptions;
use futures::FutureExt;
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::persist::history::{HistoryStore, default_path as history_path};
use mt::persist::profiles::{default_path, load_from};
use mt::session::commands::Command;
use mt::session::{DeviceSession, Event};
use mt::transport::{BoxedTransport, ble, serial, tcp};
use mt::ui::App;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,mt=info")))
        .try_init();

    let profiles_path = default_path();
    let profiles = load_from(&profiles_path).unwrap_or_default();
    let store = match HistoryStore::open(&history_path()) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!(%e, "failed to open history db; running without persistence");
            None
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            tracing::error!(%e, "failed to build tokio runtime");
            return Err(eframe::Error::AppCreation(Box::new(e)));
        }
    };

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
    let (ev_tx, ev_rx) = mpsc::channel::<Event>(256);

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
        rt_for_session.block_on(session.run(cmd_rx, ev_tx));
    });

    let _guard = runtime.enter();
    eframe::run_native(
        "Meshtastic",
        NativeOptions::default(),
        Box::new(move |_cc| {
            Ok(Box::new(App::new(profiles, profiles_path, cmd_tx, ev_rx, store)))
        }),
    )
}
