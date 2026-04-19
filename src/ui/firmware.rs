use eframe::egui;

/// Minimum firmware that supports the full parity feature set — specifically
/// Curve25519 DM encryption and admin_key-based remote admin.
pub const MIN_PARITY_MAJOR: u32 = 2;
pub const MIN_PARITY_MINOR: u32 = 5;

/// Parse a Meshtastic firmware version string (e.g. `"2.5.4.abcdef"`,
/// `"2.4.0-beta"`, `"1.3.7"`) into (major, minor). Unknown parts are ignored.
pub fn parse_version(raw: &str) -> Option<(u32, u32)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

pub fn is_below_parity(raw: &str) -> bool {
    match parse_version(raw) {
        Some((major, minor)) => {
            major < MIN_PARITY_MAJOR || (major == MIN_PARITY_MAJOR && minor < MIN_PARITY_MINOR)
        }
        None => false,
    }
}

pub fn render_banner_if_old(ctx: &egui::Context, firmware: &str) {
    if !is_below_parity(firmware) {
        return;
    }
    egui::TopBottomPanel::top("firmware_banner").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(230, 200, 90),
                format!(
                    "⚠ Firmware {firmware} is older than {MIN_PARITY_MAJOR}.{MIN_PARITY_MINOR}. \
                     Remote admin and DM encryption (per-node public keys) need firmware ≥ \
                     {MIN_PARITY_MAJOR}.{MIN_PARITY_MINOR} on both ends — some features in this \
                     client will be silently ignored.",
                ),
            );
        });
    });
}
