# Boxxy Terminal Companion — COSMIC Desktop Applet

A native [COSMIC Desktop](https://github.com/pop-os/cosmic-epoch) panel applet written in Rust with `libcosmic` that serves as a companion to [Boxxy Terminal](https://github.com/boxxy-dev/boxxy). It provides a panel indicator and popup menu to monitor `boxxy-agent`, manage **Detached Background Shells**, focus active AI characters, and synchronize global character focus shortcuts with COSMIC Settings.


## Installation

Build, install, and enable the applet in your COSMIC Panel using the provided `Makefile`:

```bash
# Build release binary and install to ~/.local
make install

# Add the applet to COSMIC Panel (right wing) immediately
make enable
```


## CLI Usage

In addition to running as a panel applet when invoked with no arguments, `cosmic-boxxy-companion` provides CLI subcommands:

```bash
# Focus an active AI character by slug or display name
cosmic-boxxy-companion focus niko

# Focus the character assigned to shortcut slot 1..10
cosmic-boxxy-companion slot 1

# Restore a specific detached background shell by PID (or omit/0 to restore all)
cosmic-boxxy-companion restore 121283
cosmic-boxxy-companion restore

# Terminate a detached background shell by PID (or omit/0 to terminate all)
cosmic-boxxy-companion kill 121283

# Control the boxxy-agent daemon
cosmic-boxxy-companion start-agent
cosmic-boxxy-companion stop-agent
cosmic-boxxy-companion restart-agent
```

## D-Bus Contract (Session Bus)

- **Bus:** Session Bus
- **Service Name:** `dev.boxxy.BoxxyAgent`
- **Object Path:** `/dev/boxxy/Agent`
- **Interface:** `dev.boxxy.BoxxyTerminal.Agent`

| Method / Signal | Signature | Type | Description |
|-----------------|-----------|------|-------------|
| `GetRegistrySnapshot` | `() -> (s)` | Method | Returns a JSON string (`RegistrySnapshot`) containing the full character catalog (`catalog`) and active claims (`claims`). |
| `RequestFocusPane` | `(s) -> ()` | Method | Accepts a `pane_id` string. Signals the Boxxy UI to raise the window and focus the pane where the character is active. |
| `ClaimsChanged` | `(s)` | Signal | Emitted whenever a character is claimed or released by a terminal pane. |
| `GetDetachedSessions` | `() -> (s)` | Method | Returns a JSON array of `DetachedSessionInfo` objects for all background shells kept alive in `boxxy-agent`. |
| `RequestRestoreDetached` | `(u) -> ()` | Method | Accepts `pid` (`u32`, or `0` to restore **all** detached shells). |
| `TerminateDetachedSession` | `(u) -> ()` | Method | Accepts `pid` (`u32`, or `0` to terminate **all** detached shells). |
| `DetachedSessionsChanged` | `(s)` | Signal | Emitted whenever a shell detaches, reattaches, is terminated, or exits. |

## License

MIT
