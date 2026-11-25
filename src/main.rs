#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod calendar;
mod clock;
mod config;
mod lyrics;
mod platform;
mod state;
mod todo;
mod tray;
mod ui;

use std::process::ExitCode;
use std::sync::{Arc, mpsc};

use eframe::egui;

use crate::config::Config;
use crate::tray::TrayController;
use crate::ui::{AppChannels, IslandApp, WINDOW_TITLE};

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
    let tray = match TrayController::new(config.auto_hide_when_idle, config.notch_dock_mode) {
        Ok(t) => Some(t),
        Err(err) => {
            eprintln!("Warning: tray unavailable ({err})");
            None
        }
    };

    let (event_tx, event_rx) = mpsc::channel();
    let (lyrics_req_tx, lyrics_req_rx) = mpsc::channel();
    let (media_cmd_tx, media_cmd_rx) = mpsc::channel();
    let (calendar_cmd_tx, calendar_cmd_rx) = mpsc::channel();

    let ear_w = if config.notch_dock_mode {
        (12.0 * config.scale).round() * 2.0
    } else {
        0.0
    };
    let initial_w = config.idle_width * config.scale + ear_w;
    let initial_h = 30.0 * config.scale;
    let initial_x = ((platform::primary_monitor_logical_width(1.0) - initial_w) * 0.5).max(0.0);
    let initial_y = if config.notch_dock_mode {
        0.0
    } else {
        config.top_offset_px
    };

    platform::prepare_overlay_rules(WINDOW_TITLE, initial_y);

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(false)
            .with_resizable(platform::overlay_resizable())
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_close_button(false)
            .with_inner_size([initial_w, initial_h])
            .with_position([initial_x, initial_y]),
        ..Default::default()
    };

    let run = eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            ui::install_base_fallback_fonts(&cc.egui_ctx);
            let egui_ctx = cc.egui_ctx.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || egui_ctx.request_repaint());

            platform::spawn_listener(event_tx.clone(), wake.clone());
            platform::spawn_media_listener(event_tx.clone(), media_cmd_rx, wake.clone());
            lyrics::spawn_lyrics_worker(lyrics_req_rx, event_tx.clone(), wake.clone());
            calendar::spawn_calendar_worker(
                config.google_calendar_ics_url.clone(),
                calendar_cmd_rx,
                event_tx,
                wake,
            );

            Ok(Box::new(IslandApp::new(
                config,
                AppChannels {
                    event_rx,
                    lyrics_req_tx,
                    media_cmd_tx,
                    calendar_cmd_tx,
                },
                tray,
                demo_mode,
            )))
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
