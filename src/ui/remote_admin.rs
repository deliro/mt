use eframe::egui;
use tokio::sync::mpsc;

use crate::domain::ids::NodeId;
use crate::domain::snapshot::DeviceSnapshot;
use crate::session::commands::{AdminAction, Command};

#[derive(Default)]
pub struct RemoteAdminUi {
    pub target: Option<NodeId>,
    pub pending: Option<AdminAction>,
    pub last_dispatched: Option<String>,
}

pub fn render(
    ctx: &egui::Context,
    snapshot: &DeviceSnapshot,
    ui_state: &mut RemoteAdminUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let Some(target) = ui_state.target else { return };
    let target_name = snapshot
        .nodes
        .get(&target)
        .map_or_else(|| format!("!{:08x}", target.0), display_name);
    let target_pubkey_empty = snapshot
        .nodes
        .get(&target)
        .is_some_and(|n| n.public_key.is_empty());

    let mut close = false;
    let mut picked: Option<AdminAction> = None;
    egui::Window::new(format!("Remote admin: {target_name}"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(420.0);
            if target_pubkey_empty {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "⚠ This node has no public key. Remote admin needs firmware ≥ 2.5 on the target.",
                );
            }
            ui.label(
                "Sends the selected admin command to the target node over the mesh. The \
                 target accepts it only if YOUR public key is in its Admin keys list.",
            );
            ui.separator();
            for action in remote_admin_actions() {
                if action_button(ui, action).clicked() {
                    picked = Some(action);
                }
            }
            ui.separator();
            if let Some(last) = ui_state.last_dispatched.as_ref() {
                ui.colored_label(
                    egui::Color32::LIGHT_GREEN,
                    format!("Sent: {last} (ack via routing may take a moment)"),
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
    if let Some(action) = picked {
        ui_state.pending = Some(action);
    }
    if close {
        ui_state.target = None;
        ui_state.pending = None;
        ui_state.last_dispatched = None;
    }

    confirm_modal(ctx, target, ui_state, cmd);
}

fn confirm_modal(
    ctx: &egui::Context,
    target: NodeId,
    ui_state: &mut RemoteAdminUi,
    cmd: &mpsc::UnboundedSender<Command>,
) {
    let Some(action) = ui_state.pending else { return };
    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new(format!("Confirm remote: {}", action.label()))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            if action.is_destructive() {
                ui.colored_label(egui::Color32::LIGHT_RED, "⚠ Destructive action");
            }
            ui.label(action.warning());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                let btn = if action.is_destructive() {
                    egui::Button::new("Yes, do it").fill(egui::Color32::from_rgb(150, 40, 40))
                } else {
                    egui::Button::new("Confirm")
                };
                if ui.add(btn).clicked() {
                    confirm = true;
                }
            });
        });
    if confirm {
        let _ = cmd.send(Command::RemoteAdmin { target, action });
        ui_state.last_dispatched = Some(action.label().into());
        ui_state.pending = None;
    } else if cancel {
        ui_state.pending = None;
    }
}

fn action_button(ui: &mut egui::Ui, action: AdminAction) -> egui::Response {
    let label = action.label();
    let btn = if action.is_destructive() {
        egui::Button::new(label).fill(egui::Color32::from_rgb(90, 30, 30))
    } else {
        egui::Button::new(label)
    };
    ui.add_sized([ui.available_width(), 28.0], btn).on_hover_text(action.warning())
}

const fn remote_admin_actions() -> [AdminAction; 6] {
    [
        AdminAction::Reboot { seconds: 5 },
        AdminAction::Shutdown { seconds: 5 },
        AdminAction::RebootOta { seconds: 5 },
        AdminAction::NodedbReset,
        AdminAction::FactoryResetConfig,
        AdminAction::FactoryResetDevice,
    ]
}

fn display_name(node: &crate::domain::node::Node) -> String {
    if !node.long_name.is_empty() {
        node.long_name.clone()
    } else if !node.short_name.is_empty() {
        node.short_name.clone()
    } else {
        format!("!{:08x}", node.id.0)
    }
}
