//! HUD and settings window management.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const HUD_LABEL: &str = "hud";
pub const SETTINGS_LABEL: &str = "settings";

pub const HUD_WIDTH: f64 = 340.0;
pub const HUD_HEIGHT: f64 = 96.0;
/// Logical gap between the HUD and the bottom edge of the screen.
const HUD_BOTTOM_MARGIN: f64 = 84.0;

/// The HUD: a small, frameless, transparent, always-on-top, click-through
/// capsule that lives near the bottom of whichever monitor holds the cursor.
pub fn create_hud(app: &AppHandle) -> tauri::Result<WebviewWindow> {
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
    Ok(window)
}

pub fn create_settings(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(
        app,
        SETTINGS_LABEL,
        WebviewUrl::App("settings.html".into()),
    )
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

/// Position the HUD bottom-center on the monitor containing the cursor, then
/// show it (without stealing focus).
pub fn show_hud(app: &AppHandle) {
    let Some(window) = app.get_webview_window(HUD_LABEL) else { return };
    position_hud(app, &window);
    let _ = window.show();
}

pub fn hide_hud(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        let _ = window.hide();
    }
}

/// Toggle click-through: the HUD accepts the cursor only in the Error state
/// (for the Retry button).
pub fn set_hud_interactive(app: &AppHandle, interactive: bool) {
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        let _ = window.set_ignore_cursor_events(!interactive);
    }
}

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
    let _ = window.set_position(tauri::PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
