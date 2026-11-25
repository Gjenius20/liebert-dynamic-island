---
type: dashboard
status: in-progress
deadline: 2026-12-09
tags:
  - project
  - liebert-dynamic-island
  - mvp
---

# 00 - Dashboard Proyek: Liebert Dynamic Island

## Ringkasan Tujuan Proyek
Membangun overlay **Dynamic Island** yang ringan (< 25 MB RAM idle), instan (< 500 ms startup), dan non-intrusif di bagian top-center layar untuk **Windows 10/11** dan **Linux (X11/Wayland)** menggunakan **Rust native**. Fokus MVP (1–2 minggu) adalah menangkap dan menampilkan notifikasi sistem secara real-time.

## Navigasi Dokumentasi Utama
- [[PRD-LiebertDynamicIsland-MVP]] — Product Requirements Document (MVP v1.0)
- [[01 - Arsitektur & Spesifikasi]] — Tech stack, struktur kode, dan alur sistem
- [[02 - Daftar Tugas & Backlog]] — Daftar tugas P0, P1, P2
- [[03 - Catatan Rapat & Keputusan]] — Architecture Decision Records (ADR)
- [[04 - Changelog & Dev Log]] — Riwayat perubahan kode kronologis

## Milestones Utama (MVP)
- [x] Inisialisasi PRD MVP ([[PRD-LiebertDynamicIsland-MVP]]) & struktur dokumentasi Obsidian
- [ ] **M1 — Technical Spike:** Windows WinRT, renderer `eframe`, koneksi D-Bus Linux, & overlay Hyprland (Wayland) terverifikasi; menunggu uji compositor non-Hyprland/X11
- [ ] **M2 — Core Overlay:** Implementasi `src/ui/mod.rs` & positioning Hyprland terverifikasi live (`140×30` → `340×58` top-center); menunggu uji visual penuh & DPI scaling Linux
- [ ] **M3 — Notification Bridge:** Listener Windows & state/queue selesai (7 unit tests pass); build release Linux lolos & spike D-Bus OK, menunggu verifikasi runtime listener Linux (daemon notifikasi lain masih aktif)
- [ ] **M4 — Tray, Config & Release:** System tray & `config.toml` selesai; menunggu `README.md`, uji manual, & benchmark RAM `--release`

## Progres Tambahan (Terverifikasi)
- [x] **Text Alignment di Settings:** Tombol `Left` / `Center` / `Right` (`src/config.rs`, `src/ui/settings.rs`, `src/ui/mod.rs`).
- [x] **Daur Hidup & RAM Linux:** Close request WM dibatalkan, auto-hide memakai viewport `1×1`, layer-shell `closed` → fallback eframe, dan cache/antrean dibatasi agar RAM tidak terus naik. Lihat [[04 - Changelog & Dev Log]].
- [x] **Distribusi v0.1.0:** AppImage + `.tar.gz` Linux, paket AUR (`PKGBUILD` + `.SRCINFO`), dan installer NSIS Windows siap; aset dipublikasikan lewat workflow release pada tag `v0.1.0`.
