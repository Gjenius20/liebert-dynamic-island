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

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

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

pub fn primary_monitor_logical_width() -> f32 {
    #[cfg(windows)]
    return windows::primary_monitor_logical_width();

    #[cfg(target_os = "linux")]
    return linux::primary_monitor_logical_width();

    #[cfg(not(any(windows, target_os = "linux")))]
    1920.0
}

pub fn prepare_overlay_rules(window_title: &str, top_offset_px: f32) {
    #[cfg(target_os = "linux")]
    linux::prepare_overlay_rules(window_title, top_offset_px);

    #[cfg(not(target_os = "linux"))]
    let _ = (window_title, top_offset_px);
}

pub fn apply_no_activate_hook(window_title: &str) {
    #[cfg(windows)]
    windows::apply_no_activate_hook(window_title);

    #[cfg(target_os = "linux")]
    linux::apply_no_activate_hook(window_title);

    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = window_title;
}

pub fn pin_window_top_y(window_title: &str, target_y_px: i32) {
    #[cfg(windows)]
    windows::pin_window_top_y(window_title, target_y_px);

    #[cfg(not(windows))]
    let _ = (window_title, target_y_px);
}

pub fn bring_window_to_front(window_title: &str) -> bool {
    #[cfg(windows)]
    return windows::bring_window_to_front(window_title);

    #[cfg(not(windows))]
    {
        let _ = window_title;
        true
    }
}

pub fn is_cursor_over_window(window_title: &str, padding_px: i32) -> Option<bool> {
    #[cfg(windows)]
    return windows::is_cursor_over_window(window_title, padding_px);

    #[cfg(target_os = "linux")]
    return linux::is_cursor_over_window(window_title, padding_px);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (window_title, padding_px);
        None
    }
}

pub fn is_cursor_in_top_trigger(
    top_offset_px: f32,
    trigger_width_px: f32,
    pixels_per_point: f32,
) -> Option<bool> {
    #[cfg(windows)]
    return windows::is_cursor_in_top_trigger(top_offset_px, trigger_width_px, pixels_per_point);

    #[cfg(target_os = "linux")]
    return linux::is_cursor_in_top_trigger(top_offset_px, trigger_width_px, pixels_per_point);

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (top_offset_px, trigger_width_px, pixels_per_point);
        None
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
