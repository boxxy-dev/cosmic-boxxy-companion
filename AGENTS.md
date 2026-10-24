# Boxxy Companion (COSMIC Desktop) — Developer Context

`cosmic-boxxy-companion` is a native COSMIC Desktop panel applet written in Rust using `libcosmic` (`applet` and `applet-token` features). It mirrors the capabilities of `gnome-shell-boxxy-companion`: monitoring and controlling the `boxxy-agent` daemon, restoring or terminating detached background shells, focusing active AI characters, and managing global character focus shortcuts.

## Architecture

- **`src/main.rs` (Entry Point & CLI):**
  - When invoked with no arguments (`cosmic-boxxy-companion`), starts the panel applet via `cosmic::applet::run::<applet::BoxxyApplet>(())`.
  - Also supports CLI subcommands (`focus <slug>`, `slot <1..10>`, `restore [pid]`, `kill [pid]`, `start-agent`, `stop-agent`, `restart-agent`, `sync-shortcuts`) so global keybindings in `cosmic-comp` can invoke character focus or shell restoration with `XDG_ACTIVATION_TOKEN` forwarded from the compositor.
- **`src/applet.rs` (`BoxxyApplet`):**
  - Implements `cosmic::Application` (`APP_ID = "dev.boxxy.CosmicBoxxyCompanion"`).
  - Embeds `boxxyclaw-symbolic.svg` and `boxxyclaw-off-symbolic.svg` at compile time (`include_bytes!`) and renders them as symbolic icons that adapt to dark/light COSMIC panel themes.
  - Displays a live pill counter badge next to the panel icon whenever `detached_sessions` is non-empty, switching to warning amber (`#e5a50a`) when any detached session has `waiting_for_input: true`. Wraps the panel button in `core.applet.autosize_window(...)` so `cosmic-panel` dynamically resizes the applet surface when the badge appears or disappears.
  - Uses `cosmic::surface::action::{app_popup, destroy_popup}` for the Wayland popup window, with two views (`PopupPage::Main` and `PopupPage::Shortcuts`).
  - Subscribes to `activation_token_subscription` (`applet-token` feature) and sets `X-HostWaylandDisplay=true` in the `.desktop` file so clicking `Restore` or an active character obtains a real Wayland activation token from `cosmic-comp` and raises the `boxxy-terminal` window reliably.
- **`src/dbus.rs` (Session D-Bus Watcher & RPC):**
  - Uses `zbus` on the session bus to watch `org.freedesktop.DBus.NameOwnerChanged` for `dev.boxxy.BoxxyAgent` without auto-starting the daemon.
  - Subscribes to `ClaimsChanged` and `DetachedSessionsChanged` signals on `/dev/boxxy/Agent` (`dev.boxxy.BoxxyTerminal.Agent`) and streams `DaemonEvent` updates to the UI via `cosmic::iced::Subscription::run_with`.
  - Provides async RPC helpers for `GetRegistrySnapshot`, `GetDetachedSessions`, `RequestFocusPane(pane_id)`, `RequestRestoreDetached(pid)`, and `TerminateDetachedSession(pid)`.
- **`src/models.rs` (Protocol DTOs):**
  - Serde types matching `boxxy-claw-protocol` (`RegistrySnapshot`, `CharacterInfo`, `CharacterConfig`, `CharacterStatus`, `CharacterClaim`, `HolderKind`) and `boxxy-agent` (`DetachedSessionInfo`), plus formatting helpers (`format_short_cwd`, `format_idle_duration`, `parse_hex_color`).
- **`src/launcher.rs` (Binary & Asset Resolution):**
  - Resolves `boxxy-agent` and `boxxy-terminal` across native (`~/.local/boxxy-terminal/...`, `~/.local/bin/...`, `PATH`) and Flatpak (`dev.boxxy.BoxxyTerminal`) installations.
  - `present_or_launch_terminal(activation_token)` spawns `boxxy-terminal --present` with `XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID`. Because `boxxy-terminal` is a single-instance `GApplication` (`HANDLES_COMMAND_LINE`), `--present` either raises the existing window or launches a new window to drain pending restores via `TakePendingRestores`.
  - Resolves character `AVATAR.png` files from `~/.config/boxxy-terminal/boxxyclaw/characters/<slug>/AVATAR.png` (respecting `BOXXY_CONFIG_DIR` and Flatpak config paths) and pre-renders 48×48 circular anti-aliased RGBA handles.
- **`src/config.rs` (Shortcuts & COSMIC Sync):**
  - Persists 10 character focus slots (`slots[0..10]`) in `~/.config/boxxy-terminal/cosmic-companion.json`.
  - Synchronizes active slots into `~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom` as `Spawn("<bin> slot <1..10>")` RON entries while preserving all non-Boxxy custom shortcuts.

## D-Bus Contract (Session Bus)
- **Service / Well-known name:** `dev.boxxy.BoxxyAgent`
- **Object Path:** `/dev/boxxy/Agent`
- **Interface:** `dev.boxxy.BoxxyTerminal.Agent`

| Method / Signal | Notes |
|-----------------|-------|
| `GetRegistrySnapshot` | Method. Returns `(s)` (JSON string of `RegistrySnapshot`). |
| `RequestFocusPane(s)` | Method. Takes `pane_id` string. |
| `ClaimsChanged` | Signal. Pushed on claim change; carries `(s)` JSON payload. |
| `GetDetachedSessions` | Method. Returns `(s)` (JSON array of `DetachedSessionInfo`). |
| `RequestRestoreDetached(u)` | Method. Takes `pid` (`0` = restore all). Restores into open window or queues for next launch. |
| `TerminateDetachedSession(u)` | Method. Takes `pid` (`0` = terminate all). |
| `DetachedSessionsChanged` | Signal. Pushed on detach/reattach/exit; carries `(s)` JSON payload. |
| `GetPendingTasks` | Method. Returns `(s)` (JSON array of `PendingTaskInfo`). |
| `CancelPendingTask(s, s)` | Method. Takes `(pane_id, task_id)` and cancels the scheduled task. |
| `PendingTasksChanged` | Signal. Pushed when reminders/tasks are scheduled, completed, cancelled, or detached/reattached; carries `(s)` JSON payload. |
