---
type: backlog
status: active
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - tasks
---

# 02 - Daftar Tugas & Backlog

Referensi cakupan fitur: [[PRD-LiebertDynamicIsland-MVP]].

## P0 — Must Have (MVP)
### Fase 1: Setup Proyek & Technical Spike
- [x] Inisialisasi `Cargo.toml` dan skeleton modul (`src/main.rs`, `src/config.rs`, `src/state.rs`, `src/tray.rs`, `src/ui/`, `src/platform/`)
- [x] Spike Windows: uji `UserNotificationListener::Current()` pada `.exe` unpackaged vs Sparse Package/MSIX manifest *(lulus di `.exe` unpackaged: `Access: Allowed`, `21` active toasts terbaca)*
- [ ] Spike Linux: uji D-Bus `org.freedesktop.Notifications` (`zbus`) & overlay `wlr-layer-shell` (Wayland) / dock (X11) di environment Linux
- [x] Finalisasi pilihan renderer Rust (`eframe 0.37` + `Renderer::Glow`) di [[03 - Catatan Rapat & Keputusan]]

### Fase 2: Core Features
- [x] Definisikan `NotificationItem`, state machine (`Idle` ↔ `Compact` ↔ `Expanded`), dan queue sederhana di `src/state.rs`
- [ ] Implementasikan bridge Windows WinRT & Linux D-Bus beserta error handling izin/daemon *(Windows terimplementasi & lolos spike; Linux `src/platform/linux.rs` belum dikompilasi/diuji di host Linux)*
- [x] Implementasikan System Tray (`Pause/Resume`, `Auto-hide when idle`, `Exit`) dan persistensi `config.toml` (`%APPDATA%` / `~/.config`)

### Fase 3: UI & Animation
- [x] Bangun window overlay borderless transparan top-center yang non-focus-stealing (`src/ui/mod.rs` + Win32 `WS_EX_NOACTIVATE`)
- [x] Implementasikan layout 3 state (`Idle`, `Compact`, `Expanded`) beserta animasi transisi ukuran (`eframe` `Glow`)
- [x] Implementasikan interaksi: auto-collapse 5 detik, hover expand & pause timer, serta tombol `Dismiss`

### Fase 4: Testing & Release
- [x] Tambahkan unit test state machine, config, clock, lyrics, todo, & calendar (`cargo test` — 19 tests passed)
- [ ] Uji manual edge cases: multi-notifikasi berurutan, teks panjang/Unicode, multi-DPI scaling, hover expand live, top-edge hover reveal saat auto-hide, Spotify lyrics, & Google Calendar `.ics`
- [ ] Verifikasi target performa `--release`: RAM idle `< 25 MB`, CPU idle `< 0.5%`, latency `< 150 ms`
- [ ] Perbarui `README.md` (cara build, izin Windows, catatan daemon Linux) & lakukan smoke test binary Windows + Linux

## Fitur Tambahan & Ekspansi (Di Luar Scope Awal P0)
- [x] **Music & Synced Lyrics:** Pembaca media SMTC (Windows) & MPRIS2 (Linux) + LRC lyrics via LRCLIB (`src/lyrics.rs`)
- [x] **Menu Settings:** Secondary viewport untuk ukuran, warna, tema preset, corner radius, dan toggle modul (`src/ui/settings.rs`)
- [x] **Todo List:** Checklist lokal dengan persistensi `todos.toml` (`src/todo.rs`)
- [x] **Google Calendar:** Sync feed Secret iCal `.ics` + upcoming events & alert 5 menit (`src/calendar.rs`)
- [x] **Hover Fix:** Perbaikan hover-expand saat `Idle` & Win32 cursor hit-test untuk window `WS_EX_NOACTIVATE`
- [x] **Jam & Zona Waktu + Idle Display Mode:** Jam IANA (`chrono-tz`), format 12h/24h, detik, dan pilihan tampilan idle (`Auto`, `Clock`, `Lyrics`, `NextEvent`, `Minimal`) di `src/clock.rs` & `src/ui/`
- [x] **Delayed Auto-Hide + Top-Edge Hover Reveal:** Auto-hide setelah *N* detik idle (`auto_hide_delay_secs`) dan muncul kembali saat kursor mengarah ke tepi atas tengah layar (Windows)
- [x] **Fullscreen / Game Auto-Hide:** Sembunyikan pill otomatis saat aplikasi/game/video fullscreen aktif di monitor utama (Windows; toggle di Settings)
- [ ] **Quick Timer / Pomodoro:** Countdown fokus langsung di pill idle yang terhubung dengan Todo List
- [ ] **Launch at Startup & Multi-Monitor Picker:** Toggle autostart OS dan pilihan monitor target di Settings
- [ ] **System HUD (Volume & Brightness):** Bar indikator custom saat volume atau brightness berubah
- [ ] **Click-to-Focus App:** Klik notifikasi untuk memfokuskan aplikasi pengirimnya
- [ ] **Quick Drop / Clipboard Shelf:** Area drag-and-drop sementara untuk file atau teks antar window
- [ ] **Battery / System Mini Status:** Indikator baterai/charging di pill saat idle

## P2 — Could Have (Future)
- [ ] Timer / Pomodoro cepat di dalam pill
- [ ] Quick Drop / File Shelf sementara
- [ ] Multi-monitor picker & custom theme editor
