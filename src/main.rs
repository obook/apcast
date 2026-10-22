//! main.rs
//! apcast: cast the screen and sound of a Linux Wayland desktop to a screen mirroring receiver.
//! This file sets up the log, reads the command line and runs the command:
//!   capture/ (PC side), device/ (finding receivers), sender/ (the receiver protocol, a stub
//!   for now), menu/ (terminal interface), session/ (how a cast runs).
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: MIT

mod capture;
mod cli;
mod device;
mod menu;
mod sender;
mod session;

use std::io::{IsTerminal, Write};
use std::time::Duration;

use anyhow::bail;
use clap::Parser;
use cli::{Cli, Command};
use session::{pick_device, spin, stop_flag, Out};
use tracing_subscriber::EnvFilter;

/// Set while the menu runs: logs go to this file, opened once.
static MENU_LOG: std::sync::OnceLock<std::fs::File> = std::sync::OnceLock::new();

/// Menu log, on the per-user tmpfs (private, cleared at logout).
fn log_path() -> std::path::PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, Into::into).join("apcast.log")
}

fn main() -> anyhow::Result<()> {
    // RUST_LOG prints to stderr. The menu owns the terminal: it logs to a file instead, always,
    // so a problem seen in the menu can be read afterwards.
    let verbose = std::env::var_os("RUST_LOG").is_some();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("apcast=info")))
        .with_ansi(false)
        .with_writer(move || -> Box<dyn Write> {
            if let Some(file) = MENU_LOG.get() {
                Box::new(file)
            } else if verbose {
                Box::new(std::io::stderr())
            } else {
                Box::new(std::io::sink())
            }
        })
        .init();

    // `apcast` alone opens the cast menu.
    let cli = if std::env::args_os().len() == 1 { Cli::parse_from(["apcast", "cast"]) } else { Cli::parse() };
    match cli.command {
        Command::List { timeout } => {
            let devices = spin(session::SEARCHING, || device::discovery::discover(Duration::from_secs(timeout)))?;
            if devices.is_empty() {
                bail!("{}", device::discovery::NONE_FOUND);
            }
            for d in devices {
                println!("{}\t{}:{}\t{}\t{}", d.name, d.ip, d.port, d.model, d.id);
            }
            Ok(())
        }
        Command::Cast { device, audio_delay, stream } => {
            let settings = menu::Settings { stream, audio_delay };
            // In a terminal without --device, the menu picks the receiver and runs the casts.
            let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
            if device.is_none() && interactive {
                return session::run_menu(settings);
            }
            // Find the receiver before the portal dialog, so a typo fails fast.
            let device = pick_device(device.as_deref())?;
            let screen = capture::ScreenCast::open()?;
            session::cast_screen(&device, &settings, &screen, stop_flag()?, &Out::Term)
        }
    }
}
