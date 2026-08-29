//! TOML configuration, stored at the platform config dir for
//! `ProjectDirs("sh", "tenet", "una")` (e.g. `~/Library/Application
//! Support/sh.tenet.una/config.toml` on macOS, `~/.config/una/config.toml` on
//! Linux).

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Hotkey binding representation
// ---------------------------------------------------------------------------

/// A parsed hotkey binding. The TOML file stores this as a plain string in
/// one of two forms:
///
/// - `"Ctrl+Alt+Space"` — a combo routed to the global-shortcut plugin.
/// - `"native:<keycode>:<Name>"` — a single physical key (including bare
///   modifiers like Fn or Right ⌘) captured by the native event-tap backend
///   on macOS. `<keycode>` is the platform virtual keycode; `<Name>` is the
///   human-readable label shown in the UI and may contain any characters
///   except a leading digit-colon ambiguity (it is everything after the
///   second colon).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    /// Modifier+key combo string understood by the global-shortcut plugin.
    Combo(String),
    /// Single physical key matched by keycode via the native event tap.
    Native { keycode: u32, name: String },
}

impl Binding {
    pub fn parse(s: &str) -> Result<Self, BindingParseError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(BindingParseError::Empty);
        }
        if let Some(rest) = s.strip_prefix("native:") {
            let mut parts = rest.splitn(2, ':');
            let code = parts.next().unwrap_or_default();
            let keycode: u32 = code
                .parse()
                .map_err(|_| BindingParseError::BadKeycode(code.to_string()))?;
            let name = parts.next().unwrap_or_default().trim().to_string();
            let name = if name.is_empty() {
                format!("Key {keycode}")
            } else {
                name
            };
            Ok(Self::Native { keycode, name })
        } else {
            Ok(Self::Combo(s.to_string()))
        }
    }

    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native { .. })
    }

    /// Human-facing label: the combo string itself, or the native key name.
    pub fn label(&self) -> &str {
        match self {
            Self::Combo(c) => c,
            Self::Native { name, .. } => name,
        }
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Combo(c) => f.write_str(c),
            Self::Native { keycode, name } => write!(f, "native:{keycode}:{name}"),
        }
    }
}

impl FromStr for Binding {
    type Err = BindingParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for Binding {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindingParseError {
    #[error("empty hotkey binding")]
    Empty,
    #[error("invalid native binding keycode {0:?} (expected native:<keycode>:<Name>)")]
    BadKeycode(String),
}

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

impl Config {
    /// Canonicalize anything a config file or the settings window may have set.
    pub fn normalize(&mut self) {
        self.server.normalize();
    }
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
    /// Candidate base URLs, e.g. `["http://192.168.1.20:8100",
    /// "http://box.tailnet.ts.net:8100"]`. Every dictation goes to whichever
    /// of these answers first, so one config works both on the home LAN and
    /// remotely over a VPN without the user switching anything.
    ///
    /// Empty means "not configured" — autodiscovery is used instead when
    /// enabled.
    pub urls: Vec<String>,
    /// Legacy single-URL form. Folded into `urls` by [`ServerConfig::normalize`]
    /// when an older config file is loaded, then never written back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub autodiscover: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            urls: Vec::new(),
            url: None,
            autodiscover: true,
        }
    }
}

impl ServerConfig {
    /// Fold a legacy `url` into `urls`, then trim, drop blanks and dedupe.
    ///
    /// Runs on load and on every save, so hand-edited files and anything the
    /// settings window sends converge on the same shape.
    pub fn normalize(&mut self) {
        if let Some(legacy) = self.url.take() {
            let legacy = legacy.trim().to_string();
            if !legacy.is_empty() {
                self.urls.insert(0, legacy);
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        self.urls = std::mem::take(&mut self.urls)
            .into_iter()
            .map(|u| u.trim().trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty())
            .filter(|u| seen.insert(u.clone()))
            .collect();
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
    /// "pill" (always-visible bottom pill, the default) or "flash"
    /// (show the HUD only while a dictation is in flight).
    pub hud_mode: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            sounds: false,
            hud_mode: "pill".into(),
        }
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
        let mut cfg: Config = toml::from_str(&raw)?;
        cfg.normalize();
        Ok(cfg)
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
    let mut cfg = cfg.clone();
    cfg.normalize();
    let raw = toml::to_string_pretty(&cfg)?;
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
        std::fs::write(&path, "[server]\nurls = [\"http://x:1\"]\n").unwrap();
        let cfg = load_or_create_at(&path).unwrap();
        assert_eq!(cfg.server.urls, vec!["http://x:1"]);
        assert!(cfg.server.autodiscover);
        assert_eq!(cfg.hotkey.mode, "hybrid");
        assert_eq!(cfg.insert.restore_delay_ms, 300);
        assert_eq!(cfg.ui.hud_mode, "pill");
    }

    #[test]
    fn legacy_single_url_is_migrated_to_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[server]\nurl = \"http://192.168.1.253:8100\"\n").unwrap();
        let cfg = load_or_create_at(&path).unwrap();
        assert_eq!(cfg.server.urls, vec!["http://192.168.1.253:8100"]);
        assert_eq!(cfg.server.url, None, "legacy field is consumed");
    }

    #[test]
    fn legacy_url_leads_the_list_and_is_not_duplicated() {
        let mut server = ServerConfig {
            urls: vec!["http://b:8100".into(), "http://a:8100".into()],
            url: Some("http://a:8100".into()),
            autodiscover: true,
        };
        server.normalize();
        // The legacy value keeps priority, and the duplicate further down goes.
        assert_eq!(server.urls, vec!["http://a:8100", "http://b:8100"]);
    }

    #[test]
    fn normalize_trims_blanks_and_trailing_slashes() {
        let mut server = ServerConfig {
            urls: vec![
                "  http://a:8100/  ".into(),
                "".into(),
                "   ".into(),
                "http://a:8100".into(),
            ],
            url: None,
            autodiscover: true,
        };
        server.normalize();
        assert_eq!(server.urls, vec!["http://a:8100"]);
    }

    #[test]
    fn saved_config_never_writes_the_legacy_field_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[server]\nurl = \"http://old:8100\"\n").unwrap();
        let cfg = load_or_create_at(&path).unwrap();
        save_at(&cfg, &path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("urls = [\"http://old:8100\"]"), "{raw}");
        assert!(!raw.contains("\nurl ="), "legacy key should be gone:\n{raw}");
        // And it round-trips unchanged.
        assert_eq!(load_or_create_at(&path).unwrap().server.urls, cfg.server.urls);
    }

    #[test]
    fn binding_parses_combo() {
        let b = Binding::parse("Ctrl+Alt+Space").unwrap();
        assert_eq!(b, Binding::Combo("Ctrl+Alt+Space".into()));
        assert!(!b.is_native());
        assert_eq!(b.to_string(), "Ctrl+Alt+Space");
        assert_eq!(b.label(), "Ctrl+Alt+Space");
    }

    #[test]
    fn binding_parses_native() {
        let b = Binding::parse("native:63:Fn").unwrap();
        assert_eq!(
            b,
            Binding::Native {
                keycode: 63,
                name: "Fn".into()
            }
        );
        assert!(b.is_native());
        assert_eq!(b.label(), "Fn");
    }

    #[test]
    fn binding_native_name_may_contain_colons_and_unicode() {
        let b = Binding::parse("native:54:Right ⌘").unwrap();
        assert_eq!(
            b,
            Binding::Native {
                keycode: 54,
                name: "Right ⌘".into()
            }
        );
        let b = Binding::parse("native:41:a:b").unwrap();
        assert_eq!(
            b,
            Binding::Native {
                keycode: 41,
                name: "a:b".into()
            }
        );
    }

    #[test]
    fn binding_native_missing_name_gets_placeholder() {
        let b = Binding::parse("native:96:").unwrap();
        assert_eq!(
            b,
            Binding::Native {
                keycode: 96,
                name: "Key 96".into()
            }
        );
        let b = Binding::parse("native:96").unwrap();
        assert_eq!(b.label(), "Key 96");
    }

    #[test]
    fn binding_display_roundtrips() {
        for s in ["Ctrl+Alt+Space", "F12", "native:63:Fn", "native:54:Right ⌘"] {
            let b = Binding::parse(s).unwrap();
            assert_eq!(Binding::parse(&b.to_string()).unwrap(), b);
        }
    }

    #[test]
    fn binding_rejects_garbage() {
        assert_eq!(Binding::parse(""), Err(BindingParseError::Empty));
        assert_eq!(Binding::parse("   "), Err(BindingParseError::Empty));
        assert!(matches!(
            Binding::parse("native:abc:X"),
            Err(BindingParseError::BadKeycode(_))
        ));
        assert!(matches!(
            Binding::parse("native:"),
            Err(BindingParseError::BadKeycode(_))
        ));
    }

    #[test]
    fn binding_serde_roundtrips() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Holder {
            binding: Binding,
        }
        for s in ["Ctrl+Alt+Space", "native:61:Right ⌥"] {
            let h = Holder {
                binding: Binding::parse(s).unwrap(),
            };
            let toml_str = toml::to_string(&h).unwrap();
            let back: Holder = toml::from_str(&toml_str).unwrap();
            assert_eq!(back, h);
        }
        // Deserializing a malformed native string is an error, not a panic.
        assert!(toml::from_str::<Holder>("binding = \"native:zz:Fn\"").is_err());
    }

    #[test]
    fn config_with_native_binding_roundtrips_as_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.hotkey.binding = "native:63:Fn".into();
        save_at(&cfg, &path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("binding = \"native:63:Fn\""));
        let loaded = load_or_create_at(&path).unwrap();
        assert_eq!(loaded.hotkey.binding, "native:63:Fn");
        assert_eq!(
            Binding::parse(&loaded.hotkey.binding).unwrap(),
            Binding::Native {
                keycode: 63,
                name: "Fn".into()
            }
        );
    }
}
