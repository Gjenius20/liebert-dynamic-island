use std::sync::mpsc;

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, ViewportBuilder, ViewportId};

use crate::calendar::{CalendarCommand, CalendarEvent, format_countdown, format_local_time, upcoming_events};
use crate::clock::{COMMON_TIMEZONES, format_clock, is_valid_timezone};
use crate::config::{Config, IdleDisplayMode, KaraokeLyricMode};
use crate::lyrics::TRANSLATE_LANGUAGES;
use crate::platform::InstalledFont;
use crate::todo::TodoStore;

pub const SETTINGS_WINDOW_TITLE: &str = "Liebert Dynamic Island — Settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Todo,
    Calendar,
}

pub struct SettingsWindowState<'a> {
    pub open: &'a mut bool,
    pub focus_requested: &'a mut bool,
    pub tab: &'a mut SettingsTab,
    pub config: &'a mut Config,
    pub todos: &'a mut TodoStore,
    pub new_todo_input: &'a mut String,
    pub calendar_url_input: &'a mut String,
    pub calendar_events: &'a [CalendarEvent],
    pub calendar_status: &'a str,
    pub calendar_cmd_tx: &'a mpsc::Sender<CalendarCommand>,
    pub config_dirty: &'a mut bool,
    pub test_notification_requested: &'a mut bool,
    pub installed_fonts: &'a mut Vec<InstalledFont>,
    pub font_search: &'a mut String,
}

pub fn render_settings_viewport(ctx: &egui::Context, st: SettingsWindowState<'_>) {
    if !*st.open {
        return;
    }

    ctx.show_viewport_immediate(
        ViewportId::from_hash_of("liebert_settings"),
        ViewportBuilder::default()
            .with_title(SETTINGS_WINDOW_TITLE)
            .with_inner_size([450.0, 540.0])
            .with_active(true)
            .with_resizable(true),
        |ui, _class| {
            if ui.input(|i| i.viewport().close_requested()) {
                *st.open = false;
                *st.focus_requested = false;
                return;
            }
            if *st.focus_requested {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }

            apply_settings_theme(ui);
            let panel_frame = egui::Frame::new()
                .fill(Color32::from_rgb(19, 19, 23))
                .inner_margin(egui::Margin::same(14));

            egui::CentralPanel::default().frame(panel_frame).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(st.tab, SettingsTab::Appearance, "Appearance");
                    ui.selectable_value(st.tab, SettingsTab::Todo, "Todo List");
                    ui.selectable_value(st.tab, SettingsTab::Calendar, "Google Calendar");
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::ScrollArea::vertical().show(ui, |ui| match *st.tab {
                    SettingsTab::Appearance => render_appearance_tab(
                        ui,
                        st.config,
                        st.config_dirty,
                        st.test_notification_requested,
                        st.installed_fonts,
                        st.font_search,
                    ),
                    SettingsTab::Todo => {
                        render_todo_tab(ui, st.todos, st.new_todo_input);
                    }
                    SettingsTab::Calendar => render_calendar_tab(
                        ui,
                        st.config,
                        st.calendar_url_input,
                        st.calendar_events,
                        st.calendar_status,
                        st.calendar_cmd_tx,
                        st.config_dirty,
                    ),
                });
            });
        },
    );
}

fn render_appearance_tab(
    ui: &mut egui::Ui,
    config: &mut Config,
    config_dirty: &mut bool,
    test_notification_requested: &mut bool,
    installed_fonts: &mut Vec<InstalledFont>,
    font_search: &mut String,
) {
    if installed_fonts.is_empty() {
        *installed_fonts = crate::platform::list_installed_fonts();
    }
    let mut changed = false;

    ui.label(RichText::new("Display Mode (Notch / Dynamic Island)").strong());
    ui.horizontal(|ui| {
        let btn_w = ((ui.available_width() - ui.spacing().item_spacing.x) * 0.5).max(140.0);
        for (is_notch, label) in [
            (true, "Top Notch (Seamless Top)"),
            (false, "Floating Dynamic Island"),
        ] {
            let selected = config.notch_dock_mode == is_notch;
            let fill = if selected {
                Color32::from_rgba_unmultiplied(224, 92, 98, 70)
            } else {
                Color32::from_rgb(32, 32, 38)
            };
            let stroke = if selected {
                Stroke::new(1.5, Color32::from_rgb(224, 92, 98))
            } else {
                Stroke::new(1.0, Color32::from_rgb(65, 65, 76))
            };
            let text_col = if selected {
                Color32::WHITE
            } else {
                Color32::from_rgb(185, 185, 195)
            };
            let btn = egui::Button::new(RichText::new(label).color(text_col).strong())
                .min_size(egui::vec2(btn_w, 30.0))
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(8));
            if ui.add(btn).clicked() && !selected {
                config.notch_dock_mode = is_notch;
                changed = true;
            }
        }
    });
    ui.label(
        RichText::new(if config.notch_dock_mode {
            "Notch menempel di tepi atas monitor"
        } else {
            "Floating pill dengan jarak Top Offset"
        })
        .small()
        .color(Color32::GRAY),
    );
    changed |= ui
        .add_enabled(
            !config.notch_dock_mode,
            egui::Slider::new(&mut config.top_offset_px, 0.0..=60.0).text("Top Offset (px)"),
        )
        .changed();

    ui.add_space(8.0);
    ui.label(RichText::new("Theme Presets").strong());
    ui.horizontal(|ui| {
        for (id, label) in [
            ("crimson", "Crimson"),
            ("obsidian", "Obsidian"),
            ("midnight", "Midnight"),
            ("amoled", "AMOLED"),
            ("slate", "Slate"),
        ] {
            if ui.button(label).clicked() {
                config.apply_preset(id);
                changed = true;
            }
        }
    });

    ui.add_space(8.0);
    ui.label(RichText::new("Size & Position").strong());
    changed |= ui
        .add(egui::Slider::new(&mut config.scale, 0.75..=1.5).text("UI Scale"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.idle_width, 110.0..=240.0).text("Idle Width (px)"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.expanded_width, 320.0..=520.0).text("Expanded Width (px)"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.corner_radius, 8..=28).text("Corner Radius"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut config.collapse_timeout_secs, 2..=15).text("Auto-collapse (s)"))
        .changed();

    ui.add_space(8.0);
    ui.label(RichText::new("Colors").strong());
    ui.horizontal(|ui| {
        ui.label("Background:");
        let mut bg = Color32::from_rgba_unmultiplied(
            config.bg_color[0],
            config.bg_color[1],
            config.bg_color[2],
            config.bg_color[3],
        );
        if ui.color_edit_button_srgba(&mut bg).changed() {
            config.bg_color = bg.to_srgba_unmultiplied();
            changed = true;
        }
        ui.label("Accent:");
        if ui.color_edit_button_srgb(&mut config.accent_color).changed() {
            changed = true;
        }
        ui.label("Text:");
        if ui.color_edit_button_srgb(&mut config.text_color).changed() {
            changed = true;
        }
    });
    changed |= ui
        .add(egui::Slider::new(&mut config.border_alpha, 0..=160).text("Border Opacity"))
        .changed();

    ui.add_space(8.0);
    ui.label(RichText::new("Font (Installed on Device)").strong());
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(font_search)
                .hint_text("Filter fonts...")
                .desired_width(130.0),
        );
        let selected_font = if config.font_name.is_empty() {
            "System Default"
        } else {
            config.font_name.as_str()
        };
        let query = font_search.trim().to_ascii_lowercase();
        egui::ComboBox::from_id_salt("device_font_selector")
            .selected_text(selected_font)
            .width(235.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(config.font_name.is_empty(), "System Default")
                    .clicked()
                    && !config.font_name.is_empty()
                {
                    config.font_name.clear();
                    changed = true;
                }
                for f in installed_fonts.iter().filter(|f| {
                    query.is_empty() || f.name.to_ascii_lowercase().contains(&query)
                }) {
                    let is_sel = config.font_name.eq_ignore_ascii_case(&f.name);
                    if ui.selectable_label(is_sel, &f.name).clicked() && !is_sel {
                        config.font_name = f.name.clone();
                        changed = true;
                    }
                }
            });
    });

    ui.add_space(8.0);
    ui.label(RichText::new("Idle Display & Clock").strong());
    egui::ComboBox::from_id_salt("idle_display_mode")
        .selected_text(config.idle_display.label())
        .show_ui(ui, |ui| {
            for mode in [
                IdleDisplayMode::Auto,
                IdleDisplayMode::Clock,
                IdleDisplayMode::Lyrics,
                IdleDisplayMode::NextEvent,
                IdleDisplayMode::Pomodoro,
                IdleDisplayMode::Minimal,
            ] {
                if ui
                    .selectable_value(&mut config.idle_display, mode, mode.label())
                    .changed()
                {
                    changed = true;
                }
            }
        });

    let selected_tz_label = COMMON_TIMEZONES
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(config.clock_timezone.trim()))
        .map(|(_, label)| *label)
        .unwrap_or(config.clock_timezone.as_str());

    egui::ComboBox::from_id_salt("clock_tz_preset")
        .selected_text(selected_tz_label)
        .show_ui(ui, |ui| {
            for (id, label) in COMMON_TIMEZONES {
                if ui
                    .selectable_label(config.clock_timezone.eq_ignore_ascii_case(id), *label)
                    .clicked()
                {
                    config.clock_timezone = (*id).to_owned();
                    changed = true;
                }
            }
        });

    ui.horizontal(|ui| {
        ui.label("Custom IANA TZ:");
        if ui
            .add(
                egui::TextEdit::singleline(&mut config.clock_timezone)
                    .hint_text("Local or Asia/Jakarta")
                    .desired_width(170.0),
            )
            .changed()
        {
            changed = true;
        }
        let valid = is_valid_timezone(&config.clock_timezone);
        let (preview, tz_label) = format_clock(
            chrono::Utc::now(),
            &config.clock_timezone,
            config.clock_24h,
            config.clock_show_seconds,
        );
        if valid {
            ui.label(
                RichText::new(format!("{preview} ({tz_label})"))
                    .small()
                    .color(Color32::from_rgb(100, 210, 150)),
            );
        } else {
            ui.label(
                RichText::new("Invalid TZ (using Local)")
                    .small()
                    .color(Color32::from_rgb(255, 130, 120)),
            );
        }
    });
    ui.horizontal(|ui| {
        changed |= ui.checkbox(&mut config.clock_24h, "24-hour format").changed();
        changed |= ui
            .checkbox(&mut config.clock_show_seconds, "Show seconds")
            .changed();
    });

    ui.add_space(8.0);
    ui.label(RichText::new("Modules & Auto-Hide").strong());
    changed |= ui
        .checkbox(&mut config.auto_hide_when_idle, "Auto-hide island when idle")
        .changed();
    changed |= ui
        .add_enabled(
            config.auto_hide_when_idle,
            egui::Slider::new(&mut config.auto_hide_delay_secs, 0..=60)
                .text("Auto-hide delay (s)"),
        )
        .changed();
    if config.auto_hide_when_idle {
        ui.label(
            RichText::new("Tip: arahkan kursor ke tepi atas tengah layar untuk memunculkan notch kembali.")
                .small()
                .color(Color32::GRAY),
        );
    }
    changed |= ui
        .checkbox(
            &mut config.hide_in_fullscreen,
            "Hide when a fullscreen app/game is active",
        )
        .changed();
    ui.horizontal(|ui| {
        changed |= ui
            .checkbox(&mut config.show_notifications, "Show notification popups")
            .changed();
        if ui.button("Send Test Notification").clicked() {
            *test_notification_requested = true;
        }
    });
    ui.label(
        RichText::new("Supports WhatsApp, Discord, Browser, Gmail, and Windows toasts")
            .small()
            .color(Color32::GRAY),
    );
    changed |= ui
        .checkbox(&mut config.show_lyrics, "Show Music & Synced Lyrics")
        .changed();
    if config.show_lyrics {
        ui.indent("lyrics_sub_opts", |ui| {
            changed |= ui
                .checkbox(
                    &mut config.dynamic_album_color,
                    "Dynamic accent & glow from album art",
                )
                .changed();
            changed |= ui
                .checkbox(&mut config.karaoke_lyrics, "Karaoke highlight")
                .changed();
            ui.horizontal(|ui| {
                ui.label("Karaoke lyric:");
                ui.add_enabled_ui(config.karaoke_lyrics, |ui| {
                    egui::ComboBox::from_id_salt("karaoke_lyric_mode")
                        .selected_text(config.karaoke_lyric_mode.label())
                        .show_ui(ui, |ui| {
                            for mode in [
                                KaraokeLyricMode::Original,
                                KaraokeLyricMode::Romaji,
                                KaraokeLyricMode::Translate,
                            ] {
                                if ui
                                    .selectable_value(
                                        &mut config.karaoke_lyric_mode,
                                        mode,
                                        mode.label(),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        });
                });
            });
            changed |= ui
                .checkbox(&mut config.romaji_lyrics, "Romaji for non-Latin lyrics")
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut config.lyric_offset_ms, -5000..=5000)
                        .text("Lyric sync offset (ms)"),
                )
                .changed();
            ui.horizontal(|ui| {
                changed |= ui
                    .checkbox(&mut config.translate_lyrics, "Translate lyrics to:")
                    .changed();
                ui.add_enabled_ui(config.translate_lyrics, |ui| {
                    let selected_lang = TRANSLATE_LANGUAGES
                        .iter()
                        .find(|(code, _)| *code == config.lyric_translate_lang)
                        .map(|(_, name)| *name)
                        .unwrap_or(config.lyric_translate_lang.as_str());
                    egui::ComboBox::from_id_salt("lyric_translate_lang")
                        .selected_text(selected_lang)
                        .show_ui(ui, |ui| {
                            for (code, name) in TRANSLATE_LANGUAGES {
                                if ui
                                    .selectable_label(
                                        config.lyric_translate_lang == *code,
                                        format!("{name} ({code})"),
                                    )
                                    .clicked()
                                {
                                    config.lyric_translate_lang = (*code).to_owned();
                                    changed = true;
                                }
                            }
                        });
                });
            });
        });
    }
    changed |= ui
        .checkbox(&mut config.show_todo, "Show Todo List tab")
        .changed();
    changed |= ui
        .checkbox(&mut config.show_calendar, "Show Google Calendar tab")
        .changed();
    changed |= ui
        .checkbox(&mut config.show_pomodoro, "Show Pomodoro Timer tab")
        .changed();
    if config.show_pomodoro {
        ui.indent("pomodoro_sub_opts", |ui| {
            changed |= ui
                .add(egui::Slider::new(&mut config.pomodoro_focus_mins, 1..=120).text("Focus (min)"))
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut config.pomodoro_short_break_mins, 1..=60)
                        .text("Short Break (min)"),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut config.pomodoro_long_break_mins, 1..=90)
                        .text("Long Break (min)"),
                )
                .changed();
        });
    }

    if ui.button("Reset to Defaults").clicked() {
        let keep_url = config.google_calendar_ics_url.clone();
        *config = Config {
            google_calendar_ics_url: keep_url,
            ..Config::default()
        };
        changed = true;
    }

    if changed {
        *config_dirty = true;
    }
}

fn render_todo_tab(ui: &mut egui::Ui, todos: &mut TodoStore, new_todo_input: &mut String) {
    let mut dirty = false;
    ui.label(RichText::new("Add Task").strong());
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(new_todo_input)
                .hint_text("Type a task and press Enter...")
                .desired_width(310.0),
        );
        let submit = (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            || ui.button("Add").clicked();
        if submit && todos.add(new_todo_input).is_some() {
            new_todo_input.clear();
            dirty = true;
            resp.request_focus();
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Tasks ({} pending)", todos.pending_count())).strong());
        if ui.button("Clear Completed").clicked() {
            todos.clear_done();
            dirty = true;
        }
    });
    ui.separator();

    let mut toggle_id = None;
    let mut remove_id = None;
    for item in &todos.items {
        ui.horizontal(|ui| {
            let mut done = item.done;
            if ui.checkbox(&mut done, &item.text).changed() {
                toggle_id = Some(item.id);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×").clicked() {
                    remove_id = Some(item.id);
                }
            });
        });
    }
    if let Some(id) = toggle_id {
        todos.toggle(id);
        dirty = true;
    }
    if let Some(id) = remove_id {
        todos.remove(id);
        dirty = true;
    }

    if dirty && let Err(err) = todos.save() {
        eprintln!("Failed to save todos: {err}");
    }
}

fn apply_settings_theme(ui: &mut egui::Ui) {
    let style = ui.style_mut();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(19, 19, 23);
    style.visuals.window_fill = Color32::from_rgb(19, 19, 23);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(32, 32, 38);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(32, 32, 38);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(7);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(46, 46, 56);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(46, 46, 56);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(7);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(7);
    style.visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(224, 92, 98, 175);
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(224, 92, 98));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
}

fn render_calendar_tab(
    ui: &mut egui::Ui,
    config: &mut Config,
    url_input: &mut String,
    events: &[CalendarEvent],
    status: &str,
    cmd_tx: &mpsc::Sender<CalendarCommand>,
    config_dirty: &mut bool,
) {
    ui.label(RichText::new("Google Calendar Secret iCal URL (.ics)").strong());
    ui.label(
        RichText::new("Google Calendar > Settings > [Your Calendar] > Secret address in iCal format")
            .small()
            .color(Color32::GRAY),
    );
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(url_input)
            .hint_text("https://calendar.google.com/calendar/ical/.../basic.ics")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Save & Sync").clicked() {
            config.google_calendar_ics_url = url_input.trim().to_owned();
            *config_dirty = true;
            let _ = cmd_tx.send(CalendarCommand::SetUrl(
                config.google_calendar_ics_url.clone(),
            ));
        }
        if ui.button("Refresh Now").clicked() {
            let _ = cmd_tx.send(CalendarCommand::Refresh);
        }
        ui.label(RichText::new(status).small());
    });

    ui.add_space(10.0);
    ui.label(RichText::new("Upcoming Events").strong());
    ui.separator();

    let now_utc = chrono::Utc::now().timestamp();
    let upcoming = upcoming_events(events, now_utc, 8);
    if upcoming.is_empty() {
        ui.label(RichText::new("No upcoming events loaded.").color(Color32::GRAY));
    } else {
        for ev in upcoming {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format_local_time(ev.start_utc))
                        .monospace()
                        .strong(),
                );
                ui.label(&ev.summary);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format_countdown(ev.start_utc, ev.end_utc, now_utc))
                            .small()
                            .color(Color32::GRAY),
                    );
                });
            });
        }
    }
}
