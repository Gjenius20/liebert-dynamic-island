use std::sync::{Arc, mpsc};

use crate::state::{ListenerStatus, NotificationItem};

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    Notification(NotificationItem),
    Removed(String),
    Status(ListenerStatus),
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
