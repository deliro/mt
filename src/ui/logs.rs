use std::collections::VecDeque;
use std::time::SystemTime;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

const CAPACITY: usize = 5000;

pub struct LogsUi {
    pub entries: VecDeque<Entry>,
    pub filter: String,
    pub min_level: Level,
    pub paused: bool,
}

impl Default for LogsUi {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(CAPACITY),
            filter: String::new(),
            min_level: Level::Debug,
            paused: false,
        }
    }
}

pub struct Entry {
    pub at: SystemTime,
    pub level: Level,
    pub source: String,
    pub message: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Critical,
    Unknown,
}

impl Level {
    pub const fn from_proto(v: i32) -> Self {
        match v {
            5 => Self::Trace,
            10 => Self::Debug,
            20 => Self::Info,
            30 => Self::Warning,
            40 => Self::Error,
            50 => Self::Critical,
            _ => Self::Unknown,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warning => "WARN",
            Self::Error => "ERROR",
            Self::Critical => "CRIT",
            Self::Unknown => "?",
        }
    }

    const fn color(self) -> egui::Color32 {
        match self {
            Self::Trace => egui::Color32::from_rgb(130, 130, 130),
            Self::Debug => egui::Color32::from_rgb(160, 160, 160),
            Self::Info => egui::Color32::from_rgb(200, 200, 200),
            Self::Warning => egui::Color32::from_rgb(230, 200, 90),
            Self::Error => egui::Color32::from_rgb(230, 120, 120),
            Self::Critical => egui::Color32::from_rgb(255, 80, 80),
            Self::Unknown => egui::Color32::GRAY,
        }
    }
}

impl LogsUi {
    pub fn push(&mut self, at: SystemTime, level: i32, source: String, message: String) {
        if self.paused {
            return;
        }
        if self.entries.len() >= CAPACITY {
            let _ = self.entries.pop_front();
        }
        self.entries.push_back(Entry { at, level: Level::from_proto(level), source, message });
    }
}

pub fn render(ui: &mut egui::Ui, state: &mut LogsUi) {
    toolbar(ui, state);
    ui.separator();
    render_list(ui, state);
}

fn toolbar(ui: &mut egui::Ui, state: &mut LogsUi) {
    ui.horizontal(|ui| {
        ui.label("Min level:");
        for level in
            [Level::Trace, Level::Debug, Level::Info, Level::Warning, Level::Error, Level::Critical]
        {
            if ui.selectable_label(state.min_level == level, level.label()).clicked() {
                state.min_level = level;
            }
        }
        ui.separator();
        ui.label("Filter:");
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text("source or message")
                .desired_width(220.0),
        );
        if !state.filter.is_empty() && ui.small_button("clear").clicked() {
            state.filter.clear();
        }
        ui.separator();
        let pause_label = if state.paused { "▶ Resume" } else { "⏸ Pause" };
        if ui.button(pause_label).clicked() {
            state.paused = !state.paused;
        }
        if ui.button("Clear").clicked() {
            state.entries.clear();
        }
        if ui
            .button("Copy")
            .on_hover_text("Copy the visible (filtered) rows to the clipboard.")
            .clicked()
        {
            let text = export_text(state);
            ui.ctx().copy_text(text);
        }
        ui.separator();
        ui.label(format!("{} captured", state.entries.len()));
    });
}

fn render_list(ui: &mut egui::Ui, state: &LogsUi) {
    let filter = state.filter.trim().to_lowercase();
    let rows: Vec<&Entry> = state
        .entries
        .iter()
        .rev()
        .filter(|e| e.level >= state.min_level || e.level == Level::Unknown)
        .filter(|e| {
            filter.is_empty()
                || e.source.to_lowercase().contains(&filter)
                || e.message.to_lowercase().contains(&filter)
        })
        .collect();

    if rows.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.weak(if state.entries.is_empty() {
                "No logs yet. Enable 'Debug log over API' in Settings → Security to have \
                 the device stream its internal log here."
            } else {
                "No entries match the current filter."
            });
        });
        return;
    }

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::remainder())
            .header(20.0, |mut header| {
                for h in ["Time", "Level", "Source", "Message"] {
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
                            ui.colored_label(entry.level.color(), entry.level.label());
                        });
                        row.col(|ui| {
                            ui.monospace(&entry.source);
                        });
                        row.col(|ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&entry.message).monospace())
                                    .truncate(),
                            )
                            .on_hover_text(&entry.message);
                        });
                    });
                }
            });
    });
}

fn format_time(t: SystemTime) -> String {
    let offset = crate::ui::chat::local_offset();
    let dt = time::OffsetDateTime::from(t).to_offset(offset);
    format!("{:02}:{:02}:{:02}.{:03}", dt.hour(), dt.minute(), dt.second(), dt.millisecond())
}

fn export_text(state: &LogsUi) -> String {
    use std::fmt::Write as _;
    let filter = state.filter.trim().to_lowercase();
    let mut out = String::new();
    for entry in &state.entries {
        if entry.level < state.min_level && entry.level != Level::Unknown {
            continue;
        }
        if !filter.is_empty()
            && !entry.source.to_lowercase().contains(&filter)
            && !entry.message.to_lowercase().contains(&filter)
        {
            continue;
        }
        let _ = writeln!(
            out,
            "{} [{}] {}: {}",
            format_time(entry.at),
            entry.level.label(),
            entry.source,
            entry.message,
        );
    }
    out
}
