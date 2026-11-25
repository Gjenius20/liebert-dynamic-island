use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub auto_hide_when_idle: bool,
    pub top_offset_px: f32,
    pub collapse_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_hide_when_idle: false,
            top_offset_px: 10.0,
            collapse_timeout_secs: 5,
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
        toml::from_str(raw).unwrap_or_default()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let cfg = Config::default();
        assert!(!cfg.auto_hide_when_idle);
        assert_eq!(cfg.top_offset_px, 10.0);
        assert_eq!(cfg.collapse_timeout_secs, 5);
    }

    #[test]
    fn toml_round_trip() {
        let cfg = Config {
            auto_hide_when_idle: true,
            top_offset_px: 14.5,
            collapse_timeout_secs: 8,
        };
        let raw = toml::to_string_pretty(&cfg).expect("serialize config");
        let parsed = Config::from_toml_str(&raw);
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn invalid_toml_falls_back_to_default() {
        let parsed = Config::from_toml_str("not valid = [toml");
        assert_eq!(parsed, Config::default());
    }
}
