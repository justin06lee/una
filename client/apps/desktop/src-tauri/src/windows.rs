//! HUD and settings window management.
//!
//! The HUD is a fixed-size, frameless, transparent, always-on-top,
//! click-through window anchored bottom-center of the display that holds the
//! cursor. All visual state changes (idle lozenge -> recording pill -> …)
//! are pure CSS inside the webview, so the window itself never resizes.
//!
//! Two modes, from `[ui] hud_mode`:
//! - "pill" (default): the window is always visible; while idle it renders a
//!   tiny dim lozenge, Wispr Flow style.
//! - "flash": the previous behavior — hidden while idle, shown by the FSM's
//!   ShowHud/HideHud effects.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use una_core::state::Snapshot;

use crate::app_state::AppState;

pub const HUD_LABEL: &str = "hud";
pub const SETTINGS_LABEL: &str = "settings";

/// Fixed outer window size; the visual pill (max ~360x44) floats inside.
pub const HUD_WIDTH: f64 = 400.0;
pub const HUD_HEIGHT: f64 = 72.0;
/// Logical gap between the window and the bottom edge of the screen. The
/// remaining visual offset (~8px) is CSS padding inside the webview.
const HUD_BOTTOM_MARGIN: f64 = 0.0;

/// Create the HUD window. `pill` decides initial visibility.
pub fn create_hud(app: &AppHandle, pill: bool) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, HUD_LABEL, WebviewUrl::App("hud.html".into()))
        .title("una")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .visible_on_all_workspaces(true)
        .accept_first_mouse(true)
        .inner_size(HUD_WIDTH, HUD_HEIGHT)
        .build()?;
    let _ = window.set_ignore_cursor_events(true);

    // The pill sits at the very bottom edge, over the Dock area: raise it
    // above the Dock's window level or the Dock would cover it.
    #[cfg(target_os = "macos")]
    if let Ok(ptr) = window.ns_window() {
        unsafe { una_platform::macos::raise_window_above_dock(ptr) };
    }

    position_hud(app, &window);
    if pill {
        let _ = window.show();
    }
    Ok(window)
}

pub fn create_settings(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window =
        WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
            .title("Una Settings")
            .inner_size(760.0, 520.0)
            .min_inner_size(640.0, 420.0)
            .visible(false)
            .build()?;

    // Hide instead of destroy on close, so the window can be reopened
    // instantly from the tray.
    let win = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = win.hide();
        }
    });
    Ok(window)
}

fn hud_is_pill(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .map(|s| s.config.read().unwrap().ui.hud_mode != "flash")
        .unwrap_or(true)
}

/// FSM ShowHud effect: move to the display containing the cursor, then show
/// (without stealing focus). In pill mode the window is already visible; the
/// reposition still runs so the pill follows the cursor's display whenever a
/// recording starts.
pub fn show_hud(app: &AppHandle) {
    let Some(window) = app.get_webview_window(HUD_LABEL) else {
        return;
    };
    position_hud(app, &window);
    let _ = window.show();
}

/// FSM HideHud effect: a no-op in pill mode (the pill just shrinks back to
/// its idle lozenge via CSS), hides the window in flash mode.
pub fn hide_hud(app: &AppHandle) {
    if hud_is_pill(app) {
        return;
    }
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        let _ = window.hide();
    }
}

/// Live-apply a hud_mode config change without restart.
pub fn apply_hud_mode(app: &AppHandle, pill: bool) {
    let Some(window) = app.get_webview_window(HUD_LABEL) else {
        return;
    };
    if pill {
        position_hud(app, &window);
        let _ = window.show();
    } else {
        // Only hide immediately when idle; mid-dictation the FSM's next
        // HideHud effect takes care of it.
        let idle = app
            .try_state::<AppState>()
            .map(|s| matches!(*s.last_snapshot.lock().unwrap(), Snapshot::Idle))
            .unwrap_or(true);
        if idle {
            let _ = window.hide();
        }
    }
}

/// Toggle click-through: the HUD accepts the cursor only in the Error state
/// (for the Retry button).
pub fn set_hud_interactive(app: &AppHandle, interactive: bool) {
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        let _ = window.set_ignore_cursor_events(!interactive);
    }
}

/// Bottom-center on the monitor containing the cursor, correct for scaled
/// and multi-display setups (monitor position/size are physical pixels).
fn position_hud(app: &AppHandle, window: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|pos| app.monitor_from_point(pos.x, pos.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };

    let scale = monitor.scale_factor();
    let mpos = monitor.position();
    let msize = monitor.size();
    let w = HUD_WIDTH * scale;
    let h = HUD_HEIGHT * scale;
    let x = mpos.x as f64 + (msize.width as f64 - w) / 2.0;
    let y = mpos.y as f64 + msize.height as f64 - h - HUD_BOTTOM_MARGIN * scale;
    let _ = window.set_position(tauri::PhysicalPosition::new(
        x.round() as i32,
        y.round() as i32,
    ));
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
