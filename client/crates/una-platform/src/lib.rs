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
        Self {
            restore_clipboard: true,
            restore_delay: Duration::from_millis(300),
            paste_combo: None,
        }
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

// ---------------------------------------------------------------------------
// Correction capture
//
// Noticing that the user edited what una pasted needs three things the rest
// of the platform surface doesn't: reading the focused field's text, handing
// focus back to the app the text came from, and (where the text can't be
// read) clearing a span by keystroke. Only macOS implements them today —
// Linux has no equivalent of the accessibility text API that works across
// X11 and Wayland toolkits, so the whole feature is inert there and the
// caller falls back to doing nothing.
// ---------------------------------------------------------------------------

/// Whether this platform can observe edits to pasted text at all.
pub const fn supports_correction_capture() -> bool {
    cfg!(target_os = "macos")
}

/// Whether the focused control publishes text the accessibility API can read
/// — the difference between capturing an edit silently and having to ask.
pub fn focused_text_readable() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::axtext::focused_field_is_readable()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Process id of the frontmost application, for handing focus back later.
pub fn frontmost_pid() -> Option<i32> {
    #[cfg(target_os = "macos")]
    {
        macos::frontmost_pid()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Bring a process's application back to the front.
pub fn activate_pid(pid: i32) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::activate_pid(pid)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        false
    }
}

/// Post `n` backspaces to the focused app.
pub fn send_backspaces(n: usize) -> Result<(), InjectError> {
    #[cfg(target_os = "macos")]
    {
        macos::send_backspaces(n)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = n;
        Err(InjectError::Unavailable(
            "backspace injection is not implemented on this platform".into(),
        ))
    }
}
