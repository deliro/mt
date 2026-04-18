use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::domain::ids::NodeId;
use crate::domain::message::{Direction, Recipient, TextMessage};
use crate::domain::node::Node;

/// Persistent alert configuration. Stored in the `settings` table under
/// the `alerts` key as JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Master kill switch. When `false`, nothing fires regardless of the
    /// individual rules below.
    pub enabled: bool,
    /// Fire on every incoming direct message.
    pub notify_on_dm: bool,
    /// Case-insensitive substrings; any match in message text or sender
    /// name fires an alert.
    pub keywords: Vec<String>,
    /// Per-node battery thresholds. Alert fires when the battery level
    /// crosses from ≥ threshold to < threshold.
    pub battery_rules: Vec<BatteryRule>,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            notify_on_dm: true,
            keywords: Vec::new(),
            battery_rules: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatteryRule {
    pub node: NodeId,
    pub threshold_percent: u8,
}

/// Mutable runtime state the evaluator needs between events.
#[derive(Default)]
pub struct AlertRuntime {
    /// Last battery reading seen per node — used to detect the
    /// ≥threshold → <threshold transition exactly once per crossing.
    pub last_battery: HashMap<NodeId, u8>,
}

pub struct AlertEvent {
    pub title: String,
    pub body: String,
}

/// Evaluate rules for an incoming text message. Returns the alerts to fire.
pub fn on_message(
    cfg: &AlertConfig,
    my_node: NodeId,
    msg: &TextMessage,
    sender_name: &str,
) -> Vec<AlertEvent> {
    if !cfg.enabled {
        return Vec::new();
    }
    if msg.direction != Direction::Incoming {
        return Vec::new();
    }
    let mut out = Vec::new();
    let is_dm = matches!(msg.to, Recipient::Node(id) if id == my_node);
    if is_dm && cfg.notify_on_dm {
        out.push(AlertEvent {
            title: format!("DM from {sender_name}"),
            body: msg.text.clone(),
        });
    }
    if !cfg.keywords.is_empty() {
        let text_lc = msg.text.to_lowercase();
        let name_lc = sender_name.to_lowercase();
        for kw in &cfg.keywords {
            let needle = kw.trim().to_lowercase();
            if needle.is_empty() {
                continue;
            }
            if text_lc.contains(&needle) || name_lc.contains(&needle) {
                out.push(AlertEvent {
                    title: format!("Keyword: {kw}"),
                    body: format!("{sender_name}: {}", msg.text),
                });
                break;
            }
        }
    }
    out
}

/// Evaluate rules for a node update. Returns alerts to fire and mutates
/// the runtime's `last_battery` bookkeeping.
pub fn on_node(
    cfg: &AlertConfig,
    runtime: &mut AlertRuntime,
    node: &Node,
) -> Vec<AlertEvent> {
    if !cfg.enabled {
        return Vec::new();
    }
    let Some(level) = node.battery_level else { return Vec::new() };
    let previous = runtime.last_battery.insert(node.id, level);
    let mut out = Vec::new();
    for rule in &cfg.battery_rules {
        if rule.node != node.id {
            continue;
        }
        let was_above = previous.is_none_or(|p| p >= rule.threshold_percent);
        let now_below = level < rule.threshold_percent;
        if was_above && now_below {
            let display = display_name(node);
            out.push(AlertEvent {
                title: format!("{display}: battery below {}%", rule.threshold_percent),
                body: format!("Current: {level}%"),
            });
        }
    }
    out
}

pub fn fire(event: &AlertEvent) {
    let _ = notify_rust::Notification::new()
        .summary(&event.title)
        .body(&event.body)
        .appname("mt")
        .show();
}

fn display_name(node: &Node) -> String {
    if !node.long_name.is_empty() {
        node.long_name.clone()
    } else if !node.short_name.is_empty() {
        node.short_name.clone()
    } else {
        format!("!{:08x}", node.id.0)
    }
}
