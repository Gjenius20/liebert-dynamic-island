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
- **UI / Renderer:** `eframe 0.37` (`egui`, `Renderer::Glow`, fitur `x11` & `wayland`, multi-viewport `show_viewport_immediate` untuk Settings) pada Windows/X11, serta native Wayland **`wlr-layer-shell`** (`smithay-client-toolkit 0.19` + `glutin 0.32` EGL + `egui_glow 0.37` + `calloop 0.13`) pada Linux Wayland — lihat [[03 - Catatan Rapat & Keputusan]]
- **Windows Bridge:** `windows 0.62` — Win32 (`WS_EX_NOACTIVATE`, `WS_EX_TOOLWINDOW`, `GetCursorPos`/`GetWindowRect`, `GetSystemMetrics`, `Win32_Graphics_Gdi` `GetForegroundWindow` + `GetMonitorInfoW`), WinRT `UserNotificationListener`, `Media`/`Media_Control` (SMTC + Shuffle/Repeat/Seek), & `Storage_Streams` (Album Cover)
- **Linux Bridge:** `zbus 6.5` (D-Bus `org.freedesktop.Notifications` — builder `session()/name()/serve_at()` mengembalikan `Self`, error ditunda ke `build().await`, hints memakai `OwnedVariant` — & MPRIS2 `org.mpris.MediaPlayer2.*` + Shuffle/Loop/Seek/`mpris:artUrl`), native Wayland `zwlr_layer_surface_v1` (`Layer::Overlay`, `Anchor::TOP`, `exclusive_zone=-1`) + Hyprland UNIX IPC socket (`.socket.sock` dengan awalan `/` untuk `/eval`, `/dispatch`, `/keyword`, `hl.layer_rule`/`order=-1`), + fallback `eframe`/`winit` (X11)
- **System Tray, Config & Integrations:** `tray-icon 0.27` (fitur `ksni` murni D-Bus StatusNotifierItem tanpa dependensi GTK di Linux), `serde` + `serde_json` + `toml 1.2`, `directories 6.0`, `ureq 3.5` (LRCLIB, Google Translate `dt=t&dt=rm`, & Google Calendar `.ics`), `image 0.25` (decode cover album & ekstraksi warna dominan), `chrono 0.4` + `chrono-tz 0.10`

## Struktur Folder Kode
```text
liebert-dynamic-island/
├── Cargo.toml
├── AGENTS.md                    # Panduan kolaborasi & aturan repo
├── docs/                        # Dokumentasi Obsidian & PRD
├── scripts/
│   └── build-appimage.sh        # Skrip build & packaging AppImage Linux (x86_64)
└── src/
    ├── main.rs                  # Entry point, CLI flags, & spawn background workers
    ├── config.rs                # Load/save config.toml (size, colors, presets, clock, idle mode, .ics URL)
    ├── state.rs                 # Island state machine (Idle, Compact, Expanded), queue, & auto-hide delay
    ├── clock.rs                 # Jam digital & formatter zona waktu IANA (chrono-tz)
    ├── lyrics.rs                # LRC / Enhanced LRC parser, karaoke progress, romaji & translate worker
    ├── todo.rs                  # TodoStore & persistensi todos.toml
    ├── calendar.rs              # RFC 5545 .ics parser & Google Calendar sync worker
    ├── tray.rs                  # System tray menu & lifecycle control
    ├── ui/
    │   ├── mod.rs               # Overlay pill (Idle/Compact/Expanded tabs), unified AA notch mesh, & UI loop
    │   ├── icons.rs             # Crisp Painter-drawn vector icons & responsive icon buttons
    │   └── settings.rs          # Secondary viewport Settings (Appearance, Todo, Calendar)
    └── platform/
        ├── mod.rs               # Facade lintas OS (overlay, topmost, hover, font, MediaTrack, PlatformEvent)
        ├── windows.rs           # Win32 hover/no-activate/DWM, WinRT Notifications & SMTC
        ├── linux.rs             # D-Bus Notifications, MPRIS2, Hyprland IPC (/eval/layer_rule), & fontconfig lookup
        └── wayland_layer.rs     # Native Wayland wlr-layer-shell EGL/egui_glow overlay & Settings IPC runner
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
- **`Idle`:** Pill compact di top-center atau menyatu seamless dengan tepi atas monitor (`y = 0`) lewat transisi `notch_morph` (`Top Notch` ↔ `Floating Island`), menampilkan mini cover + lirik karaoke (`Original` atau `Romaji` sesuai `karaoke_lyric_mode`, disinkronkan via `duration` LRCLIB, `end_ms` jeda instrumental, & `lyric_offset_ms`) + 3-bar equalizer saat musik diputar (dengan opsi gradasi `#000` ke warna dominan album di paling bawah via `compact_album_gradient`), jam zona waktu, atau jadwal terdekat; otomatis tersembunyi saat `auto_hide` atau `hide_in_fullscreen` aktif.
- **`Compact`:** Pill melebar menampilkan avatar berwarna brand aplikasi (`WhatsApp`, `Discord`, `Gmail`, browser, dll.), badge nama aplikasi terpisah tanpa overlap, judul, preview singkat, serta indikator jumlah antrean (turut mendukung gradasi `#000` → warna album bila musik aktif & `compact_album_gradient` menyala); timer auto-collapse default 5 detik.
- **`Expanded`:** Aktif saat hover pada `Compact` (menjeda timer), menampilkan slot kiri/kanan terklip bersih (tab `Home`, `♪ Music`, `☑ N`, `Cal` + tombol cepat ganti mode `Island`/`Notch` + tombol `⚙` yang membawa jendela Settings ke depan layar via `bring_window_to_front`), dan `ScrollArea` pada kolom lirik kanan. Juga menghormati opsi `show_notifications`.

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
