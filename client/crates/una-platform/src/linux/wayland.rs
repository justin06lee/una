//! Wayland paste helpers: ydotool (preferred) or wtype.
//!
//! Unverified on this development machine (written on macOS).

use std::path::PathBuf;
use std::process::Command;

use super::PasteChord;
use crate::InjectError;

// Linux input event codes (input-event-codes.h).
const KEY_LEFTCTRL: u32 = 29;
const KEY_LEFTSHIFT: u32 = 42;
const KEY_V: u32 = 47;

/// Where ydotoold's socket may live.
fn ydotool_socket_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(explicit) = std::env::var("YDOTOOL_SOCKET") {
        if !explicit.is_empty() {
            candidates.push(PathBuf::from(explicit));
        }
    }
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        if !runtime.is_empty() {
            candidates.push(PathBuf::from(&runtime).join(".ydotool_socket"));
        }
    }
    candidates.push(PathBuf::from("/tmp/.ydotool_socket"));
    candidates
}

pub fn ydotool_socket() -> Option<PathBuf> {
    ydotool_socket_candidates().into_iter().find(|p| p.exists())
}

pub fn ydotool_available() -> bool {
    ydotool_socket().is_some() && which("ydotool")
}

pub fn wtype_available() -> bool {
    which("wtype")
}

fn which(bin: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(bin).is_file())
}

/// Name of the focused application on compositors that expose it.
///
/// Hyprland: `hyprctl activewindow -j` -> "class". Sway (and compatible
/// wlroots compositors with SWAYSOCK): `swaymsg -t get_tree` -> the focused
/// node's app_id (native Wayland) or window_properties.class (XWayland).
/// GNOME/KDE expose nothing queryable; returns None there.
pub fn frontmost_app() -> Option<String> {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok_and(|v| !v.is_empty()) {
        let out = Command::new("hyprctl")
            .args(["activewindow", "-j"])
            .output()
            .ok()?;
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        return json["class"].as_str().filter(|s| !s.is_empty()).map(String::from);
    }
    if std::env::var("SWAYSOCK").is_ok_and(|v| !v.is_empty()) {
        let out = Command::new("swaymsg")
            .args(["-t", "get_tree"])
            .output()
            .ok()?;
        let tree: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        return find_focused(&tree);
    }
    None
}

fn find_focused(node: &serde_json::Value) -> Option<String> {
    if node["focused"].as_bool() == Some(true) {
        return node["app_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| node["window_properties"]["class"].as_str())
            .map(String::from);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for child in children {
                if let Some(found) = find_focused(child) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// `ydotool key 29:1 47:1 47:0 29:0` (Ctrl+V), with Shift interleaved for
/// Ctrl+Shift+V.
pub fn paste_ydotool(chord: PasteChord) -> Result<(), InjectError> {
    let mut args: Vec<String> = vec!["key".into()];
    let press = |k: u32| format!("{k}:1");
    let release = |k: u32| format!("{k}:0");
    args.push(press(KEY_LEFTCTRL));
    if chord.shift {
        args.push(press(KEY_LEFTSHIFT));
    }
    args.push(press(KEY_V));
    args.push(release(KEY_V));
    if chord.shift {
        args.push(release(KEY_LEFTSHIFT));
    }
    args.push(release(KEY_LEFTCTRL));

    let mut cmd = Command::new("ydotool");
    if let Some(socket) = ydotool_socket() {
        cmd.env("YDOTOOL_SOCKET", socket);
    }
    let status = cmd
        .args(&args)
        .status()
        .map_err(|e| InjectError::Keystroke(format!("could not run ydotool: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(InjectError::Keystroke(format!(
            "ydotool exited with {status}"
        )))
    }
}

/// `wtype -M ctrl -k v -m ctrl` (with shift for Ctrl+Shift+V).
pub fn paste_wtype(chord: PasteChord) -> Result<(), InjectError> {
    let mut args: Vec<&str> = Vec::new();
    args.extend(["-M", "ctrl"]);
    if chord.shift {
        args.extend(["-M", "shift"]);
    }
    args.extend(["-k", "v"]);
    if chord.shift {
        args.extend(["-m", "shift"]);
    }
    args.extend(["-m", "ctrl"]);

    let status = Command::new("wtype")
        .args(&args)
        .status()
        .map_err(|e| InjectError::Keystroke(format!("could not run wtype: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(InjectError::Keystroke(format!(
            "wtype exited with {status}"
        )))
    }
}
