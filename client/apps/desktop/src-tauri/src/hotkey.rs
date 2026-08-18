//! Global hotkey registration. The handler itself is installed on the
//! global-shortcut plugin at build time (see main.rs); this module only
//! parses and (re)registers bindings.

use std::str::FromStr;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

/// Parse a user-facing binding like "Ctrl+Alt+Space" into a Shortcut.
pub fn parse_binding(binding: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(binding).map_err(|e| format!("invalid hotkey {binding:?}: {e}"))
}

/// Replace whatever is registered with `binding`.
pub fn register(app: &AppHandle, binding: &str) -> Result<(), String> {
    let shortcut = parse_binding(binding)?;
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    gs.register(shortcut).map_err(|e| e.to_string())?;
    tracing::info!("registered global hotkey {binding}");
    Ok(())
}
