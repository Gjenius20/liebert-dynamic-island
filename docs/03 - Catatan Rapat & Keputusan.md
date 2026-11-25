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
- **Status:** Partially Accepted *(Renderer, Windows, dan build/spike D-Bus Linux diterima; verifikasi overlay Wayland/X11 masih pending)*
- **Konteks:** [[PRD-LiebertDynamicIsland-MVP]] dan [[01 - Arsitektur & Spesifikasi]] memerlukan keputusan pada 3 spike Fase 1 sebelum mengunci implementasi penuh:
  1. **Renderer Rust:** `eframe 0.37` dengan `Renderer::Glow` dipilih untuk MVP karena mendukung transparent borderless viewport, `WindowLevel::AlwaysOnTop`, event-driven `request_repaint_after`, serta waktu build dan kompleksitas minimal dalam target 1–2 minggu.
  2. **Windows WinRT `UserNotificationListener`:** Hasil eksekusi `cargo run -- --spike-notifications` di Windows host menunjukkan `SPIKE OK: Access: Allowed | Active toasts: 21` langsung dari `.exe` unpackaged (tanpa Sparse Package/MSIX).
  3. **Linux D-Bus & Wayland Positioning:** `src/platform/linux.rs` kini lolos build release dengan `zbus 6.5` (API builder `session()/name()/serve_at()` mengembalikan `Self`, error baru muncul di `build().await`; `OwnedVariant` menggantikan `OwnedValue`). `cargo run --release -- --spike-notifications` melaporkan sesi D-Bus OK, namun `org.freedesktop.Notifications` saat ini dipegang daemon lain, sehingga klaim nama/listener runtime belum terverifikasi. Positioning `wlr-layer-shell` pada Wayland belum didukung oleh `eframe`/`winit` dan dicatat sebagai limitasi sementara.
- **Keputusan:** Mengunci `eframe 0.37` (`Glow`) dan jalur WinRT unpackaged untuk Windows; build & spike D-Bus Linux divalidasi di host Linux, uji runtime/overlay menyusul.
- **Konsekuensi:** MVP Windows dapat langsung diuji harian dan binary Linux (`target/release/liebert-dynamic-island`) dapat dibangun. Untuk uji notifikasi Linux, daemon incumbent (mis. dunst/mako) harus dihentikan agar aplikasi dapat mengklaim `org.freedesktop.Notifications`. Untuk target Wayland native positioning di masa depan, layer windowing Linux perlu ditingkatkan ke `smithay-client-toolkit` atau `iced_layershell` jika posisi top-center `winit` tidak memadai pada compositor tertentu.

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

## ADR-008: Migrasi Tray Linux ke `ksni` & Integrasi Hyprland IPC
- **Tanggal:** 2026-11-25
- **Status:** Accepted
- **Konteks:** Pada Linux Hyprland (CachyOS), aplikasi gagal berjalan saat startup karena `tray-icon 0.27` default memakai backend `libappindicator`/GTK3 yang panik (`GTK has not been initialized`) sebelum `eframe`/`winit` berjalan. Selain itu, Wayland mengabaikan `OuterPosition` serta mengunci ukuran saat `with_resizable(false)`, sehingga pill terjebak di tengah layar dan tidak membesar.
- **Keputusan:**
  1. Mengubah dependensi `tray-icon` menjadi `default-features = false, features = ["ksni"]` agar tray Linux berjalan lewat D-Bus `StatusNotifierItem` murni tanpa GTK (backend Windows/macOS tetap native).
  2. Menambahkan bridge UNIX IPC socket Hyprland (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`) di `src/platform/linux.rs` untuk mendaftarkan `window_rule` (`float`, `pin`, `no_initial_focus`, `no_blur`, `no_shadow`, `border_size=0`, `no_anim`, `move`), menyinkronkan `resize` + `move` saat ukuran pill berubah, membaca resolusi monitor/posisi kursor (`is_cursor_over_window`, `is_cursor_in_top_trigger`), dan mendeteksi fullscreen (`j/activewindow`).
  3. Mengaktifkan `.with_resizable(cfg!(target_os = "linux"))` di `src/main.rs` agar Wayland `xdg_toplevel` tidak mengunci `min_size == max_size`.
- **Konsekuensi:** Aplikasi dan AppImage berjalan tanpa panic GTK di Hyprland serta otomatis berada di top-center dan responsif berubah ukuran; compositor Wayland non-Hyprland tetap menunggu implementasi `wlr-layer-shell`.
