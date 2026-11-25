---
type: decision-log
status: active
project: "[[00 - Dashboard Proyek]]"
tags:
  - project
  - decisions
---

# 03 - Catatan Rapat & Keputusan

Log *Architecture Decision Records (ADR)* untuk proyek [[00 - Dashboard Proyek]].

---

## ADR-001: Target Stack Native Rust GUI (Tanpa Electron/WebView)
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Aplikasi utility overlay selalu berjalan di background. Electron/WebView memakan 150–400 MB RAM dan lambat saat startup.
- **Keputusan:** Menggunakan Rust dengan native/lightweight GUI (`egui`/`slint`/`iced` atau native hooks) tanpa WebView.
- **Konsekuensi:** Footprint RAM < 25 MB dan startup instan, namun styling animasi harus ditulis di Rust dan perlu penanganan khusus per-OS untuk windowing overlay.

## ADR-002: Scope MVP Fokus pada System Notifications (Windows & Linux)
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Timeline MVP adalah 1–2 minggu oleh 1 developer. Memasukkan Media Control (SMTC/MPRIS2) dan System HUD sekaligus akan memperbesar risiko keterlambatan.
- **Keputusan:** P0 dibatasi pada Floating Pill Overlay, System Notification Listener (Windows WinRT + Linux D-Bus), interaksi dasar (hover/expand/dismiss), dan System Tray + Config minimal. Media Control & System HUD dipindah ke P1.
- **Konsekuensi:** Arsitektur Platform Bridge harus modular agar fitur P1 bisa ditambahkan tanpa refactor besar.

## ADR-003: Hasil Spike Fase 1 — Renderer `eframe 0.37` (Glow) & WinRT Unpackaged
- **Tanggal:** 2026-11-25
- **Status:** Partially Accepted *(Renderer & Windows Accepted; Linux Spike Pending)*
- **Konteks:** [[PRD-LiebertDynamicIsland-MVP]] dan [[01 - Arsitektur & Spesifikasi]] memerlukan keputusan pada 3 spike Fase 1 sebelum mengunci implementasi penuh:
  1. **Renderer Rust:** `eframe 0.37` dengan `Renderer::Glow` dipilih untuk MVP karena mendukung transparent borderless viewport, `WindowLevel::AlwaysOnTop`, event-driven `request_repaint_after`, serta waktu build dan kompleksitas minimal dalam target 1–2 minggu.
  2. **Windows WinRT `UserNotificationListener`:** Hasil eksekusi `cargo run -- --spike-notifications` di Windows host menunjukkan `SPIKE OK: Access: Allowed | Active toasts: 21` langsung dari `.exe` unpackaged (tanpa Sparse Package/MSIX).
  3. **Linux D-Bus & Wayland Positioning:** Implementasi `zbus` (`org.freedesktop.Notifications`) telah disiapkan di `src/platform/linux.rs`, namun belum dikompilasi/diuji di host Linux. Positioning `wlr-layer-shell` pada Wayland belum didukung oleh `eframe`/`winit` dan dicatat sebagai limitasi sementara.
- **Keputusan:** Mengunci `eframe 0.37` (`Glow`) dan jalur WinRT unpackaged untuk Windows; memvalidasi build & perilaku Linux pada sesi pengujian Linux berikutnya.
- **Konsekuensi:** MVP Windows dapat langsung diuji harian. Untuk target Wayland native positioning di masa depan, layer windowing Linux perlu ditingkatkan ke `smithay-client-toolkit` atau `iced_layershell` jika posisi top-center `winit` tidak memadai pada compositor tertentu.

## ADR-004: Arsitektur Lyrics, Settings Viewport, Todo, Google Calendar `.ics`, & Perbaikan Hover
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna meminta tambahan 4 fitur (lirik musik seperti Spotify, menu settings ukuran/warna/tampilan, todo list, dan Google Calendar) serta perbaikan bug saat hover Dynamic Island tidak membesar.
- **Keputusan:**
  1. **Hover Fix:** `IslandState::set_hovered` diubah agar `Idle` selalu bertransisi ke `Expanded` saat hover (sebelumnya hanya bekerja bila `active.is_some()`), dan di Windows posisi kursor dicek langsung via Win32 `GetCursorPos` + `GetWindowRect` (dengan hysteresis padding) karena window `WS_EX_NOACTIVATE` sering tidak menerima event hover pointer standar.
  2. **Settings Window Terpisah (`show_viewport_immediate`):** Karena pill utama memakai `WS_EX_NOACTIVATE` (tidak bisa menerima focus keyboard), input teks untuk Todo & Google Calendar serta slider/color picker ditempatkan di window sekunder normal `eframe` yang bisa difokuskan.
  3. **Music Lyrics (SMTC/MPRIS2 + LRCLIB):** Metadata & timeline dibaca dari OS (`Media_Control` SMTC di Windows, MPRIS2 D-Bus di Linux) dan lirik LRC diambil dari API publik `lrclib.net` dengan cache di memori — tanpa login Spotify OAuth.
  4. **Google Calendar via Secret `.ics` URL:** Menggunakan feed iCal `.ics` resmi Google Calendar + parser RFC 5545 lokal (`src/calendar.rs`) agar tidak memerlukan OAuth client secret maupun local redirect server.
- **Konsekuensi:** Menambah dependensi ringan `ureq 3.5`, `serde_json`, `chrono`, dan fitur `Media_Control`, namun seluruh interaksi overlay tetap non-focus-stealing dan bebas login OAuth eksternal.

## ADR-005: Jam IANA (`chrono-tz`), Idle Display Mode, & Top-Edge Hover Reveal saat Auto-Hide
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna membutuhkan jam dengan pilihan zona waktu, opsi memilih konten apa yang tampil saat idle, serta auto-hide setelah jeda *N* detik yang tetap bisa dimunculkan lagi dengan mengarahkan kursor ke tepi atas layar.
- **Keputusan:**
  1. **Zona Waktu (`chrono-tz`):** Menggunakan `chrono-tz 0.10` di `src/clock.rs` untuk mendukung preset umum sekaligus input IANA timezone bebas (dengan fallback aman ke `Local`).
  2. **Idle Display Mode:** Menambahkan enum `IdleDisplayMode` (`Auto`, `Clock`, `Lyrics`, `NextEvent`, `Minimal`) pada `Config`.
  3. **Delayed Auto-Hide & Top-Edge Reveal:** `IslandState` melacak `idle_since` dan `auto_hide_delay` untuk menunda penyembunyian selama *N* detik. Saat window disembunyikan (`ViewportCommand::Visible(false)`), `eframe::App::logic` tetap dibangunkan tiap `80 ms` dan memanggil Win32 `GetCursorPos` (`is_cursor_in_top_trigger`) untuk mendeteksi kursor di zona tepi atas tengah layar dan memunculkan notch kembali.
- **Konsekuensi:** Fitur berjalan mulus di Windows dengan CPU tetap rendah, namun top-edge reveal saat window tersembunyi belum memiliki implementasi global pointer hook di Linux (`is_cursor_in_top_trigger` mengembalikan `None`).

## ADR-006: Karaoke Clip-Rect Painter, Translate+Romaji (`dt=t&dt=rm`), & Lazy CJK Font Loading
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna menambahkan kebutuhan karaoke lyric, translate lyric, romaji untuk aksara non-abjad (Jepang, Cina, Korea, Arab, dll.), dan playbar penuh (seek, shuffle, prev, play, next, repeat) tanpa mengorbankan target RAM idle `< 25 MB`.
- **Keputusan:**
  1. **Karaoke Highlight:** `src/lyrics.rs` mendukung Enhanced LRC (`<mm:ss.xx>`) maupun interpolasi linear per baris, lalu `src/ui/mod.rs` merender dua galley dengan `Painter::with_clip_rect` sehingga sapuan warna berjalan mulus di `~30 FPS` (`33 ms`) saat playback aktif.
  2. **Translate + Romaji Satu Request:** Worker `src/lyrics.rs` menggabungkan 20 baris per batch ke `translate.googleapis.com/translate_a/single` (`dt=t&dt=rm`) dengan cache dua tingkat (`raw_cache` & `enriched_cache`) serta fallback `kana_to_romaji` offline.
  3. **Lazy Font Fallback:** Font ringan (`segoeui.ttf`, `seguisym.ttf`) dimuat saat startup, sedangkan font CJK/Hangul (`msgothic.ttc`, `malgun.ttf`) baru dimuat dari sistem ketika teks CJK/Hangul terdeteksi.
- **Konsekuensi:** RAM saat idle tanpa lagu CJK tetap rendah, namun fitur Shuffle/Repeat/Seek bergantung pada dukungan SMTC/MPRIS2 masing-masing media player.

## ADR-007: Redesign Music Card 2-Kolom (Referensi Visual), Album Art Gradient Wash, & `LayoutJob` Karaoke
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna mengunggah desain referensi (dark charcoal card dengan bottom wash coral/merah dari cover album, kolom kiri cover+kontrol, kolom kanan multi-line karaoke original+romaji+translate + upcoming redup, dan 2 tombol pill di bawah).
- **Keputusan:**
  1. **Album Art & Dominant Accent:** WinRT `Storage_Streams::DataReader` (Windows) dan `mpris:artUrl` (Linux) membaca cover album sekali per perubahan lagu, lalu `image 0.25` mengecilkan ke maks `160×160` dan `extract_dominant_accent` menghitung warna aksen dominan dinamis.
  2. **Gradient Wash:** `ensure_gradient_texture` menghasilkan tekstur `1×64` yang di-cache dan dirender lewat `RectShape::filled(...).with_texture(...)` dengan corner radius card.
  3. **Wrapped Multi-Line Karaoke:** `render_wrapped_karaoke` menggunakan `egui::text::LayoutJob` dengan segment warna lit/dim per karakter agar karaoke tetap rapi meski baris panjang mengalami word-wrap.
- **Konsekuensi:** Ukuran expanded card saat di tab Musik melebar ke `436×308` (dikalikan `scale`), dan pengguna tetap bisa menonaktifkan `dynamic_album_color` atau memilih preset `"Crimson"` di Settings.

## ADR-008: Notch Dock Mode (`egui::Mesh` Concave Ears) & Romanisasi Offline (`kakasi` + `any_ascii`)
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pengguna menginginkan bentuk notch yang menyatu dengan bezel atas layar (seperti MacBook Notch, tersembunyi saat fullscreen), memindahkan seluruh toggle Karaoke/Romaji/Translate ke popup Settings saja, serta memperbaiki Romaji yang gagal/tidak akurat saat bergantung pada API Google `dt=rm` (sering terkena rate-limit/HTML block atau gagal di Kanji).
- **Keputusan:**
  1. **Notch Dock Mode:** Menambahkan `notch_dock_mode` (`y = 0`, sudut atas rata, telinga cekung kiri/kanan via `egui::Mesh`, mini album cover + lirik + 3-bar equalizer saat idle) sambil tetap menghormati `hide_in_fullscreen`.
  2. **Romanisasi Offline:** Mengganti `dt=rm` + tabel manual dengan `kakasi 0.1` (mendukung Kanji+Kana Jepang, diaktifkan per-lagu bila lagu mengandung Kana) dan `any_ascii 0.3` (untuk Cina, Korea, Arab, Cyrillic, dll.) sehingga Romaji 100% offline, deterministik, dan selaras baris demi baris.
  3. **Settings-Only Lyric Toggles:** Menghapus tombol inline `KR`, `Translate`, dan `Romaji` dari pulau utama agar layout Music Card lebih bersih dan fokus.
- **Konsekuensi:** Fitur Romaji tidak lagi bergantung pada jaringan/Google `dt=rm`; perlu diperhatikan bahwa crate `kakasi 0.1` berlisensi **GPL-3.0**.

## ADR-009: Seamless Windows 11 DWM Notch, Polling Fallback WinRT 500ms, Romaji Karaoke, & Morph `Top Notch` ↔ `Floating Island`
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pada Windows 11, DWM tetap memberi frame border tipis dan corner rounding pada window transparan `winit` sehingga muncul kotak abu-abu dan celah `1–2 px` di tepi atas monitor. Selain itu, callback `UserNotificationListener::NotificationChanged` tidak selalu terpanggil pada `.exe` unpackaged, pengguna juga meminta Romaji bisa dipilih sebagai teks karaoke utama, dan ingin transisi halus saat berpindah antara mode `Notch` dan `Floating Island`.
- **Keputusan:**
  1. **Seamless Windows 11 Top Edge:** Menambahkan fitur `Win32_Graphics_Dwm` dan menyetel `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND`, `DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE`, serta `DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED` di `apply_no_activate_hook`, membersihkan `WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME` dari `GWL_STYLE`, serta memanggil `pin_window_top_y(WINDOW_TITLE, 0)` ketika mode Notch aktif penuh.
  2. **Reliable Toast Popups (`500 ms` Fallback Polling):** `spawn_listener` di `src/platform/windows.rs` menggunakan `sig_rx.recv_timeout(Duration::from_millis(500))` dan fallback `visual.Bindings()` (dan hanya memanggil `on_wake()` saat ada perubahan) agar toast WhatsApp, Discord, browser, Gmail, dll. tetap muncul meski `NotificationChanged` gagal memicu callback.
  3. **Romaji Karaoke & Morphing Transition:** Menambahkan `KaraokeLyricMode` (`Original` / `Romaji`) pada `Config` serta state `notch_morph: f32` di `IslandApp` untuk menginterpolasi `y`, lebar telinga cekung, corner radius atas/bawah, dan border opacity secara kontinu saat berganti antara `Top Notch` dan `Floating Island`.
- **Konsekuensi:** Overlay menyatu rata dengan tepi atas Windows 11 tanpa outline DWM, notifikasi desktop masuk dalam `<= 500 ms` saat Idle CPU tetap rendah, dan perpindahan mode tampilan berlangsung mulus baik dari header island, Settings, maupun System Tray.

## ADR-010: Perbaikan DWM Glitch saat Hover, SMTC `LastUpdatedTime` Sync, Dual-Galley Karaoke, & Foreground Settings
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pasca rilis fitur morphing notch & Romaji karaoke, ditemukan 4 regresi/bug: (1) tombol chrome klasik Win32 (minimize/maximize/close) berkedip saat hover/resize, (2) beberapa elemen UI saling menumpuk pada mode compact/expanded/music, (3) lirik & karaoke Romaji tidak sinkron atau melompat saat word-wrap, dan (4) tombol Settings sulit diklik / jendela Settings gagal naik ke foreground karena root window memakai `WS_EX_NOACTIVATE`.
- **Keputusan:**
  1. **Hover Chrome Fix:** Menghapus `DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED` (yang merusak komposisi transparan DWM dan memunculkan non-client buttons Win32 saat resize), menggantinya dengan pembersihan `WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX` + `.with_*_button(false)` sambil tetap mempertahankan `DWMWCP_DONOTROUND` & `DWMWA_COLOR_NONE`.
  2. **Akurasi Lyric & Romaji Sync:** Mengekstrapolasi `Position` SMTC Windows menggunakan `LastUpdatedTime` (`UniversalTime`) agar posisi awal tidak usang, mencocokkan `duration` di LRCLIB, mempertahankan baris kosong LRC sebagai batas `end_ms` jeda instrumental, menyediakan slider `lyric_offset_ms` (`-5000..=5000`), serta mengganti `render_wrapped_karaoke` ke dual-galley per-baris agar highlight Romaji halus tanpa membelah kata.
  3. **Layout Anti-Overlap & Akses Settings:** Membagi header `render_expanded` menjadi slot kiri/kanan terpisah ber-clip, menjaga panel expanded hanya dirender saat `height >= 95 * scale`, membungkus kolom lirik kanan dengan `ScrollArea::vertical()`, dan memanggil `bring_window_to_front` (`HWND_TOPMOST` -> `HWND_NOTOPMOST` + `SetForegroundWindow`) sambil menjaga root viewport tetap visible selama Settings terbuka.
- **Konsekuensi:** Tidak ada lagi glitch tombol caption Win32 atau tabrakan widget UI, lirik Romaji tersinkron halus dengan dukungan kalibrasi offset pengguna, dan jendela Settings selalu langsung muncul ke depan ketika tombol `⚙` diklik.
