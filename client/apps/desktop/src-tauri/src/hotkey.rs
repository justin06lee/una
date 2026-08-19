//! Hotkey backend selection and (re)registration.
//!
//! Two backends, chosen by the binding's form (see `una_core::config::
//! Binding`):
//!
//! - Combo strings ("Ctrl+Alt+Space") go to the tauri global-shortcut
//!   plugin; its handler is installed at build time in main.rs.
//! - "native:<keycode>:<Name>" bindings go to the macOS CGEventTap backend
//!   (`una_platform::macos::eventtap`), which matches raw keycodes and so
//!   supports single keys — including bare modifiers like Fn or Right ⌘ —
//!   as push-to-talk keys. On Linux, native bindings are disabled with a
//!   notice (the settings UI points at compositor keybinds + `una` CLI).
//!
//! [`apply`] tears down whatever was registered before applying the new
//! binding, so live rebinds from the settings window need no restart.

use std::str::FromStr;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use una_core::config::Binding;

#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
use tauri::Manager;
#[cfg(target_os = "macos")]
use una_core::state::Command;
#[cfg(target_os = "macos")]
use una_platform::macos::eventtap::EventTap;

/// The lazily-created, process-wide event tap (macOS only). Created on first
/// use — a native binding or a capture request — never before, so users on
/// combo bindings pay nothing.
#[cfg(target_os = "macos")]
static EVENT_TAP: Mutex<Option<Arc<EventTap>>> = Mutex::new(None);

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

/// Get or create the shared event tap, wiring key edges to the controller.
#[cfg(target_os = "macos")]
pub fn ensure_tap(app: &AppHandle) -> Result<Arc<EventTap>, String> {
    let mut guard = EVENT_TAP.lock().unwrap();
    if let Some(tap) = guard.as_ref() {
        return Ok(tap.clone());
    }
    let state = app
        .try_state::<crate::app_state::AppState>()
        .ok_or_else(|| "app state not ready".to_string())?;
    let controller = state.controller.clone();
    let tap = EventTap::spawn(Box::new(move |down| {
        controller.command(if down {
            Command::HotkeyDown
        } else {
            Command::HotkeyUp
        });
    }))?;
    let tap = Arc::new(tap);
    *guard = Some(tap.clone());
    Ok(tap)
}

/// Abort a pending capture, if any.
#[cfg(target_os = "macos")]
pub fn cancel_capture() {
    if let Some(tap) = EVENT_TAP.lock().unwrap().as_ref() {
        tap.cancel_capture();
    }
}

/// Tear down the previous backend and apply `binding` live.
pub fn apply(app: &AppHandle, binding: &str) -> Result<(), String> {
    let parsed = Binding::parse(binding).map_err(|e| e.to_string())?;
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    if let Some(tap) = EVENT_TAP.lock().unwrap().as_ref() {
        tap.set_binding(None);
    }

    match parsed {
        Binding::Combo(combo) => {
            let shortcut = parse_combo(&combo)?;
            gs.register(shortcut).map_err(|e| e.to_string())?;
            tracing::info!("registered global hotkey {combo}");
        }
        Binding::Native { keycode, name } => {
            #[cfg(target_os = "macos")]
            {
                let tap = ensure_tap(app)?;
                tap.set_binding(Some(keycode as u16));
                tracing::info!("armed native event-tap hotkey {name} (keycode {keycode})");
            }
            #[cfg(not(target_os = "macos"))]
            {
                tracing::warn!(
                    "native hotkey binding {name:?} (keycode {keycode}) is not supported on \
                     this platform; the hotkey is disabled. Use a key combo instead, or bind \
                     a compositor shortcut to run `una toggle` / `una start` / `una stop`."
                );
            }
        }
    }
    Ok(())
}
