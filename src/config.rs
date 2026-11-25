use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub scale: f32,
    pub idle_width: f32,
    pub expanded_width: f32,
    pub top_offset_px: f32,
    pub collapse_timeout_secs: u64,
    pub bg_color: [u8; 4],
    pub accent_color: [u8; 3],
    pub text_color: [u8; 3],
    pub border_alpha: u8,
    pub corner_radius: u8,
    pub auto_hide_when_idle: bool,
    pub show_lyrics: bool,
    pub show_todo: bool,
    pub show_calendar: bool,
    pub google_calendar_ics_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scale: 1.0,
            idle_width: 140.0,
            expanded_width: 390.0,
            top_offset_px: 10.0,
            collapse_timeout_secs: 5,
            bg_color: [16, 16, 20, 240],
            accent_color: [110, 195, 255],
            text_color: [245, 245, 248],
            border_alpha: 36,
            corner_radius: 22,
            auto_hide_when_idle: false,
            show_lyrics: true,
            show_todo: true,
            show_calendar: true,
            google_calendar_ics_url: String::new(),
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "liebert-dynamic-island")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        Self::from_toml_str(&raw)
    }

    pub fn from_toml_str(raw: &str) -> Self {
        let mut cfg: Self = toml::from_str(raw).unwrap_or_default();
        cfg.scale = cfg.scale.clamp(0.75, 1.5);
        cfg.idle_width = cfg.idle_width.clamp(110.0, 240.0);
        cfg.expanded_width = cfg.expanded_width.clamp(320.0, 520.0);
        cfg.top_offset_px = cfg.top_offset_px.clamp(0.0, 60.0);
        cfg.collapse_timeout_secs = cfg.collapse_timeout_secs.clamp(2, 15);
        cfg.corner_radius = cfg.corner_radius.clamp(8, 28);
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, encoded)
    }

    pub fn apply_preset(&mut self, name: &str) {
        match name {
            "obsidian" => {
                self.bg_color = [16, 16, 20, 240];
                self.accent_color = [110, 195, 255];
                self.text_color = [245, 245, 248];
                self.border_alpha = 36;
            }
            "midnight" => {
                self.bg_color = [15, 23, 42, 235];
                self.accent_color = [96, 165, 250];
                self.text_color = [241, 245, 249];
                self.border_alpha = 48;
            }
            "amoled" => {
                self.bg_color = [0, 0, 0, 252];
                self.accent_color = [52, 211, 153];
                self.text_color = [255, 255, 255];
                self.border_alpha = 28;
            }
            "slate" => {
                self.bg_color = [36, 40, 48, 236];
                self.accent_color = [244, 162, 97];
                self.text_color = [248, 250, 252];
                self.border_alpha = 52;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let cfg = Config::default();
        assert!(!cfg.auto_hide_when_idle);
        assert_eq!(cfg.scale, 1.0);
        assert_eq!(cfg.top_offset_px, 10.0);
        assert_eq!(cfg.collapse_timeout_secs, 5);
        assert!(cfg.show_lyrics && cfg.show_todo && cfg.show_calendar);
    }

    #[test]
    fn toml_round_trip() {
        let mut cfg = Config {
            auto_hide_when_idle: true,
            top_offset_px: 14.5,
            collapse_timeout_secs: 8,
            google_calendar_ics_url: "https://calendar.google.com/calendar/ical/test/basic.ics"
                .to_owned(),
            ..Config::default()
        };
        cfg.apply_preset("amoled");
        let raw = toml::to_string_pretty(&cfg).expect("serialize config");
        let parsed = Config::from_toml_str(&raw);
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn legacy_partial_toml_loads_with_new_defaults() {
        let legacy = "auto_hide_when_idle = true\ntop_offset_px = 18.0\ncollapse_timeout_secs = 7\n";
        let parsed = Config::from_toml_str(legacy);
        assert!(parsed.auto_hide_when_idle);
        assert_eq!(parsed.top_offset_px, 18.0);
        assert_eq!(parsed.collapse_timeout_secs, 7);
        assert_eq!(parsed.scale, 1.0);
        assert_eq!(parsed.bg_color, Config::default().bg_color);
    }

    #[test]
    fn invalid_toml_falls_back_to_default() {
        let parsed = Config::from_toml_str("not valid = [toml");
        assert_eq!(parsed, Config::default());
    }
}
