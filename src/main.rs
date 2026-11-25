#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod platform;
mod state;
mod tray;
mod ui;

use std::process::ExitCode;
use std::sync::{Arc, mpsc};

use eframe::egui;

use crate::config::Config;
use crate::tray::TrayController;
use crate::ui::{IslandApp, WINDOW_TITLE};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--spike-notifications") {
        match platform::run_notification_spike() {
            Ok(summary) => {
                println!("SPIKE OK: {summary}");
                return ExitCode::SUCCESS;
            }
            Err(err) => {
                eprintln!("SPIKE ERR: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    let demo_mode = args.iter().any(|a| a == "--demo");
    let config = Config::load();
    let tray = match TrayController::new(config.auto_hide_when_idle) {
        Ok(t) => Some(t),
        Err(err) => {
            eprintln!("Warning: tray unavailable ({err})");
            None
        }
    };

    let (tx, rx) = mpsc::channel();
    let initial_w = 132.0;
    let initial_x = ((platform::primary_monitor_logical_width() - initial_w) * 0.5).max(0.0);

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(false)
            .with_resizable(false)
            .with_inner_size([initial_w, 30.0])
            .with_position([initial_x, config.top_offset_px]),
        ..Default::default()
    };

    let run = eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            let egui_ctx = cc.egui_ctx.clone();
            let wake = Arc::new(move || egui_ctx.request_repaint());
            platform::spawn_listener(tx, wake);
            Ok(Box::new(IslandApp::new(config, rx, tray, demo_mode)))
        }),
    );

    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Fatal UI error: {err}");
            ExitCode::FAILURE
        }
    }
}
