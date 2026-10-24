mod applet;
mod config;
mod dbus;
mod launcher;
mod models;

use config::CompanionConfig;
use launcher::{present_or_launch_terminal, spawn_agent_command};
use std::env;

fn main() -> cosmic::iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("zbus", log::LevelFilter::Warn)
        .filter_module("sctk", log::LevelFilter::Warn)
        .filter_module("winit", log::LevelFilter::Warn)
        .init();

    let args: Vec<String> = env::args().skip(1).collect();
    if let Some(first) = args.first().map(String::as_str) {
        match first {
            "focus" | "--focus" => {
                if let Some(slug) = args.get(1) {
                    run_focus_cli(slug);
                } else {
                    eprintln!("Usage: cosmic-boxxy-companion focus <character-slug>");
                }
                return Ok(());
            }
            "slot" | "--slot" => {
                if let Some(slot_str) = args.get(1)
                    && let Ok(slot_num) = slot_str.parse::<usize>()
                {
                    let cfg = CompanionConfig::load();
                    if let Some(slug) = cfg.slug_for_slot(slot_num) {
                        run_focus_cli(slug);
                    } else {
                        log::info!("[boxxy] slot {slot_num} has no character assigned");
                    }
                } else {
                    eprintln!("Usage: cosmic-boxxy-companion slot <1..10>");
                }
                return Ok(());
            }
            "restore" | "--restore" => {
                let pid = args
                    .get(1)
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                run_restore_cli(pid);
                return Ok(());
            }
            "kill" | "--kill" => {
                let pid = args
                    .get(1)
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                run_kill_cli(pid);
                return Ok(());
            }
            "start-agent" => {
                spawn_agent_command("start");
                return Ok(());
            }
            "stop-agent" => {
                spawn_agent_command("stop");
                return Ok(());
            }
            "restart-agent" => {
                spawn_agent_command("restart");
                return Ok(());
            }
            "sync-shortcuts" => {
                let cfg = CompanionConfig::load();
                if let Err(e) = cfg.sync_cosmic_shortcuts() {
                    eprintln!("Failed to sync COSMIC shortcuts: {e}");
                }
                return Ok(());
            }
            "-h" | "--help" | "help" => {
                print_help();
                return Ok(());
            }
            "-V" | "--version" | "version" => {
                println!("cosmic-boxxy-companion {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
    }

    cosmic::applet::run::<applet::BoxxyApplet>(())
}

fn run_focus_cli(slug: &str) {
    let token = env::var("XDG_ACTIVATION_TOKEN")
        .or_else(|_| env::var("DESKTOP_STARTUP_ID"))
        .ok();

    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };

    let focused = rt
        .block_on(dbus::focus_character_by_slug(slug))
        .unwrap_or(false);
    if focused {
        present_or_launch_terminal(token.as_deref());
    }
}

fn run_restore_cli(pid: u32) {
    let token = env::var("XDG_ACTIVATION_TOKEN")
        .or_else(|_| env::var("DESKTOP_STARTUP_ID"))
        .ok();

    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };

    let _ = rt.block_on(dbus::request_restore_detached(pid));
    present_or_launch_terminal(token.as_deref());
}

fn run_kill_cli(pid: u32) {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };

    let _ = rt.block_on(dbus::terminate_detached_session(pid));
}

fn print_help() {
    println!(
        "cosmic-boxxy-companion {} — COSMIC Desktop Panel Applet for Boxxy Terminal\n\n\
         USAGE:\n  \
           cosmic-boxxy-companion                  Run the COSMIC panel applet\n  \
           cosmic-boxxy-companion focus <slug>     Focus an active AI character by slug or name\n  \
           cosmic-boxxy-companion slot <1..10>     Focus the character assigned to shortcut slot 1..10\n  \
           cosmic-boxxy-companion restore [pid]    Restore detached background shell(s) (0 = all)\n  \
           cosmic-boxxy-companion kill [pid]       Terminate detached background shell(s) (0 = all)\n  \
           cosmic-boxxy-companion start-agent      Start boxxy-agent daemon\n  \
           cosmic-boxxy-companion stop-agent       Stop boxxy-agent daemon\n  \
           cosmic-boxxy-companion restart-agent    Restart boxxy-agent daemon\n  \
           cosmic-boxxy-companion sync-shortcuts   Sync configured slots to COSMIC custom shortcuts",
        env!("CARGO_PKG_VERSION")
    );
}
