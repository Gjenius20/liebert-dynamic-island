# Liebert Dynamic Island (Lidi)
> Lightweight native Rust Dynamic Island overlay for **Windows 10/11** and **Linux (X11/Wayland)** that surfaces system notifications, media controls, synced karaoke lyrics, a Pomodoro timer, a local todo list, and Google Calendar alerts at the top-center of your screen without stealing window focus.

## UI

| | |
| :---: | :---: |
| <img height="130" alt="UI Preview 1" src="https://github.com/user-attachments/assets/beecbc2a-f66c-43bf-b421-154a759ec6c9" /> | <img height="90" alt="UI Preview 2" src="https://github.com/user-attachments/assets/23af6133-3afa-4038-8a3f-e82aa42d356b" /> |

> Modern UI That You Can Choose.

---

## Settings

<p align="center">
  <img width="340" alt="Settings Preview" src="https://github.com/user-attachments/assets/43106fe9-4c85-42a5-a46e-5f4cc7f00baa" />
</p>

> Fully Customize Settings.
## Key Features

- **Non-Intrusive Notification Overlay:** Smooth `Idle` ↔ `Compact` ↔ `Expanded` pill transitions with branded app accents (WhatsApp, Discord, Gmail, Chrome, Telegram, Slack, etc.), notification queueing, hover-to-expand, and configurable auto-collapse.
- **Top Notch & Floating Island Modes:** Switch in Settings between a seamless bezel-docked **Top Notch** (with concave ears) and a **Floating Dynamic Island**, with automatic hiding during fullscreen / windowed-fullscreen apps (while still surfacing incoming notifications) and configurable delayed auto-hide.
- **Media Controls & Karaoke Lyrics:** Read playback state and control Shuffle, Previous, Play/Pause, Next, Repeat, and seek position via Windows SMTC or Linux MPRIS2. Fetches synced `.lrc` lyrics from LRCLIB with progressive karaoke highlight in **Original**, offline **Romaji** (`kakasi` + `any_ascii`), or **Translate** mode, plus dynamic accent glow extracted from album art.
- **Built-In Productivity:** Includes a **Pomodoro Timer** (`Focus` / `Short Break` / `Long Break` with 4-session cycles and idle countdown), local **Todo List** (`todos.toml`), **Google Calendar** `.ics` sync with 5-minute upcoming alerts, and an IANA timezone **Clock**.
- **Device Font Picker & Live Settings:** Secondary Settings viewport for theme presets (`Crimson`, `Obsidian`, `Midnight`, `AMOLED`, `Slate`), custom RGB/opacity, scale, dimensions, installed device font selection, and system tray controls.

## Tech Stack / Architecture

- **Core & UI:** Rust (`edition = "2024"`), `eframe 0.37` (`egui`, `Renderer::Glow` with `x11` and `wayland` support), `tray-icon 0.27` (`ksni`), `serde`, `toml`, `directories`.
- **Windows Bridge (`windows 0.62`):** Win32 `WS_EX_NOACTIVATE` / `WS_EX_TOOLWINDOW` / `WS_EX_TRANSPARENT` click-through canvas, DWM borderless integration, WinRT `UserNotificationListener`, SMTC (`Media_Control`), `Storage_Streams` album art, and `SHQueryUserNotificationState` + DWM bounds fullscreen detection.
- **Linux Bridge (`zbus 6.5`):** D-Bus `org.freedesktop.Notifications` listener, MPRIS2 (`org.mpris.MediaPlayer2.*`), and Hyprland UNIX socket IPC positioning.
- **Integrations & Text:** `ureq` (LRCLIB, Google Translate, Google Calendar `.ics`), `image` (album art & dominant accent extraction), `kakasi` + `any_ascii` (offline romanization), `chrono` + `chrono-tz`.

## Prerequisites & Installation

- **Rust Toolchain:** Rust with `edition = "2024"` support (e.g. `rustc 1.85+`).
- **OS Notes:**
  - **Windows 10/11:** Works directly from an unpackaged `.exe` (`UserNotificationListener` permission prompt may appear on first run if notification access is disabled in Windows Privacy settings).
  - **Linux:** `org.freedesktop.Notifications` allows only one active notification daemon per desktop session; stop any existing notification daemon (e.g. `dunst`, `mako`) before testing notifications.

```sh
git clone https://github.com/Zahwan316/liebert-dynamic-island.git
cd liebert-dynamic-island
cargo build --release
```

The release binary is output to `target/release/liebert-dynamic-island` (or `.exe` on Windows).

## Configuration

Settings are stored locally in `config.toml` (and tasks in `todos.toml`) under your OS config directory:
- **Windows:** `%APPDATA%\liebert-dynamic-island\config.toml`
- **Linux:** `~/.config/liebert-dynamic-island/config.toml`

Missing or invalid fields safely fall back to `Default`. Key options:

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `notch_dock_mode` | `bool` | `true` | `true` for seamless Top Notch, `false` for Floating Dynamic Island |
| `scale` | `f32` | `1.0` | UI scale factor (`0.75`–`1.5`) |
| `idle_width` / `expanded_width` | `f32` | `140.0` / `390.0` | Pill width in idle (`110`–`240`) and expanded (`320`–`520`) states |
| `top_offset_px` | `f32` | `10.0` | Top margin in Floating Island mode (`0`–`60`) |
| `auto_hide_when_idle` | `bool` | `false` | Automatically hide the overlay after `auto_hide_delay_secs` |
| `auto_hide_delay_secs` | `u64` | `5` | Idle delay before auto-hiding (`0`–`60` seconds) |
| `hide_in_fullscreen` | `bool` | `true` | Hide idle pill when a fullscreen or windowed-fullscreen app is active |
| `idle_display` | `enum` | `Auto` | `Auto`, `Clock`, `Lyrics`, `NextEvent`, `Pomodoro`, or `Minimal` |
| `clock_timezone` | `string` | `"Local"` | `"Local"` or any IANA timezone (e.g. `"Asia/Jakarta"`) |
| `font_name` | `string` | `""` | Installed device font name (empty uses system default) |
| `karaoke_lyric_mode` | `enum` | `Original` | Primary karaoke text: `Original`, `Romaji`, or `Translate` |
| `lyric_offset_ms` | `i64` | `0` | Lyric sync offset in milliseconds (`-5000`–`5000`) |
| `google_calendar_ics_url` | `string` | `""` | Secret iCal `.ics` URL for Google Calendar sync |

## Usage

Run the overlay normally:

```sh
cargo run
```

Supported CLI flags:

```sh
cargo run -- --demo                 # Launch UI and trigger a demo notification after 1s
cargo run -- --spike-notifications  # Verify OS notification listener access and print active count
```

## Project Structure

```text
liebert-dynamic-island/
├── Cargo.toml
├── AGENTS.md
├── docs/                        # Obsidian project notes & MVP PRD
└── src/
    ├── main.rs                  # Entry point, CLI flags, & background workers
    ├── config.rs                # Local config.toml persistence & defaults
    ├── state.rs                 # OS-agnostic IslandState machine & queue
    ├── clock.rs                 # Digital clock & IANA timezone formatting
    ├── lyrics.rs                # LRC/Enhanced LRC parser, offline romaji & translation
    ├── pomodoro.rs              # Pomodoro timer state machine (Focus/Break cycles)
    ├── todo.rs                  # Local TodoStore & todos.toml persistence
    ├── calendar.rs              # RFC 5545 .ics parser & Google Calendar sync
    ├── tray.rs                  # System tray icon & menu actions
    ├── ui/
    │   ├── mod.rs               # Overlay pill renderer, tabs, & permanent canvas
    │   └── settings.rs          # Secondary Settings viewport
    └── platform/
        ├── mod.rs               # Cross-platform facade & font directory scanner
        ├── windows.rs           # Win32 overlay hooks, WinRT notifications & SMTC
        └── linux.rs             # D-Bus notifications, MPRIS2, & Hyprland IPC
```

## Development & Testing

```sh
cargo test                                 # Run all unit tests
cargo clippy --all-targets -- -D warnings  # Run Clippy with warnings treated as errors
cargo build --release                      # Build stripped LTO release binary
```
