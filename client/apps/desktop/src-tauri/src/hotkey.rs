//! Hotkey backend selection and (re)registration.
//!
//! Two backends, chosen by the binding's form (see `una_core::config::
//! Binding`):
//!
//! - Combo strings ("Ctrl+Alt+Space") go to the tauri global-shortcut
//!   plugin; its handler is installed at build time in main.rs.
//! - "native:<keycode>:<Name>" bindings go to the key tap
//!   (`una_platform::hotkeys`: the CGEventTap on macOS, XInput2 raw keys on
//!   X11), which matches raw keycodes and so supports single keys —
//!   including bare modifiers like Fn, Right ⌘ or Right Ctrl — as
//!   push-to-talk keys. Keycodes are the platform's own. Where there is no
//!   tap (Wayland), native bindings are disabled with a notice (the settings
//!   UI points at compositor keybinds + `una` CLI).
//!
//! [`apply`] tears down whatever was registered before applying the new
//! binding, so live rebinds from the settings window need no restart.

use std::str::FromStr;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use una_core::config::Binding;

use std::sync::{Arc, Mutex};

use tauri::Manager;
use una_core::state::Command;
use una_platform::hotkeys::{HotkeyEdge, KeyTap};

/// The lazily-created, process-wide key tap. Created on first use — a native
/// binding or a capture request — never before, so users on combo bindings
/// pay nothing.
static KEY_TAP: Mutex<Option<Arc<KeyTap>>> = Mutex::new(None);

/// Parse a combo string like "Ctrl+Alt+Space" into a plugin Shortcut.
pub fn parse_combo(combo: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(combo).map_err(|e| format!("invalid hotkey {combo:?}: {e}"))
}

/// Validate a binding string of either form without applying it.
pub fn validate_binding(binding: &str) -> Result<(), String> {
    match Binding::parse(binding).map_err(|e| e.to_string())? {
        Binding::Combo(combo) => parse_combo(&combo).map(|_| ()),
        Binding::Native { keycode, .. } => {
            if keycode > u16::MAX as u32 {
                return Err(format!("native keycode {keycode} out of range"));
            }
            Ok(())
        }
    }
}

/// The binding as it should be stored. A native binding on a keycode that
/// is really another key is rewritten to that key: Apple keyboards send the
/// 🌐 key as keycode 179 alongside Fn, and earlier builds recorded it as
/// "Key 179".
pub fn canonical_binding(binding: &str) -> String {
    #[cfg(target_os = "macos")]
    if let Ok(Binding::Native { keycode, .. }) = Binding::parse(binding) {
        use una_platform::macos::keys;
        if let Ok(kc) = u16::try_from(keycode) {
            let canonical = keys::canonical(kc);
            if canonical != kc {
                return format!("native:{canonical}:{}", keys::key_name(canonical));
            }
        }
    }
    binding.to_string()
}

/// Whether single keys can be bound here: always on macOS, on Linux only in
/// an X11 session (Wayland shows apps no keys but their own).
pub fn native_supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        una_platform::linux::detect::session_kind() == una_platform::linux::SessionKind::X11
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Get or create the shared key tap, wiring key edges to the controller.
pub fn ensure_tap(app: &AppHandle) -> Result<Arc<KeyTap>, String> {
    let mut guard = KEY_TAP.lock().unwrap();
    if let Some(tap) = guard.as_ref() {
        return Ok(tap.clone());
    }
    let state = app
        .try_state::<crate::app_state::AppState>()
        .ok_or_else(|| "app state not ready".to_string())?;
    let controller = state.controller.clone();
    let tap = KeyTap::spawn(Box::new(move |edge| {
        controller.command(match edge {
            HotkeyEdge::Down => Command::HotkeyDown,
            HotkeyEdge::Up => Command::HotkeyUp,
            HotkeyEdge::Shortcut => Command::HotkeyAbort,
        });
    }))?;
    let tap = Arc::new(tap);
    *guard = Some(tap.clone());
    Ok(tap)
}

/// Abort a pending capture, if any.
pub fn cancel_capture() {
    if let Some(tap) = KEY_TAP.lock().unwrap().as_ref() {
        tap.cancel_capture();
    }
}

/// Tear down the previous backend and apply `binding` live.
pub fn apply(app: &AppHandle, binding: &str) -> Result<(), String> {
    let parsed = Binding::parse(binding).map_err(|e| e.to_string())?;
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    if let Some(tap) = KEY_TAP.lock().unwrap().as_ref() {
        tap.set_binding(None);
    }

    match parsed {
        Binding::Combo(combo) => {
            let shortcut = parse_combo(&combo)?;
            gs.register(shortcut).map_err(|e| e.to_string())?;
            tracing::info!("registered global hotkey {combo}");
        }
        Binding::Native { keycode, name } => {
            if native_supported() {
                let tap = ensure_tap(app)?;
                tap.set_binding(Some(keycode as u16));
                tracing::info!("armed native hotkey {name} (keycode {keycode})");
            } else {
                tracing::warn!(
                    "native hotkey binding {name:?} (keycode {keycode}) is not supported in \
                     this session; the hotkey is disabled. Use a key combo instead, or bind \
                     a compositor shortcut to run `una toggle` / `una start` / `una stop`."
                );
            }
        }
    }
    Ok(())
}
