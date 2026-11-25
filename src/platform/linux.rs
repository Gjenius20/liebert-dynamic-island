use std::collections::HashMap;
use std::future::pending;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use zbus::{Connection, Dict, OwnedVariant, Proxy, Variant, connection, interface};

use crate::platform::{MediaCommand, MediaTrack, PlatformEvent};
use crate::state::{ListenerStatus, NotificationItem};

// shortcut: Wayland wlr-layer-shell positioning unsupported in winit/eframe, upgrade to smithay-client-toolkit or iced_layershell when targeting native Wayland positioning.

pub fn is_foreground_fullscreen() -> bool {
    // shortcut: Linux fullscreen detection unimplemented (needs X11 _NET_WM_STATE_FULLSCREEN or compositor IPC), upgrade when targeting Linux runtime.
    false
}

struct NotificationServer {
    next_id: AtomicU32,
    tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<&str>,
        _hints: HashMap<&str, zbus::OwnedVariant>,
        _expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id.fetch_add(1, Ordering::Relaxed)
        };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let item = NotificationItem {
            id: id.to_string(),
            app_name: if app_name.is_empty() {
                "Linux".to_owned()
            } else {
                app_name.to_owned()
            },
            title: summary.trim().to_owned(),
            body: body.trim().to_owned(),
            icon_rgba: None,
            timestamp,
        };
        let _ = self.tx.send(PlatformEvent::Notification(item));
        (self.on_wake)();
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.tx.send(PlatformEvent::Removed(id.to_string()));
        (self.on_wake)();
    }

    fn get_capabilities(&self) -> Vec<&'static str> {
        vec!["body", "persistence"]
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("liebert-dynamic-island", "Liebert", "0.1.0", "1.2")
    }
}

pub fn run_notification_spike() -> Result<String, String> {
    zbus::block_on(async {
        let conn = Connection::session()
            .await
            .map_err(|e| format!("D-Bus session connection failed: {e}"))?;

        let proxy = zbus::fdo::DBusProxy::new(&conn)
            .await
            .map_err(|e| format!("DBusProxy failed: {e}"))?;
        let name = "org.freedesktop.Notifications"
            .try_into()
            .map_err(|e| format!("Invalid bus name: {e}"))?;
        let has_owner = proxy
            .name_has_owner(name)
            .await
            .map_err(|e| format!("name_has_owner failed: {e}"))?;

        if has_owner {
            Ok("D-Bus session OK; org.freedesktop.Notifications currently owned by another daemon (stop dunst/mako to bind directly)".to_owned())
        } else {
            Ok("D-Bus session OK; org.freedesktop.Notifications is free to claim".to_owned())
        }
    })
}

pub fn spawn_listener(tx: mpsc::Sender<PlatformEvent>, on_wake: Arc<dyn Fn() + Send + Sync>) {
    std::thread::Builder::new()
        .name("linux-dbus-notifications".to_owned())
        .spawn(move || {
            let server = NotificationServer {
                next_id: AtomicU32::new(1),
                tx: tx.clone(),
                on_wake: on_wake.clone(),
            };

            let async_tx = tx.clone();
            let async_on_wake = on_wake.clone();
            let res = zbus::block_on(async move {
                let _conn = connection::Builder::session()
                    .name("org.freedesktop.Notifications")
                    .serve_at("/org/freedesktop/Notifications", server)
                    .build()
                    .await?;

                let _ = async_tx.send(PlatformEvent::Status(ListenerStatus::Listening));
                async_on_wake();
                pending::<()>().await;
                Ok::<(), zbus::Error>(())
            });

            if let Err(err) = res {
                let _ = tx.send(PlatformEvent::Status(ListenerStatus::Error(format!(
                    "D-Bus daemon error (another notification daemon active?): {err}"
                ))));
                on_wake();
            }
        })
        .expect("spawn linux notification thread");
}

pub fn spawn_media_listener(
    tx: mpsc::Sender<PlatformEvent>,
    cmd_rx: mpsc::Receiver<MediaCommand>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("linux-mpris-media".to_owned())
        .spawn(move || {
            let mut last_snapshot: Option<(String, String, bool, u64)> = None;
            let _ = zbus::block_on(async move {
                let conn = Connection::session().await?;
                let dbus = zbus::fdo::DBusProxy::new(&conn).await?;

                loop {
                    let player_name = dbus
                        .list_names()
                        .await
                        .ok()
                        .and_then(|names| {
                            names
                                .into_iter()
                                .map(|n| n.to_string())
                                .find(|n| n.starts_with("org.mpris.MediaPlayer2."))
                        });

                    let current = if let Some(ref bus) = player_name {
                        read_mpris_track(&conn, bus).await
                    } else {
                        None
                    };

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
                            if let Some(ref bus) = player_name
                                && let Ok(player) = Proxy::new(
                                    &conn,
                                    bus.as_str(),
                                    "/org/mpris/MediaPlayer2",
                                    "org.mpris.MediaPlayer2.Player",
                                )
                                .await
                            {
                                let method = match cmd {
                                    MediaCommand::TogglePlayPause => "PlayPause",
                                    MediaCommand::Next => "Next",
                                    MediaCommand::Previous => "Previous",
                                };
                                let _: Result<(), _> = player.call(method, &()).await;
                            }
                            last_snapshot = None;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                Ok::<(), zbus::Error>(())
            });
        })
        .expect("spawn linux media thread");
}

async fn read_mpris_track(conn: &Connection, bus: &str) -> Option<MediaTrack> {
    let props = Proxy::new(
        conn,
        bus,
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
    )
    .await
    .ok()?;
    let all: HashMap<String, OwnedVariant> = props
        .call("GetAll", &("org.mpris.MediaPlayer2.Player",))
        .await
        .ok()?;

    let status = all
        .get("PlaybackStatus")
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or("Stopped");
    let is_playing = status == "Playing";

    let position_ms = all
        .get("Position")
        .and_then(|v| i64::try_from(v).ok())
        .map(|us| (us / 1000).max(0) as u64)
        .unwrap_or(0);

    let meta_val = all.get("Metadata")?;
    let dict = Dict::try_from(meta_val.downcast_ref::<Variant>().ok()?).ok()?;

    let title = dict
        .get::<&str, &str>(&"xesam:title")
        .ok()
        .flatten()
        .map(|s| s.trim().to_owned())?;
    if title.is_empty() {
        return None;
    }

    let artist = dict
        .get::<&str, Variant>(&"xesam:artist")
        .ok()
        .flatten()
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .map(|list| list.join(", "))
        .unwrap_or_default();

    let duration_ms = dict
        .get::<&str, i64>(&"mpris:length")
        .ok()
        .flatten()
        .map(|us| (us / 1000).max(0) as u64)
        .unwrap_or(0);

    Some(MediaTrack {
        title,
        artist,
        is_playing,
        position_ms,
        duration_ms,
        updated_at: Instant::now(),
    })
}
