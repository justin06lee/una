//! una-platform: OS-specific text injection, frontmost-app lookup, and
//! permission checks, behind small traits so una-core and the app stay
//! platform-agnostic.

use std::time::Duration;

use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Granted,
    Denied,
    /// Not yet requested.
    Undetermined,
    /// Cannot be determined on this platform/backend.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectOutcome {
    /// Text was pasted into the focused app.
    Injected,
    /// Text was only placed on the clipboard (no paste possible — e.g.
    /// secure input active, or no Wayland injection backend).
    ClipboardOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectProbe {
    /// Name of the backend that would be used ("cgevent", "xtest",
    /// "ydotool", "wtype", "clipboard-only").
    pub backend: String,
    /// Whether a real paste (not just clipboard) is currently possible.
    pub can_paste: bool,
    /// Human-readable detail / fix-it guidance.
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct InjectOptions {
    pub restore_clipboard: bool,
    pub restore_delay: Duration,
    /// Paste chord: "ctrl+v", "ctrl+shift+v" (Linux). macOS always uses
    /// Cmd+V and ignores this.
    pub paste_combo: Option<String>,
}

impl Default for InjectOptions {
    fn default() -> Self {
        Self { restore_clipboard: true, restore_delay: Duration::from_millis(300), paste_combo: None }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("paste keystroke failed: {0}")]
    Keystroke(String),
    #[error("injection unavailable: {0}")]
    Unavailable(String),
}

/// Injects text into the focused application (clipboard + paste chord).
pub trait TextInjector: Send + Sync {
    fn probe(&self) -> InjectProbe;
    fn inject(&self, text: &str, opts: &InjectOptions) -> Result<InjectOutcome, InjectError>;
}

/// Reports the frontmost (focused) application's name.
pub trait FrontmostApp: Send + Sync {
    fn current(&self) -> Option<String>;
}

/// OS permission checks.
pub trait Permissions: Send + Sync {
    fn mic(&self) -> PermissionState;
    /// Request mic access if undetermined (fire-and-forget).
    fn request_mic(&self) {}
    fn accessibility(&self) -> PermissionState;
    /// Show the OS accessibility-permission prompt.
    fn prompt_accessibility(&self);
}

// ---------------------------------------------------------------------------
// Factories
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub fn injector() -> Box<dyn TextInjector> {
    Box::new(macos::MacInjector::new())
}
#[cfg(target_os = "macos")]
pub fn frontmost() -> Box<dyn FrontmostApp> {
    Box::new(macos::MacFrontmost)
}
#[cfg(target_os = "macos")]
pub fn permissions() -> Box<dyn Permissions> {
    Box::new(macos::MacPermissions)
}

#[cfg(target_os = "linux")]
pub fn injector() -> Box<dyn TextInjector> {
    Box::new(linux::LinuxInjector::new())
}
#[cfg(target_os = "linux")]
pub fn frontmost() -> Box<dyn FrontmostApp> {
    Box::new(linux::LinuxFrontmost)
}
#[cfg(target_os = "linux")]
pub fn permissions() -> Box<dyn Permissions> {
    Box::new(linux::LinuxPermissions)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod unsupported {
    use super::*;

    pub struct Noop;

    impl TextInjector for Noop {
        fn probe(&self) -> InjectProbe {
            InjectProbe {
                backend: "none".into(),
                can_paste: false,
                detail: "unsupported platform".into(),
            }
        }
        fn inject(&self, _: &str, _: &InjectOptions) -> Result<InjectOutcome, InjectError> {
            Err(InjectError::Unavailable("unsupported platform".into()))
        }
    }
    impl FrontmostApp for Noop {
        fn current(&self) -> Option<String> {
            None
        }
    }
    impl Permissions for Noop {
        fn mic(&self) -> PermissionState {
            PermissionState::Unknown
        }
        fn accessibility(&self) -> PermissionState {
            PermissionState::Unknown
        }
        fn prompt_accessibility(&self) {}
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn injector() -> Box<dyn TextInjector> {
    Box::new(unsupported::Noop)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn frontmost() -> Box<dyn FrontmostApp> {
    Box::new(unsupported::Noop)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn permissions() -> Box<dyn Permissions> {
    Box::new(unsupported::Noop)
}
