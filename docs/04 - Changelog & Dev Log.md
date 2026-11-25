---
type: changelog
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - dev-log
---

# 04 - Changelog & Dev Log

## 2026-11-25
### Added
- `AGENTS.md`: Menambahkan panduan kolaborasi agent/developer (perintah build/test, batas modul, standar kualitas, API gotchas, dan aturan wajib sinkronisasi Obsidian `/docs`). Lihat [[01 - Arsitektur & Spesifikasi]] dan [[03 - Catatan Rapat & Keputusan]].
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
