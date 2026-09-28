//! HUD, settings, correction, and review window management.
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
//!
//! Una is a regular Dock app: the settings window ("the Una window") opens on
//! launch and from the Dock icon, stays put behind other apps so ⌘-Tab gets
//! back to it, and hides rather than closes on the red button or ⌘W.
//!
//! The correction and review windows are the opposite of the HUD: they
//! deliberately take focus, because they exist to be typed into. The
//! correction window pops up over whatever the user was typing in, so it
//! hands focus back to that app when it goes away.

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use una_core::state::Snapshot;

use crate::app_state::AppState;

pub const HUD_LABEL: &str = "hud";
pub const SETTINGS_LABEL: &str = "settings";
pub const CORRECTION_LABEL: &str = "correction";
pub const REVIEW_LABEL: &str = "review";

/// The correction window is a small centered panel, sized for a sentence or
/// two of dictated text in both forms, the recording's player, and its buttons.
const CORRECTION_WIDTH: f64 = 560.0;
const CORRECTION_HEIGHT: f64 = 470.0;

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
        unsafe {
            una_platform::macos::raise_window_above_dock(ptr);
            una_platform::macos::keep_out_of_window_lists(ptr);
        }
    }

    position_hud(app, &window);
    if pill {
        let _ = window.show();
    }
    Ok(window)
}

pub fn create_settings(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let builder =
        WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
            .title("Una")
            .inner_size(780.0, 560.0)
            .min_inner_size(660.0, 440.0)
            .visible(false);
    // The sidebar runs to the top edge with the traffic lights drawn over it;
    // the page leaves room for them and marks a drag region.
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    let window = builder.build()?;

    // Hide instead of destroy on close (the red button and ⌘W), so it comes
    // back instantly from the Dock icon or the tray and Una keeps running.
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

/// Create the correction window: a small always-on-top panel that takes
/// focus, because its whole job is to be typed into.
///
/// Kept alive and hidden between uses like the settings window — recreating a
/// webview per correction would add a visible delay to something that has to
/// appear the instant an edit starts.
pub fn create_correction(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(
        app,
        CORRECTION_LABEL,
        WebviewUrl::App("correction.html".into()),
    )
    .title("Fix dictation")
    .inner_size(CORRECTION_WIDTH, CORRECTION_HEIGHT)
    .min_inner_size(420.0, 380.0)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .center();
    // Same chrome as settings: the page draws its own title strip.
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    let window = builder.build()?;

    // Closing the window is "never mind": drop the pending correction so a
    // later dictation can't be filed against it, and go back to the app it
    // came from.
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let pid = handle
                .try_state::<AppState>()
                .and_then(|s| s.pending_correction.lock().unwrap().take())
                .and_then(|p| p.app_pid);
            hide_correction(&handle);
            return_focus(pid);
        }
    });
    Ok(window)
}

/// Bring the correction window up in front of whatever the user is doing.
///
/// Called from the correction watcher's task, not the UI thread, so the
/// AppKit-touching parts are dispatched to the main thread explicitly.
pub fn show_correction(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(CORRECTION_LABEL) else {
            return;
        };
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("correction-opened", ());
    });
}

pub fn hide_correction(app: &AppHandle) {
    // Hidden, not destroyed: tell the page, or its playback would go on unseen.
    if let Some(window) = app.get_webview_window(CORRECTION_LABEL) {
        let _ = window.emit("correction-closed", ());
    }
    hide_window(app, CORRECTION_LABEL);
}

fn hide_window(app: &AppHandle, label: &'static str) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(label) {
            let _ = window.hide();
        }
    });
}

/// Put the user back in the app the correction window popped up over. Una
/// is a regular app now, so hiding its window alone would leave Una in front
/// with nothing showing.
pub fn return_focus(pid: Option<i32>) {
    if let Some(pid) = pid {
        std::thread::spawn(move || una_platform::activate_pid(pid));
    }
}

/// Create the review window: a regular window for going through recent
/// dictations with the teacher's guesses. Kept alive and hidden between uses.
pub fn create_review(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let builder =
        WebviewWindowBuilder::new(app, REVIEW_LABEL, WebviewUrl::App("review.html".into()))
            .title("Review dictations")
            .inner_size(640.0, 660.0)
            .min_inner_size(520.0, 520.0)
            .visible(false)
            .center();
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    let window = builder.build()?;

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            hide_review(&handle);
        }
    });
    Ok(window)
}

/// Bring up the review window, focused, and tell it to fetch the queue.
pub fn show_review(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(REVIEW_LABEL) else {
            return;
        };
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("review-opened", ());
    });
}

pub fn hide_review(app: &AppHandle) {
    // Hidden, not destroyed: tell the page, or its audio would play on unseen.
    if let Some(window) = app.get_webview_window(REVIEW_LABEL) {
        let _ = window.emit("review-closed", ());
    }
    hide_window(app, REVIEW_LABEL);
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The Dock icon was clicked. The always-on HUD pill counts as a visible
/// window to macOS, so decide here: bring forward a correction or review
/// window that is already up, otherwise open the Una window.
pub fn reopen(app: &AppHandle) {
    let up = [CORRECTION_LABEL, REVIEW_LABEL, SETTINGS_LABEL]
        .into_iter()
        .filter_map(|l| app.get_webview_window(l))
        .find(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    match up {
        Some(window) => {
            let _ = window.set_focus();
        }
        None => show_settings(app),
    }
}
