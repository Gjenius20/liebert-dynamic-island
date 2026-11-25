mod settings;

use std::collections::HashSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, RichText, Stroke, Vec2, ViewportCommand};

use crate::calendar::{
    CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events,
};
use crate::config::Config;
use crate::lyrics::{LyricsData, active_lyric_line};
use crate::platform::{self, MediaCommand, MediaTrack, PlatformEvent};
use crate::state::{IslandState, IslandVisualState, ListenerStatus, NotificationItem};
use crate::todo::TodoStore;
use crate::tray::{TrayAction, TrayController};
use crate::ui::settings::{SettingsTab, SettingsWindowState, render_settings_viewport};

pub const WINDOW_TITLE: &str = "Liebert Dynamic Island";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IslandTab {
    Overview,
    Music,
    Todo,
    Calendar,
}

pub struct AppChannels {
    pub event_rx: mpsc::Receiver<PlatformEvent>,
    pub lyrics_req_tx: mpsc::Sender<(String, String)>,
    pub media_cmd_tx: mpsc::Sender<MediaCommand>,
    pub calendar_cmd_tx: mpsc::Sender<CalendarCommand>,
}

pub struct IslandApp {
    config: Config,
    state: IslandState,
    channels: AppChannels,
    tray: Option<TrayController>,
    media: Option<MediaTrack>,
    lyrics: LyricsData,
    last_lyrics_key: Option<(String, String)>,
    todos: TodoStore,
    calendar_events: Vec<CalendarEvent>,
    calendar_status: String,
    alerted_events: HashSet<(String, i64)>,
    active_tab: IslandTab,
    show_settings: bool,
    settings_tab: SettingsTab,
    new_todo_input: String,
    calendar_url_input: String,
    current_size: Vec2,
    applied_size: Vec2,
    last_frame: Instant,
    no_activate_applied: bool,
    last_visible: bool,
    last_egui_hovered: bool,
    demo_deadline: Option<Instant>,
}

impl IslandApp {
    pub fn new(
        config: Config,
        channels: AppChannels,
        tray: Option<TrayController>,
        demo_mode: bool,
    ) -> Self {
        let timeout = Duration::from_secs(config.collapse_timeout_secs.max(1));
        let state = IslandState::new(config.auto_hide_when_idle, timeout);
        let now = Instant::now();
        let calendar_url_input = config.google_calendar_ics_url.clone();
        let initial_w = config.idle_width * config.scale;
        let initial_h = 30.0 * config.scale;
        let initial_size = Vec2::new(initial_w, initial_h);

        Self {
            config,
            state,
            channels,
            tray,
            media: None,
            lyrics: LyricsData::NotFound,
            last_lyrics_key: None,
            todos: TodoStore::load(),
            calendar_events: Vec::new(),
            calendar_status: if calendar_url_input.is_empty() {
                "Not configured".to_owned()
            } else {
                "Syncing...".to_owned()
            },
            alerted_events: HashSet::new(),
            active_tab: IslandTab::Overview,
            show_settings: false,
            settings_tab: SettingsTab::Appearance,
            new_todo_input: String::new(),
            calendar_url_input,
            current_size: initial_size,
            applied_size: initial_size,
            last_frame: now,
            no_activate_applied: false,
            last_visible: true,
            last_egui_hovered: false,
            demo_deadline: demo_mode.then(|| now + Duration::from_secs(1)),
        }
    }

    fn target_size(&self) -> Vec2 {
        let s = self.config.scale;
        match self.state.visual {
            IslandVisualState::Idle => {
                if matches!(self.state.listener_status, ListenerStatus::AccessDenied(_)) {
                    Vec2::new(330.0 * s, 44.0 * s)
                } else if self.config.show_lyrics && self.media.is_some() {
                    Vec2::new(self.config.idle_width.max(250.0) * s, 32.0 * s)
                } else {
                    Vec2::new(self.config.idle_width * s, 30.0 * s)
                }
            }
            IslandVisualState::Compact => Vec2::new(340.0 * s, 58.0 * s),
            IslandVisualState::Expanded => Vec2::new(self.config.expanded_width * s, 168.0 * s),
        }
    }

    fn sync_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.update_status(&self.state.listener_status, self.state.paused);
        }
    }

    fn current_playback_ms(&self, now: Instant) -> u64 {
        let Some(track) = &self.media else {
            return 0;
        };
        if track.is_playing {
            track.position_ms + now.saturating_duration_since(track.updated_at).as_millis() as u64
        } else {
            track.position_ms
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
                    body: "Hover untuk expand, lihat Lyrics/Todo/Calendar, atau klik Dismiss."
                        .to_owned(),
                    icon_rgba: None,
                    timestamp: 0,
                },
                now,
            );
        }

        let mut status_changed = false;
        while let Ok(ev) = self.channels.event_rx.try_recv() {
            match ev {
                PlatformEvent::Notification(item) => self.state.push_notification(item, now),
                PlatformEvent::Removed(id) => self.state.remove_by_id(&id, now),
                PlatformEvent::Status(status) => {
                    self.state.listener_status = status;
                    status_changed = true;
                }
                PlatformEvent::Media(track) => {
                    if let Some(ref t) = track {
                        let key = (t.artist.clone(), t.title.clone());
                        if self.last_lyrics_key.as_ref() != Some(&key) {
                            self.last_lyrics_key = Some(key.clone());
                            self.lyrics = LyricsData::Loading;
                            let _ = self.channels.lyrics_req_tx.send(key);
                        }
                    } else {
                        self.last_lyrics_key = None;
                        self.lyrics = LyricsData::NotFound;
                    }
                    self.media = track;
                }
                PlatformEvent::LyricsLoaded {
                    artist,
                    title,
                    lyrics,
                } => {
                    if self.last_lyrics_key.as_ref() == Some(&(artist, title)) {
                        self.lyrics = lyrics;
                    }
                }
                PlatformEvent::CalendarUpdated(res) => match res {
                    Ok(events) => {
                        self.calendar_status = format!("Synced ({} events)", events.len());
                        self.calendar_events = events;
                    }
                    Err(err) => {
                        self.calendar_status = err;
                    }
                },
            }
        }

        if let Some(tray) = &self.tray {
            for action in tray.poll_actions() {
                match action {
                    TrayAction::OpenSettings => {
                        self.show_settings = true;
                        ctx.request_repaint();
                    }
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

        if self.config.show_calendar && !self.calendar_events.is_empty() {
            let now_utc = chrono::Utc::now().timestamp();
            for ev in upcoming_events(&self.calendar_events, now_utc, 3) {
                let secs_until = ev.start_utc - now_utc;
                if (0..=300).contains(&secs_until) {
                    let key = (ev.summary.clone(), ev.start_utc);
                    if self.alerted_events.insert(key) {
                        self.state.push_notification(
                            NotificationItem {
                                id: format!("cal-{}-{}", ev.start_utc, ev.summary),
                                app_name: "Google Calendar".to_owned(),
                                title: ev.summary,
                                body: format!(
                                    "{} • {}",
                                    format_local_time(ev.start_utc),
                                    format_countdown(ev.start_utc, ev.end_utc, now_utc)
                                ),
                                icon_rgba: None,
                                timestamp: now_utc.max(0) as u64,
                            },
                            now,
                        );
                    }
                }
            }
        }

        if status_changed {
            self.sync_tray();
        }
    }

    fn open_settings(&mut self, tab: SettingsTab) {
        self.settings_tab = tab;
        self.show_settings = true;
    }

    fn bg_color32(&self) -> Color32 {
        let [r, g, b, a] = self.config.bg_color;
        Color32::from_rgba_unmultiplied(r, g, b, a)
    }

    fn accent_color32(&self) -> Color32 {
        let [r, g, b] = self.config.accent_color;
        Color32::from_rgb(r, g, b)
    }

    fn text_color32(&self) -> Color32 {
        let [r, g, b] = self.config.text_color;
        Color32::from_rgb(r, g, b)
    }

    fn muted_color32(&self) -> Color32 {
        let [r, g, b] = self.config.text_color;
        Color32::from_rgba_unmultiplied(r, g, b, 170)
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

        // Win32 WS_EX_NOACTIVATE windows can drop pointer-hover events; poll OS cursor bounds directly.
        let pad = if self.state.hovered { 6 } else { 0 };
        let hovered =
            platform::is_cursor_over_window(WINDOW_TITLE, pad).unwrap_or(self.last_egui_hovered);
        let hover_changed = self.state.set_hovered(hovered, now);

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

        if (self.current_size - self.applied_size).length_sq() > 0.25 || hover_changed {
            self.applied_size = self.current_size;
            let screen_w = platform::primary_monitor_logical_width();
            let x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.current_size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
                x,
                self.config.top_offset_px,
            )));
        }

        let playing_lyrics = self.config.show_lyrics
            && self.media.as_ref().is_some_and(|m| m.is_playing)
            && matches!(self.lyrics, LyricsData::Synced(_));

        if animating || hover_changed {
            ctx.request_repaint();
        } else if playing_lyrics {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if let Some(deadline) = self.state.next_deadline() {
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(80));
            ctx.request_repaint_after(wait);
        } else if let Some(demo_at) = self.demo_deadline {
            ctx.request_repaint_after(demo_at.saturating_duration_since(now));
        } else {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        self.last_egui_hovered = ui.rect_contains_pointer(ui.max_rect());

        let radius = if self.state.visual == IslandVisualState::Expanded {
            self.config.corner_radius.min(26)
        } else {
            (self.current_size.y * 0.5).round().clamp(12.0, 28.0) as u8
        };

        let frame = egui::Frame::new()
            .fill(self.bg_color32())
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, self.config.border_alpha),
            ))
            .corner_radius(CornerRadius::same(radius))
            .inner_margin(egui::Margin::symmetric(14, 8));

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            match self.state.visual {
                IslandVisualState::Expanded => self.render_expanded(ui, &ctx, now),
                IslandVisualState::Compact => self.render_compact(ui),
                IslandVisualState::Idle => self.render_idle(ui, now),
            }
        });

        let mut config_dirty = false;
        render_settings_viewport(
            &ctx,
            SettingsWindowState {
                open: &mut self.show_settings,
                tab: &mut self.settings_tab,
                config: &mut self.config,
                todos: &mut self.todos,
                new_todo_input: &mut self.new_todo_input,
                calendar_url_input: &mut self.calendar_url_input,
                calendar_events: &self.calendar_events,
                calendar_status: &self.calendar_status,
                calendar_cmd_tx: &self.channels.calendar_cmd_tx,
                config_dirty: &mut config_dirty,
            },
        );
        if config_dirty {
            self.state.auto_hide = self.config.auto_hide_when_idle;
            self.state
                .set_collapse_timeout(Duration::from_secs(self.config.collapse_timeout_secs));
            if let Err(err) = self.config.save() {
                eprintln!("Failed to save config: {err}");
            }
            ctx.request_repaint();
        }
    }
}

impl IslandApp {
    fn render_idle(&self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.accent_color32();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        if let ListenerStatus::AccessDenied(msg) = &self.state.listener_status {
            ui.horizontal_centered(|ui| {
                ui.label(
                    RichText::new("!")
                        .color(Color32::from_rgb(255, 190, 90))
                        .strong(),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(msg)
                            .font(FontId::proportional(11.0 * s))
                            .color(text_col),
                    )
                    .truncate(),
                );
            });
            return;
        }

        if self.config.show_lyrics
            && let Some(track) = &self.media
        {
            let pos_ms = self.current_playback_ms(now);
            let line = active_lyric_line(&self.lyrics, pos_ms)
                .map(|(curr, _)| curr.to_owned())
                .unwrap_or_else(|| {
                    if track.artist.is_empty() {
                        track.title.clone()
                    } else {
                        format!("{} — {}", track.artist, track.title)
                    }
                });

            ui.horizontal_centered(|ui| {
                ui.label(
                    RichText::new(if track.is_playing { "♪" } else { "⏸" })
                        .font(FontId::proportional(11.0 * s))
                        .color(accent),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(line)
                            .font(FontId::proportional(11.0 * s))
                            .color(text_col),
                    )
                    .truncate(),
                );
            });
            return;
        }

        ui.centered_and_justified(|ui| {
            let (dot_color, text) = if self.state.paused {
                (Color32::from_gray(140), "Paused")
            } else {
                (accent, "Liebert")
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("●")
                        .font(FontId::proportional(9.5 * s))
                        .color(dot_color),
                );
                ui.label(
                    RichText::new(text)
                        .font(FontId::proportional(11.0 * s))
                        .color(muted),
                );
            });
        });
    }

    fn render_compact(&self, ui: &mut egui::Ui) {
        let Some(item) = &self.state.active else {
            return;
        };
        let accent = self.accent_color32();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        ui.horizontal_centered(|ui| {
            ui.label(
                RichText::new("●")
                    .font(FontId::proportional(11.0 * s))
                    .color(accent),
            );
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(&item.title)
                            .font(FontId::proportional(12.5 * s))
                            .color(text_col)
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
                            .font(FontId::proportional(11.0 * s))
                            .color(muted),
                    )
                    .truncate(),
                );
            });
            let queued = self.state.queued_count();
            if queued > 0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("+{queued}"))
                            .font(FontId::proportional(11.0 * s))
                            .color(muted),
                    );
                });
            }
        });
    }

    fn render_expanded(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.accent_color32();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        if let Some(item) = self.state.active.clone() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("● {}", item.app_name))
                        .font(FontId::proportional(11.5 * s))
                        .color(accent)
                        .strong(),
                );
                let queued = self.state.queued_count();
                if queued > 0 {
                    ui.label(
                        RichText::new(format!("+{queued}"))
                            .font(FontId::proportional(10.5 * s))
                            .color(muted),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("⚙").clicked() {
                        self.open_settings(SettingsTab::Appearance);
                    }
                    if ui.small_button("Dismiss").clicked() {
                        self.state.dismiss_active(now);
                        ctx.request_repaint();
                    }
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(&item.title)
                    .font(FontId::proportional(13.5 * s))
                    .color(text_col)
                    .strong(),
            );
            if !item.body.is_empty() {
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(&item.body)
                            .font(FontId::proportional(12.0 * s))
                            .color(muted),
                    )
                    .wrap(),
                );
            }
            return;
        }

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.active_tab, IslandTab::Overview, "Overview");
            if self.config.show_lyrics {
                ui.selectable_value(&mut self.active_tab, IslandTab::Music, "♪ Lyrics");
            }
            if self.config.show_todo {
                let label = format!("☑ {}", self.todos.pending_count());
                ui.selectable_value(&mut self.active_tab, IslandTab::Todo, label);
            }
            if self.config.show_calendar {
                ui.selectable_value(&mut self.active_tab, IslandTab::Calendar, "Cal");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⚙").clicked() {
                    self.open_settings(SettingsTab::Appearance);
                }
            });
        });
        ui.separator();

        match self.active_tab {
            IslandTab::Overview => self.render_overview_tab(ui, now),
            IslandTab::Music => self.render_music_tab(ui, now),
            IslandTab::Todo => self.render_todo_tab(ui),
            IslandTab::Calendar => self.render_calendar_tab(ui),
        }
    }

    fn render_overview_tab(&mut self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.accent_color32();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        if self.config.show_lyrics {
            if let Some(track) = &self.media {
                let pos_ms = self.current_playback_ms(now);
                let lyric = active_lyric_line(&self.lyrics, pos_ms)
                    .map(|(curr, _)| curr)
                    .unwrap_or("No synced lyric");
                if ui
                    .add(
                        egui::Label::new(
                            RichText::new(format!("♪ {} — {}", track.title, lyric))
                                .font(FontId::proportional(11.5 * s))
                                .color(accent),
                        )
                        .truncate()
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    self.active_tab = IslandTab::Music;
                }
            } else {
                ui.label(
                    RichText::new("♪ No music playing")
                        .font(FontId::proportional(11.5 * s))
                        .color(muted),
                );
            }
        }

        if self.config.show_calendar {
            let now_utc = chrono::Utc::now().timestamp();
            let next = upcoming_events(&self.calendar_events, now_utc, 1);
            let cal_text = if let Some(ev) = next.first() {
                format!(
                    "📅 {} ({})",
                    ev.summary,
                    format_countdown(ev.start_utc, ev.end_utc, now_utc)
                )
            } else if self.config.google_calendar_ics_url.is_empty() {
                "📅 Connect Google Calendar in Settings".to_owned()
            } else {
                "📅 No upcoming events".to_owned()
            };
            if ui
                .add(
                    egui::Label::new(
                        RichText::new(cal_text)
                            .font(FontId::proportional(11.5 * s))
                            .color(text_col),
                    )
                    .truncate()
                    .sense(egui::Sense::click()),
                )
                .clicked()
            {
                self.active_tab = IslandTab::Calendar;
            }
        }

        if self.config.show_todo {
            let pending = self.todos.pending_count();
            if ui
                .add(
                    egui::Label::new(
                        RichText::new(format!("☑ {pending} pending task(s)"))
                            .font(FontId::proportional(11.5 * s))
                            .color(muted),
                    )
                    .sense(egui::Sense::click()),
                )
                .clicked()
            {
                self.active_tab = IslandTab::Todo;
            }
        }
    }

    fn render_music_tab(&self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.accent_color32();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        let Some(track) = &self.media else {
            ui.label(
                RichText::new("Play a song on Spotify or any media app...")
                    .font(FontId::proportional(11.5 * s))
                    .color(muted),
            );
            return;
        };

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(&track.title)
                            .font(FontId::proportional(12.5 * s))
                            .color(text_col)
                            .strong(),
                    )
                    .truncate(),
                );
                if !track.artist.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&track.artist)
                                .font(FontId::proportional(10.5 * s))
                                .color(muted),
                        )
                        .truncate(),
                    );
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⏭").clicked() {
                    let _ = self.channels.media_cmd_tx.send(MediaCommand::Next);
                }
                let play_icon = if track.is_playing { "⏸" } else { "▶" };
                if ui.small_button(play_icon).clicked() {
                    let _ = self
                        .channels
                        .media_cmd_tx
                        .send(MediaCommand::TogglePlayPause);
                }
                if ui.small_button("⏮").clicked() {
                    let _ = self.channels.media_cmd_tx.send(MediaCommand::Previous);
                }
            });
        });

        ui.add_space(4.0);
        let pos_ms = self.current_playback_ms(now);
        match &self.lyrics {
            LyricsData::Loading => {
                ui.label(
                    RichText::new("Fetching lyrics...")
                        .font(FontId::proportional(11.0 * s))
                        .color(muted),
                );
            }
            LyricsData::NotFound => {
                ui.label(
                    RichText::new("Lyrics not found for this track")
                        .font(FontId::proportional(11.0 * s))
                        .color(muted),
                );
            }
            LyricsData::Synced(_) | LyricsData::Plain(_) => {
                if let Some((curr, next)) = active_lyric_line(&self.lyrics, pos_ms) {
                    ui.add(
                        egui::Label::new(
                            RichText::new(curr)
                                .font(FontId::proportional(12.0 * s))
                                .color(accent)
                                .strong(),
                        )
                        .truncate(),
                    );
                    if let Some(next_line) = next {
                        ui.add(
                            egui::Label::new(
                                RichText::new(next_line)
                                    .font(FontId::proportional(10.5 * s))
                                    .color(muted),
                            )
                            .truncate(),
                        );
                    }
                }
            }
        }
    }

    fn render_todo_tab(&mut self, ui: &mut egui::Ui) {
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} pending", self.todos.pending_count()))
                    .font(FontId::proportional(11.0 * s))
                    .color(muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+ Manage / Add").clicked() {
                    self.open_settings(SettingsTab::Todo);
                }
            });
        });

        let mut toggle_id = None;
        let mut remove_id = None;
        let mut dirty = false;

        egui::ScrollArea::vertical()
            .max_height(72.0 * s)
            .show(ui, |ui| {
                if self.todos.items.is_empty() {
                    ui.label(
                        RichText::new("No tasks yet. Click '+ Manage / Add'.")
                            .font(FontId::proportional(11.0 * s))
                            .color(muted),
                    );
                }
                for item in &self.todos.items {
                    ui.horizontal(|ui| {
                        let mut done = item.done;
                        let label = if item.done {
                            RichText::new(&item.text)
                                .font(FontId::proportional(11.5 * s))
                                .color(muted)
                                .strikethrough()
                        } else {
                            RichText::new(&item.text)
                                .font(FontId::proportional(11.5 * s))
                                .color(text_col)
                        };
                        if ui.checkbox(&mut done, label).changed() {
                            toggle_id = Some(item.id);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").clicked() {
                                remove_id = Some(item.id);
                            }
                        });
                    });
                }
            });

        if let Some(id) = toggle_id {
            self.todos.toggle(id);
            dirty = true;
        }
        if let Some(id) = remove_id {
            self.todos.remove(id);
            dirty = true;
        }
        if dirty && let Err(err) = self.todos.save() {
            eprintln!("Failed to save todos: {err}");
        }
    }

    fn render_calendar_tab(&mut self, ui: &mut egui::Ui) {
        let accent = self.accent_color32();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        if self.config.google_calendar_ics_url.is_empty() {
            ui.label(
                RichText::new("Connect your Google Calendar (.ics) to see upcoming events.")
                    .font(FontId::proportional(11.5 * s))
                    .color(muted),
            );
            ui.add_space(4.0);
            if ui.button("Configure Google Calendar").clicked() {
                self.open_settings(SettingsTab::Calendar);
            }
            return;
        }

        let now_utc = chrono::Utc::now().timestamp();
        let upcoming = upcoming_events(&self.calendar_events, now_utc, 3);
        if upcoming.is_empty() {
            ui.label(
                RichText::new(format!("No upcoming events ({})", self.calendar_status))
                    .font(FontId::proportional(11.5 * s))
                    .color(muted),
            );
            if ui.small_button("Refresh").clicked() {
                let _ = self.channels.calendar_cmd_tx.send(CalendarCommand::Refresh);
            }
            return;
        }

        for ev in upcoming {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format_local_time(ev.start_utc))
                        .font(FontId::monospace(11.0 * s))
                        .color(accent),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(&ev.summary)
                            .font(FontId::proportional(11.5 * s))
                            .color(text_col),
                    )
                    .truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format_countdown(ev.start_utc, ev.end_utc, now_utc))
                            .font(FontId::proportional(10.5 * s))
                            .color(muted),
                    );
                });
            });
        }
    }
}
