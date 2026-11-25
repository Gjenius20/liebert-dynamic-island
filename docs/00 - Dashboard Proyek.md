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
- [ ] **M1 — Technical Spike:** Windows WinRT & renderer `eframe` terverifikasi; menunggu uji Linux D-Bus & Wayland di host Linux
- [ ] **M2 — Core Overlay:** Implementasi awal `src/ui/mod.rs` selesai; menunggu verifikasi visual, DPI, & Linux overlay
- [ ] **M3 — Notification Bridge:** Listener Windows & state/queue selesai (7 unit tests pass); menunggu verifikasi runtime Linux
- [ ] **M4 — Tray, Config & Release:** System tray & `config.toml` selesai; menunggu `README.md`, uji manual, & benchmark RAM `--release`
