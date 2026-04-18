#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use eframe::NativeOptions;
use futures::FutureExt;
use mt::domain::profile::{ConnectionProfile, TransportKind};
use mt::persist::history::{HistoryStore, default_path as history_path};
use mt::persist::profiles::{default_path as legacy_profiles_path, load_from as load_legacy_profiles};
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
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,mt=info")))
        .try_init();

    let store = match HistoryStore::open(&history_path()) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::warn!(%e, "failed to open history db; running without persistence");
            None
        }
    };

    let (profiles, last_active_key) = load_profiles_and_migrate(store.as_ref());

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
        Box::new(move |cc| {
            mt::ui::install_fonts(&cc.egui_ctx);
            Ok(Box::new(App::new(profiles, last_active_key, cmd_tx, ev_rx, store)))
        }),
    )
}

/// Load the profile list and last-active key from the `SQLite` store,
/// migrating from the legacy `profiles.toml` file on first run. The TOML
/// file is renamed to `profiles.toml.migrated` after a successful import
/// so the user can see where their old data went.
fn load_profiles_and_migrate(store: Option<&HistoryStore>) -> (Vec<ConnectionProfile>, Option<String>) {
    let Some(store) = store else {
        // Fall back to the legacy file if we couldn't open the DB at all.
        let stored = load_legacy_profiles(&legacy_profiles_path()).unwrap_or_default();
        return (stored.profiles, stored.last_active);
    };
    let mut profiles = store.load_profiles().unwrap_or_default();
    let mut last_active = store.load_last_active().unwrap_or_default();
    if profiles.is_empty() && last_active.is_none() {
        let legacy_path = legacy_profiles_path();
        if let Ok(stored) = load_legacy_profiles(&legacy_path)
            && (!stored.profiles.is_empty() || stored.last_active.is_some())
        {
            if let Err(e) = store.save_profiles(&stored.profiles) {
                tracing::warn!(%e, "migrating legacy profiles into sqlite failed");
            } else if let Err(e) = store.save_last_active(stored.last_active.as_deref()) {
                tracing::warn!(%e, "migrating last_active into sqlite failed");
            } else {
                profiles = stored.profiles;
                last_active = stored.last_active;
                let backup = legacy_path.with_extension("toml.migrated");
                if let Err(e) = std::fs::rename(&legacy_path, &backup) {
                    tracing::warn!(%e, ?backup, "renaming legacy profiles.toml failed");
                }
            }
        }
    }
    (profiles, last_active)
}
