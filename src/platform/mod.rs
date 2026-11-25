use std::sync::{Arc, mpsc};
use std::time::Instant;

use crate::calendar::CalendarEvent;
use crate::lyrics::LyricsData;
use crate::state::{ListenerStatus, NotificationItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumArt {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub dominant_rgb: Option<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTrack {
    pub title: String,
    pub artist: String,
    pub is_playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub album_art: Option<AlbumArt>,
    pub updated_at: Instant,
}

pub fn decode_album_art(bytes: &[u8]) -> Option<AlbumArt> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = img.resize(160, 160, image::imageops::FilterType::Triangle);
    let rgba_img = resized.to_rgba8();
    let width = rgba_img.width() as usize;
    let height = rgba_img.height() as usize;
    let rgba = rgba_img.into_raw();
    let dominant_rgb = extract_dominant_accent(&rgba);
    Some(AlbumArt {
        width,
        height,
        rgba,
        dominant_rgb,
    })
}

pub fn extract_dominant_accent(rgba: &[u8]) -> Option<[u8; 3]> {
    let mut sum_r = 0.0f64;
    let mut sum_g = 0.0f64;
    let mut sum_b = 0.0f64;
    let mut weight_sum = 0.0f64;

    for &[r, g, b, a] in rgba.as_chunks::<4>().0.iter().step_by(4) {
        if a < 180 {
            continue;
        }
        let max_c = r.max(g).max(b);
        let min_c = r.min(g).min(b);
        if max_c < 45 {
            continue;
        }
        let sat = (max_c - min_c) as f64 / max_c as f64;
        if sat < 0.22 {
            continue;
        }
        let w = sat * (max_c as f64 / 255.0);
        sum_r += r as f64 * w;
        sum_g += g as f64 * w;
        sum_b += b as f64 * w;
        weight_sum += w;
    }

    if weight_sum <= 0.0 {
        return None;
    }

    let mut r = sum_r / weight_sum;
    let mut g = sum_g / weight_sum;
    let mut b = sum_b / weight_sum;
    let peak = r.max(g).max(b);
    if peak > 0.0 && peak < 180.0 {
        let boost = 180.0 / peak;
        r = (r * boost).min(255.0);
        g = (g * boost).min(255.0);
        b = (b * boost).min(255.0);
    }

    Some([r.round() as u8, g.round() as u8, b.round() as u8])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    TogglePlayPause,
    Next,
    Previous,
    SetShuffle(bool),
    SetRepeat(RepeatMode),
    SeekMs(u64),
}

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    Notification(NotificationItem),
    Removed(String),
    Status(ListenerStatus),
    Media(Option<MediaTrack>),
    LyricsLoaded {
        artist: String,
        title: String,
        lyrics: LyricsData,
    },
    CalendarUpdated(Result<Vec<CalendarEvent>, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamilyTarget {
    Proportional,
    Monospace,
    Both,
}

#[derive(Debug, Clone)]
pub struct FontFileSpec {
    pub name: String,
    pub path: String,
    pub family: FontFamilyTarget,
    pub face_indices: &'static [u32],
}

#[derive(Debug, Clone, Copy)]
pub struct HoverPollInput<'a> {
    pub window_title: &'a str,
    pub ctx_hovered: bool,
    pub pointer_busy: bool,
    pub currently_hovered: bool,
    pub visible: bool,
    pub animating: bool,
    pub top_offset_px: f32,
    pub trigger_width_px: f32,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub pixels_per_point: f32,
}

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub mod wayland_layer;

pub fn spawn_listener(tx: mpsc::Sender<PlatformEvent>, on_wake: Arc<dyn Fn() + Send + Sync>) {
    #[cfg(windows)]
    windows::spawn_listener(tx, on_wake);

    #[cfg(target_os = "linux")]
    linux::spawn_listener(tx, on_wake);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = tx.send(PlatformEvent::Status(ListenerStatus::Error(
            "Unsupported OS".to_owned(),
        )));
        on_wake();
    }
}

pub fn spawn_media_listener(
    tx: mpsc::Sender<PlatformEvent>,
    cmd_rx: mpsc::Receiver<MediaCommand>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    #[cfg(windows)]
    windows::spawn_media_listener(tx, cmd_rx, on_wake);

    #[cfg(target_os = "linux")]
    linux::spawn_media_listener(tx, cmd_rx, on_wake);

    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = (tx, cmd_rx, on_wake);
}

pub fn run_notification_spike() -> Result<String, String> {
    #[cfg(windows)]
    return windows::run_notification_spike();

    #[cfg(target_os = "linux")]
    return linux::run_notification_spike();

    #[cfg(not(any(windows, target_os = "linux")))]
    Err("Notification spike is only supported on Windows and Linux".to_owned())
}

pub fn overlay_resizable() -> bool {
    #[cfg(target_os = "linux")]
    return true;

    #[cfg(not(target_os = "linux"))]
    false
}

pub fn should_send_outer_position() -> bool {
    #[cfg(target_os = "linux")]
    return linux::should_send_outer_position();

    #[cfg(not(target_os = "linux"))]
    true
}

pub fn primary_monitor_logical_width(pixels_per_point: f32) -> f32 {
    #[cfg(windows)]
    return windows::primary_monitor_logical_width(pixels_per_point);

    #[cfg(target_os = "linux")]
    return linux::primary_monitor_logical_width(pixels_per_point);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = pixels_per_point;
        1920.0
    }
}

pub fn prepare_overlay_rules(window_title: &str, top_offset_px: f32) {
    #[cfg(target_os = "linux")]
    linux::prepare_overlay_rules(window_title, top_offset_px);

    #[cfg(not(target_os = "linux"))]
    let _ = (window_title, top_offset_px);
}

pub fn apply_overlay_window_setup(window_title: &str) -> bool {
    #[cfg(windows)]
    {
        windows::apply_no_activate_hook(window_title);
        return true;
    }

    #[cfg(target_os = "linux")]
    return linux::apply_no_activate_hook(window_title);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = window_title;
        true
    }
}

pub fn ensure_overlay_topmost(window_title: &str, force: bool) {
    #[cfg(target_os = "linux")]
    linux::ensure_overlay_topmost(window_title, force);

    #[cfg(not(target_os = "linux"))]
    let _ = (window_title, force);
}

pub fn sync_overlay_window(
    window_title: &str,
    x_logical: f32,
    y_logical: f32,
    w_logical: f32,
    h_logical: f32,
    notch_dock_mode: bool,
) {
    #[cfg(windows)]
    windows::sync_overlay_window(window_title, x_logical, y_logical, w_logical, h_logical, notch_dock_mode);

    #[cfg(target_os = "linux")]
    linux::sync_overlay_window(
        window_title,
        x_logical,
        y_logical,
        w_logical,
        h_logical,
        notch_dock_mode,
    );

    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = (window_title, x_logical, y_logical, w_logical, h_logical, notch_dock_mode);
}

pub fn detect_overlay_hover(input: HoverPollInput<'_>) -> bool {
    #[cfg(windows)]
    return windows::detect_overlay_hover(input);

    #[cfg(target_os = "linux")]
    return linux::detect_overlay_hover(input);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        input.visible && (input.pointer_busy || input.ctx_hovered)
    }
}

pub fn bring_window_to_front(window_title: &str) -> bool {
    #[cfg(windows)]
    return windows::bring_window_to_front(window_title);

    #[cfg(target_os = "linux")]
    return linux::bring_window_to_front(window_title);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = window_title;
        true
    }
}

pub fn is_foreground_fullscreen() -> bool {
    #[cfg(windows)]
    return windows::is_foreground_fullscreen();

    #[cfg(target_os = "linux")]
    return linux::is_foreground_fullscreen();

    #[cfg(not(any(windows, target_os = "linux")))]
    false
}

pub fn base_fallback_fonts() -> Vec<FontFileSpec> {
    #[cfg(windows)]
    return windows::base_fallback_fonts();

    #[cfg(target_os = "linux")]
    return linux::base_fallback_fonts();

    #[cfg(not(any(windows, target_os = "linux")))]
    Vec::new()
}

pub fn cjk_fallback_fonts() -> Vec<FontFileSpec> {
    #[cfg(windows)]
    return windows::cjk_fallback_fonts();

    #[cfg(target_os = "linux")]
    return linux::cjk_fallback_fonts();

    #[cfg(not(any(windows, target_os = "linux")))]
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_dominant_accent_picks_vibrant_color_and_skips_grayscale() {
        let mut rgba = Vec::new();
        for _ in 0..80 {
            rgba.extend_from_slice(&[250, 250, 250, 255]);
        }
        for _ in 0..20 {
            rgba.extend_from_slice(&[215, 70, 78, 255]);
        }
        let accent = extract_dominant_accent(&rgba).expect("red accent");
        assert!(accent[0] > 180 && accent[1] < 110 && accent[2] < 120);

        let gray = [180, 180, 180, 255].repeat(32);
        assert_eq!(extract_dominant_accent(&gray), None);
    }
}
