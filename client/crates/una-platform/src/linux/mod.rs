//! Linux implementation.
//!
//! NOTE: this module is written for Linux targets and cannot be
//! compile-verified from the macOS development machine. It is deliberately
//! kept simple; uncertain pieces are structured stubs returning
//! `Unavailable` with TODO notes.
//!
//! Injection strategy:
//! - X11: arboard clipboard + XTest Ctrl+V (or a per-app override chord).
//! - Wayland: arboard clipboard + `ydotool` (if its socket exists), else
//!   `wtype` (if in PATH), else clipboard-only.

pub mod detect;
pub mod wayland;
pub mod x11;

use std::time::Duration;

use crate::{
    FrontmostApp, InjectError, InjectOptions, InjectOutcome, InjectProbe, PermissionState,
    Permissions, TextInjector,
};

pub use detect::SessionKind;

/// Parsed paste chord: Ctrl+V or Ctrl+Shift+V.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteChord {
    pub shift: bool,
}

impl PasteChord {
    pub fn parse(combo: Option<&str>) -> Self {
        match combo.map(|c| c.to_ascii_lowercase()) {
            Some(c) if c.contains("shift") => Self { shift: true },
            _ => Self { shift: false },
        }
    }
}

fn set_clipboard(text: &str) -> Result<(), InjectError> {
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| InjectError::Clipboard(format!("clipboard unavailable: {e}")))?;
    cb.set_text(text.to_string())
        .map_err(|e| InjectError::Clipboard(format!("could not set clipboard: {e}")))?;
    Ok(())
}

fn get_clipboard() -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    cb.get_text().ok()
}

pub struct LinuxInjector {
    session: SessionKind,
}

impl LinuxInjector {
    pub fn new() -> Self {
        Self {
            session: detect::session_kind(),
        }
    }
}

impl Default for LinuxInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl TextInjector for LinuxInjector {
    fn probe(&self) -> InjectProbe {
        match self.session {
            SessionKind::X11 => InjectProbe {
                backend: "xtest".into(),
                can_paste: true,
                detail: "X11 session: clipboard + XTest Ctrl+V.".into(),
            },
            SessionKind::Wayland => {
                if wayland::ydotool_available() {
                    InjectProbe {
                        backend: "ydotool".into(),
                        can_paste: true,
                        detail: "Wayland session: clipboard + ydotool paste chord.".into(),
                    }
                } else if wayland::wtype_available() {
                    InjectProbe {
                        backend: "wtype".into(),
                        can_paste: true,
                        detail: "Wayland session: clipboard + wtype paste chord. \
                                 Note: wtype does not work on GNOME."
                            .into(),
                    }
                } else {
                    InjectProbe {
                        backend: "clipboard-only".into(),
                        can_paste: false,
                        detail: "Wayland session without ydotool or wtype: text is \
                                 copied to the clipboard; paste manually with Ctrl+V. \
                                 Install ydotool (recommended) or wtype to enable \
                                 automatic pasting — see docs/linux.md."
                            .into(),
                    }
                }
            }
            SessionKind::Unknown => InjectProbe {
                backend: "clipboard-only".into(),
                can_paste: false,
                detail: "No graphical session detected (neither WAYLAND_DISPLAY nor \
                         DISPLAY set)."
                    .into(),
            },
        }
    }

    fn inject(&self, text: &str, opts: &InjectOptions) -> Result<InjectOutcome, InjectError> {
        let chord = PasteChord::parse(opts.paste_combo.as_deref());
        // arboard restores text-only content; non-text clipboard data is not
        // preserved on Linux.
        let saved = if opts.restore_clipboard {
            get_clipboard()
        } else {
            None
        };
        set_clipboard(text)?;
        std::thread::sleep(Duration::from_millis(50));

        let outcome = match self.session {
            SessionKind::X11 => {
                x11::paste(chord)?;
                InjectOutcome::Injected
            }
            SessionKind::Wayland => {
                if wayland::ydotool_available() {
                    wayland::paste_ydotool(chord)?;
                    InjectOutcome::Injected
                } else if wayland::wtype_available() {
                    wayland::paste_wtype(chord)?;
                    InjectOutcome::Injected
                } else {
                    InjectOutcome::ClipboardOnly
                }
            }
            SessionKind::Unknown => InjectOutcome::ClipboardOnly,
        };

        if outcome == InjectOutcome::Injected {
            if let Some(saved) = saved {
                std::thread::sleep(opts.restore_delay);
                let _ = set_clipboard(&saved);
            }
        }
        Ok(outcome)
    }
}

pub struct LinuxFrontmost;

impl FrontmostApp for LinuxFrontmost {
    fn current(&self) -> Option<String> {
        match detect::session_kind() {
            SessionKind::X11 => x11::frontmost_app(),
            SessionKind::Wayland => wayland::frontmost_app(),
            // GNOME/KDE Wayland expose no focused-window query; the server
            // accepts a missing app_name.
            SessionKind::Unknown => None,
        }
    }
}

pub struct LinuxPermissions;

impl Permissions for LinuxPermissions {
    fn mic(&self) -> PermissionState {
        // No TCC equivalent; ALSA/PipeWire access is governed by group
        // membership and portal policies we cannot reliably query.
        PermissionState::Granted
    }

    fn accessibility(&self) -> PermissionState {
        match detect::session_kind() {
            SessionKind::X11 => PermissionState::Granted,
            SessionKind::Wayland => {
                if wayland::ydotool_available() || wayland::wtype_available() {
                    PermissionState::Granted
                } else {
                    // Not denied per se — the injection helper just isn't set
                    // up yet.
                    PermissionState::Unknown
                }
            }
            SessionKind::Unknown => PermissionState::Unknown,
        }
    }

    fn prompt_accessibility(&self) {
        // Nothing to prompt on Linux; setup is documented in docs/linux.md.
    }
}
