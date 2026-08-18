//! TOML configuration, stored at the platform config dir for
//! `ProjectDirs("sh", "tenet", "una")` (e.g. `~/Library/Application
//! Support/sh.tenet.una/config.toml` on macOS, `~/.config/una/config.toml` on
//! Linux).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub hotkey: HotkeyConfig,
    pub audio: AudioConfig,
    pub insert: InsertConfig,
    pub ui: UiConfig,
    pub general: GeneralConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            hotkey: HotkeyConfig::default(),
            audio: AudioConfig::default(),
            insert: InsertConfig::default(),
            ui: UiConfig::default(),
            general: GeneralConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ServerConfig {
    /// Manual server base URL, e.g. `http://192.168.1.20:8765`. Empty means
    /// "not configured" — autodiscovery is used instead when enabled.
    pub url: String,
    pub autodiscover: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            autodiscover: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HotkeyConfig {
    pub binding: String,
    /// "hold" | "toggle" | "hybrid"
    pub mode: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            binding: "Ctrl+Alt+Space".into(),
            mode: "hybrid".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioConfig {
    /// "auto" or an exact input device name.
    pub input_device: String,
    pub prefer_builtin: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: "auto".into(),
            prefer_builtin: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct InsertConfig {
    pub restore_clipboard: bool,
    pub restore_delay_ms: u64,
    /// Per-app paste chord overrides (mostly Linux terminals).
    pub paste_overrides: BTreeMap<String, String>,
}

impl Default for InsertConfig {
    fn default() -> Self {
        let mut paste_overrides = BTreeMap::new();
        for term in ["kitty", "alacritty", "foot", "gnome-terminal", "konsole"] {
            paste_overrides.insert(term.to_string(), "ctrl+shift+v".to_string());
        }
        Self {
            restore_clipboard: true,
            restore_delay_ms: 300,
            paste_overrides,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiConfig {
    pub sounds: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { sounds: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GeneralConfig {
    pub launch_at_login: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            launch_at_login: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not determine a config directory for this platform")]
    NoProjectDirs,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid config file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("could not serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub fn project_dirs() -> Result<directories::ProjectDirs, ConfigError> {
    directories::ProjectDirs::from("sh", "tenet", "una").ok_or(ConfigError::NoProjectDirs)
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    Ok(project_dirs()?.config_dir().join("config.toml"))
}

/// Load the config file, creating it with defaults when missing.
pub fn load_or_create() -> Result<Config, ConfigError> {
    let path = config_path()?;
    load_or_create_at(&path)
}

pub fn load_or_create_at(path: &std::path::Path) -> Result<Config, ConfigError> {
    if path.exists() {
        let raw = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&raw)?)
    } else {
        let cfg = Config::default();
        save_at(&cfg, path)?;
        Ok(cfg)
    }
}

pub fn save(cfg: &Config) -> Result<(), ConfigError> {
    let path = config_path()?;
    save_at(cfg, &path)
}

pub fn save_at(cfg: &Config, path: &std::path::Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(cfg)?;
    std::fs::write(path, raw)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let created = load_or_create_at(&path).unwrap();
        assert_eq!(created, Config::default());
        assert!(path.exists());
        let loaded = load_or_create_at(&path).unwrap();
        assert_eq!(loaded, created);
        assert_eq!(loaded.hotkey.binding, "Ctrl+Alt+Space");
        assert_eq!(
            loaded.insert.paste_overrides.get("kitty").unwrap(),
            "ctrl+shift+v"
        );
    }

    #[test]
    fn partial_file_gets_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[server]\nurl = \"http://x:1\"\n").unwrap();
        let cfg = load_or_create_at(&path).unwrap();
        assert_eq!(cfg.server.url, "http://x:1");
        assert!(cfg.server.autodiscover);
        assert_eq!(cfg.hotkey.mode, "hybrid");
        assert_eq!(cfg.insert.restore_delay_ms, 300);
    }
}
