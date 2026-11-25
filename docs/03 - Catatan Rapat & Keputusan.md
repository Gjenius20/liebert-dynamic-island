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

## ADR-011: Pemisahan Logika Platform Windows/Linux, Responsivitas Tombol Linux, & Ikon Vektor
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pasca penggabungan fitur Windows ke `main`, ditemukan 3 masalah utama di Linux (khususnya Hyprland/Wayland): (1) tombol terasa tidak responsif akibat `.with_resizable(false)` yang mengunci ukuran surface `xdg_toplevel`, 5–7 panggilan sinkron UNIX socket Hyprland IPC per frame di `logic()`, hitbox tombol transport/playbar yang sempit, dan `settings_focus_requested` yang ter-reset sebelum `render_settings_viewport()` berjalan; (2) logika overlay/hover/font Windows dan Linux masih bercampur di `src/ui/mod.rs` dan `src/main.rs`; serta (3) ikon emoji/simbol Unicode (`⚙`, `♪`, `☑`, `📅`, `🔀`, `⏮`, `⏸`, `▶`, `⏭`, `🔁`, `🔂`) dan path font Debian-only (`/usr/share/fonts/truetype/...`) menyebabkan glyph tofu/rusak di Arch Linux dan distro non-Debian.
- **Keputusan:**
  1. **Pemisahan Platform (`src/platform/{mod,windows,linux}.rs`):** Memindahkan perilaku spesifik OS ke belakang facade `platform` (`overlay_resizable`, `should_send_outer_position`, `primary_monitor_logical_width`, `apply_overlay_window_setup`, `sync_overlay_window`, `detect_overlay_hover`, `bring_window_to_front`, `base_fallback_fonts`, `cjk_fallback_fonts`).
  2. **Responsivitas Linux:** Mengaktifkan `.with_resizable(true)` di Linux, memakai fast-path hover nol-IPC saat pointer berada di atas window atau tombol ditekan (`ctx_hovered`/`pointer_busy`), meng-cache `j/monitors` (`500 ms`) & men-throttle probe cursor (`50–75 ms`) serta mem-batch `resize`+`move` hanya saat geometri bulat berubah, memperluas hitbox + `CursorIcon::PointingHand` pada kontrol, dan menunda `bring_window_to_front` hingga `render_settings_viewport` mengirim `ViewportCommand::Focus`.
  3. **Ikon Vektor & Font Lintas Distro:** Membuat `src/ui/icons.rs` berbasis `egui::Painter` untuk menggantikan ikon emoji/simbol teks yang rapuh, serta melengkapi discovery font Linux (`/usr/share/fonts/noto/`, `/usr/share/fonts/TTF/`, `/usr/share/fonts/noto-cjk/`, path Debian/Fedora, plus `fc-match`) termasuk memuat indeks koleksi `0..=3` pada `NotoSansCJK-Regular.ttc` lewat `FontData::from_blob`.
- **Konsekuensi:** Interaksi klik dan hover di Linux menjadi instan tanpa hambatan IPC, tampilan ikon identik dan tajam di Windows maupun Linux tanpa ketergantungan font emoji OS, serta kode platform terisolasi rapi untuk pemeliharaan lintas OS.

## ADR-012: Perbaikan Z-Order Hyprland (`pin` + `alter_zorder`), Compositor `rounding=0`, Unified AA Notch Mesh, & Skrip AppImage
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Pada uji runtime Linux (Hyprland 0.57.2), ditemukan bahwa overlay notch tidak berada di z-index paling atas dan bentuk border radius serta telinga notch tampak terpotong/berantakan. Penyebabnya: (1) `apply_no_activate_hook` mengirim string ke `hl.dsp.window.float`/`pin` alih-alih tabel `{action="set", window=...}`, sehingga evaluasi Lua gagal dan jatuh ke fallback `dispatch pin` yang justru men-toggle `pin` kembali ke `false`, ditambah tidak adanya `alter_zorder(top)` serta `apply_no_activate_hook` berhenti sebelum surface Wayland termap; (2) `hl.window_rule` belum menyertakan `rounding=0`, `decorate=false`, `no_dim=true`, `no_glow=true`, dan `no_wobble=true`, sehingga `decoration:rounding` bawaan Hyprland memotong sudut atas telinga notch dan mengklip border radius; dan (3) gambar telinga cekung `Mesh` tanpa anti-aliasing terpisah dari `RectShape` body dengan `top_y = rect.top() - 1.0`.
- **Keputusan:**
  1. **Topmost & Floating Fixed di Hyprland/Linux (`src/platform/linux.rs`, `src/main.rs`, `src/ui/mod.rs`):** Menambahkan awalan slash wajib socket1 Hyprland (`/eval`, `/dispatch`, `/keyword`), menyetel `with_app_id("liebert-dynamic-island")` untuk rule `class` saat map awal, memperbaiki seluruh dispatcher Lua Hyprland (`{action="set", window=...}`, `alter_zorder({mode="top", ...})`, `hl.dsp.focus`), menjaga fallback legacy tetap idempoten via `j/clients`, me-retry `apply_overlay_window_setup` hingga jendela termap, menambahkan `ensure_overlay_topmost` ber-throttle `1000 ms`, serta mengirim ulang `ViewportCommand::WindowLevel(AlwaysOnTop)` saat jendela kembali visible (untuk X11).
  2. **Unified Anti-Aliased Notch, Rounding, & Compact Pill Album Gradient (`src/platform/linux.rs`, `src/config.rs`, `src/ui/`):** Menambahkan `rounding=0`, `decorate=false`, `no_dim=true`, `no_glow=true`, `no_wobble=true` pada `hl.window_rule`; mengganti rendering notch menjadi satu mesh terpadu (`build_notch_surface_mesh`) dengan tepi feather anti-aliasing `0.75 px` untuk warna solid maupun tekstur gradien; memisahkan fase transisi `top_radius` vs `ear_w`; meng-clamp corner radius terhadap dimensi aktual `card_rect`; serta menambahkan opsi `compact_album_gradient` di `Config` & Settings untuk gradasi hitam `#000` menuju warna dominan album di paling bawah saat mode `Idle`/`Compact` pill.
  3. **Otomasi Packaging AppImage (`scripts/build-appimage.sh`):** Menyediakan skrip standar untuk membangun binary `--release` langsung menjadi `dist/Liebert_Dynamic_Island-0.1.0-x86_64.AppImage`.
- **Konsekuensi:** Overlay tetap berada di lapis teratas di atas jendela floating/tiled lain pada Hyprland dan X11, sudut pill dan telinga cekung notch tampil halus tanpa terpotong compositor, serta build AppImage Linux dapat direproduksi dengan satu perintah.

## ADR-013: Native Wayland `wlr-layer-shell` (`smithay-client-toolkit` + `egui_glow`) & Pill Lyric Translation
- **Tanggal:** 2026-11-26
- **Status:** Accepted
- **Konteks:** Walaupun aturan window Hyprland sudah diperbaiki, jendela `xdg_toplevel` bawaan `winit`/`eframe` pada dasarnya masih merupakan klien workspace (tetap berada di bawah layer panel Quickshell `Layer::Top`, dan masih rentan diposisikan ulang oleh manajemen workspace Hyprland). Pengguna juga meminta terjemahan lirik tampil di bawah lirik asli saat mode pill/compact beserta toggle pengaturannya.
- **Keputusan:**
  1. **Native Wayland `wlr-layer-shell` (`src/platform/wayland_layer.rs`):** Di Linux Wayland, overlay utama dijalankan langsung di atas `zwlr_layer_surface_v1` (`Layer::Overlay`, `Anchor::TOP`, `exclusive_zone = -1`, `KeyboardInteractivity::None`) menggunakan `smithay-client-toolkit 0.19` + `glutin 0.32` EGL + `egui_glow 0.37` + `calloop 0.13`, ditambah `hl.layer_rule` Hyprland (`order = -1`, `above_lock = false`, `no_anim = true`, `blur = false`). Dengan ini overlay terpisah penuh dari seluruh workspace, tetap tepat di atas Quickshell namun di bawah lockscreen dan layer overlay ber-order lebih tinggi, serta memakai region input kosong (`set_input_region`) saat tersembunyi. Mode `--settings --ipc-sock` menangani jendela Settings interaktif secara terpisah lewat `eframe` dengan sinkronisasi live via UNIX socket. Jalur `eframe` tetap dipertahankan sebagai fallback otomatis untuk X11/compositor non-`wlr-layer-shell` dan Windows.
  2. **Pill Lyric Translation (`show_pill_translation`):** Menambahkan `Config::show_pill_translation` (default `true`) beserta checkbox di Settings (`Music & Synced Lyrics`), serta memperluas `IdleDisplayMode::Lyrics` agar merender baris terjemahan di bawah lirik utama (`44 px * scale` saat terjemahan tersedia).
  3. **Animasi Spring Bouncy ala macOS (`step_spring`):** Mengganti animasi eksponensial di `src/ui/mod.rs` dengan fisika pegas harmonik teredam 2nd-order ber-substep (`omega_x`/`omega_y` & `zeta` berbeda untuk expand vs collapse serta pop-in saat `became_visible`), dilengkapi animasi spring `notch_morph` dan pacing `6 ms` pada runner Wayland.
- **Konsekuensi:** Di Wayland (`Hyprland`/`Sway`/`River`/`Niri`), Dynamic Island tidak pernah lagi masuk atau berpindah ke workspace aplikasi biasa dan posisinya stabil di top-center di atas Quickshell.
