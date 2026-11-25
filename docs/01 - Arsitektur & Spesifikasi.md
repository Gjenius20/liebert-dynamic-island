---
type: spec
status: active
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - architecture
---

# 01 - Arsitektur & Spesifikasi

Referensi spesifikasi produk: [[PRD-LiebertDynamicIsland-MVP]].

## Tech Stack (Aktual)
- **Bahasa:** Rust (`edition = "2024"`, `rustc 1.98.1` Linux / `rustc 1.99.1` Windows)
- **UI / Renderer:** `eframe 0.37` (`egui`, `Renderer::Glow`, fitur `x11` & `wayland`, multi-viewport `show_viewport_immediate` untuk Settings) — lihat [[03 - Catatan Rapat & Keputusan]]
- **Windows Bridge:** `windows 0.62` — Win32 (`WS_EX_NOACTIVATE`, `WS_EX_TOOLWINDOW`, `GetCursorPos`/`GetWindowRect`, `GetSystemMetrics`, `Win32_Graphics_Gdi` `GetForegroundWindow` + `GetMonitorInfoW`), WinRT `UserNotificationListener`, & WinRT `Media_Control` (SMTC)
- **Linux Bridge:** `zbus 6.5` (D-Bus `org.freedesktop.Notifications` — builder `session()/name()/serve_at()` mengembalikan `Self`, error ditunda ke `build().await`, hints memakai `OwnedVariant` — & MPRIS2 `org.mpris.MediaPlayer2.*`) + windowing `eframe`/`winit` (X11 / Wayland)
- **System Tray, Config & Integrations:** `tray-icon 0.27`, `serde` + `serde_json` + `toml 1.2`, `directories 6.0`, `ureq 3.5` (LRCLIB & Google Calendar `.ics`), `chrono 0.4` + `chrono-tz 0.10`

## Struktur Folder Kode
```text
liebert-dynamic-island/
├── Cargo.toml
├── AGENTS.md                    # Panduan kolaborasi & aturan repo
├── docs/                        # Dokumentasi Obsidian & PRD
└── src/
    ├── main.rs                  # Entry point, CLI flags, & spawn background workers
    ├── config.rs                # Load/save config.toml (size, colors, presets, clock, idle mode, .ics URL)
    ├── state.rs                 # Island state machine (Idle, Compact, Expanded), queue, & auto-hide delay
    ├── clock.rs                 # Jam digital & formatter zona waktu IANA (chrono-tz)
    ├── lyrics.rs                # LRC parser & LRCLIB synced lyrics fetcher
    ├── todo.rs                  # TodoStore & persistensi todos.toml
    ├── calendar.rs              # RFC 5545 .ics parser & Google Calendar sync worker
    ├── tray.rs                  # System tray menu & lifecycle control
    ├── ui/
    │   ├── mod.rs               # Overlay pill (Idle/Compact/Expanded tabs) & Win32 hover fix
    │   └── settings.rs          # Secondary viewport Settings (Appearance, Todo, Calendar)
    └── platform/
        ├── mod.rs               # Facade lintas OS, MediaTrack, & PlatformEvent
        ├── windows.rs           # Win32 hover/no-activate, WinRT Notifications & SMTC
        └── linux.rs             # D-Bus Notifications & MPRIS2 media reader
```

## Skema Data & State Machine
### Notification Payload
```rust
pub struct NotificationItem {
    pub id: String,
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub icon_rgba: Option<Vec<u8>>,
    pub timestamp: u64,
}
```

### State Machine Overlay
- **`Idle`:** Pill compact di top-center (atau tersembunyi jika `auto_hide = true`).
- **`Compact`:** Pill melebar menampilkan ikon app, judul, dan preview singkat; timer auto-collapse default 5 detik.
- **`Expanded`:** Aktif saat hover pada `Compact` (menjeda timer), menampilkan body lengkap & tombol `Dismiss`.

## Alur Komponen Utama
1. **Startup:** `config.rs` membaca `config.toml` → `tray.rs` membuat ikon tray → `platform` memunculkan listener notifikasi di background thread/async task → `ui` membuka overlay window dalam state `Idle`.
2. **Notifikasi Masuk:** OS memicu event (`UserNotificationListener` di Windows / D-Bus di Linux) → `platform` memetakan payload ke `NotificationItem` → kirim via channel ke `state.rs` (antrean sederhana) → `ui` beranimasi ke `Compact`.
3. **Interaksi & Dismiss:** Hover menjeda timer dan masuk ke `Expanded`; klik `Dismiss` (atau timer 5 detik habis) menutup item aktif dan menampilkan item berikutnya di queue atau kembali ke `Idle`.

## Target Non-Fungsional
- **Startup Time:** `< 500 ms` hingga pill tampil.
- **Notification Latency:** `< 150 ms` dari event OS hingga frame animasi masuk.
- **Resource Footprint:** `< 25 MB` RAM idle (`< 50 MB` saat expanded), `< 0.5%` CPU idle (event-driven, tanpa busy-polling).
- **Rendering & DPI:** `>= 60 FPS` saat animasi transisi; Per-Monitor DPI Aware v2 (Windows) & fractional scaling (Linux).
- **Privasi & Jaringan:** Data notifikasi OS diproses 100% lokal tanpa telemetri maupun log persisten. Permintaan jaringan keluar bersifat opsional dan terbatas pada metadata lagu (`artist`/`title`) ke `lrclib.net` (saat `show_lyrics` aktif) serta fetch URL `.ics` Google Calendar yang dikonfigurasi pengguna.
