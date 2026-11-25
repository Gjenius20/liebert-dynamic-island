use std::collections::HashMap;
use std::future::pending;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use zbus::{Connection, connection, interface};

use crate::platform::PlatformEvent;
use crate::state::{ListenerStatus, NotificationItem};

// shortcut: Wayland wlr-layer-shell positioning unsupported in winit/eframe, upgrade to smithay-client-toolkit or iced_layershell when targeting native Wayland positioning.

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
        _hints: HashMap<&str, zbus::zvariant::OwnedValue>,
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

            let res = zbus::block_on(async move {
                let _conn = connection::Builder::session()?
                    .name("org.freedesktop.Notifications")?
                    .serve_at("/org/freedesktop/Notifications", server)?
                    .build()
                    .await?;

                let _ = tx.send(PlatformEvent::Status(ListenerStatus::Listening));
                on_wake();
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
