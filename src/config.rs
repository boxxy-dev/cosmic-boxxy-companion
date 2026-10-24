use crate::launcher::get_boxxy_config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SLOT_COUNT: usize = 10;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShortcutSlot {
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub shortcut: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompanionConfig {
    #[serde(default = "default_slots")]
    pub slots: Vec<ShortcutSlot>,
}

fn default_slots() -> Vec<ShortcutSlot> {
    (0..SLOT_COUNT).map(|_| ShortcutSlot::default()).collect()
}

impl Default for CompanionConfig {
    fn default() -> Self {
        Self {
            slots: default_slots(),
        }
    }
}

impl CompanionConfig {
    pub fn config_path() -> PathBuf {
        get_boxxy_config_dir().join("cosmic-companion.json")
    }

    pub fn load() -> Self {
        Self::load_from_path(&Self::config_path())
    }

    pub fn load_from_path(path: &Path) -> Self {
        let mut cfg = fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default();
        cfg.normalize();
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        self.save_to_path(&path)?;
        let _ = self.sync_cosmic_shortcuts();
        Ok(())
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut normalized = self.clone();
        normalized.normalize();
        let json = serde_json::to_string_pretty(&normalized)?;
        fs::write(path, json)
    }

    pub fn normalize(&mut self) {
        self.slots.resize_with(SLOT_COUNT, ShortcutSlot::default);
        self.slots.truncate(SLOT_COUNT);
    }

    pub fn slug_for_slot(&self, one_based_index: usize) -> Option<&str> {
        if (1..=SLOT_COUNT).contains(&one_based_index) {
            let slug = self.slots[one_based_index - 1].slug.trim();
            if !slug.is_empty() {
                return Some(slug);
            }
        }
        None
    }

    /// Synchronizes active shortcut slots into COSMIC's native custom shortcuts config
    /// (`~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom`), preserving
    /// any existing non-Boxxy custom shortcuts.
    pub fn sync_cosmic_shortcuts(&self) -> std::io::Result<()> {
        let Some(home) = dirs::home_dir() else {
            return Ok(());
        };
        let cosmic_custom_path = home
            .join(".config")
            .join("cosmic")
            .join("com.system76.CosmicSettings.Shortcuts")
            .join("v1")
            .join("custom");

        let binary_cmd = resolve_self_binary_command();
        self.sync_cosmic_shortcuts_to_path(&cosmic_custom_path, &binary_cmd)
    }

    pub fn sync_cosmic_shortcuts_to_path(
        &self,
        custom_path: &Path,
        binary_cmd: &str,
    ) -> std::io::Result<()> {
        let existing = fs::read_to_string(custom_path).unwrap_or_default();
        let updated = merge_cosmic_custom_ron(&existing, &self.slots, binary_cmd);

        if updated.trim() == "{}" && !custom_path.exists() {
            return Ok(());
        }

        if let Some(parent) = custom_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(custom_path, updated)
    }
}

fn resolve_self_binary_command() -> String {
    if let Ok(exe) = std::env::current_exe()
        && let Some(s) = exe.to_str()
        && !s.is_empty()
    {
        return s.to_string();
    }
    "cosmic-boxxy-companion".to_string()
}

/// Parses a human or GTK-style shortcut string (e.g. `"Super+Shift+1"` or `"<Control><Alt>n"`)
/// into COSMIC's `(modifiers: [...], key: "...")` RON binding tuple.
pub fn parse_shortcut_to_cosmic_ron(shortcut: &str) -> Option<String> {
    let cleaned = shortcut
        .replace('<', "")
        .replace('>', "+")
        .trim()
        .to_string();
    if cleaned.is_empty() {
        return None;
    }

    let mut has_super = false;
    let mut has_ctrl = false;
    let mut has_alt = false;
    let mut has_shift = false;
    let mut key_part: Option<String> = None;

    for token in cleaned.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "super" | "meta" | "win" | "logo" | "mod4" => has_super = true,
            "ctrl" | "control" | "primary" => has_ctrl = true,
            "alt" | "mod1" => has_alt = true,
            "shift" => has_shift = true,
            _ => {
                let normalized_key = if token.len() == 1 {
                    token.to_ascii_lowercase()
                } else {
                    token.to_string()
                };
                key_part = Some(normalized_key);
            }
        }
    }

    let key = key_part?;
    let mut mods = Vec::new();
    if has_super {
        mods.push("Super");
    }
    if has_ctrl {
        mods.push("Ctrl");
    }
    if has_alt {
        mods.push("Alt");
    }
    if has_shift {
        mods.push("Shift");
    }

    let escaped_key = key.replace('\\', "\\\\").replace('"', "\\\"");
    Some(format!(
        "(modifiers: [{}], key: \"{}\")",
        mods.join(", "),
        escaped_key
    ))
}

/// Merges Boxxy companion shortcut slots into a COSMIC `custom` RON map string,
/// preserving all user entries that do not belong to `cosmic-boxxy-companion`.
pub fn merge_cosmic_custom_ron(
    existing_ron: &str,
    slots: &[ShortcutSlot],
    binary_cmd: &str,
) -> String {
    let mut kept_entries: Vec<String> = extract_non_boxxy_ron_entries(existing_ron);

    let escaped_bin = binary_cmd.replace('\\', "\\\\").replace('"', "\\\"");
    for (idx, slot) in slots.iter().enumerate().take(SLOT_COUNT) {
        let slug = slot.slug.trim();
        let shortcut = slot.shortcut.trim();
        if slug.is_empty() || shortcut.is_empty() {
            continue;
        }
        if let Some(binding_ron) = parse_shortcut_to_cosmic_ron(shortcut) {
            let slot_num = idx + 1;
            kept_entries.push(format!(
                "    {binding_ron}: Spawn(\"{escaped_bin} slot {slot_num}\"),"
            ));
        }
    }

    if kept_entries.is_empty() {
        "{\n}\n".to_string()
    } else {
        format!("{{\n{}\n}}\n", kept_entries.join("\n"))
    }
}

fn extract_non_boxxy_ron_entries(existing_ron: &str) -> Vec<String> {
    let trimmed = existing_ron.trim();
    let inner = trimmed
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(trimmed);

    let mut entries = Vec::new();
    let mut current = String::new();
    let mut paren_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut in_quotes = false;
    let mut prev_escape = false;

    for ch in inner.chars() {
        current.push(ch);
        if in_quotes {
            if ch == '"' && !prev_escape {
                in_quotes = false;
            }
            prev_escape = ch == '\\' && !prev_escape;
            continue;
        }

        match ch {
            '"' => in_quotes = true,
            '(' => paren_depth += 1,
            ')' => paren_depth = (paren_depth - 1).max(0),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = (bracket_depth - 1).max(0),
            ',' if paren_depth == 0 && bracket_depth == 0 => {
                let candidate = current.trim();
                if !candidate.is_empty() && !candidate.contains("cosmic-boxxy-companion") {
                    entries.push(format!("    {candidate}"));
                }
                current.clear();
            }
            _ => {}
        }
    }

    let trailing = current.trim();
    if !trailing.is_empty() && !trailing.contains("cosmic-boxxy-companion") {
        let with_comma = if trailing.ends_with(',') {
            trailing.to_string()
        } else {
            format!("{trailing},")
        };
        entries.push(format!("    {with_comma}"));
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts_to_cosmic_ron() {
        assert_eq!(
            parse_shortcut_to_cosmic_ron("Super+Shift+1").as_deref(),
            Some("(modifiers: [Super, Shift], key: \"1\")")
        );
        assert_eq!(
            parse_shortcut_to_cosmic_ron("<Control><Alt>N").as_deref(),
            Some("(modifiers: [Ctrl, Alt], key: \"n\")")
        );
        assert_eq!(parse_shortcut_to_cosmic_ron(""), None);
    }

    #[test]
    fn merges_custom_shortcuts_preserving_existing_entries() {
        let existing = r#"{
    (
        modifiers: [
            Super,
        ],
    ): System(WorkspaceOverview),
    (modifiers: [Super, Shift], key: "9"): Spawn("cosmic-boxxy-companion slot 9"),
}"#;

        let mut slots = default_slots();
        slots[0] = ShortcutSlot {
            slug: "niko".to_string(),
            shortcut: "Super+Shift+1".to_string(),
        };

        let merged = merge_cosmic_custom_ron(existing, &slots, "cosmic-boxxy-companion");
        assert!(merged.contains("System(WorkspaceOverview)"));
        assert!(!merged.contains("slot 9"));
        assert!(merged.contains(
            "(modifiers: [Super, Shift], key: \"1\"): Spawn(\"cosmic-boxxy-companion slot 1\"),"
        ));
    }

    #[test]
    fn roundtrips_companion_config_on_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cosmic-companion.json");

        let mut cfg = CompanionConfig::default();
        cfg.slots[2] = ShortcutSlot {
            slug: "kuro".to_string(),
            shortcut: "Ctrl+Alt+3".to_string(),
        };
        cfg.save_to_path(&path).expect("save config");

        let loaded = CompanionConfig::load_from_path(&path);
        assert_eq!(loaded.slug_for_slot(3), Some("kuro"));
        assert_eq!(loaded.slots[2].shortcut, "Ctrl+Alt+3");
    }
}
