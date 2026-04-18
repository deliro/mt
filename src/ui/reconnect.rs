use std::time::{Duration, Instant};

use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::profile::ConnectionProfile;
use crate::session::commands::Command;

/// Back-off schedule (seconds). Applied to the 1-based attempt number; the
/// last value is repeated forever so we keep trying a dead radio politely.
const BACKOFF_SCHEDULE_SECS: &[u64] = &[3, 10, 30, 60];

#[derive(Default)]
pub struct ReconnectUi {
    /// Profile to auto-reconnect to. Set on every successful user-initiated
    /// Connect; cleared on user-initiated Disconnect. Populated from the
    /// persisted `last_active` on startup so the very first boot reconnects
    /// without a click.
    pub profile: Option<ConnectionProfile>,
    /// `key()` of the profile we should persist as "last active" the next
    /// time the profile list is saved.
    pub last_active: Option<String>,
    /// Current attempt number — 0 while connected or idle, N for the Nth
    /// pending reconnect.
    pub attempt: u32,
    /// Wall-clock instant at which to fire the next reconnect attempt.
    pub next_at: Option<Instant>,
    /// `true` when the last session termination came from the user clicking
    /// Disconnect. Skips auto-reconnect until the next user Connect.
    pub intentional_disconnect: bool,
}

impl ReconnectUi {
    pub fn arm_from_startup(&mut self, profile: ConnectionProfile) {
        self.last_active = Some(profile.key());
        self.profile = Some(profile);
        self.attempt = 0;
        self.next_at = Some(Instant::now());
        self.intentional_disconnect = false;
    }

    /// User clicked Connect on a profile.
    pub fn mark_user_connect(&mut self, profile: &ConnectionProfile) {
        self.profile = Some(profile.clone());
        self.last_active = Some(profile.key());
        self.attempt = 0;
        self.next_at = None;
        self.intentional_disconnect = false;
    }

    /// User clicked Disconnect (or Cancel on a connecting attempt).
    pub fn mark_user_disconnect(&mut self) {
        self.intentional_disconnect = true;
        self.next_at = None;
        self.attempt = 0;
    }

    /// Session just reached Connected — a live link exists, so drop the
    /// retry counter.
    pub fn on_connected(&mut self) {
        self.attempt = 0;
        self.next_at = None;
        self.intentional_disconnect = false;
    }

    /// Session emitted Disconnected. If the user asked for it, stay idle;
    /// otherwise schedule the next attempt.
    pub fn on_disconnected(&mut self) {
        if self.intentional_disconnect {
            self.intentional_disconnect = false;
            return;
        }
        if self.profile.is_none() {
            return;
        }
        self.attempt = self.attempt.saturating_add(1);
        let secs = BACKOFF_SCHEDULE_SECS
            .get(usize::try_from(self.attempt.saturating_sub(1)).unwrap_or(0))
            .or_else(|| BACKOFF_SCHEDULE_SECS.last())
            .copied()
            .unwrap_or(60);
        self.next_at = Instant::now().checked_add(Duration::from_secs(secs));
    }

    /// Stop the reconnect loop (user hit Stop on the banner).
    pub fn cancel(&mut self) {
        self.profile = None;
        self.next_at = None;
        self.attempt = 0;
        self.intentional_disconnect = true;
    }

    /// If a scheduled attempt has come due, consume it and return the
    /// profile to dial.
    pub fn pop_due(&mut self, now: Instant) -> Option<ConnectionProfile> {
        let due = self.next_at?;
        if now < due {
            return None;
        }
        self.next_at = None;
        self.profile.clone()
    }
}

pub fn render_banner(
    ctx: &egui::Context,
    state: &ReconnectUi,
    disconnected: bool,
    connecting: bool,
    now: Instant,
    on_stop: &mut bool,
) {
    let Some(profile) = state.profile.as_ref() else { return };
    if !disconnected && !connecting {
        return;
    }
    if state.attempt == 0 && !connecting {
        // First automatic attempt after startup — no need for a banner yet.
        return;
    }
    egui::TopBottomPanel::top("reconnect_banner").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spinner();
            if connecting {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 200, 90),
                    format!(
                        "Reconnecting to {} — attempt {}…",
                        profile.name(),
                        state.attempt.max(1),
                    ),
                );
            } else if let Some(next) = state.next_at {
                let remain = next.saturating_duration_since(now).as_secs();
                ui.colored_label(
                    egui::Color32::from_rgb(230, 200, 90),
                    format!(
                        "Link lost. Retrying {} in {}s (attempt {}).",
                        profile.name(),
                        remain,
                        state.attempt.saturating_add(1),
                    ),
                );
            }
            if ui.button("Stop").on_hover_text("Stop auto-reconnect loop.").clicked() {
                *on_stop = true;
            }
        });
    });
}

pub fn tick(
    state: &mut ReconnectUi,
    status_is_disconnected: bool,
    now: Instant,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    if !status_is_disconnected {
        return;
    }
    let Some(profile) = state.pop_due(now) else { return };
    let _ = cmd.send(Command::Connect(profile));
}
