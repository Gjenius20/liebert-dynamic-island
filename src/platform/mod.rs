use std::sync::{Arc, mpsc};
use std::time::Instant;

use crate::calendar::CalendarEvent;
use crate::lyrics::LyricsData;
use crate::state::{ListenerStatus, NotificationItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTrack {
    pub title: String,
    pub artist: String,
    pub is_playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub updated_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    TogglePlayPause,
    Next,
    Previous,
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

    #[cfg(not(windows))]
    1920.0
}

pub fn apply_no_activate_hook(window_title: &str) {
    #[cfg(windows)]
    windows::apply_no_activate_hook(window_title);

    #[cfg(not(windows))]
    let _ = window_title;
}

pub fn is_cursor_over_window(window_title: &str, padding_px: i32) -> Option<bool> {
    #[cfg(windows)]
    return windows::is_cursor_over_window(window_title, padding_px);

    #[cfg(not(windows))]
    {
        let _ = (window_title, padding_px);
        None
    }
}
