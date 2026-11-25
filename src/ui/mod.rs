mod settings;

use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, FontId, Mesh, Pos2, Rect, RichText, Sense, Shape,
    Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2, ViewportCommand,
};
use eframe::epaint::RectShape;
use eframe::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

use crate::calendar::{
    CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events,
};
use crate::clock::format_clock;
use crate::config::{Config, IdleDisplayMode, KaraokeLyricMode};
use crate::lyrics::{
    LyricsData, LyricsRequest, active_karaoke, has_cjk_or_hangul,
};
use crate::platform::{self, MediaCommand, MediaTrack, PlatformEvent, RepeatMode};
use crate::state::{IslandState, IslandVisualState, ListenerStatus, NotificationItem};
use crate::todo::TodoStore;
use crate::tray::{TrayAction, TrayController};
use crate::ui::settings::{
    SETTINGS_WINDOW_TITLE, SettingsTab, SettingsWindowState, render_settings_viewport,
};

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
    settings_focus_requested: bool,
    settings_tab: SettingsTab,
    new_todo_input: String,
    calendar_url_input: String,
    current_size: Vec2,
    applied_size: Vec2,
    notch_morph: f32,
    applied_top_y: f32,
    test_notif_seq: u32,
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
        let ear_pad = if config.notch_dock_mode {
            (12.0 * config.scale).round() * 2.0
        } else {
            0.0
        };
        let initial_w = config.idle_width * config.scale + ear_pad;
        let initial_h = 30.0 * config.scale;
        let initial_size = Vec2::new(initial_w, initial_h);
        let notch_morph = if config.notch_dock_mode { 1.0 } else { 0.0 };

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
            settings_focus_requested: false,
            settings_tab: SettingsTab::Appearance,
            new_todo_input: String::new(),
            calendar_url_input,
            current_size: initial_size,
            applied_size: Vec2::ZERO,
            notch_morph,
            applied_top_y: -1.0,
            test_notif_seq: 0,
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

    fn notch_ear_width(&self) -> f32 {
        12.0 * self.config.scale * self.notch_morph
    }

    fn effective_top_offset(&self) -> f32 {
        self.config.top_offset_px * (1.0 - self.notch_morph)
    }

    fn toggle_dock_mode(&mut self) {
        self.config.notch_dock_mode = !self.config.notch_dock_mode;
        if let Some(tray) = &self.tray {
            tray.update_dock_mode(self.config.notch_dock_mode);
        }
        if let Err(err) = self.config.save() {
            eprintln!("Failed to save config: {err}");
        }
    }

    fn push_test_notification(&mut self, now: Instant) {
        let samples = [
            ("WhatsApp", "Budi", "Meeting jam 14:00 jadi pindah ke Discord?"),
            ("Discord", "#general — Alex", "Deploy staging barusan sukses, cek log!"),
            ("Gmail", "GitHub", "[liebert-dynamic-island] PR #12 merged"),
            ("Google Chrome", "Download Complete", "release-v0.1.0-x86_64.zip siap dibuka"),
        ];
        let idx = (self.test_notif_seq as usize) % samples.len();
        self.test_notif_seq = self.test_notif_seq.wrapping_add(1);
        let (app_name, title, body) = samples[idx];
        self.state.push_notification(
            NotificationItem {
                id: format!("test-{}-{}", self.test_notif_seq, app_name),
                app_name: app_name.to_owned(),
                title: title.to_owned(),
                body: body.to_owned(),
                icon_rgba: None,
                timestamp: 0,
            },
            now,
        );
    }

    fn target_size(&self) -> Vec2 {
        let s = self.config.scale;
        let ear_pad = self.notch_ear_width() * 2.0;
        let mut size = match self.state.visual {
            IslandVisualState::Idle => {
                if matches!(self.state.listener_status, ListenerStatus::AccessDenied(_)) {
                    Vec2::new(330.0 * s, 44.0 * s)
                } else {
                    match self.effective_idle_mode() {
                        IdleDisplayMode::Lyrics
                            if self.config.show_lyrics && self.media.is_some() =>
                        {
                            Vec2::new(self.config.idle_width.max(255.0) * s, 32.0 * s)
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
            IslandVisualState::Compact => Vec2::new(348.0 * s, 60.0 * s),
            IslandVisualState::Expanded => {
                if self.state.active.is_none()
                    && self.active_tab == IslandTab::Music
                    && self.media.is_some()
                {
                    Vec2::new(self.config.expanded_width.max(468.0) * s, 286.0 * s)
                } else {
                    Vec2::new(self.config.expanded_width.max(430.0) * s, 184.0 * s)
                }
            }
        };
        size.x += ear_pad;
        size
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
        let duration_secs = self
            .media
            .as_ref()
            .and_then(|m| (m.duration_ms > 0).then_some(m.duration_ms / 1000));
        self.lyrics = LyricsData::Loading;
        let _ = self.channels.lyrics_req_tx.send(LyricsRequest {
            artist,
            title,
            duration_secs,
            translate_lang,
            need_romaji: self.config.romaji_lyrics
                || self.config.karaoke_lyric_mode == KaraokeLyricMode::Romaji,
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

    fn current_lyric_ms(&self, now: Instant) -> u64 {
        let base = self.current_playback_ms(now) as i64;
        (base + self.config.lyric_offset_ms).max(0) as u64
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
                PlatformEvent::Notification(item) => {
                    if self.config.show_notifications {
                        self.state.push_notification(item, now);
                    }
                }
                PlatformEvent::Removed(id) => self.state.remove_by_id(&id, now),
                PlatformEvent::Status(status) => {
                    self.state.listener_status = status;
                    status_changed = true;
                }
                PlatformEvent::Media(track) => {
                    self.media = track;
                    if let Some((artist, title)) = self
                        .media
                        .as_ref()
                        .map(|t| (t.artist.clone(), t.title.clone()))
                    {
                        let key = (artist.clone(), title.clone());
                        if self.last_lyrics_key.as_ref() != Some(&key) {
                            if has_cjk_or_hangul(&title) || has_cjk_or_hangul(&artist) {
                                self.ensure_cjk_font_loaded(ctx);
                            }
                            self.last_lyrics_key = Some(key);
                            self.request_lyrics_for_current();
                        }
                    } else {
                        self.last_lyrics_key = None;
                        self.lyrics = LyricsData::NotFound;
                    }
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

        let tray_events = self
            .tray
            .as_ref()
            .map(|tray| (tray.poll_actions(), tray.is_auto_hide_checked()));
        if let Some((actions, auto_hide_checked)) = tray_events {
            for action in actions {
                match action {
                    TrayAction::OpenSettings => {
                        self.open_settings(SettingsTab::Appearance, ctx);
                    }
                    TrayAction::ToggleDockMode => {
                        self.toggle_dock_mode();
                        ctx.request_repaint();
                    }
                    TrayAction::TestNotification => {
                        self.push_test_notification(now);
                        ctx.request_repaint();
                    }
                    TrayAction::TogglePause => {
                        self.state.set_paused(!self.state.paused);
                        status_changed = true;
                    }
                    TrayAction::ToggleAutoHide => {
                        self.state.set_auto_hide(auto_hide_checked, now);
                        self.config.auto_hide_when_idle = auto_hide_checked;
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

    fn open_settings(&mut self, tab: SettingsTab, ctx: &egui::Context) {
        self.settings_tab = tab;
        self.show_settings = true;
        self.settings_focus_requested = true;
        ctx.request_repaint();
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
            self.no_activate_applied = true;
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.001, 0.05);
        self.last_frame = now;

        self.handle_events(ctx, now);

        let target_morph = if self.config.notch_dock_mode { 1.0 } else { 0.0 };
        let morph_diff = target_morph - self.notch_morph;
        let morphing = morph_diff.abs() >= 0.004;
        if morphing {
            let m_t = 1.0 - (-12.0 * dt).exp();
            self.notch_morph += morph_diff * m_t;
            if (target_morph - self.notch_morph).abs() < 0.004 {
                self.notch_morph = target_morph;
            }
        } else {
            self.notch_morph = target_morph;
        }

        let mut fs_changed = false;
        if now.duration_since(self.last_fullscreen_check) >= Duration::from_millis(250) {
            self.last_fullscreen_check = now;
            let fs = self.config.hide_in_fullscreen && platform::is_foreground_fullscreen();
            fs_changed = self.state.set_fullscreen_active(fs);
        }

        // Win32 WS_EX_NOACTIVATE windows can drop pointer-hover events; poll OS cursor bounds
        // and top-edge trigger zone (even while auto-hidden) so hovering top-center reveals the notch.
        let top_y = self.effective_top_offset();
        let trigger_w = self.config.idle_width.max(180.0) * self.config.scale;
        let top_trigger =
            platform::is_cursor_in_top_trigger(top_y, trigger_w, ctx.pixels_per_point());
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

        let visible = self.show_settings || self.state.should_be_visible(now);
        if visible != self.last_visible {
            self.last_visible = visible;
            ctx.send_viewport_cmd(ViewportCommand::Visible(visible));
        }

        if self.show_settings && self.settings_focus_requested {
            if platform::bring_window_to_front(SETTINGS_WINDOW_TITLE) {
                self.settings_focus_requested = false;
            } else {
                ctx.request_repaint();
            }
        }

        let target = self.target_size();
        let t = 1.0 - (-14.0 * dt).exp();
        self.current_size += (target - self.current_size) * t;
        let animating = (target - self.current_size).length_sq() > 0.25;
        if !animating {
            self.current_size = target;
        }

        if (self.current_size - self.applied_size).length_sq() > 0.25
            || (top_y - self.applied_top_y).abs() > 0.25
            || hover_changed
        {
            self.applied_size = self.current_size;
            self.applied_top_y = top_y;
            let screen_w =
                platform::primary_monitor_logical_width() / ctx.pixels_per_point().max(1.0);
            let x = ((screen_w - self.current_size.x) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.current_size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(x, top_y)));
        }
        if self.notch_morph >= 0.999 && self.last_visible {
            platform::pin_window_top_y(WINDOW_TITLE, 0);
        }

        let playing_music = self.config.show_lyrics && self.media.as_ref().is_some_and(|m| m.is_playing);
        let playing_lyrics = playing_music && matches!(self.lyrics, LyricsData::Synced(_));
        let showing_idle_bars =
            self.state.visual == IslandVisualState::Idle && playing_music;
        let showing_live_seconds = self.state.visual == IslandVisualState::Idle
            && self.effective_idle_mode() == IdleDisplayMode::Clock
            && self.config.clock_show_seconds;

        if animating || morphing || hover_changed || fs_changed {
            ctx.request_repaint();
        } else if playing_lyrics && self.config.karaoke_lyrics {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if showing_idle_bars {
            ctx.request_repaint_after(Duration::from_millis(50));
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

        let pill_radius = (self.current_size.y * 0.5).clamp(12.0, 28.0);
        let notch_radius = (14.0 * self.config.scale).clamp(10.0, 20.0);
        let bot_radius = if self.state.visual == IslandVisualState::Expanded {
            self.config.corner_radius.min(26)
        } else {
            lerp_u8(
                pill_radius.round() as u8,
                notch_radius.round() as u8,
                self.notch_morph,
            )
        };
        let top_radius = ((bot_radius as f32) * (1.0 - self.notch_morph)).round() as u8;
        let border_alpha = ((self.config.border_alpha as f32) * (1.0 - self.notch_morph)).round() as u8;

        let is_music_card = self.state.visual == IslandVisualState::Expanded
            && self.state.active.is_none()
            && self.active_tab == IslandTab::Music
            && self.media.is_some();

        let ear_w = self.notch_ear_width();
        let card_rect = ui.max_rect();

        let frame = if is_music_card {
            let floating_bg = [30, 29, 28, self.config.bg_color[3].max(240)];
            let docked_bg = [12, 12, 14, 252];
            let base_bg = [
                lerp_u8(floating_bg[0], docked_bg[0], self.notch_morph),
                lerp_u8(floating_bg[1], docked_bg[1], self.notch_morph),
                lerp_u8(floating_bg[2], docked_bg[2], self.notch_morph),
                lerp_u8(floating_bg[3], docked_bg[3], self.notch_morph),
            ];
            let accent_rgb = self.effective_accent_rgb();
            self.ensure_gradient_texture(&ctx, base_bg, accent_rgb);
            paint_island_surface(
                ui.painter(),
                card_rect,
                SurfaceStyle {
                    ear_w,
                    top_radius,
                    bot_radius,
                    fill: Color32::from_rgba_unmultiplied(
                        base_bg[0], base_bg[1], base_bg[2], base_bg[3],
                    ),
                    grad_tex: self.gradient_tex.as_ref(),
                    border_alpha,
                },
            );
            let pad_x = (16.0 + ear_w).round() as i8;
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(pad_x, 10))
        } else {
            let base = self.bg_color32();
            let fill = if self.state.visual == IslandVisualState::Idle {
                Color32::from_rgba_unmultiplied(
                    lerp_u8(base.r(), 7, self.notch_morph),
                    lerp_u8(base.g(), 7, self.notch_morph),
                    lerp_u8(base.b(), 9, self.notch_morph),
                    lerp_u8(base.a(), 252, self.notch_morph),
                )
            } else {
                base
            };
            paint_island_surface(
                ui.painter(),
                card_rect,
                SurfaceStyle {
                    ear_w,
                    top_radius,
                    bot_radius,
                    fill,
                    grad_tex: None,
                    border_alpha,
                },
            );
            let pad_x = (13.0 + ear_w).round() as i8;
            let pad_y = if self.state.visual == IslandVisualState::Idle {
                5
            } else {
                9
            };
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(pad_x, pad_y))
        };

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            match self.state.visual {
                IslandVisualState::Expanded => {
                    if self.current_size.y >= 95.0 * self.config.scale {
                        self.render_expanded(ui, &ctx, now);
                    }
                }
                IslandVisualState::Compact => self.render_compact(ui),
                IslandVisualState::Idle => self.render_idle(ui, &ctx, now),
            }
        });

        let prev_lyric_opts = (
            self.config.romaji_lyrics,
            self.config.karaoke_lyric_mode,
            self.config.translate_lyrics,
            self.config.lyric_translate_lang.clone(),
        );
        let mut config_dirty = false;
        let mut test_notif_req = false;
        render_settings_viewport(
            &ctx,
            SettingsWindowState {
                open: &mut self.show_settings,
                focus_requested: &mut self.settings_focus_requested,
                tab: &mut self.settings_tab,
                config: &mut self.config,
                todos: &mut self.todos,
                new_todo_input: &mut self.new_todo_input,
                calendar_url_input: &mut self.calendar_url_input,
                calendar_events: &self.calendar_events,
                calendar_status: &self.calendar_status,
                calendar_cmd_tx: &self.channels.calendar_cmd_tx,
                config_dirty: &mut config_dirty,
                test_notification_requested: &mut test_notif_req,
            },
        );
        if test_notif_req {
            self.push_test_notification(now);
            ctx.request_repaint();
        }
        if config_dirty {
            let new_lyric_opts = (
                self.config.romaji_lyrics,
                self.config.karaoke_lyric_mode,
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
                tray.update_dock_mode(self.config.notch_dock_mode);
            }
            if let Err(err) = self.config.save() {
                eprintln!("Failed to save config: {err}");
            }
            ctx.request_repaint();
        }
    }
}

impl IslandApp {
    fn render_idle(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
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
                if let Some(track) = self.media.clone() {
                    self.sync_album_art_texture(ctx, &track);
                    let pos_ms = self.current_lyric_ms(now);
                    let active = active_karaoke(&self.lyrics, pos_ms);

                    ui.horizontal_centered(|ui| {
                        // Left mini album cover or soft circular music disc (matching reference notch).
                        let badge_sz = 19.0 * s;
                        let (badge_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(badge_sz), Sense::hover());
                        let badge_rad = CornerRadius::same((5.0 * s).round() as u8);
                        if let Some(tex) = &self.album_art_tex {
                            let shape =
                                RectShape::filled(badge_rect, badge_rad, Color32::WHITE)
                                    .with_texture(
                                        tex.id(),
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                    );
                            ui.painter().add(shape);
                        } else {
                            ui.painter().rect_filled(
                                badge_rect,
                                badge_rad,
                                Color32::from_rgba_unmultiplied(36, 36, 42, 235),
                            );
                            ui.painter().circle_filled(
                                badge_rect.center(),
                                badge_sz * 0.34,
                                Color32::from_rgba_unmultiplied(235, 235, 240, 225),
                            );
                            ui.painter().text(
                                badge_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "♪",
                                FontId::proportional(9.5 * s),
                                Color32::from_rgb(20, 20, 24),
                            );
                        }

                        ui.add_space(3.0 * s);
                        let bars_reserve = 22.0 * s;
                        let text_w = (ui.available_width() - bars_reserve).max(40.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(text_w, 20.0 * s),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new("♪")
                                        .font(FontId::proportional(11.0 * s))
                                        .color(muted),
                                );
                                if let Some(k) = active {
                                    let lyric_text = if self.config.karaoke_lyric_mode
                                        == KaraokeLyricMode::Romaji
                                    {
                                        k.romaji.unwrap_or(k.text)
                                    } else {
                                        k.text
                                    };
                                    if self.config.karaoke_lyrics {
                                        render_karaoke_line(
                                            ui,
                                            lyric_text,
                                            k.progress,
                                            FontId::proportional(11.5 * s),
                                            muted,
                                            text_col,
                                        );
                                    } else {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(lyric_text)
                                                    .font(FontId::proportional(11.5 * s))
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
                                                .font(FontId::proportional(11.5 * s))
                                                .color(text_col),
                                        )
                                        .truncate(),
                                    );
                                }
                            },
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let secs = ui.input(|i| i.time) as f32;
                            render_audio_bars(ui, secs, track.is_playing, Color32::from_gray(225), s);
                        });
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
        let accent = self.effective_accent();
        let brand = app_brand_color(&item.app_name, accent);
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;

        ui.horizontal_centered(|ui| {
            render_app_avatar(ui, &item.app_name, brand, 28.0 * s, s);
            ui.add_space(6.0 * s);
            let badge_reserve = if self.state.queued_count() > 0 {
                34.0 * s
            } else {
                0.0
            };
            let text_w = (ui.available_width() - badge_reserve).max(60.0);
            ui.allocate_ui_with_layout(
                Vec2::new(text_w, 38.0 * s),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.horizontal(|ui| {
                        if !item.app_name.eq_ignore_ascii_case(&item.title) {
                            egui::Frame::new()
                                .fill(Color32::from_rgba_unmultiplied(
                                    brand.r(),
                                    brand.g(),
                                    brand.b(),
                                    36,
                                ))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(5, 1))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(&item.app_name)
                                            .font(FontId::proportional(9.0 * s))
                                            .color(brand)
                                            .strong(),
                                    );
                                });
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(&item.title)
                                    .font(FontId::proportional(12.5 * s))
                                    .color(text_col)
                                    .strong(),
                            )
                            .truncate(),
                        );
                    });
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
                },
            );
            let queued = self.state.queued_count();
            if queued > 0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 22))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(7, 2))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("+{queued}"))
                                    .font(FontId::proportional(10.5 * s))
                                    .color(text_col),
                            );
                        });
                });
            }
        });
    }

    fn render_expanded(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: Instant) {
        let accent = self.effective_accent();
        let text_col = self.text_color32();
        let muted = self.muted_color32();
        let s = self.config.scale;
        let mode_btn_label = if self.config.notch_dock_mode {
            "Island"
        } else {
            "Notch"
        };

        if let Some(item) = self.state.active.clone() {
            let brand = app_brand_color(&item.app_name, accent);
            let row_h = 26.0 * s;
            let right_w = 152.0 * s;
            let left_w = (ui.available_width() - right_w - 6.0 * s).max(80.0);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(left_w, row_h),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_clip_rect(ui.max_rect());
                        render_app_avatar(ui, &item.app_name, brand, 22.0 * s, s);
                        let queued = self.state.queued_count();
                        if queued > 0 {
                            ui.label(
                                RichText::new(format!("+{queued}"))
                                    .font(FontId::proportional(10.5 * s))
                                    .color(muted),
                            );
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(&item.app_name)
                                    .font(FontId::proportional(11.5 * s))
                                    .color(brand)
                                    .strong(),
                            )
                            .truncate(),
                        );
                    },
                );
                ui.add_space(6.0 * s);
                ui.allocate_ui_with_layout(
                    Vec2::new(right_w, row_h),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.set_clip_rect(ui.max_rect());
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("⚙")
                                        .font(FontId::proportional(12.0 * s))
                                        .color(muted),
                                )
                                .min_size(Vec2::new(24.0 * s, 22.0 * s))
                                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 16))
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .on_hover_text("Open Settings")
                            .clicked()
                        {
                            self.open_settings(SettingsTab::Appearance, ctx);
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(mode_btn_label)
                                        .font(FontId::proportional(10.5 * s))
                                        .color(muted),
                                )
                                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 16))
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .on_hover_text("Switch Notch / Floating Island")
                            .clicked()
                        {
                            self.toggle_dock_mode();
                            ctx.request_repaint();
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Dismiss")
                                        .font(FontId::proportional(11.0 * s))
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 24))
                                .corner_radius(CornerRadius::same(10)),
                            )
                            .clicked()
                        {
                            self.state.dismiss_active(now);
                            ctx.request_repaint();
                        }
                    },
                );
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(&item.title)
                    .font(FontId::proportional(13.5 * s))
                    .color(text_col)
                    .strong(),
            );
            if !item.body.is_empty() {
                ui.add_space(3.0);
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
        let row_h = 24.0 * s;
        let right_w = 88.0 * s;
        let left_w = (ui.available_width() - right_w - 6.0 * s).max(160.0);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, row_h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.spacing_mut().item_spacing.x = 4.0 * s;
                    render_pill_tab(ui, &mut self.active_tab, IslandTab::Overview, "Home", accent, s);
                    if self.config.show_lyrics {
                        render_pill_tab(ui, &mut self.active_tab, IslandTab::Music, "♪ Music", accent, s);
                    }
                    if self.config.show_todo {
                        let label = format!("☑ {}", self.todos.pending_count());
                        render_pill_tab(ui, &mut self.active_tab, IslandTab::Todo, &label, accent, s);
                    }
                    if self.config.show_calendar {
                        render_pill_tab(ui, &mut self.active_tab, IslandTab::Calendar, "Cal", accent, s);
                    }
                },
            );
            ui.add_space(6.0 * s);
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, row_h),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.spacing_mut().item_spacing.x = 4.0 * s;
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("⚙")
                                    .font(FontId::proportional(12.0 * s))
                                    .color(muted),
                            )
                            .min_size(Vec2::new(24.0 * s, 22.0 * s))
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 16))
                            .corner_radius(CornerRadius::same(10)),
                        )
                        .on_hover_text("Open Settings")
                        .clicked()
                    {
                        self.open_settings(SettingsTab::Appearance, ctx);
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(mode_btn_label)
                                    .font(FontId::proportional(10.5 * s))
                                    .color(muted),
                            )
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 16))
                            .corner_radius(CornerRadius::same(10)),
                        )
                        .on_hover_text("Switch Notch / Floating Island")
                        .clicked()
                    {
                        self.toggle_dock_mode();
                        ctx.request_repaint();
                    }
                },
            );
        });
        if !is_music_card {
            ui.add_space(2.0);
            ui.separator();
            ui.add_space(2.0);
        } else {
            ui.add_space(6.0);
        }

        match self.active_tab {
            IslandTab::Overview => self.render_overview_tab(ui, now),
            IslandTab::Music => self.render_music_tab(ui, ctx, now),
            IslandTab::Todo => self.render_todo_tab(ui),
            IslandTab::Calendar => self.render_calendar_tab(ui),
        }
    }

    fn render_overview_tab(&mut self, ui: &mut egui::Ui, now: Instant) {
        let accent = self.effective_accent();
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
                    .font(FontId::monospace(15.0 * s))
                    .color(text_col)
                    .strong(),
            );
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 16))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(tz_label)
                            .font(FontId::proportional(9.5 * s))
                            .color(muted),
                    );
                });
        });
        ui.add_space(4.0);

        if self.config.show_lyrics {
            if let Some(track) = &self.media {
                let pos_ms = self.current_lyric_ms(now);
                let lyric = active_karaoke(&self.lyrics, pos_ms)
                    .map(|k| {
                        if self.config.karaoke_lyric_mode == KaraokeLyricMode::Romaji {
                            k.romaji.unwrap_or(k.text)
                        } else {
                            k.text
                        }
                    })
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
        let lyric_ms = self.current_lyric_ms(now);
        let total_w = ui.available_width();
        let col_gap = 14.0 * s;
        let left_w = (total_w * 0.43).max(148.0 * s);
        let right_w = (total_w - left_w - col_gap).max(120.0 * s);
        let body_h = ui.available_height().max(140.0 * s);

        ui.horizontal_top(|ui| {
            // Left column: Album Art, centered Title & Artist, Playbar, and Transport Controls.
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, body_h),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    let art_size = (left_w - 26.0 * s).clamp(84.0 * s, 118.0 * s);
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
                    ui.set_clip_rect(ui.max_rect());
                    egui::ScrollArea::vertical()
                        .id_salt("music_lyrics_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match &self.lyrics {
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
                                if let Some(k) = active_karaoke(&self.lyrics, lyric_ms) {
                                    let dim_white =
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 90);
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
                                    let romaji_primary = self.config.karaoke_lyric_mode
                                        == KaraokeLyricMode::Romaji
                                        && k.romaji.is_some();
                                    let primary_text = if romaji_primary {
                                        k.romaji.unwrap_or(k.text)
                                    } else {
                                        k.text
                                    };
                                    let wrap_w = ui.available_width().max(60.0);
                                    render_wrapped_karaoke(
                                        ui,
                                        primary_text,
                                        orig_prog,
                                        FontId::proportional(14.0 * s),
                                        text_col,
                                        dim_white,
                                        wrap_w,
                                    );

                                    if romaji_primary {
                                        ui.add_space(2.0 * s);
                                        render_wrapped_karaoke(
                                            ui,
                                            k.text,
                                            orig_prog,
                                            FontId::proportional(11.5 * s),
                                            lit_romaji,
                                            dim_romaji,
                                            wrap_w,
                                        );
                                    } else if self.config.romaji_lyrics
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
                                            wrap_w,
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
                                        let (next_primary, next_secondary) = if self
                                            .config
                                            .karaoke_lyric_mode
                                            == KaraokeLyricMode::Romaji
                                            && let Some(next_rom) = k.next_romaji
                                        {
                                            (next_rom, Some(next_line))
                                        } else {
                                            (
                                                next_line,
                                                if self.config.romaji_lyrics {
                                                    k.next_romaji
                                                } else {
                                                    None
                                                },
                                            )
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(next_primary)
                                                    .font(FontId::proportional(12.5 * s))
                                                    .color(Color32::from_rgba_unmultiplied(
                                                        255, 255, 255, 58,
                                                    ))
                                                    .strong(),
                                            )
                                            .wrap(),
                                        );
                                        if let Some(next_sub) = next_secondary {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(next_sub)
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
                        });
                },
            );
        });
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
                    self.open_settings(SettingsTab::Todo, ui.ctx());
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
                self.open_settings(SettingsTab::Calendar, ui.ctx());
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
    let wrap_w = max_width.max(40.0);
    let dim_galley = ui
        .painter()
        .layout(text.to_owned(), font.clone(), dim_color, wrap_w);
    let lit_galley = ui.painter().layout(text.to_owned(), font, lit_color, wrap_w);
    let (rect, _) = ui.allocate_exact_size(dim_galley.size(), Sense::hover());

    let clip = ui.clip_rect();
    ui.painter()
        .with_clip_rect(clip)
        .galley(rect.min, dim_galley.clone(), Color32::TRANSPARENT);

    let p = progress.clamp(0.0, 1.0);
    let total_w: f32 = dim_galley.rows.iter().map(|r| r.rect().width()).sum();
    let mut rem = total_w * p;
    for placed in &dim_galley.rows {
        if rem <= 0.5 {
            break;
        }
        let rr = placed.rect().translate(rect.min.to_vec2());
        let rw = rr.width().max(0.0);
        let lit_w = rem.min(rw);
        if lit_w > 0.5 {
            let row_clip = Rect::from_min_max(rr.min, Pos2::new(rr.left() + lit_w, rr.bottom()))
                .intersect(clip);
            if row_clip.width() > 0.0 && row_clip.height() > 0.0 {
                ui.painter().with_clip_rect(row_clip).galley(
                    rect.min,
                    lit_galley.clone(),
                    Color32::TRANSPARENT,
                );
            }
        }
        rem -= rw;
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    ((a as f32) + ((b as f32) - (a as f32)) * t).round() as u8
}

struct SurfaceStyle<'a> {
    ear_w: f32,
    top_radius: u8,
    bot_radius: u8,
    fill: Color32,
    grad_tex: Option<&'a TextureHandle>,
    border_alpha: u8,
}

fn paint_island_surface(painter: &egui::Painter, rect: Rect, style: SurfaceStyle<'_>) {
    let SurfaceStyle {
        ear_w,
        top_radius,
        bot_radius,
        fill,
        grad_tex,
        border_alpha,
    } = style;
    let top_y = if ear_w > 0.5 {
        rect.top() - 1.0
    } else {
        rect.top()
    };
    let body_rect = Rect::from_min_max(
        Pos2::new(rect.left() + ear_w, top_y),
        Pos2::new(rect.right() - ear_w, rect.bottom()),
    );
    let body_corners = CornerRadius {
        nw: top_radius,
        ne: top_radius,
        sw: bot_radius,
        se: bot_radius,
    };

    if let Some(tex) = grad_tex {
        let shape = RectShape::filled(body_rect, body_corners, Color32::WHITE)
            .with_texture(tex.id(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)));
        painter.add(shape);
    } else {
        painter.rect_filled(body_rect, body_corners, fill);
    }

    if border_alpha > 0 {
        painter.rect_stroke(
            body_rect,
            body_corners,
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, border_alpha),
            ),
            StrokeKind::Inside,
        );
    }

    if ear_w <= 0.5 {
        return;
    }

    let ear_color = if grad_tex.is_some() {
        Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), fill.a())
    } else {
        fill
    };
    let r = ear_w.min(rect.height() * 0.45);
    const SEGS: usize = 10;

    // Top-left concave ear
    let mut left_mesh = Mesh::default();
    let left_apex = Pos2::new(body_rect.left(), top_y);
    let left_center = Pos2::new(body_rect.left() - r, top_y + r);
    left_mesh.colored_vertex(left_apex, ear_color);
    for i in 0..=SEGS {
        let t = i as f32 / SEGS as f32;
        let angle = -FRAC_PI_2 + FRAC_PI_2 * t;
        let p = Pos2::new(
            left_center.x + r * angle.cos(),
            left_center.y + r * angle.sin(),
        );
        left_mesh.colored_vertex(p, ear_color);
        if i > 0 {
            left_mesh.add_triangle(0, i as u32, (i + 1) as u32);
        }
    }
    painter.add(Shape::mesh(left_mesh));

    // Top-right concave ear
    let mut right_mesh = Mesh::default();
    let right_apex = Pos2::new(body_rect.right(), top_y);
    let right_center = Pos2::new(body_rect.right() + r, top_y + r);
    right_mesh.colored_vertex(right_apex, ear_color);
    for i in 0..=SEGS {
        let t = i as f32 / SEGS as f32;
        let angle = -FRAC_PI_2 - (PI - FRAC_PI_2) * t;
        let p = Pos2::new(
            right_center.x + r * angle.cos(),
            right_center.y + r * angle.sin(),
        );
        right_mesh.colored_vertex(p, ear_color);
        if i > 0 {
            right_mesh.add_triangle(0, i as u32, (i + 1) as u32);
        }
    }
    painter.add(Shape::mesh(right_mesh));
}

fn app_brand_color(app_name: &str, fallback: Color32) -> Color32 {
    let lower = app_name.to_ascii_lowercase();
    if lower.contains("whatsapp") {
        Color32::from_rgb(37, 211, 102)
    } else if lower.contains("discord") {
        Color32::from_rgb(88, 101, 242)
    } else if lower.contains("gmail") || lower == "mail" {
        Color32::from_rgb(234, 67, 53)
    } else if lower.contains("outlook") {
        Color32::from_rgb(0, 120, 212)
    } else if lower.contains("chrome") {
        Color32::from_rgb(66, 133, 244)
    } else if lower.contains("edge") {
        Color32::from_rgb(0, 120, 215)
    } else if lower.contains("firefox") {
        Color32::from_rgb(255, 113, 57)
    } else if lower.contains("brave") {
        Color32::from_rgb(251, 84, 43)
    } else if lower.contains("telegram") {
        Color32::from_rgb(42, 171, 238)
    } else if lower.contains("slack") {
        Color32::from_rgb(224, 30, 90)
    } else if lower.contains("spotify") {
        Color32::from_rgb(29, 185, 84)
    } else {
        fallback
    }
}

fn render_audio_bars(
    ui: &mut egui::Ui,
    time_secs: f32,
    is_playing: bool,
    color: Color32,
    scale: f32,
) {
    let bar_w = 2.4 * scale;
    let gap = 2.2 * scale;
    let total_w = bar_w * 3.0 + gap * 2.0;
    let max_h = 12.0 * scale;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, max_h), Sense::hover());
    let painter = ui.painter();

    for i in 0..3 {
        let h_frac = if is_playing {
            let phase = time_secs * (5.2 + i as f32 * 1.4) + i as f32 * 1.9;
            0.35 + 0.65 * (0.5 + 0.5 * phase.sin())
        } else {
            [0.35, 0.55, 0.35][i]
        };
        let h = (max_h * h_frac).max(3.0 * scale);
        let x = rect.left() + i as f32 * (bar_w + gap);
        let bar_rect = Rect::from_center_size(
            Pos2::new(x + bar_w * 0.5, rect.center().y),
            Vec2::new(bar_w, h),
        );
        painter.rect_filled(bar_rect, CornerRadius::same(2), color);
    }
}

fn render_app_avatar(
    ui: &mut egui::Ui,
    app_name: &str,
    accent: Color32,
    size: f32,
    scale: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let initial = app_name
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('L')
        .to_uppercase()
        .to_string();
    let painter = ui.painter();
    painter.circle_filled(
        rect.center(),
        size * 0.5,
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 44),
    );
    painter.circle_stroke(rect.center(), size * 0.5, Stroke::new(1.0, accent));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        FontId::proportional((size * 0.48).max(9.0 * scale)),
        Color32::WHITE,
    );
}

fn render_pill_tab(
    ui: &mut egui::Ui,
    active: &mut IslandTab,
    value: IslandTab,
    label: &str,
    accent: Color32,
    scale: f32,
) {
    let selected = *active == value;
    let fill = if selected {
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 48)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 10)
    };
    let text_col = if selected {
        Color32::WHITE
    } else {
        Color32::from_gray(175)
    };
    let stroke = if selected {
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 140))
    } else {
        Stroke::NONE
    };
    let btn = egui::Button::new(
        RichText::new(label)
            .font(FontId::proportional(11.0 * scale))
            .color(text_col),
    )
    .fill(fill)
    .stroke(stroke)
    .corner_radius(CornerRadius::same(10));
    if ui.add(btn).clicked() {
        *active = value;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_brand_color_maps_known_apps_and_falls_back() {
        let fb = Color32::from_rgb(10, 20, 30);
        assert_eq!(app_brand_color("WhatsApp", fb), Color32::from_rgb(37, 211, 102));
        assert_eq!(app_brand_color("Discord", fb), Color32::from_rgb(88, 101, 242));
        assert_eq!(app_brand_color("Gmail", fb), Color32::from_rgb(234, 67, 53));
        assert_eq!(app_brand_color("Google Chrome", fb), Color32::from_rgb(66, 133, 244));
        assert_eq!(app_brand_color("Unknown App", fb), fb);
    }
}
