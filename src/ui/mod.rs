mod settings;

use std::collections::HashSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, TextureHandle, TextureOptions, Vec2, ViewportCommand,
    text::{LayoutJob, TextFormat},
};
use eframe::epaint::RectShape;
use eframe::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

use crate::calendar::{
    CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events,
};
use crate::clock::format_clock;
use crate::config::{Config, IdleDisplayMode};
use crate::lyrics::{
    LyricsData, LyricsRequest, active_karaoke, has_cjk_or_hangul,
};
use crate::platform::{self, MediaCommand, MediaTrack, PlatformEvent, RepeatMode};
use crate::state::{IslandState, IslandVisualState, ListenerStatus, NotificationItem};
use crate::todo::TodoStore;
use crate::tray::{TrayAction, TrayController};
use crate::ui::settings::{SettingsTab, SettingsWindowState, render_settings_viewport};

pub const WINDOW_TITLE: &str = "Liebert Dynamic Island";

pub fn install_base_fallback_fonts(ctx: &egui::Context) {
    #[cfg(windows)]
    let candidates = [
        ("win_segoe_ui", "C:\\Windows\\Fonts\\segoeui.ttf"),
        ("win_segoe_sym", "C:\\Windows\\Fonts\\seguisym.ttf"),
    ];
    #[cfg(target_os = "linux")]
    let candidates = [
        (
            "linux_dejavu",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ),
        (
            "linux_noto",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        ),
    ];
    #[cfg(not(any(windows, target_os = "linux")))]
    let candidates: [(&str, &str); 0] = [];

    for (name, path) in candidates {
        load_font_file(ctx, name, path);
    }
}

fn load_font_file(ctx: &egui::Context, name: &str, path: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    ctx.add_font(FontInsert::new(
        name,
        egui::FontData::from_owned(bytes),
        vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: FontPriority::Lowest,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: FontPriority::Lowest,
            },
        ],
    ));
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IslandTab {
    Overview,
    Music,
    Todo,
    Calendar,
}

pub struct AppChannels {
    pub event_rx: mpsc::Receiver<PlatformEvent>,
    pub lyrics_req_tx: mpsc::Sender<LyricsRequest>,
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
    album_art_tex: Option<TextureHandle>,
    album_art_key: Option<(String, String)>,
    gradient_tex: Option<TextureHandle>,
    gradient_key: Option<([u8; 4], [u8; 3])>,
    cjk_fonts_loaded: bool,
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
    last_fullscreen_check: Instant,
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
        let mut state = IslandState::new(config.auto_hide_when_idle, timeout);
        state.set_auto_hide_delay(Duration::from_secs(config.auto_hide_delay_secs));
        state.set_hide_in_fullscreen(config.hide_in_fullscreen);
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
            album_art_tex: None,
            album_art_key: None,
            gradient_tex: None,
            gradient_key: None,
            cjk_fonts_loaded: false,
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
            last_fullscreen_check: now - Duration::from_secs(1),
            no_activate_applied: false,
            last_visible: true,
            last_egui_hovered: false,
            demo_deadline: demo_mode.then(|| now + Duration::from_secs(1)),
        }
    }

    fn effective_idle_mode(&self) -> IdleDisplayMode {
        match self.config.idle_display {
            IdleDisplayMode::Auto => {
                if self.config.show_lyrics && self.media.is_some() {
                    IdleDisplayMode::Lyrics
                } else {
                    IdleDisplayMode::Clock
                }
            }
            other => other,
        }
    }

    fn target_size(&self) -> Vec2 {
        let s = self.config.scale;
        match self.state.visual {
            IslandVisualState::Idle => {
                if matches!(self.state.listener_status, ListenerStatus::AccessDenied(_)) {
                    Vec2::new(330.0 * s, 44.0 * s)
                } else {
                    match self.effective_idle_mode() {
                        IdleDisplayMode::Lyrics
                            if self.config.show_lyrics && self.media.is_some() =>
                        {
                            Vec2::new(self.config.idle_width.max(250.0) * s, 32.0 * s)
                        }
                        IdleDisplayMode::NextEvent if self.config.show_calendar => {
                            let now_utc = chrono::Utc::now().timestamp();
                            if upcoming_events(&self.calendar_events, now_utc, 1).is_empty() {
                                Vec2::new(self.config.idle_width * s, 30.0 * s)
                            } else {
                                Vec2::new(self.config.idle_width.max(220.0) * s, 30.0 * s)
                            }
                        }
                        IdleDisplayMode::Clock => {
                            let needs_wide = self.config.clock_show_seconds
                                || !self.config.clock_24h
                                || !self.config.clock_timezone.eq_ignore_ascii_case("local");
                            let w = if needs_wide {
                                self.config.idle_width.max(162.0)
                            } else {
                                self.config.idle_width
                            };
                            Vec2::new(w * s, 30.0 * s)
                        }
                        _ => Vec2::new(self.config.idle_width * s, 30.0 * s),
                    }
                }
            }
            IslandVisualState::Compact => Vec2::new(340.0 * s, 58.0 * s),
            IslandVisualState::Expanded => {
                if self.state.active.is_none()
                    && self.active_tab == IslandTab::Music
                    && self.media.is_some()
                {
                    Vec2::new(self.config.expanded_width.max(436.0) * s, 308.0 * s)
                } else {
                    Vec2::new(self.config.expanded_width * s, 168.0 * s)
                }
            }
        }
    }

    fn ensure_cjk_font_loaded(&mut self, ctx: &egui::Context) {
        if self.cjk_fonts_loaded {
            return;
        }
        self.cjk_fonts_loaded = true;
        #[cfg(windows)]
        let candidates = [
            ("win_msgothic", "C:\\Windows\\Fonts\\msgothic.ttc"),
            ("win_malgun", "C:\\Windows\\Fonts\\malgun.ttf"),
        ];
        #[cfg(target_os = "linux")]
        let candidates = [
            (
                "linux_noto_cjk",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            ),
            (
                "linux_noto_cjk_var",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            ),
        ];
        #[cfg(not(any(windows, target_os = "linux")))]
        let candidates: [(&str, &str); 0] = [];

        for (name, path) in candidates {
            load_font_file(ctx, name, path);
        }
    }

    fn request_lyrics_for_current(&mut self) {
        let Some((artist, title)) = self.last_lyrics_key.clone() else {
            return;
        };
        let translate_lang = self
            .config
            .translate_lyrics
            .then(|| self.config.lyric_translate_lang.clone());
        self.lyrics = LyricsData::Loading;
        let _ = self.channels.lyrics_req_tx.send(LyricsRequest {
            artist,
            title,
            translate_lang,
            need_romaji: self.config.romaji_lyrics,
        });
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
                            if has_cjk_or_hangul(&t.title) || has_cjk_or_hangul(&t.artist) {
                                self.ensure_cjk_font_loaded(ctx);
                            }
                            self.last_lyrics_key = Some(key);
                            self.request_lyrics_for_current();
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
                        let has_cjk = match &lyrics {
                            LyricsData::Synced(lines) => {
                                lines.iter().any(|l| has_cjk_or_hangul(&l.text))
                            }
                            LyricsData::Plain(lines) => {
                                lines.iter().any(|l| has_cjk_or_hangul(&l.text))
                            }
                            _ => false,
                        };
                        if has_cjk {
                            self.ensure_cjk_font_loaded(ctx);
                        }
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
                        self.state.set_auto_hide(checked, now);
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

    fn effective_accent_rgb(&self) -> [u8; 3] {
        if self.config.dynamic_album_color
            && let Some(track) = &self.media
            && let Some(art) = &track.album_art
            && let Some(dom) = art.dominant_rgb
        {
            return dom;
        }
        self.config.accent_color
    }

    fn effective_accent(&self) -> Color32 {
        let [r, g, b] = self.effective_accent_rgb();
        Color32::from_rgb(r, g, b)
    }

    fn ensure_gradient_texture(&mut self, ctx: &egui::Context, base_bg: [u8; 4], accent: [u8; 3]) {
        let key = (base_bg, accent);
        if self.gradient_key == Some(key) && self.gradient_tex.is_some() {
            return;
        }
        const H: usize = 64;
        let mut pixels = Vec::with_capacity(H);
        // Blend accent toward warm soft rose/coral highlight for bottom wash matching reference.
        let glow = [
            lerp_u8(accent[0], 246, 0.32),
            lerp_u8(accent[1], 195, 0.32),
            lerp_u8(accent[2], 198, 0.32),
        ];
        for y in 0..H {
            let norm = y as f32 / (H - 1) as f32;
            let t = ((norm - 0.50) / 0.50).clamp(0.0, 1.0);
            let ease = t * t * (3.0 - 2.0 * t) * 0.85;
            let r = lerp_u8(base_bg[0], glow[0], ease);
            let g = lerp_u8(base_bg[1], glow[1], ease);
            let b = lerp_u8(base_bg[2], glow[2], ease);
            pixels.push(Color32::from_rgba_unmultiplied(r, g, b, base_bg[3]));
        }
        let img = ColorImage::new([1, H], pixels);
        if let Some(tex) = &mut self.gradient_tex {
            tex.set(img, TextureOptions::LINEAR);
        } else {
            self.gradient_tex = Some(ctx.load_texture("music_bg_grad", img, TextureOptions::LINEAR));
        }
        self.gradient_key = Some(key);
    }

    fn sync_album_art_texture(&mut self, ctx: &egui::Context, track: &MediaTrack) {
        let key = (track.artist.clone(), track.title.clone());
        if self.album_art_key.as_ref() == Some(&key) {
            return;
        }
        self.album_art_key = Some(key);
        if let Some(art) = &track.album_art {
            let img = ColorImage::from_rgba_unmultiplied([art.width, art.height], &art.rgba);
            if let Some(tex) = &mut self.album_art_tex {
                tex.set(img, TextureOptions::LINEAR);
            } else {
                self.album_art_tex = Some(ctx.load_texture("album_art", img, TextureOptions::LINEAR));
            }
        } else {
            self.album_art_tex = None;
        }
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
            let screen_w =
                platform::primary_monitor_logical_width() / ctx.pixels_per_point().max(1.0);
            let x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
            platform::position_overlay_window(
                WINDOW_TITLE,
                x,
                self.config.top_offset_px,
                self.current_size.x,
                self.current_size.y,
            );
            self.no_activate_applied = true;
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.001, 0.05);
        self.last_frame = now;

        self.handle_events(ctx, now);

        let mut fs_changed = false;
        if now.duration_since(self.last_fullscreen_check) >= Duration::from_millis(250) {
            self.last_fullscreen_check = now;
            let fs = self.config.hide_in_fullscreen && platform::is_foreground_fullscreen();
            fs_changed = self.state.set_fullscreen_active(fs);
        }

        // Win32 WS_EX_NOACTIVATE windows can drop pointer-hover events; poll OS cursor bounds
        // and top-edge trigger zone (even while auto-hidden) so hovering top-center reveals the notch.
        let trigger_w = self.config.idle_width.max(180.0) * self.config.scale;
        let top_trigger = platform::is_cursor_in_top_trigger(
            self.config.top_offset_px,
            trigger_w,
            ctx.pixels_per_point(),
        );
        let hovered = if self.state.hide_in_fullscreen && self.state.fullscreen_active {
            false
        } else if !self.last_visible {
            top_trigger.unwrap_or(false)
        } else {
            let pad = if self.state.hovered { 6 } else { 0 };
            let over_win = platform::is_cursor_over_window(WINDOW_TITLE, pad)
                .unwrap_or(self.last_egui_hovered);
            over_win || (self.state.hovered && top_trigger.unwrap_or(false))
        };
        let hover_changed = self.state.set_hovered(hovered, now);

        self.state.tick(now);

        let visible = self.state.should_be_visible(now);
        let became_visible = visible && !self.last_visible;
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

        if (self.current_size - self.applied_size).length_sq() > 0.25
            || hover_changed
            || became_visible
        {
            self.applied_size = self.current_size;
            let screen_w =
                platform::primary_monitor_logical_width() / ctx.pixels_per_point().max(1.0);
            let x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.current_size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
                x,
                self.config.top_offset_px,
            )));
            if visible {
                platform::position_overlay_window(
                    WINDOW_TITLE,
                    x,
                    self.config.top_offset_px,
                    self.current_size.x,
                    self.current_size.y,
                );
            }
        }

        let playing_lyrics = self.config.show_lyrics
            && self.media.as_ref().is_some_and(|m| m.is_playing)
            && matches!(self.lyrics, LyricsData::Synced(_));
        let showing_live_seconds = self.state.visual == IslandVisualState::Idle
            && self.effective_idle_mode() == IdleDisplayMode::Clock
            && self.config.clock_show_seconds;

        if animating || hover_changed || fs_changed {
            ctx.request_repaint();
        } else if playing_lyrics && self.config.karaoke_lyrics {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if (self.state.hide_in_fullscreen && self.state.fullscreen_active)
            || playing_lyrics
            || showing_live_seconds
        {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if let Some(deadline) = self
            .state
            .next_deadline()
            .or_else(|| self.state.auto_hide_deadline())
        {
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

        let is_music_card = self.state.visual == IslandVisualState::Expanded
            && self.state.active.is_none()
            && self.active_tab == IslandTab::Music
            && self.media.is_some();

        let frame = if is_music_card {
            let base_bg = [30, 29, 28, self.config.bg_color[3].max(240)];
            let accent_rgb = self.effective_accent_rgb();
            self.ensure_gradient_texture(&ctx, base_bg, accent_rgb);
            let card_rect = ui.max_rect();
            if let Some(tex) = &self.gradient_tex {
                let shape = RectShape::filled(card_rect, CornerRadius::same(radius), Color32::WHITE)
                    .with_texture(tex.id(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)));
                ui.painter().add(shape);
            }
            ui.painter().rect_stroke(
                card_rect,
                CornerRadius::same(radius),
                Stroke::new(
                    1.0,
                    Color32::from_rgba_unmultiplied(255, 255, 255, self.config.border_alpha),
                ),
                StrokeKind::Inside,
            );
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 10))
        } else {
            egui::Frame::new()
                .fill(self.bg_color32())
                .stroke(Stroke::new(
                    1.0,
                    Color32::from_rgba_unmultiplied(255, 255, 255, self.config.border_alpha),
                ))
                .corner_radius(CornerRadius::same(radius))
                .inner_margin(egui::Margin::symmetric(14, 8))
        };

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            match self.state.visual {
                IslandVisualState::Expanded => self.render_expanded(ui, &ctx, now),
                IslandVisualState::Compact => self.render_compact(ui),
                IslandVisualState::Idle => self.render_idle(ui, now),
            }
        });

        let prev_lyric_opts = (
            self.config.romaji_lyrics,
            self.config.translate_lyrics,
            self.config.lyric_translate_lang.clone(),
        );
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
            let new_lyric_opts = (
                self.config.romaji_lyrics,
                self.config.translate_lyrics,
                self.config.lyric_translate_lang.clone(),
            );
            if new_lyric_opts != prev_lyric_opts {
                self.request_lyrics_for_current();
            }
            self.state.set_auto_hide(self.config.auto_hide_when_idle, now);
            self.state
                .set_auto_hide_delay(Duration::from_secs(self.config.auto_hide_delay_secs));
            self.state
                .set_hide_in_fullscreen(self.config.hide_in_fullscreen);
            self.last_fullscreen_check = now - Duration::from_secs(1);
            self.state
                .set_collapse_timeout(Duration::from_secs(self.config.collapse_timeout_secs));
            if let Some(tray) = &self.tray {
                tray.set_auto_hide_checked(self.config.auto_hide_when_idle);
            }
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

        match self.effective_idle_mode() {
            IdleDisplayMode::Lyrics if self.config.show_lyrics && self.media.is_some() => {
                if let Some(track) = &self.media {
                    let pos_ms = self.current_playback_ms(now);
                    let active = active_karaoke(&self.lyrics, pos_ms);

                    ui.horizontal_centered(|ui| {
                        ui.label(
                            RichText::new(if track.is_playing { "♪" } else { "⏸" })
                                .font(FontId::proportional(11.0 * s))
                                .color(accent),
                        );
                        if let Some(k) = active {
                            if self.config.karaoke_lyrics {
                                render_karaoke_line(
                                    ui,
                                    k.text,
                                    k.progress,
                                    FontId::proportional(11.0 * s),
                                    muted,
                                    accent,
                                );
                            } else {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(k.text)
                                            .font(FontId::proportional(11.0 * s))
                                            .color(text_col),
                                    )
                                    .truncate(),
                                );
                            }
                        } else {
                            let fallback = if track.artist.is_empty() {
                                track.title.clone()
                            } else {
                                format!("{} — {}", track.artist, track.title)
                            };
                            ui.add(
                                egui::Label::new(
                                    RichText::new(fallback)
                                        .font(FontId::proportional(11.0 * s))
                                        .color(text_col),
                                )
                                .truncate(),
                            );
                        }
                    });
                    return;
                }
            }
            IdleDisplayMode::Clock => {
                let (time_str, tz_label) = format_clock(
                    chrono::Utc::now(),
                    &self.config.clock_timezone,
                    self.config.clock_24h,
                    self.config.clock_show_seconds,
                );
                let show_tz = !self.config.clock_timezone.eq_ignore_ascii_case("local");
                ui.centered_and_justified(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("●")
                                .font(FontId::proportional(8.5 * s))
                                .color(if self.state.paused {
                                    Color32::from_gray(140)
                                } else {
                                    accent
                                }),
                        );
                        ui.label(
                            RichText::new(time_str)
                                .font(FontId::monospace(12.0 * s))
                                .color(text_col)
                                .strong(),
                        );
                        if show_tz {
                            ui.label(
                                RichText::new(tz_label)
                                    .font(FontId::proportional(9.5 * s))
                                    .color(muted),
                            );
                        }
                    });
                });
                return;
            }
            IdleDisplayMode::NextEvent if self.config.show_calendar => {
                let now_utc = chrono::Utc::now().timestamp();
                if let Some(ev) = upcoming_events(&self.calendar_events, now_utc, 1).first() {
                    ui.horizontal_centered(|ui| {
                        ui.label(
                            RichText::new(format_countdown(ev.start_utc, ev.end_utc, now_utc))
                                .font(FontId::proportional(10.5 * s))
                                .color(accent)
                                .strong(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&ev.summary)
                                    .font(FontId::proportional(11.0 * s))
                                    .color(text_col),
                            )
                            .truncate(),
                        );
                    });
                    return;
                }
            }
            _ => {}
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

        let is_music_card = self.active_tab == IslandTab::Music && self.media.is_some();
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
                if is_music_card
                    && ui
                        .selectable_label(self.config.karaoke_lyrics, "KR")
                        .on_hover_text("Toggle Karaoke sweep")
                        .clicked()
                {
                    self.config.karaoke_lyrics = !self.config.karaoke_lyrics;
                    let _ = self.config.save();
                }
            });
        });
        if !is_music_card {
            ui.separator();
        } else {
            ui.add_space(4.0);
        }

        match self.active_tab {
            IslandTab::Overview => self.render_overview_tab(ui, now),
            IslandTab::Music => self.render_music_tab(ui, ctx, now),
            IslandTab::Todo => self.render_todo_tab(ui),
            IslandTab::Calendar => self.render_calendar_tab(ui),
        }
    }

    fn render_overview_tab(&mut self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.accent_color32();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        let (time_str, tz_label) = format_clock(
            chrono::Utc::now(),
            &self.config.clock_timezone,
            self.config.clock_24h,
            self.config.clock_show_seconds,
        );
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(time_str)
                    .font(FontId::monospace(12.5 * s))
                    .color(text_col)
                    .strong(),
            );
            ui.label(
                RichText::new(tz_label)
                    .font(FontId::proportional(10.5 * s))
                    .color(muted),
            );
        });

        if self.config.show_lyrics {
            if let Some(track) = &self.media {
                let pos_ms = self.current_playback_ms(now);
                let lyric = active_karaoke(&self.lyrics, pos_ms)
                    .map(|k| k.text)
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

    fn render_music_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
        let accent_rgb = self.effective_accent_rgb();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        let Some(track) = self.media.clone() else {
            ui.label(
                RichText::new("Play a song on Spotify or any media app...")
                    .font(FontId::proportional(11.5 * s))
                    .color(muted),
            );
            return;
        };

        self.sync_album_art_texture(ctx, &track);
        let pos_ms = self.current_playback_ms(now);
        let total_w = ui.available_width();
        let col_gap = 14.0 * s;
        let left_w = (total_w * 0.43).max(148.0 * s);
        let right_w = (total_w - left_w - col_gap).max(120.0 * s);
        let body_h = (ui.available_height() - 40.0 * s).max(140.0 * s);

        ui.horizontal_top(|ui| {
            // Left column: Album Art, centered Title & Artist, Playbar, and Transport Controls.
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, body_h),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    let art_size = (left_w - 22.0 * s).clamp(96.0 * s, 134.0 * s);
                    let (art_rect, _) =
                        ui.allocate_exact_size(Vec2::splat(art_size), Sense::hover());
                    let art_radius = CornerRadius::same((12.0 * s).round() as u8);

                    if let Some(tex) = &self.album_art_tex {
                        let shape = RectShape::filled(art_rect, art_radius, Color32::WHITE)
                            .with_texture(
                                tex.id(),
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            );
                        ui.painter().add(shape);
                    } else {
                        ui.painter().rect_filled(
                            art_rect,
                            art_radius,
                            Color32::from_rgba_unmultiplied(45, 42, 44, 220),
                        );
                        ui.painter().circle_stroke(
                            art_rect.center(),
                            art_size * 0.24,
                            Stroke::new(2.0 * s, accent),
                        );
                        ui.painter().text(
                            art_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "♪",
                            FontId::proportional(24.0 * s),
                            accent,
                        );
                    }

                    ui.add_space(6.0 * s);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&track.title)
                                .font(FontId::proportional(13.0 * s))
                                .color(text_col)
                                .strong(),
                        )
                        .truncate(),
                    );
                    if !track.artist.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&track.artist)
                                    .font(FontId::proportional(11.0 * s))
                                    .color(muted),
                            )
                            .truncate(),
                        );
                    }

                    ui.add_space(6.0 * s);
                    if let Some(seek_ms) =
                        render_playbar(ui, pos_ms, track.duration_ms, accent, muted, s)
                    {
                        let _ = self.channels.media_cmd_tx.send(MediaCommand::SeekMs(seek_ms));
                        if let Some(m) = &mut self.media {
                            m.position_ms = seek_ms;
                            m.updated_at = now;
                        }
                    }

                    ui.add_space(4.0 * s);
                    ui.horizontal(|ui| {
                        let controls_w = 148.0 * s;
                        let side = ((ui.available_width() - controls_w) * 0.5).max(0.0);
                        ui.add_space(side);

                        let shuf_col = if track.shuffle { accent } else { muted };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("🔀").font(FontId::proportional(12.0 * s)).color(shuf_col),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Shuffle")
                            .clicked()
                        {
                            let next_shuf = !track.shuffle;
                            let _ = self
                                .channels
                                .media_cmd_tx
                                .send(MediaCommand::SetShuffle(next_shuf));
                            if let Some(m) = &mut self.media {
                                m.shuffle = next_shuf;
                            }
                        }

                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("⏮").font(FontId::proportional(13.5 * s)).color(text_col),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Previous")
                            .clicked()
                        {
                            let _ = self.channels.media_cmd_tx.send(MediaCommand::Previous);
                        }

                        let play_icon = if track.is_playing { "⏸" } else { "▶" };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(play_icon)
                                        .font(FontId::proportional(16.0 * s))
                                        .color(Color32::WHITE)
                                        .strong(),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Play / Pause")
                            .clicked()
                        {
                            let _ = self
                                .channels
                                .media_cmd_tx
                                .send(MediaCommand::TogglePlayPause);
                        }

                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("⏭").font(FontId::proportional(13.5 * s)).color(text_col),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Next")
                            .clicked()
                        {
                            let _ = self.channels.media_cmd_tx.send(MediaCommand::Next);
                        }

                        let (rep_icon, rep_col) = match track.repeat {
                            RepeatMode::Off => ("🔁", muted),
                            RepeatMode::All => ("🔁", accent),
                            RepeatMode::One => ("🔂", accent),
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(rep_icon)
                                        .font(FontId::proportional(12.0 * s))
                                        .color(rep_col),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Repeat (Off / All / One)")
                            .clicked()
                        {
                            let next_rep = track.repeat.next();
                            let _ = self
                                .channels
                                .media_cmd_tx
                                .send(MediaCommand::SetRepeat(next_rep));
                            if let Some(m) = &mut self.media {
                                m.repeat = next_rep;
                            }
                        }
                    });
                },
            );

            ui.add_space(col_gap);

            // Right column: Multi-line wrapped Karaoke Lyric, Romaji, Translation & Upcoming preview.
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, body_h),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    match &self.lyrics {
                        LyricsData::Loading => {
                            ui.label(
                                RichText::new("Fetching lyrics...")
                                    .font(FontId::proportional(12.0 * s))
                                    .color(muted),
                            );
                        }
                        LyricsData::NotFound => {
                            ui.label(
                                RichText::new("Lyrics not found for this track")
                                    .font(FontId::proportional(12.0 * s))
                                    .color(muted),
                            );
                        }
                        LyricsData::Synced(_) | LyricsData::Plain(_) => {
                            if let Some(k) = active_karaoke(&self.lyrics, pos_ms) {
                                let dim_white = Color32::from_rgba_unmultiplied(255, 255, 255, 90);
                                let lit_romaji = Color32::from_rgb(
                                    accent_rgb[0],
                                    accent_rgb[1],
                                    accent_rgb[2],
                                );
                                let dim_romaji = Color32::from_rgba_unmultiplied(
                                    (accent_rgb[0] as u16 / 2) as u8,
                                    (accent_rgb[1] as u16 / 2) as u8,
                                    (accent_rgb[2] as u16 / 2) as u8,
                                    135,
                                );

                                let orig_prog = if self.config.karaoke_lyrics {
                                    k.progress
                                } else {
                                    1.0
                                };
                                render_wrapped_karaoke(
                                    ui,
                                    k.text,
                                    orig_prog,
                                    FontId::proportional(14.0 * s),
                                    text_col,
                                    dim_white,
                                    right_w,
                                );

                                if self.config.romaji_lyrics
                                    && let Some(rom) = k.romaji
                                {
                                    ui.add_space(2.0 * s);
                                    render_wrapped_karaoke(
                                        ui,
                                        rom,
                                        orig_prog,
                                        FontId::proportional(11.5 * s),
                                        lit_romaji,
                                        dim_romaji,
                                        right_w,
                                    );
                                }

                                if self.config.translate_lyrics
                                    && let Some(tr) = k.translation
                                {
                                    ui.add_space(2.0 * s);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(tr)
                                                .font(FontId::proportional(10.5 * s))
                                                .color(Color32::from_gray(215)),
                                        )
                                        .wrap(),
                                    );
                                }

                                if let Some(next_line) = k.next_text {
                                    ui.add_space(8.0 * s);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(next_line)
                                                .font(FontId::proportional(12.5 * s))
                                                .color(Color32::from_rgba_unmultiplied(
                                                    255, 255, 255, 58,
                                                ))
                                                .strong(),
                                        )
                                        .wrap(),
                                    );
                                    if self.config.romaji_lyrics
                                        && let Some(next_rom) = k.next_romaji
                                    {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(next_rom)
                                                    .font(FontId::proportional(10.5 * s))
                                                    .color(Color32::from_rgba_unmultiplied(
                                                        accent_rgb[0],
                                                        accent_rgb[1],
                                                        accent_rgb[2],
                                                        58,
                                                    )),
                                            )
                                            .wrap(),
                                        );
                                    }
                                    if self.config.translate_lyrics
                                        && let Some(next_tr) = k.next_translation
                                    {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(next_tr)
                                                    .font(FontId::proportional(10.0 * s))
                                                    .color(Color32::from_rgba_unmultiplied(
                                                        220, 220, 220, 50,
                                                    )),
                                            )
                                            .wrap(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                },
            );
        });

        ui.add_space(6.0 * s);
        let pill_w = 132.0 * s;
        let pill_h = 28.0 * s;
        let pill_gap = 14.0 * s;
        let pair_w = pill_w * 2.0 + pill_gap;
        let mut lyric_prefs_changed = false;

        ui.horizontal(|ui| {
            let pad = ((ui.available_width() - pair_w) * 0.5).max(0.0);
            ui.add_space(pad);

            let tr_fill = if self.config.translate_lyrics {
                Color32::from_rgba_unmultiplied(
                    lerp_u8(accent_rgb[0], 120, 0.35),
                    lerp_u8(accent_rgb[1], 40, 0.35),
                    lerp_u8(accent_rgb[2], 45, 0.35),
                    225,
                )
            } else {
                Color32::from_rgba_unmultiplied(28, 28, 32, 160)
            };
            let tr_btn = egui::Button::new(
                RichText::new("Translate")
                    .font(FontId::proportional(12.0 * s))
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(tr_fill)
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 40),
            ))
            .corner_radius(CornerRadius::same((pill_h * 0.5).round() as u8))
            .min_size(Vec2::new(pill_w, pill_h));

            if ui.add(tr_btn).clicked() {
                self.config.translate_lyrics = !self.config.translate_lyrics;
                lyric_prefs_changed = true;
            }

            ui.add_space(pill_gap);

            let rm_fill = if self.config.romaji_lyrics {
                Color32::from_rgba_unmultiplied(accent_rgb[0], accent_rgb[1], accent_rgb[2], 78)
            } else {
                Color32::from_rgba_unmultiplied(28, 28, 32, 160)
            };
            let rm_stroke = if self.config.romaji_lyrics {
                Stroke::new(1.3, accent)
            } else {
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40))
            };
            let rm_btn = egui::Button::new(
                RichText::new("Romaji")
                    .font(FontId::proportional(12.0 * s))
                    .color(Color32::WHITE)
                    .strong(),
            )
            .fill(rm_fill)
            .stroke(rm_stroke)
            .corner_radius(CornerRadius::same((pill_h * 0.5).round() as u8))
            .min_size(Vec2::new(pill_w, pill_h));

            if ui.add(rm_btn).clicked() {
                self.config.romaji_lyrics = !self.config.romaji_lyrics;
                lyric_prefs_changed = true;
            }
        });

        if lyric_prefs_changed {
            let _ = self.config.save();
            self.request_lyrics_for_current();
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

fn render_wrapped_karaoke(
    ui: &mut egui::Ui,
    text: &str,
    progress: f32,
    font: FontId,
    lit_color: Color32,
    dim_color: Color32,
    max_width: f32,
) {
    let total_chars = text.chars().count();
    let lit_chars = ((total_chars as f32) * progress.clamp(0.0, 1.0)).round() as usize;
    let split_byte = text
        .char_indices()
        .nth(lit_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len());
    let (lit_part, dim_part) = text.split_at(split_byte);

    let mut job = LayoutJob::default();
    job.wrap.max_width = max_width.max(40.0);
    if !lit_part.is_empty() {
        job.append(
            lit_part,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: lit_color,
                ..Default::default()
            },
        );
    }
    if !dim_part.is_empty() {
        job.append(
            dim_part,
            0.0,
            TextFormat {
                font_id: font,
                color: dim_color,
                ..Default::default()
            },
        );
    }
    ui.add(egui::Label::new(job).selectable(false));
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    ((a as f32) + ((b as f32) - (a as f32)) * t).round() as u8
}

fn render_karaoke_line(
    ui: &mut egui::Ui,
    text: &str,
    progress: f32,
    font: FontId,
    dim_color: Color32,
    lit_color: Color32,
) {
    let avail_w = ui.available_width().max(24.0);
    let dim_galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), dim_color);
    let lit_galley = ui.painter().layout_no_wrap(text.to_owned(), font, lit_color);
    let h = dim_galley.size().y.max(14.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, h), Sense::hover());

    let text_w = dim_galley.size().x.min(avail_w);
    let scroll_x = if dim_galley.size().x > avail_w {
        let max_scroll = dim_galley.size().x - avail_w;
        -(max_scroll * progress.clamp(0.0, 1.0))
    } else {
        0.0
    };
    let draw_pos = Pos2::new(rect.left() + scroll_x, rect.top());
    let base_painter = ui.painter().with_clip_rect(rect);
    base_painter.galley(draw_pos, dim_galley, Color32::TRANSPARENT);

    let lit_width = (text_w * progress.clamp(0.0, 1.0)).max(0.0);
    if lit_width > 0.5 {
        let lit_clip = Rect::from_min_max(rect.min, Pos2::new(rect.left() + lit_width, rect.bottom()));
        ui.painter()
            .with_clip_rect(lit_clip.intersect(rect))
            .galley(draw_pos, lit_galley, Color32::TRANSPARENT);
    }
}

fn render_playbar(
    ui: &mut egui::Ui,
    pos_ms: u64,
    duration_ms: u64,
    accent: Color32,
    muted: Color32,
    scale: f32,
) -> Option<u64> {
    let mut seek_target = None;
    let clamped_pos = if duration_ms > 0 {
        pos_ms.min(duration_ms)
    } else {
        pos_ms
    };
    let frac = if duration_ms > 0 {
        (clamped_pos as f32 / duration_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format_mm_ss(clamped_pos))
                .font(FontId::monospace(9.5 * scale))
                .color(muted),
        );

        let bar_w = (ui.available_width() - 38.0 * scale).max(40.0);
        let (rect, resp) = ui.allocate_exact_size(
            Vec2::new(bar_w, 12.0 * scale),
            Sense::click_and_drag(),
        );
        let cy = rect.center().y;
        let track_h = 3.5 * scale;
        let track_rect = Rect::from_center_size(Pos2::new(rect.center().x, cy), Vec2::new(bar_w, track_h));
        let painter = ui.painter();
        painter.rect_filled(
            track_rect,
            CornerRadius::same(2),
            Color32::from_rgba_unmultiplied(255, 255, 255, 42),
        );

        let fill_w = bar_w * frac;
        if fill_w > 0.5 {
            let fill_rect = Rect::from_min_size(track_rect.min, Vec2::new(fill_w, track_h));
            painter.rect_filled(fill_rect, CornerRadius::same(2), accent);
        }
        if duration_ms > 0 && (resp.hovered() || resp.dragged()) {
            painter.circle_filled(Pos2::new(rect.left() + fill_w, cy), 4.5 * scale, accent);
        }

        if duration_ms > 0
            && (resp.clicked() || resp.dragged())
            && let Some(pointer) = resp.interact_pointer_pos()
        {
            let ratio = ((pointer.x - rect.left()) / bar_w).clamp(0.0, 1.0);
            seek_target = Some((duration_ms as f64 * ratio as f64).round() as u64);
        }

        ui.label(
            RichText::new(format_mm_ss(duration_ms))
                .font(FontId::monospace(9.5 * scale))
                .color(muted),
        );
    });

    seek_target
}

fn format_mm_ss(ms: u64) -> String {
    let total_secs = ms / 1000;
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}
