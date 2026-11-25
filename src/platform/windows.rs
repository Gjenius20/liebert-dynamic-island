use std::collections::HashSet;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::UI::Notifications::Management::{
    UserNotificationListener, UserNotificationListenerAccessStatus,
};
use windows::UI::Notifications::{
    KnownNotificationBindings, NotificationKinds, UserNotification,
    UserNotificationChangedEventArgs,
};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GWL_EXSTYLE, GetCursorPos, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
    SM_CXSCREEN, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::core::HSTRING;

use crate::platform::{MediaCommand, MediaTrack, PlatformEvent};
use crate::state::{ListenerStatus, NotificationItem};

fn init_com_mta() {
    // Ignore RPC_E_CHANGED_MODE if COM is already initialized on this thread.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn access_status_label(status: UserNotificationListenerAccessStatus) -> &'static str {
    match status {
        UserNotificationListenerAccessStatus::Allowed => "Allowed",
        UserNotificationListenerAccessStatus::Denied => "Denied",
        UserNotificationListenerAccessStatus::Unspecified => "Unspecified",
        _ => "Unknown",
    }
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn extract_notification_item(notif: &UserNotification) -> Option<NotificationItem> {
    let id = notif.Id().ok()?.to_string();
    let app_name = notif
        .AppInfo()
        .and_then(|info| info.DisplayInfo())
        .and_then(|disp| disp.DisplayName())
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "Windows".to_owned());

    let mut lines = Vec::new();
    if let Ok(visual) = notif.Notification().and_then(|n| n.Visual())
        && let Ok(template) = KnownNotificationBindings::ToastGeneric()
        && let Ok(binding) = visual.GetBinding(&template)
        && let Ok(elements) = binding.GetTextElements()
    {
        for i in 0..elements.Size().unwrap_or(0) {
            if let Ok(elem) = elements.GetAt(i)
                && let Ok(text) = elem.Text()
            {
                let trimmed = text.to_string().trim().to_owned();
                if !trimmed.is_empty() {
                    lines.push(trimmed);
                }
            }
        }
    }

    let title = lines
        .first()
        .cloned()
        .unwrap_or_else(|| app_name.clone());
    let body = if lines.len() > 1 {
        lines[1..].join(" — ")
    } else {
        String::new()
    };

    Some(NotificationItem {
        id,
        app_name,
        title,
        body,
        icon_rgba: None,
        timestamp: unix_timestamp_secs(),
    })
}

pub fn run_notification_spike() -> Result<String, String> {
    init_com_mta();

    let listener = UserNotificationListener::Current()
        .map_err(|e| format!("UserNotificationListener::Current() failed: {e}"))?;

    let status = listener
        .RequestAccessAsync()
        .map_err(|e| format!("RequestAccessAsync() call failed: {e}"))?
        .join()
        .map_err(|e| format!("RequestAccessAsync() await failed: {e}"))?;

    if status != UserNotificationListenerAccessStatus::Allowed {
        return Err(format!(
            "Notification access status: {} (enable in Windows Settings > Privacy > Notifications)",
            access_status_label(status)
        ));
    }

    let toasts = listener
        .GetNotificationsAsync(NotificationKinds::Toast)
        .map_err(|e| format!("GetNotificationsAsync() call failed: {e}"))?
        .join()
        .map_err(|e| format!("GetNotificationsAsync() await failed: {e}"))?;

    let count = toasts.Size().unwrap_or(0);
    let mut previews = Vec::new();
    for i in 0..count.min(3) {
        if let Ok(n) = toasts.GetAt(i)
            && let Some(item) = extract_notification_item(&n)
        {
            previews.push(format!("[{}] {}", item.app_name, item.title));
        }
    }

    Ok(format!(
        "Access: Allowed | Active toasts: {} | Sample: {}",
        count,
        if previews.is_empty() {
            "none".to_owned()
        } else {
            previews.join(", ")
        }
    ))
}

pub fn spawn_listener(tx: mpsc::Sender<PlatformEvent>, on_wake: Arc<dyn Fn() + Send + Sync>) {
    std::thread::Builder::new()
        .name("win-notification-listener".to_owned())
        .spawn(move || {
            init_com_mta();

            let listener = match UserNotificationListener::Current() {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(PlatformEvent::Status(ListenerStatus::AccessDenied(format!(
                        "Listener unavailable: {e}"
                    ))));
                    on_wake();
                    return;
                }
            };

            let access = listener
                .RequestAccessAsync()
                .and_then(|op| op.join())
                .map_err(|e| e.to_string());

            match access {
                Ok(UserNotificationListenerAccessStatus::Allowed) => {}
                Ok(other) => {
                    let _ = tx.send(PlatformEvent::Status(ListenerStatus::AccessDenied(format!(
                        "Access {} — enable Windows Notifications access",
                        access_status_label(other)
                    ))));
                    on_wake();
                    return;
                }
                Err(err) => {
                    let _ = tx.send(PlatformEvent::Status(ListenerStatus::AccessDenied(err)));
                    on_wake();
                    return;
                }
            }

            let mut seen_ids: HashSet<u32> = HashSet::new();
            if let Ok(existing) = listener
                .GetNotificationsAsync(NotificationKinds::Toast)
                .and_then(|op| op.join())
            {
                for i in 0..existing.Size().unwrap_or(0) {
                    if let Ok(n) = existing.GetAt(i)
                        && let Ok(id) = n.Id()
                    {
                        seen_ids.insert(id);
                    }
                }
            }

            let (sig_tx, sig_rx) = mpsc::channel::<()>();
            let handler = TypedEventHandler::<
                UserNotificationListener,
                UserNotificationChangedEventArgs,
            >::new(move |_, _| {
                let _ = sig_tx.send(());
                Ok(())
            });

            let _token = match listener.NotificationChanged(&handler) {
                Ok(token) => token,
                Err(e) => {
                    let _ = tx.send(PlatformEvent::Status(ListenerStatus::Error(format!(
                        "NotificationChanged failed: {e}"
                    ))));
                    on_wake();
                    return;
                }
            };

            let _ = tx.send(PlatformEvent::Status(ListenerStatus::Listening));
            on_wake();

            while sig_rx.recv().is_ok() {
                while sig_rx.try_recv().is_ok() {}
                let Ok(list) = listener
                    .GetNotificationsAsync(NotificationKinds::Toast)
                    .and_then(|op| op.join())
                else {
                    continue;
                };

                let mut current_ids = HashSet::new();
                for i in 0..list.Size().unwrap_or(0) {
                    let Ok(n) = list.GetAt(i) else { continue };
                    let Ok(id) = n.Id() else { continue };
                    current_ids.insert(id);
                    if seen_ids.insert(id)
                        && let Some(item) = extract_notification_item(&n)
                    {
                        let _ = tx.send(PlatformEvent::Notification(item));
                    }
                }

                let removed: Vec<u32> = seen_ids
                    .iter()
                    .copied()
                    .filter(|id| !current_ids.contains(id))
                    .collect();
                for id in removed {
                    seen_ids.remove(&id);
                    let _ = tx.send(PlatformEvent::Removed(id.to_string()));
                }
                on_wake();
            }
        })
        .expect("spawn windows notification thread");
}

pub fn spawn_media_listener(
    tx: mpsc::Sender<PlatformEvent>,
    cmd_rx: mpsc::Receiver<MediaCommand>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("win-smtc-media".to_owned())
        .spawn(move || {
            init_com_mta();
            let Ok(manager) =
                GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                    .and_then(|op| op.join())
            else {
                return;
            };

            let mut last_snapshot: Option<(String, String, bool, u64)> = None;
            loop {
                let current = read_smtc_track(&manager);
                let snapshot = current.as_ref().map(|t| {
                    (
                        t.title.clone(),
                        t.artist.clone(),
                        t.is_playing,
                        t.position_ms / 1000,
                    )
                });
                if snapshot != last_snapshot {
                    last_snapshot = snapshot;
                    let _ = tx.send(PlatformEvent::Media(current));
                    on_wake();
                }

                match cmd_rx.recv_timeout(Duration::from_millis(600)) {
                    Ok(cmd) => {
                        if let Ok(session) = manager.GetCurrentSession() {
                            match cmd {
                                MediaCommand::TogglePlayPause => {
                                    let _ = session.TryTogglePlayPauseAsync();
                                }
                                MediaCommand::Next => {
                                    let _ = session.TrySkipNextAsync();
                                }
                                MediaCommand::Previous => {
                                    let _ = session.TrySkipPreviousAsync();
                                }
                            }
                        }
                        last_snapshot = None;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("spawn windows media thread");
}

fn read_smtc_track(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
) -> Option<MediaTrack> {
    let session = manager.GetCurrentSession().ok()?;
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = props.Title().ok()?.to_string().trim().to_owned();
    if title.is_empty() {
        return None;
    }
    let artist = props
        .Artist()
        .map(|s| s.to_string().trim().to_owned())
        .unwrap_or_default();

    let is_playing = session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .map(|s| s == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
        .unwrap_or(false);

    let (position_ms, duration_ms) = session
        .GetTimelineProperties()
        .map(|tl| {
            let pos = tl
                .Position()
                .map(|t| (t.Duration / 10_000).max(0) as u64)
                .unwrap_or(0);
            let dur = tl
                .EndTime()
                .map(|t| (t.Duration / 10_000).max(0) as u64)
                .unwrap_or(0);
            (pos, dur)
        })
        .unwrap_or((0, 0));

    Some(MediaTrack {
        title,
        artist,
        is_playing,
        position_ms,
        duration_ms,
        updated_at: Instant::now(),
    })
}

pub fn primary_monitor_logical_width() -> f32 {
    let px = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    if px > 0 { px as f32 } else { 1920.0 }
}

pub fn apply_no_activate_hook(window_title: &str) {
    let title = HSTRING::from(window_title);
    let Ok(hwnd) = (unsafe { FindWindowW(None, &title) }) else {
        return;
    };
    if hwnd.is_invalid() {
        return;
    }
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let desired = style | (WS_EX_NOACTIVATE.0 as isize) | (WS_EX_TOOLWINDOW.0 as isize);
    if style != desired {
        unsafe {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
        }
    }
}

pub fn is_cursor_over_window(window_title: &str, padding_px: i32) -> Option<bool> {
    let title = HSTRING::from(window_title);
    let hwnd = unsafe { FindWindowW(None, &title) }.ok()?;
    if hwnd.is_invalid() {
        return None;
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    let mut pt = POINT::default();
    unsafe { GetCursorPos(&mut pt) }.ok()?;

    Some(
        pt.x >= rect.left - padding_px
            && pt.x <= rect.right + padding_px
            && pt.y >= rect.top - padding_px
            && pt.y <= rect.bottom + padding_px,
    )
}
