use std::collections::VecDeque;
use std::time::SystemTime;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

const CAPACITY: usize = 1000;

pub struct InspectorUi {
    pub entries: VecDeque<Entry>,
    pub filter: String,
    pub paused: bool,
    pub selected: Option<u64>,
    next_id: u64,
}

impl Default for InspectorUi {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(CAPACITY),
            filter: String::new(),
            paused: false,
            selected: None,
            next_id: 0,
        }
    }
}

pub struct Entry {
    pub id: u64,
    pub at: SystemTime,
    pub frame_size: usize,
    pub variant: &'static str,
    pub debug: String,
}

impl InspectorUi {
    pub fn push(&mut self, at: SystemTime, frame_size: usize, variant: &'static str, debug: String) {
        if self.paused {
            return;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.entries.len() >= CAPACITY {
            let _ = self.entries.pop_front();
        }
        self.entries.push_back(Entry { id, at, frame_size, variant, debug });
    }
}

pub fn render(ui: &mut egui::Ui, state: &mut InspectorUi) {
    toolbar(ui, state);
    ui.separator();
    egui::SidePanel::right("inspector_detail")
        .resizable(true)
        .default_width(420.0)
        .show_inside(ui, |ui| render_detail(ui, state));
    egui::CentralPanel::default().show_inside(ui, |ui| render_list(ui, state));
}

fn toolbar(ui: &mut egui::Ui, state: &mut InspectorUi) {
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text("variant or substring")
                .desired_width(220.0),
        );
        if !state.filter.is_empty() && ui.small_button("clear").clicked() {
            state.filter.clear();
        }
        ui.separator();
        let pause_label = if state.paused { "▶ Resume" } else { "⏸ Pause" };
        if ui
            .button(pause_label)
            .on_hover_text("Stop recording new frames; the existing buffer stays.")
            .clicked()
        {
            state.paused = !state.paused;
        }
        if ui.button("Clear").on_hover_text("Drop all captured frames.").clicked() {
            state.entries.clear();
            state.selected = None;
        }
        if ui
            .button("Copy as JSON")
            .on_hover_text("Copy the currently-visible frames to the clipboard as JSON.")
            .clicked()
        {
            let text = export_json(state);
            ui.ctx().copy_text(text);
        }
        ui.separator();
        ui.label(format!("{} captured", state.entries.len()));
    });
}

fn render_list(ui: &mut egui::Ui, state: &mut InspectorUi) {
    let filter = state.filter.trim().to_lowercase();
    let rows: Vec<&Entry> = state
        .entries
        .iter()
        .rev()
        .filter(|e| {
            filter.is_empty()
                || e.variant.to_lowercase().contains(&filter)
                || e.debug.to_lowercase().contains(&filter)
        })
        .collect();

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        if rows.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.weak(if state.entries.is_empty() {
                    "Connect to a device to see FromRadio frames here."
                } else {
                    "No frames match the current filter."
                });
            });
            return;
        }
        TableBuilder::new(ui)
            .striped(true)
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::remainder())
            .header(20.0, |mut header| {
                for h in ["Time", "Variant", "Bytes", "Summary"] {
                    header.col(|ui| {
                        ui.strong(h);
                    });
                }
            })
            .body(|mut body| {
                for entry in rows {
                    body.row(18.0, |mut row| {
                        row.col(|ui| {
                            ui.monospace(format_time(entry.at));
                        });
                        row.col(|ui| {
                            ui.monospace(entry.variant);
                        });
                        row.col(|ui| {
                            ui.monospace(format!("{}", entry.frame_size));
                        });
                        row.col(|ui| {
                            let summary = summary_line(&entry.debug);
                            let is_selected = state.selected == Some(entry.id);
                            let resp = ui.add(
                                egui::Label::new(egui::RichText::new(summary).monospace())
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            );
                            if resp.clicked() {
                                state.selected = if is_selected { None } else { Some(entry.id) };
                            }
                        });
                    });
                }
            });
    });
}

fn render_detail(ui: &mut egui::Ui, state: &InspectorUi) {
    let Some(selected_id) = state.selected else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.weak("Click a frame to expand.");
        });
        return;
    };
    let Some(entry) = state.entries.iter().find(|e| e.id == selected_id) else {
        ui.weak("(entry evicted from buffer)");
        return;
    };
    ui.heading(entry.variant);
    ui.label(format!("{} bytes · {}", entry.frame_size, format_time_full(entry.at)));
    ui.separator();
    egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
        ui.add(
            egui::Label::new(egui::RichText::new(&entry.debug).monospace()).selectable(true),
        );
    });
}

fn summary_line(debug: &str) -> String {
    debug.lines().next().map(str::to_owned).unwrap_or_default()
}

fn format_time(t: SystemTime) -> String {
    let offset = crate::ui::chat::local_offset();
    let dt = time::OffsetDateTime::from(t).to_offset(offset);
    format!("{:02}:{:02}:{:02}.{:03}", dt.hour(), dt.minute(), dt.second(), dt.millisecond())
}

fn format_time_full(t: SystemTime) -> String {
    let offset = crate::ui::chat::local_offset();
    let dt = time::OffsetDateTime::from(t).to_offset(offset);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.millisecond(),
    )
}

fn export_json(state: &InspectorUi) -> String {
    use std::fmt::Write as _;
    let filter = state.filter.trim().to_lowercase();
    let mut out = String::from("[\n");
    let mut first = true;
    for entry in &state.entries {
        if !filter.is_empty()
            && !entry.variant.to_lowercase().contains(&filter)
            && !entry.debug.to_lowercase().contains(&filter)
        {
            continue;
        }
        if !first {
            out.push_str(",\n");
        }
        first = false;
        let at = format_time_full(entry.at);
        let debug_escaped = entry.debug.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
        let _ = write!(
            out,
            "  {{\"at\":\"{at}\",\"variant\":\"{v}\",\"bytes\":{b},\"debug\":\"{d}\"}}",
            v = entry.variant,
            b = entry.frame_size,
            d = debug_escaped,
        );
    }
    out.push_str("\n]\n");
    out
}
