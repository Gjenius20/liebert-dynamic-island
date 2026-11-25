# Product Requirements Document: Liebert Dynamic Island MVP

## Executive Summary

- **Product:** Liebert Dynamic Island
- **Version:** MVP (1.0)
- **Document Status:** Draft
- **Last Updated:** 2026-11-25

### Product Vision

Menghadirkan overlay "Dynamic Island" yang ringan, instan, dan non-intrusif di bagian atas layar **Windows (Windows 10/11)** dan **Linux (X11/Wayland)** menggunakan Rust native, dengan fokus MVP pada penangkapan dan tampilan **System Notifications (Windows Toast / Linux Desktop Notifications)** secara elegan tanpa mengganggu fokus kerja pengguna.

### Success Criteria

- Aplikasi berjalan sebagai background utility dengan penggunaan RAM idle < 25 MB dan CPU idle < 0.5%.
- Notifikasi Windows yang masuk muncul di Dynamic Island dalam waktu < 150 ms sejak diterima sistem.
- Animasi transisi pill (idle → compact notification → expanded view → idle) berjalan mulus di 60+ FPS pada layar multi-DPI.
- MVP selesai dan dapat dipakai harian dalam 1–2 minggu.

---

## Problem Statement

### Problem Definition

Notifikasi bawaan desktop (Windows Toast Banner atau daemon notifikasi default di Linux) sering kali:

1. Terlewat karena berada di pojok layar di luar garis pandang utama saat pengguna fokus di tengah/atas layar, atau sebaliknya terasa mengganggu karena menutupi konten/tombol di area kerja.
2. Aplikasi overlay sejenis yang berbasis Electron/WebView memakan RAM ratusan MB dan lambat saat startup untuk sekadar menampilkan overlay kecil.

### Impact Analysis

- **User Impact:** Pengguna mendapatkan visibilitas notifikasi real-time di area top-center layar dengan gangguan visual minimal dan konsumsi resource yang hampir nol di Windows maupun Linux.
- **Market Impact:** Komunitas pengguna Windows & Linux (power users, developer, ricing/minimalist desktop enthusiasts) aktif mencari utilitas desktop ringan berbasis Rust sebagai alternatif aplikasi overlay berat.
- **Business Impact:** Proyek open-source / indie portfolio yang berpotensi membangun basis pengguna awal lintas OS untuk pengembangan modul lanjutan (media control, system HUD) di fase P1.

---

## Target Audience

### Primary Persona: Power User / Developer (Windows & Linux)

- **Demographics:**
  - Usia 18–35 tahun, pengguna harian Windows 10/11 atau Linux desktop (laptop atau multi-monitor desktop).
- **Psychographics:**
  - Peduli pada performa sistem (anti-bloatware), menyukai UI minimalis dan responsif, terbiasa menggunakan keyboard shortcut dan utility tray.
- **Jobs to Be Done:**
  1. **Functional:** Melihat siapa pengirim dan isi ringkas notifikasi masuk tanpa harus mengalihkan pandangan jauh atau membuka notification center.
  2. **Emotional:** Merasa desktop lebih rapi, modern, dan responsif tanpa rasa bersalah karena aplikasi memakan RAM besar.
  3. **Social:** Memiliki tampilan desktop yang estetik dan clean.

### Current Solutions & Pain Points

| Current Solution                            | Pain Points                                                                       | Our Advantage                                                                   |
| :------------------------------------------ | :-------------------------------------------------------------------------------- | :------------------------------------------------------------------------------ |
| Windows Native Toast / Linux `dunst`/`mako` | Muncul di pojok layar, tampilan standar/statis atau butuh konfigurasi teks manual | Posisi top-center yang ergonomis, pill ringkas & ekspansif dengan animasi halus |
| Overlay berbasis Electron/WebView           | RAM tinggi (150–400 MB), startup lambat                                           | Native Rust GUI dengan RAM < 25 MB & startup instan                             |
| Rainmeter / Eww / Waybar widget             | Setup rumit dan terikat satu OS/compositor                                        | Satu codebase Rust yang mendukung Windows & Linux                               |

### Secondary Personas

- **Casual Minimalist User:** Pengguna Windows/Linux non-developer yang menginginkan tampilan notifikasi ala Dynamic Island yang tinggal install/jalankan tanpa konfigurasi rumit.

---

## User Stories

### Epic: Dynamic Island Notification Overlay

**Primary User Story:** "As a desktop user (Windows/Linux), I want incoming system notifications to appear smoothly inside a top-center Dynamic Island pill so that I can glance at or dismiss them without losing focus."

- **Acceptance Criteria:**
  - [ ] Pill muncul di bagian atas tengah layar utama (always-on-top, tidak mengambil fokus window aktif).
  - [ ] Saat ada notifikasi masuk, pill otomatis melebar menampilkan ikon app, judul, dan ringkasan isi notifikasi.
  - [ ] Klik pada notifikasi yang diperluas dapat membuka/fokus ke aplikasi terkait (atau menandai terbaca) dan swipe/klik tutup mengembalikan pill ke state idle.

### Supporting User Stories

1. "As a user, I want the island to stay unobtrusive when idle so that it doesn't block my browser tabs or title bars."
   - **AC:** Pill idle berukuran compact atau auto-hide/semi-transparan, serta mendukung click-through atau hover-expand.
2. "As a user, I want a system tray icon to pause, restart, or quit the app so that I always have full control over the background process."
   - **AC:** Klik kanan ikon tray menampilkan menu minimal: Status Listener, Pause/Resume, Launch at Startup (opsional toggle), dan Quit.

---

## Functional Requirements

### Core Features (MVP — P0)

#### Feature 1: Floating Island Overlay Window

- **Description:** Window borderless transparan, rounded pill, always-on-top di posisi top-center monitor utama, dengan state visual `Idle` (compact), `Alert` (compact notification preview), dan `Expanded` (detail notifikasi + aksi cepat).
- **User Value:** Memberikan jangkar visual yang konsisten dan estetik di top-center layar.
- **Business Value:** Menjadi fondasi arsitektur UI untuk semua fitur P1/P2 berikutnya.
- **Acceptance Criteria:**
  - [ ] Window tidak muncul di taskbar/Alt+Tab dan tidak mencuri keyboard focus saat notifikasi muncul (Windows: `WS_EX_TOOLWINDOW` + `WS_EX_NOACTIVATE`; Linux: Wayland `wlr-layer-shell` overlay / X11 dock/override-redirect tanpa focus).
  - [ ] Mendukung transparansi per-pixel dan sudut membulat (rounded corners) yang tajam di berbagai DPI scaling (100%, 125%, 150%, 200%).
  - [ ] Animasi transisi ukuran dan opacity berjalan halus (60+ FPS).
- **Dependencies:** Lightweight cross-platform Rust GUI/windowing (`egui`/`eframe`, `slint`, atau `winit`/`smithay-client-toolkit` + `windows-rs` untuk platform hooks).
- **Estimated Effort:** L

#### Feature 2: System Notification Listener (Windows & Linux)

- **Description:** Layanan background lintas OS di Rust yang mendengarkan notifikasi masuk melalui trait abstraksi tunggal: WinRT `UserNotificationListener` di Windows dan D-Bus `org.freedesktop.Notifications` di Linux, lalu memetakan payload (App Name, App Icon, Title, Body Text, Timestamp) ke state Dynamic Island.
- **User Value:** Menampilkan notifikasi real-time dari semua aplikasi desktop tanpa integrasi per-aplikasi.
- **Business Value:** Memberikan nilai guna langsung sejak hari pertama instalasi di Windows maupun Linux.
- **Acceptance Criteria:**
  - [ ] **Windows:** Meminta dan menangani izin akses `UserNotificationListener` dengan pesan yang jelas jika ditolak.
  - [ ] **Linux:** Menangkap notifikasi via D-Bus session bus (`zbus`) — baik sebagai notification daemon atau D-Bus monitor/eavesdrop jika daemon lain aktif.
  - [ ] Menangkap event notifikasi baru secara real-time (< 150 ms latency) dan menampilkannya di Island.
  - [ ] Menangani antrean (queue) sederhana jika beberapa notifikasi masuk berurutan (menampilkan yang terbaru atau indikator `+N`).
  - [ ] Menjaga agar tidak crash bila payload notifikasi kosong/malformed atau ikon app gagal diambil.
- **Dependencies:** `windows` crate (Windows) dan `zbus` crate (Linux D-Bus).
- **Estimated Effort:** L

#### Feature 3: Interaksi Island (Hover, Expand, Dismiss)

- **Description:** Interaksi mouse sederhana pada pill: auto-collapse setelah timeout (mis. 5 detik), hover untuk menahan/memperluas notifikasi, klik kanan atau tombol `×` untuk dismiss notifikasi dari listener.
- **User Value:** Pengguna bisa membaca detail notifikasi lebih lama atau langsung menutupnya tanpa menunggu timeout.
- **Business Value:** Menjamin UX yang terasa responsif dan tidak kaku.
- **Acceptance Criteria:**
  - [ ] Notifikasi otomatis kembali ke state `Idle` setelah durasi default (5 detik) jika kursor tidak berada di atas pill.
  - [ ] Saat kursor hover di atas pill, timer auto-collapse dijeda dan pill menampilkan versi `Expanded` (teks body lebih lengkap + tombol Dismiss).
  - [ ] Tombol Dismiss menghapus notifikasi aktif dari tampilan Island (dan opsional menghapus dari Windows Action Center).
- **Dependencies:** Feature 1 & Feature 2.
- **Estimated Effort:** S

#### Feature 4: System Tray Control & Minimal Config

- **Description:** Ikon di System Tray (Windows Tray / Linux `StatusNotifierItem`) untuk mengontrol lifecycle aplikasi (Pause/Resume notifikasi, Quit) dan konfigurasi dasar (posisi offset Y atau auto-hide saat idle) yang disimpan di file lokal sederhana (TOML).
- **User Value:** Kontrol instan jika pengguna sedang presentasi/fullscreen gaming dan ingin menjeda Island.
- **Business Value:** Mengurangi churn akibat pengguna merasa terganggu saat kondisi tertentu (misal presentasi/game).
- **Acceptance Criteria:**
  - [ ] Ikon tray tampil saat aplikasi berjalan dan mendukung menu: `Pause/Resume`, `Auto-hide when idle`, `Exit`.
  - [ ] Pengaturan tersimpan lokal di `%APPDATA%\liebert-dynamic-island\config.toml` (Windows) atau `~/.config/liebert-dynamic-island/config.toml` (Linux XDG).
- **Dependencies:** `tray-icon` crate (atau `ksni` untuk Linux SNI jika diperlukan).
- **Estimated Effort:** S

### Should Have (P1)

- **Media Control (SMTC / MPRIS2):** Menampilkan _Now Playing_ (Spotify, YouTube, browser) via SMTC di Windows dan MPRIS2 D-Bus di Linux beserta tombol Play/Pause/Next/Prev. _(Ditunda ke P1 agar MVP fokus penuh pada kestabilan overlay & notification listener)._
- **System HUD (Volume & Brightness):** Indikator visual custom saat volume/brightness berubah. _(Ditunda ke P1 karena membutuhkan hook/observer tambahan)._
- **Fullscreen / Game Auto-Hide:** Otomatis menyembunyikan pill saat ada window fullscreen aktif.

### Could Have (P2)

- **Timer / Pomodoro Cepat:** Hitung mundur langsung di dalam pill.
- **Quick Drop / File Shelf:** Area drag-and-drop sementara di pill.
- **Multi-Monitor Picker & Custom Theme:** Memilih monitor target dan kustomisasi warna/ukuran via UI settings penuh.

### Out of Scope (Not in MVP)

- **macOS Support:** Proyek ini fokus pada Windows 10/11 dan Linux (X11/Wayland) terlebih dahulu.
- **Cloud Sync / Akun Pengguna:** Tidak ada backend, login, atau sinkronisasi cloud.
- **AI Notification Summarization:** Tidak ada integrasi LLM/AI di dalam produk MVP.
- **Pengganti Total Notification Center:** Island hanya menampilkan notifikasi aktif & antrean singkat, bukan riwayat notifikasi jangka panjang.

---

## Non-Functional Requirements

### Performance

- **Startup Time:** < 500 ms hingga pill muncul di layar.
- **Notification Latency:** < 150 ms dari event WinRT diterima hingga UI mulai animasi masuk.
- **Memory Footprint:** < 25 MB RAM saat idle, < 50 MB RAM saat expanded dengan ikon/gambar notifikasi.
- **CPU Usage:** < 0.5% CPU saat idle (event-driven render loop, tidak busy-polling 60 FPS saat statis).
- **Frame Pacing:** ≥ 60 FPS saat animasi transisi ukuran/opacity berjalan.

### Security & Privacy

- **Local-Only Processing:** Seluruh data notifikasi (judul, isi teks, ikon app) diproses sepenuhnya di memori lokal. Tidak ada telemetri, tidak ada pengiriman data ke jaringan eksternal.
- **Data Protection:** Tidak menyimpan log isi notifikasi ke disk secara persisten di MVP.
- **Permissions:** Hanya meminta izin standar OS (`UserNotificationListener` di Windows, D-Bus session access di Linux) tanpa hak Administrator/root.

### AI / Automation Requirements

- **AI Surface:** None (pure native desktop utility, no product AI).

### Usability & Compatibility

- **OS Support:** Windows 10 (build 1903+) / Windows 11 (x86_64) dan Linux (x86_64 — X11 & Wayland compositors yang mendukung `wlr-layer-shell`, mis. Hyprland, Sway, KDE Plasma; GNOME Wayland bersifat best-effort/XWayland fallback).
- **DPI Awareness:** Per-Monitor DPI Aware v2 di Windows dan fractional scaling support di Linux.
- **Non-Intrusive:** Tidak mencuri fokus dari game/editor/aplikasi aktif saat notifikasi muncul.

### Scalability

- **Architecture:** Pemisahan jelas antara layer **Platform Bridge (Win32/WinRT & Linux D-Bus/X11/Wayland)** di balik trait Rust, **State/Event Engine**, dan **Renderer UI** agar modul P1 (Media SMTC/MPRIS2, Volume HUD) dapat ditambahkan tanpa merombak ulang inti aplikasi.

### Quality Standards

_Standar kualitas engineering (type safety, testing, aturan kode) akan didefinisikan di tahap implementasi/arsitektur, bukan di dokumen ini._

---

## UI/UX Requirements

### Design Principles

1. **Unobtrusive by Default:** Saat idle, Island menyatu dengan tepi atas layar (ukuran kecil atau auto-hide) agar tidak menghalangi tab browser atau window controls.
2. **Fluid & Tactile Motion:** Perubahan ukuran pill (compact ↔ expanded) menggunakan easing/spring animation yang halus, bukan muncul/hilang secara kasar.
3. **Glanceable Hierarchy:** Pada state alert, cukup tampilkan ikon app + judul/pengirim; detail teks lengkap hanya muncul saat hover/expanded.

### Information Architecture

```text
├── Dynamic Island Pill (Top-Center Overlay)
│   ├── Idle State (Minimal pill / hidden if auto-hide on)
│   ├── Compact Notification State (App Icon + Title + Short Preview)
│   └── Expanded Notification State (App Icon + App Name + Full Body + Dismiss Button)
└── System Tray Menu
    ├── Status (Listening / Paused)
    ├── Pause / Resume Notifications
    ├── Toggle: Auto-hide when idle
    └── Exit
```

### Key User Flows

**Flow 1: Notifikasi Masuk & Auto-Collapse**

1. Aplikasi lain (mis. WhatsApp/Discord) memicu Windows Toast → 2. `UserNotificationListener` menangkap event dan mengirim payload ke Island → 3. Pill beranimasi melebar ke `Compact Notification State` selama 5 detik → 4. Jika tidak ada hover, pill kembali ke `Idle State`.

**Flow 2: Hover untuk Baca Detail & Dismiss**

1. Pill sedang berada di `Compact Notification State` → 2. Pengguna mengarahkan kursor ke atas pill → 3. Timer auto-collapse dijeda dan pill melebar ke `Expanded State` → 4. Pengguna membaca teks lengkap lalu klik tombol `Dismiss` (atau menjauhkan kursor untuk auto-collapse).

**Flow 3: Izin / Daemon Notifikasi Belum Siap**

1. Aplikasi pertama kali dijalankan → 2. Sistem mengecek akses listener (`UserNotificationListener` di Windows atau D-Bus `org.freedesktop.Notifications` di Linux) → 3. Jika izin ditolak (Windows) atau bentrok dengan daemon lain (Linux), Island menampilkan pesan singkat dengan instruksi perbaikan → 4. Setelah siap, listener aktif otomatis.

---

## Success Metrics

| Category       | Metric                       | Target                             | Measurement                                     |
| :------------- | :--------------------------- | :--------------------------------- | :---------------------------------------------- |
| Performance    | Idle RAM Usage               | < 25 MB                            | Windows Task Manager / Process Explorer         |
| Performance    | Idle CPU Usage               | < 0.5%                             | Windows Task Manager (rata-rata 1 menit)        |
| Reliability    | Crash-free session           | 100% selama 8 jam pemakaian harian | Manual dogfooding & error logging lokal         |
| Responsiveness | Notification display latency | < 150 ms                           | Timestamp log antara event WinRT & render frame |

---

## Constraints & Assumptions

### Constraints

- **Budget:** \$0 (proyek solo / open-source).
- **Timeline:** 1–2 minggu untuk MVP fungsional.
- **Resources:** 1 Developer (Level B — familiar dengan Rust & pengembangan sistem).
- **Technical:**
  - Wajib menggunakan Rust dengan stack GUI native/ringan (tanpa Electron/WebView) dan mendukung target Windows + Linux.
  - **Windows:** Bergantung pada WinRT `UserNotificationListener` API (perlu verifikasi perilaku pada unpackaged `.exe` vs packaged MSIX/sparse package).
  - **Linux:** Di Wayland, positioning overlay top-center tanpa focus membutuhkan protokol `wlr-layer-shell` (GNOME Shell tidak mendukung `wlr-layer-shell` secara native); serta D-Bus `org.freedesktop.Notifications` hanya mengizinkan satu daemon aktif per session kecuali menggunakan D-Bus eavesdrop/proxy.

### Assumptions

- Pengguna menjalankan aplikasi di Windows 10/11 dengan Windows Notifications aktif, atau di Linux dengan D-Bus session aktif.
- Di Linux, pengguna bersedia menjalankan Liebert Dynamic Island sebagai notification daemon utama (menggantikan `dunst`/`mako`) atau via mode monitor jika didukung.

### Open Questions

- **WinRT `UserNotificationListener` Identity:** Apakah `UserNotificationListener::Current()` berjalan lancar pada binary Rust unpackaged (`.exe` biasa) di Windows 11 target, atau memerlukan Sparse Package / MSIX manifest minimal? _(Harus diuji di hari ke-1 implementasi)._
- **Linux Notification Mode:** Apakah di Linux MVP bertindak sebagai daemon `org.freedesktop.Notifications` penuh (menggantikan daemon bawaan) atau memantau D-Bus traffic?
- **Pilihan Renderer Cross-Platform:** Antara `egui/eframe` vs `slint` vs `iced` + layer-shell/win32 hooks — mana yang paling cepat selesai dalam 1–2 minggu dengan RAM < 25 MB.

### Dependencies

- Crate `windows` (`windows-rs`) untuk Win32 windowing & WinRT Notification API (target Windows).
- Crate `zbus` untuk D-Bus `org.freedesktop.Notifications` & `StatusNotifierItem` (target Linux).
- Crate `tray-icon` (cross-platform) + `serde` + `toml` + `directories` untuk konfigurasi lokal.

---

## MVP Definition of Done

### Feature Complete

- [ ] Semua fitur P0 (Overlay Pill, Notification Listener, Hover/Expand/Dismiss, System Tray) terimplementasi.
- [ ] Semua acceptance criteria P0 terpenuhi.
- [ ] Code review / self-review selesai tanpa warning kritis.

### Quality Assurance

- [ ] Unit test pada state machine & queue notifikasi berjalan dan lulus (`cargo test`).
- [ ] Uji manual pada skenario multi-notifikasi, teks panjang/Unicode, DPI scaling, dan permission denied.
- [ ] Target performa (RAM < 25 MB idle, CPU < 0.5% idle) tercapai pada build `--release`.

### Documentation

- [ ] `README.md` berisi cara build (`cargo build --release`), cara menjalankan, dan syarat izin notifikasi Windows.
- [ ] Konfigurasi default terdokumentasi singkat.

### Release Ready

- [ ] Binary release berjalan stabil di Windows 10/11 (`.exe` tanpa console window) dan Linux (`x86_64` binary).
- [ ] Smoke test harian selama minimal 1 hari kerja penuh berhasil tanpa crash.

---

<details>
<summary><b>Enterprise add-on — skip for MVP</b></summary>

_These frameworks matter post-launch (or for funded teams reporting to stakeholders) — not for a first MVP with zero users. Come back to them once you have real usage data._

### North Star Metric

Daily Active Users (DAU) yang menjalankan aplikasi di startup dan tidak menonaktifkannya selama > 7 hari berturut-turut.

### OKRs (First 90 Days)

**Objective 1:** Merilis MVP yang stabil dan ringan untuk komunitas Windows.

- **KR1:** 100+ GitHub Stars / download awal dalam 30 hari pasca rilis.
- **KR2:** Penggunaan RAM tetap di bawah 25 MB pada 95% perangkat pengguna.
- **KR3:** < 2% crash rate dari issue yang dilaporkan.

### Full AARRR Metrics Framework

| Category    | Metric                                   | Target    | Measurement                        |
| :---------- | :--------------------------------------- | :-------- | :--------------------------------- |
| Acquisition | GitHub Releases downloads                | 200+      | GitHub API / Release stats         |
| Activation  | Menyalakan app & memberi izin notifikasi | > 80%     | Feedback komunitas / survey        |
| Retention   | Pemakaian rutin mingguan                 | > 50%     | GitHub discussions / user feedback |
| Revenue     | N/A (Open Source)                        | -         | -                                  |
| Referral    | Organik share / Reddit r/desktops        | 10+ posts | Social mentions                    |

### Risk Matrix

| Risk                                                                                  | Probability | Impact | Mitigation                                                                                  |
| :------------------------------------------------------------------------------------ | :---------- | :----- | :------------------------------------------------------------------------------------------ |
| `UserNotificationListener` butuh MSIX/Package Identity di beberapa versi Windows      | Med         | High   | Lakukan technical spike di hari ke-1; siapkan sparse package manifest jika wajib            |
| Fragmentasi Wayland compositor di Linux (GNOME tidak dukung `wlr-layer-shell`)        | High        | Med    | Prioritaskan dukungan resmi X11 + wlroots/KDE Wayland di MVP; dokumentasikan limitasi GNOME |
| Konflik D-Bus `org.freedesktop.Notifications` dengan daemon existing (`dunst`/`mako`) | Med         | Med    | Dokumentasikan cara menonaktifkan daemon lama atau implementasi D-Bus monitor fallback      |
| Overlay terasa lag atau kedip saat animasi resize                                     | Med         | Med    | Gunakan double-buffering / framework dengan VSync stabil                                    |
| Notifikasi bertubi-tubi memenuhi layar                                                | Low         | Med    | Terapkan queue sederhana & auto-collapse timer                                              |

### Appendices

- **A. Competitive Analysis:** Dynamic Island clones di Windows umumnya berbasis Electron/C# WPF. Rust native memberikan keunggulan footprint memori dan startup instan.
- **B. Technical Specifications:** Akan disusun di tahap berikutnya (Arsitektur & Tech Stack).
- **C. Mockups/Wireframes:** Belum dibuat; cukup sketsa pill 3 state (`Idle`, `Compact`, `Expanded`).

</details>

---

**PRD Version:** 1.0 | **Next Review:** 2026-12-02 | **Owner:** Developer | **Stakeholders:** Solo Developer / End Users

## Handoff Context

<!-- Machine-readable summary for the next workflow step. Do not delete; the next prompt in the workflow reads this block. -->

- Stage: prd
- App name: Liebert Dynamic Island
- User level: B (A = vibe coder, B = developer, C = in-between)
  \*\*\*\*- Target platform: Windows 10/11 & Linux (X11/Wayland) (x86_64) — Native Rust
- Budget: $0 (Indie / Open Source)
- Timeline: 1-2 weeks
- AI in product scope: no
- Research status: not provided (user-validated requirements via interview)
- Constraints: Native/lightweight Rust GUI (low RAM, instant startup), no Electron/WebView, P0 strictly scoped to System Notifications (Windows Toast & Linux D-Bus) + minimal overlay & tray
- Decisions: App focuses on System Notifications (WinRT UserNotificationListener on Windows, D-Bus org.freedesktop.Notifications on Linux) for MVP; Media Control (SMTC/MPRIS2) and System HUD deferred to P1
- Open questions: Verify whether WinRT UserNotificationListener requires package identity (MSIX/Sparse Package) on unpackaged Rust .exe; decide Linux D-Bus daemon vs monitor mode; finalize cross-platform lightweight Rust renderer (egui/eframe vs slint vs iced) + Wayland layer-shell support
- Source files: none → docs/PRD-LiebertDynamicIsland-MVP.md
