---
type: changelog
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - dev-log
---

# 04 - Changelog & Dev Log

## 2026-11-25
### Fixed
- `src/platform/linux.rs`: Menyesuaikan listener notifikasi, kontrol MPRIS2 (`Shuffle`/`LoopStatus` memakai `zbus::Variant`), pembacaan `mpris:artUrl` (`dict.get::<&str, &str>`), dan pembaca MPRIS2 (`read_mpris_track`) dengan API `zbus 6.5` (`Builder::session()`, `name()`, dan `serve_at()` mengembalikan `Self`, bukan `Result`; mengganti `OwnedValue`, `Value`, dan `zvariant::Dict` dengan `OwnedVariant`, `Variant`, dan `Dict`). `cargo test` (23 tests), `cargo clippy --all-targets -- -D warnings`, dan `cargo build --release` lolos di Linux. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].
- `src/state.rs`: `ListenerStatus::AccessDenied` ditandai `allow(dead_code)` di luar Windows agar `cargo clippy --all-targets -- -D warnings` bersih (varian hanya diproduksi backend WinRT; kegagalan D-Bus Linux dipetakan ke `Error`). Lihat [[01 - Arsitektur & Spesifikasi]].

### Changed
- `.gitignore`: Menambahkan `dist/` dan `*.AppImage` untuk output kemasan AppImage Linux.
- `docs/00 - Dashboard Proyek.md`, `docs/01 - Arsitektur & Spesifikasi.md`, `docs/02 - Daftar Tugas & Backlog.md`, `docs/03 - Catatan Rapat & Keputusan.md`: Menyelesaikan konflik merge dengan `origin/main` (`ad1cd2d`), mencatat integrasi `image 0.25` + MPRIS2 Shuffle/Loop/Seek/`mpris:artUrl` dan verifikasi build Linux (`zbus 6.5` + `rustc 1.98.1`), serta mempertahankan `ADR-004`–`ADR-007`. Lihat [[04 - Changelog & Dev Log]].

### Added
- `dist/Liebert_Dynamic_Island-0.1.0-x86_64.AppImage` (gitignored): Memaketkan binary release Linux (`x86_64`) menjadi AppImage menggunakan `appimagetool` + `type2-runtime` beserta `.desktop` entry tervalidasi dan ikon `.DirIcon`.

### Added
- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`: Menambahkan ekstraksi cover album (`Storage_Streams::DataReader` pada WinRT SMTC & `mpris:artUrl` pada MPRIS2 Linux, cache per lagu, decode `image 0.25`) serta `extract_dominant_accent()` untuk mengekstrak warna aksen dominan album.
- `src/ui/mod.rs`, `src/ui/settings.rs`, `src/config.rs`: Mendesain ulang Music & Lyrics card mengikuti referensi visual 2-kolom (cover album membulat + title/artist + playbar + transport di kiri, multi-line wrapped `LayoutJob` karaoke original & romaji + terjemahan + upcoming preview redup di kanan, gradient wash atmosferik di bagian bawah, tombol pill `Translate` & `Romaji` di bawah, opsi `dynamic_album_color`, serta preset `"Crimson"`).
- `src/lyrics.rs`, `src/ui/mod.rs`, `src/ui/settings.rs`: Menambahkan **Karaoke progressive highlight** (dukungan LRC baris & word-level `<mm:ss.xx>` via painter clip-rect), **Romaji / romanisasi** untuk lirik non-abjad (Jepang, Cina, Korea, Arab, dll. dengan fallback Kana offline), **Terjemahan lirik** ke bahasa pilihan (`id`, `en`, `ja`, `ko`, `zh-CN`, `ar`, `es`), serta lazy fallback font sistem (CJK & simbol media).
- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`, `src/ui/mod.rs`: Menambahkan **Playbar interaktif** (seek posisi playback) beserta tombol transport lengkap (**Shuffle**, **Previous**, **Play/Pause**, **Next**, dan **Repeat** `Off`/`All`/`One`) melalui WinRT SMTC & D-Bus MPRIS2.
- `src/platform/windows.rs`, `src/platform/mod.rs`, `src/platform/linux.rs`, `src/state.rs`: Menambahkan deteksi aplikasi/game fullscreen di monitor utama (`GetForegroundWindow` + `GetMonitorInfoW`) dan otomatis menyembunyikan Dynamic Island (serta menonaktifkan top-edge hover reveal) selama fullscreen aktif; Linux saat ini menggunakan shortcut `false`.
- `src/clock.rs`: Menambahkan modul jam digital dengan dukungan zona waktu IANA (`chrono-tz 0.10`), daftar zona waktu umum (`Local`, `UTC`, `Asia/Jakarta`, `Asia/Tokyo`, dll.), format 12h/24h, opsi detik, serta 2 unit test.
- `src/state.rs`, `src/platform/mod.rs`, `src/platform/windows.rs`: Menambahkan delayed auto-hide (`auto_hide_delay_secs`, `idle_since`) dan `is_cursor_in_top_trigger` untuk memunculkan kembali notch saat kursor diarahkan ke tepi atas tengah layar.
- `src/lyrics.rs`: Menambahkan parser LRC (`[mm:ss.xx]`), sinkronisasi baris lirik aktif/berikutnya terhadap posisi playback, dan worker background (`ureq 3.5` + `serde_json`) ke `lrclib.net` dengan cache per lagu.
- `src/todo.rs`: Menambahkan `TodoStore` dan persistensi lokal `todos.toml` (add, toggle, remove, clear completed) beserta unit test.
- `src/calendar.rs`: Menambahkan parser `.ics` RFC 5545 (`parse_ics`), filter jadwal terdekat (`upcoming_events`), countdown, worker sinkronisasi berkala 5 menit untuk Google Calendar Secret iCal URL, dan alert 5 menit sebelum acara.
- `src/ui/settings.rs`: Menambahkan window sekunder yang bisa menerima focus keyboard (`show_viewport_immediate`) dengan tab **Appearance** (preset warna, scale, lebar idle/expanded, corner radius, offset, toggle modul), **Todo List**, dan **Google Calendar**.
- `src/platform/windows.rs`, `src/platform/linux.rs`: Menambahkan pembacaan media & kontrol playback (`Play/Pause`, `Next`, `Previous`) via WinRT SMTC (`Media_Control`) di Windows dan D-Bus MPRIS2 di Linux.
- `AGENTS.md`: Menambahkan panduan kolaborasi agent/developer (perintah build/test, batas modul, standar kualitas, API gotchas, dan aturan wajib sinkronisasi Obsidian `/docs`). Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].

### Changed
- `Cargo.toml`, `src/lyrics.rs`: Menambahkan `image 0.25` (`png`, `jpeg`, `webp`) & fitur WinRT `Storage_Streams` pada `windows 0.62`, serta memperluas `ActiveKaraoke` dengan `next_romaji` dan `next_translation`.
- `Cargo.toml`, `src/config.rs`, `src/main.rs`: Menambahkan fitur WinRT `Media` pada `windows 0.62`, opsi `karaoke_lyrics`, `romaji_lyrics`, `translate_lyrics`, dan `lyric_translate_lang` di `Config` & Settings, serta inisialisasi fallback font dasar di `main.rs`.
- `Cargo.toml`, `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`: Menambahkan fitur `Win32_Graphics_Gdi` pada `windows 0.62`, opsi `hide_in_fullscreen` (default `true`) di `Config` & Settings, serta polling fullscreen tiap `250 ms`.
- `Cargo.toml`: Menambahkan `chrono-tz 0.10` untuk resolusi zona waktu IANA.
- `src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`, `src/tray.rs`, `src/main.rs`: Menambahkan `IdleDisplayMode` (`Auto`, `Clock`, `Lyrics`, `NextEvent`, `Minimal`), pengaturan zona waktu/format jam, slider `auto_hide_delay_secs`, sinkronisasi checkmark tray, koreksi lebar monitor berbasis `pixels_per_point()`, serta logika tampil-kembali saat hover di tepi atas layar.
- `Cargo.toml`: Menambahkan `ureq 3.5`, `serde_json 1.0`, `chrono 0.4`, dan fitur WinRT `Media_Control` pada `windows 0.62`.
- `src/config.rs`: Memperluas `Config` dengan pengaturan ukuran, warna, corner radius, toggle fitur, URL `.ics`, serta 4 tema preset (`obsidian`, `midnight`, `amoled`, `slate`) dengan backward-compatible `#[serde(default)]`.
- `src/tray.rs`, `src/ui/mod.rs`, `src/main.rs`: Menambahkan menu tray `Settings...`, hub tab expanded (`Overview`, `Lyrics`, `Todo`, `Cal`), tampilan lirik ringkas di state idle saat musik diputar, serta wiring seluruh background worker.

### Fixed
- `src/state.rs`, `src/platform/windows.rs`, `src/ui/mod.rs`: Memperbaiki bug di mana Dynamic Island tidak membesar saat di-hover — `set_hovered(true)` kini memicu `IslandVisualState::Expanded` meski sedang `Idle`, `dismiss_active` menjaga state `Expanded` selama kursor masih hover, dan deteksi hover di Windows memakai Win32 `GetCursorPos` + `GetWindowRect` agar tidak gagal pada window `WS_EX_NOACTIVATE`.

### Added (Initial Bootstrap)
- `Cargo.toml`: Inisialisasi manifest proyek Rust (`edition = "2024"`) dengan dependensi `eframe 0.37` (`glow`), `windows 0.62`, `zbus 6.5` (target Linux), `tray-icon 0.27`, `serde`, `toml`, dan `directories`, serta profil release hemat ukuran (`lto = "thin"`, `strip = true`, `opt-level = "s"`).
- `src/config.rs`: Menambahkan loader/saver konfigurasi lokal `config.toml` (`auto_hide_when_idle`, `top_offset_px`, `collapse_timeout_secs`) beserta 3 unit test fallback & round-trip TOML.
- `src/state.rs`: Menambahkan model `NotificationItem`, state machine `IslandState` (`Idle`, `Compact`, `Expanded`), queue notifikasi dengan deduplikasi ID, timer auto-collapse, pause/auto-hide logic, serta 4 unit test alur state.
- `src/platform/mod.rs`, `src/platform/windows.rs`, `src/platform/linux.rs`: Menambahkan platform bridge untuk Windows WinRT `UserNotificationListener` + Win32 `WS_EX_NOACTIVATE`/`WS_EX_TOOLWINDOW` dan backend D-Bus `org.freedesktop.Notifications` (`zbus`) untuk Linux, beserta mode CLI `--spike-notifications`.
- `src/tray.rs`: Menambahkan `TrayController` (ikon pill RGBA in-memory dan menu `Status`, `Pause/Resume`, `Auto-hide when idle`, `Exit`).
- `src/ui/mod.rs`, `src/main.rs`: Mengimplementasikan overlay window transparan top-center berbasis `eframe 0.37`, animasi lerp ukuran pill (`Idle` ↔ `Compact` ↔ `Expanded`), hover expand/pause timer, tombol `Dismiss`, serta flag `--demo` dan `--spike-notifications`. Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].
- `docs/PRD-LiebertDynamicIsland-MVP.md`: Menyusun PRD MVP v1.0 untuk target Windows 10/11 dan Linux (X11/Wayland) dengan cakupan P0 System Notifications.
- `docs/00 - Dashboard Proyek.md`, `docs/01 - Arsitektur & Spesifikasi.md`, `docs/02 - Daftar Tugas & Backlog.md`, `docs/03 - Catatan Rapat & Keputusan.md`, `docs/04 - Changelog & Dev Log.md`: Inisialisasi struktur dokumentasi standar Obsidian Markdown.
- `.gitignore`: Menambahkan `.obsidian/`, `.obsidian/workspace.json`, dan `target/` agar konfigurasi lokal Obsidian serta artifact build Rust tidak masuk ke repository.

### Changed
- `docs/00 - Dashboard Proyek.md`: Menyelaraskan frontmatter `tags` (`project`, `liebert-dynamic-island`, `mvp`) sesuai standar workspace Obsidian proyek.
- `docs/01 - Arsitektur & Spesifikasi.md`: Menambahkan tautan eksplisit ke [[PRD-LiebertDynamicIsland-MVP]], rincian state machine overlay (`Idle` ↔ `Compact` ↔ `Expanded`), alur komponen, dan target non-fungsional (`< 500 ms` startup, `< 150 ms` latency, `< 25 MB` RAM idle, `>= 60 FPS`).
- `docs/02 - Daftar Tugas & Backlog.md`: Menyusun ulang tugas P0 ke dalam 4 fase pengerjaan (Setup Proyek & Technical Spike, Core Features, UI & Animation, Testing & Release).
- `docs/03 - Catatan Rapat & Keputusan.md`: Menambahkan `ADR-003` terkait 3 technical spike wajib di Fase 1 (WinRT identity, Linux D-Bus mode, dan pemilihan renderer Rust).
