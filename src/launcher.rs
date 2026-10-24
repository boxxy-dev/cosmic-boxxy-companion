use image::imageops::FilterType;
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const AGENT_BINARY: &str = "boxxy-agent";
const TERMINAL_BINARY: &str = "boxxy-terminal";
const FLATPAK_APP_ID: &str = "dev.boxxy.BoxxyTerminal";

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .ok()
        .is_some_and(|m| m.is_file() && (m.permissions().mode() & 0o111 != 0))
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

pub fn resolve_agent_binary() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        let candidates = [
            home.join(".local")
                .join("boxxy-terminal")
                .join("bin")
                .join(AGENT_BINARY),
            home.join(".local")
                .join("boxxy-terminal")
                .join(AGENT_BINARY),
            home.join(".local").join("bin").join(AGENT_BINARY),
        ];
        for path in candidates {
            if is_executable(&path) {
                return path;
            }
        }
    }

    let flatpak_path = PathBuf::from("/app/bin").join(AGENT_BINARY);
    if is_executable(&flatpak_path) {
        return flatpak_path;
    }

    find_in_path(AGENT_BINARY).unwrap_or_else(|| PathBuf::from(AGENT_BINARY))
}

pub fn resolve_terminal_command() -> (PathBuf, Vec<String>) {
    if let Some(home) = dirs::home_dir() {
        let candidates = [
            home.join(".local")
                .join("boxxy-terminal")
                .join("bin")
                .join(TERMINAL_BINARY),
            home.join(".local").join("bin").join(TERMINAL_BINARY),
        ];
        for path in candidates {
            if is_executable(&path) {
                return (path, Vec::new());
            }
        }
    }

    if let Some(in_path) = find_in_path(TERMINAL_BINARY) {
        return (in_path, Vec::new());
    }

    (
        PathBuf::from("flatpak"),
        vec!["run".to_string(), FLATPAK_APP_ID.to_string()],
    )
}

pub fn spawn_agent_command(subcommand: &str) {
    let binary = resolve_agent_binary();
    log::info!("[boxxy] spawning: {} {}", binary.display(), subcommand);

    match Command::new(&binary)
        .arg(subcommand)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => log::info!("[boxxy] {} {} launched", binary.display(), subcommand),
        Err(e) => log::error!(
            "[boxxy] error launching {} {}: {}",
            binary.display(),
            subcommand,
            e
        ),
    }
}

/// Spawns `boxxy-terminal` with an optional Wayland `XDG_ACTIVATION_TOKEN`.
///
/// Because `boxxy-terminal` is a single-instance `GApplication` (`HANDLES_COMMAND_LINE`),
/// invoking it with no arguments when a window is already open forwards the activation
/// token over D-Bus and calls `window.present()` on the active window. When no window
/// is open, it launches a fresh window (which automatically drains any queued detached
/// shell restores via `TakePendingRestores`).
pub fn present_or_launch_terminal(activation_token: Option<&str>) {
    let (program, args) = resolve_terminal_command();
    log::info!(
        "[boxxy] activating/launching Boxxy Terminal: {} {:?} (token={})",
        program.display(),
        args,
        activation_token.is_some()
    );

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(token) = activation_token.filter(|t| !t.trim().is_empty()) {
        cmd.env("XDG_ACTIVATION_TOKEN", token);
        cmd.env("DESKTOP_STARTUP_ID", token);
    }

    if let Err(e) = cmd.spawn() {
        log::error!("[boxxy] error launching Boxxy Terminal: {e}");
    }
}

pub fn get_boxxy_config_dir() -> PathBuf {
    if let Ok(override_dir) = env::var("BOXXY_CONFIG_DIR") {
        let trimmed = override_dir.trim();
        let path = PathBuf::from(trimmed);
        if !trimmed.is_empty() && path.is_absolute() {
            return path;
        }
    }

    if let Some(home) = dirs::home_dir() {
        let native = home.join(".config").join("boxxy-terminal");
        if native.exists() {
            return native;
        }
        let flatpak = home
            .join(".var")
            .join("app")
            .join(FLATPAK_APP_ID)
            .join("config")
            .join("boxxy-terminal");
        if flatpak.exists() {
            return flatpak;
        }
        return native;
    }

    PathBuf::from(".config/boxxy-terminal")
}

pub fn get_char_avatar_path(slug: &str) -> Option<PathBuf> {
    if slug.trim().is_empty() {
        return None;
    }

    let primary = get_boxxy_config_dir()
        .join("boxxyclaw")
        .join("characters")
        .join(slug)
        .join("AVATAR.png");
    if primary.is_file() {
        return Some(primary);
    }

    if let Some(home) = dirs::home_dir() {
        let flatpak = home
            .join(".var")
            .join("app")
            .join(FLATPAK_APP_ID)
            .join("config")
            .join("boxxy-terminal")
            .join("boxxyclaw")
            .join("characters")
            .join(slug)
            .join("AVATAR.png");
        if flatpak.is_file() {
            return Some(flatpak);
        }
    }

    None
}

/// Decodes an avatar image, resizes it to `target_px x target_px`, and applies a
/// smooth circular alpha mask so it renders as a crisp circle in the popup menu.
pub fn load_circular_avatar_pixels(path: &Path, target_px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = std::fs::read(path).ok()?;
    decode_circular_avatar_bytes(&bytes, target_px)
}

pub fn decode_circular_avatar_bytes(bytes: &[u8], target_px: u32) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = img
        .resize_to_fill(target_px, target_px, FilterType::Lanczos3)
        .to_rgba8();

    let mut raw = resized.into_raw();
    let center = (target_px as f32 - 1.0) * 0.5;
    let radius = target_px as f32 * 0.5;

    for y in 0..target_px {
        for x in 0..target_px {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * target_px + x) * 4) as usize;
            if dist >= radius {
                raw[idx + 3] = 0;
            } else if dist > radius - 1.0 {
                // 1px anti-aliased feather edge
                let coverage = (radius - dist).clamp(0.0, 1.0);
                raw[idx + 3] = (f32::from(raw[idx + 3]) * coverage).round() as u8;
            }
        }
    }

    Some((target_px, target_px, raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use std::io::Cursor;

    #[test]
    fn circular_avatar_masks_corners() {
        let img = RgbImage::from_fn(64, 64, |_, _| image::Rgb([123, 97, 255]));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png)
            .expect("encode synthetic png");

        let (w, h, rgba) =
            decode_circular_avatar_bytes(buf.get_ref(), 48).expect("decode circular avatar");
        assert_eq!(w, 48);
        assert_eq!(h, 48);
        // Top-left corner (0, 0) must be transparent
        assert_eq!(rgba[3], 0);
        // Center pixel (24, 24) must be opaque
        let center_idx = ((24 * 48 + 24) * 4) as usize;
        assert_eq!(rgba[center_idx + 3], 255);
    }
}
