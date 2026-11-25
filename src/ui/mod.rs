use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, RichText, Stroke, Vec2, ViewportCommand};

use crate::config::Config;
use crate::platform::{self, PlatformEvent};
use crate::state::{IslandState, IslandVisualState, ListenerStatus, NotificationItem};
use crate::tray::{TrayAction, TrayController};

pub const WINDOW_TITLE: &str = "Liebert Dynamic Island";

const IDLE_SIZE: Vec2 = Vec2::new(132.0, 30.0);
const COMPACT_SIZE: Vec2 = Vec2::new(330.0, 56.0);
const EXPANDED_SIZE: Vec2 = Vec2::new(368.0, 122.0);
const WARNING_SIZE: Vec2 = Vec2::new(330.0, 44.0);

pub struct IslandApp {
    config: Config,
    state: IslandState,
    rx: mpsc::Receiver<PlatformEvent>,
    tray: Option<TrayController>,
    current_size: Vec2,
    applied_size: Vec2,
    last_frame: Instant,
    no_activate_applied: bool,
    last_visible: bool,
    demo_deadline: Option<Instant>,
}

impl IslandApp {
    pub fn new(
        config: Config,
        rx: mpsc::Receiver<PlatformEvent>,
        tray: Option<TrayController>,
        demo_mode: bool,
    ) -> Self {
        let timeout = Duration::from_secs(config.collapse_timeout_secs.max(1));
        let state = IslandState::new(config.auto_hide_when_idle, timeout);
        let now = Instant::now();
        Self {
            config,
            state,
            rx,
            tray,
            current_size: IDLE_SIZE,
            applied_size: IDLE_SIZE,
            last_frame: now,
            no_activate_applied: false,
            last_visible: true,
            demo_deadline: demo_mode.then(|| now + Duration::from_secs(1)),
        }
    }

    fn target_size(&self) -> Vec2 {
        match self.state.visual {
            IslandVisualState::Idle => {
                if matches!(self.state.listener_status, ListenerStatus::AccessDenied(_)) {
                    WARNING_SIZE
                } else {
                    IDLE_SIZE
                }
            }
            IslandVisualState::Compact => COMPACT_SIZE,
            IslandVisualState::Expanded => EXPANDED_SIZE,
        }
    }

    fn sync_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.update_status(&self.state.listener_status, self.state.paused);
        }
    }

    fn handle_events(&mut self, ctx: &egui::Context, now: Instant) {
        if let Some(demo_at) = self.demo_deadline
            && now >= demo_at
        {
            self.demo_deadline = None;
            self.state.push_notification(
                NotificationItem {
                    id: "demo-1".to_owned(),
                    app_name: "Liebert Demo".to_owned(),
                    title: "Dynamic Island Ready".to_owned(),
                    body: "Hover untuk expand dan baca pesan lengkap, atau klik Dismiss."
                        .to_owned(),
                    icon_rgba: None,
                    timestamp: 0,
                },
                now,
            );
        }

        let mut status_changed = false;
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                PlatformEvent::Notification(item) => self.state.push_notification(item, now),
                PlatformEvent::Removed(id) => self.state.remove_by_id(&id, now),
                PlatformEvent::Status(status) => {
                    self.state.listener_status = status;
                    status_changed = true;
                }
            }
        }

        if let Some(tray) = &self.tray {
            for action in tray.poll_actions() {
                match action {
                    TrayAction::TogglePause => {
                        self.state.set_paused(!self.state.paused);
                        status_changed = true;
                    }
                    TrayAction::ToggleAutoHide => {
                        let checked = tray.is_auto_hide_checked();
                        self.state.auto_hide = checked;
                        self.config.auto_hide_when_idle = checked;
                        if let Err(err) = self.config.save() {
                            eprintln!("Failed to save config: {err}");
                        }
                    }
                    TrayAction::Exit => {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
            }
        }

        if status_changed {
            self.sync_tray();
        }
    }
}

impl eframe::App for IslandApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.no_activate_applied {
            platform::apply_no_activate_hook(WINDOW_TITLE);
            self.no_activate_applied = true;
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.001, 0.05);
        self.last_frame = now;

        self.handle_events(ctx, now);
        self.state.tick(now);

        let visible = self.state.should_be_visible();
        if visible != self.last_visible {
            self.last_visible = visible;
            ctx.send_viewport_cmd(ViewportCommand::Visible(visible));
        }

        let target = self.target_size();
        let t = 1.0 - (-14.0 * dt).exp();
        self.current_size += (target - self.current_size) * t;
        let animating = (target - self.current_size).length_sq() > 0.25;
        if !animating {
            self.current_size = target;
        }

        if (self.current_size - self.applied_size).length_sq() > 0.25 {
            self.applied_size = self.current_size;
            let screen_w = platform::primary_monitor_logical_width();
            let x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.current_size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
                x,
                self.config.top_offset_px,
            )));
        }

        if animating {
            ctx.request_repaint();
        } else if let Some(deadline) = self.state.next_deadline() {
            let wait = deadline.saturating_duration_since(now).min(Duration::from_millis(250));
            ctx.request_repaint_after(wait);
        } else if let Some(demo_at) = self.demo_deadline {
            ctx.request_repaint_after(demo_at.saturating_duration_since(now));
        } else {
            // Low-frequency wake only to poll tray events without busy CPU.
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        let hovered = ui.rect_contains_pointer(ui.max_rect());
        self.state.set_hovered(hovered, now);

        let radius = if self.state.visual == IslandVisualState::Expanded {
            20
        } else {
            (self.current_size.y * 0.5).round().clamp(12.0, 28.0) as u8
        };

        let frame = egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(16, 16, 20, 240))
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 32),
            ))
            .corner_radius(CornerRadius::same(radius))
            .inner_margin(egui::Margin::symmetric(14, 8));

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            if let Some(item) = self.state.active.clone() {
                match self.state.visual {
                    IslandVisualState::Expanded => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("● {}", item.app_name))
                                    .font(FontId::proportional(11.5))
                                    .color(Color32::from_rgb(110, 195, 255))
                                    .strong(),
                            );
                            let queued = self.state.queued_count();
                            if queued > 0 {
                                ui.label(
                                    RichText::new(format!("+{queued}"))
                                        .font(FontId::proportional(10.5))
                                        .color(Color32::from_gray(170)),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Dismiss").clicked() {
                                        self.state.dismiss_active(now);
                                        ctx.request_repaint();
                                    }
                                },
                            );
                        });
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(&item.title)
                                .font(FontId::proportional(13.5))
                                .color(Color32::WHITE)
                                .strong(),
                        );
                        if !item.body.is_empty() {
                            ui.add_space(2.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&item.body)
                                        .font(FontId::proportional(12.0))
                                        .color(Color32::from_gray(205)),
                                )
                                .wrap(),
                            );
                        }
                    }
                    IslandVisualState::Compact | IslandVisualState::Idle => {
                        ui.horizontal_centered(|ui| {
                            ui.label(
                                RichText::new("●")
                                    .font(FontId::proportional(11.0))
                                    .color(Color32::from_rgb(110, 195, 255)),
                            );
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&item.title)
                                            .font(FontId::proportional(12.5))
                                            .color(Color32::WHITE)
                                            .strong(),
                                    )
                                    .truncate(),
                                );
                                let sub = if item.body.is_empty() {
                                    &item.app_name
                                } else {
                                    &item.body
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(sub)
                                            .font(FontId::proportional(11.0))
                                            .color(Color32::from_gray(175)),
                                    )
                                    .truncate(),
                                );
                            });
                            let queued = self.state.queued_count();
                            if queued > 0 {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(format!("+{queued}"))
                                                .font(FontId::proportional(11.0))
                                                .color(Color32::from_gray(180)),
                                        );
                                    },
                                );
                            }
                        });
                    }
                }
            } else if let ListenerStatus::AccessDenied(msg) = &self.state.listener_status {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new("!")
                            .color(Color32::from_rgb(255, 190, 90))
                            .strong(),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(msg)
                                .font(FontId::proportional(11.0))
                                .color(Color32::from_gray(225)),
                        )
                        .truncate(),
                    );
                });
            } else {
                ui.centered_and_justified(|ui| {
                    let (dot_color, text) = if self.state.paused {
                        (Color32::from_gray(140), "Paused")
                    } else {
                        (Color32::from_rgb(100, 210, 150), "Liebert")
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("●")
                                .font(FontId::proportional(9.5))
                                .color(dot_color),
                        );
                        ui.label(
                            RichText::new(text)
                                .font(FontId::proportional(11.0))
                                .color(Color32::from_gray(210)),
                        );
                    });
                });
            }
        });
    }
}
