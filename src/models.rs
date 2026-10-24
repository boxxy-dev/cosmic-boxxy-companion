use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HolderKind {
    Pane,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterClaim {
    pub holder_id: String,
    pub holder_kind: HolderKind,
    pub character_id: String,
    pub session_id: String,
    #[serde(default)]
    pub petname: String,
    #[serde(default)]
    pub owner_bus_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegistrySnapshot {
    #[serde(default)]
    pub catalog: Vec<CharacterInfo>,
    #[serde(default)]
    pub claims: Vec<CharacterClaim>,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterConfig {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub duties: String,
    #[serde(default)]
    pub personality: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum CharacterStatus {
    #[default]
    Available,
    Active { pane_id: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterInfo {
    pub config: CharacterConfig,
    #[serde(default)]
    pub status: CharacterStatus,
    #[serde(default)]
    pub has_avatar: bool,
}

impl CharacterInfo {
    pub fn active_pane_id(&self) -> Option<&str> {
        match &self.status {
            CharacterStatus::Active { pane_id } => Some(pane_id.as_str()),
            CharacterStatus::Available => None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active_pane_id().is_some()
    }

    pub fn display_label(&self) -> String {
        if self.config.display_name.trim().is_empty() {
            prettify_character_name(&self.config.name)
        } else {
            prettify_character_name(&self.config.display_name)
        }
    }
}

pub fn prettify_character_name(raw: &str) -> String {
    raw.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DetachedSessionInfo {
    pub pid: u32,
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub idle_secs: u64,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub foreground_process: String,
    #[serde(default)]
    pub waiting_for_input: bool,
    #[serde(default)]
    pub character_id: String,
    #[serde(default)]
    pub character_name: String,
    #[serde(default)]
    pub character_slug: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub pending_task_count: u32,
}

impl DetachedSessionInfo {
    pub fn process_label(&self) -> String {
        let raw = self.foreground_process.trim();
        if raw.is_empty() {
            let char_name = prettify_character_name(&self.character_name);
            if !char_name.is_empty() {
                return format!("{char_name} (AI Agent)");
            }
            return "Background Process".to_string();
        }
        if let Some((prefix, rest)) = raw.split_once(':') {
            let clean_rest = rest.trim().trim_matches('"');
            return format!("{}: {clean_rest}", prettify_character_name(prefix));
        }
        raw.to_string()
    }

    pub fn title_text(&self) -> String {
        let proc_name = self.process_label();
        if self.waiting_for_input {
            format!("{proc_name} — Waiting for input")
        } else {
            proc_name
        }
    }

    pub fn subtitle_text(&self, home_dir: Option<&Path>) -> String {
        let short_cwd = format_short_cwd(&self.cwd, home_dir);
        let idle_str = format_idle_duration(self.idle_secs);
        let mut out = format!("{} • {}", short_cwd, idle_str);
        let char_name = prettify_character_name(&self.character_name);
        let title_lower = self.title_text().to_lowercase();
        if !char_name.is_empty() && !title_lower.starts_with(&char_name.to_lowercase()) {
            out.push_str(&format!(" • {char_name}"));
        }
        if self.pending_task_count > 0 {
            let label = if self.pending_task_count == 1 {
                "reminder"
            } else {
                "reminders"
            };
            out.push_str(&format!(" • {} {label}", self.pending_task_count));
        }
        out
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskType {
    #[default]
    Notification,
    Command,
    Query,
    #[serde(other)]
    Other,
}

impl TaskType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Query => "Query",
            Self::Notification | Self::Other => "Reminder",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingTaskInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub pane_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub character_id: String,
    #[serde(default)]
    pub character_name: String,
    #[serde(default)]
    pub character_slug: String,
    #[serde(default)]
    pub task_type: TaskType,
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub due_at_unix: i64,
    #[serde(default)]
    pub remaining_secs: i64,
    #[serde(default)]
    pub is_detached: bool,
    #[serde(default)]
    pub pid: Option<u32>,
}

impl PendingTaskInfo {
    pub fn title_text(&self) -> String {
        let payload = if self.payload.trim().is_empty() {
            "Scheduled Reminder"
        } else {
            self.payload.trim()
        };
        let char_name = if self.character_name.trim().is_empty() {
            "AI Agent".to_string()
        } else {
            prettify_character_name(&self.character_name)
        };
        format!("{char_name}: {payload}")
    }

    pub fn subtitle_text(&self, now_unix: i64) -> String {
        let rem = if self.due_at_unix > 0 {
            (self.due_at_unix - now_unix).max(0)
        } else {
            self.remaining_secs.max(0)
        };
        let countdown = format_task_countdown(rem);
        let bg_suffix = if self.is_detached {
            " • Background"
        } else {
            ""
        };
        format!(
            "{} • {}{}",
            self.task_type.label(),
            countdown,
            bg_suffix
        )
    }
}

pub fn format_task_countdown(rem_secs: i64) -> String {
    if rem_secs <= 0 {
        "due now".to_string()
    } else if rem_secs < 60 {
        format!("in {rem_secs}s")
    } else if rem_secs < 3600 {
        let m = rem_secs / 60;
        let s = rem_secs % 60;
        if s > 0 {
            format!("in {m}m {s}s")
        } else {
            format!("in {m}m")
        }
    } else {
        let h = rem_secs / 3600;
        let m = (rem_secs % 3600) / 60;
        if m > 0 {
            format!("in {h}h {m}m")
        } else {
            format!("in {h}h")
        }
    }
}

pub fn format_short_cwd(cwd: &str, home_dir: Option<&Path>) -> String {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return "~".to_string();
    }

    if let Some(home) = home_dir
        && let Some(home_str) = home.to_str()
        && !home_str.is_empty()
        && let Some(rest) = trimmed.strip_prefix(home_str)
    {
        if rest.is_empty() {
            return "~".to_string();
        } else if rest.starts_with('/') {
            return format!("~{rest}");
        }
    }

    trimmed.to_string()
}

pub fn format_idle_duration(idle_secs: u64) -> String {
    if idle_secs < 60 {
        format!("{idle_secs}s idle")
    } else if idle_secs < 3600 {
        format!("{}m idle", idle_secs / 60)
    } else {
        format!("{}h idle", idle_secs / 3600)
    }
}

pub fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let raw = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    match raw.len() {
        6 => {
            let r = u8::from_str_radix(&raw[0..2], 16).ok()?;
            let g = u8::from_str_radix(&raw[2..4], 16).ok()?;
            let b = u8::from_str_radix(&raw[4..6], 16).ok()?;
            Some((r, g, b))
        }
        3 => {
            let r = u8::from_str_radix(&raw[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&raw[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&raw[2..3], 16).ok()? * 17;
            Some((r, g, b))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_registry_snapshot_and_active_character() {
        let json = r##"{
            "catalog": [
                {
                    "config": {
                        "id": "uuid-1",
                        "name": "niko",
                        "display_name": "Niko",
                        "color": "#7B61FF",
                        "duties": "Engineer",
                        "personality": "Direct"
                    },
                    "status": {
                        "Active": {
                            "pane_id": "pane-123"
                        }
                    },
                    "has_avatar": true
                },
                {
                    "config": {
                        "id": "uuid-2",
                        "name": "levi",
                        "display_name": "Levi",
                        "color": "#33D17A",
                        "duties": "Ops",
                        "personality": "Calm"
                    },
                    "status": "Available",
                    "has_avatar": false
                }
            ],
            "claims": [],
            "revision": 7
        }"##;

        let snapshot: RegistrySnapshot = serde_json::from_str(json).expect("valid snapshot JSON");
        assert_eq!(snapshot.revision, 7);
        assert_eq!(snapshot.catalog.len(), 2);
        assert_eq!(snapshot.catalog[0].active_pane_id(), Some("pane-123"));
        assert!(snapshot.catalog[0].is_active());
        assert_eq!(snapshot.catalog[0].display_label(), "Niko");
        assert_eq!(snapshot.catalog[1].active_pane_id(), None);
        assert!(!snapshot.catalog[1].is_active());
    }

    #[test]
    fn formats_detached_session_labels() {
        let home = PathBuf::from("/tmp/test-home");
        let session = DetachedSessionInfo {
            pid: 4242,
            pane_id: "pane-abc".to_string(),
            idle_secs: 125,
            cwd: "/tmp/test-home/projects/boxxy".to_string(),
            foreground_process: "cargo".to_string(),
            waiting_for_input: true,
            ..Default::default()
        };

        assert_eq!(session.title_text(), "cargo — Waiting for input");
        assert_eq!(
            session.subtitle_text(Some(&home)),
            "~/projects/boxxy • 2m idle"
        );
    }

    #[test]
    fn parses_hex_colors() {
        assert_eq!(parse_hex_color("#7B61FF"), Some((0x7B, 0x61, 0xFF)));
        assert_eq!(parse_hex_color("#f0a"), Some((0xFF, 0x00, 0xAA)));
        assert_eq!(parse_hex_color("invalid"), None);
    }

    #[test]
    fn formats_pending_task_info_and_agent_detached_session() {
        let home = PathBuf::from("/tmp/test-home");
        let session = DetachedSessionInfo {
            pid: 9001,
            pane_id: "pane-xyz".to_string(),
            idle_secs: 15,
            cwd: "/tmp/test-home".to_string(),
            foreground_process: "niko: \"Wake me up\"".to_string(),
            waiting_for_input: false,
            character_name: "niko".to_string(),
            pending_task_count: 1,
            ..Default::default()
        };
        assert_eq!(session.title_text(), "Niko: Wake me up");
        assert_eq!(
            session.subtitle_text(Some(&home)),
            "~ • 15s idle • 1 reminder"
        );

        let task = PendingTaskInfo {
            id: "task-1".to_string(),
            pane_id: "pane-xyz".to_string(),
            session_id: "sess-1".to_string(),
            character_id: "char-1".to_string(),
            character_name: "niko".to_string(),
            character_slug: "niko".to_string(),
            task_type: TaskType::Notification,
            payload: "Wake me up".to_string(),
            due_at_unix: 1095,
            remaining_secs: 95,
            is_detached: true,
            pid: Some(9001),
        };
        assert_eq!(task.title_text(), "Niko: Wake me up");
        assert_eq!(
            task.subtitle_text(1000),
            "Reminder • in 1m 35s • Background"
        );
    }
}
