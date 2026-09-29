//! The dictation history: the server's dashboard, in a window of una's own
//! rather than a browser tab.
//!
//! The dashboard is served by the una server, so the window loads it straight
//! from whichever server address is live — nothing is bundled, and it is
//! always the version the server has. Two things make it sit in the app:
//!
//! - an init script tells the page it's in una's window, so it makes room for
//!   the macOS traffic lights (the title bar is an overlay, like the other
//!   windows) and marks a strip at the top to drag the window by;
//! - a capability added at runtime lets that server's page, and only that
//!   page, ask for the one thing a drag needs.
//!
//! With no server answering, the window shows a small local page that says so
//! and can try again. Like the other windows it hides rather than closes, and
//! every open from the tray starts again at Home, with fresh data.

use std::collections::BTreeSet;
use std::sync::Mutex;

use tauri::ipc::CapabilityBuilder;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::app_state::AppState;
use crate::windows::SETTINGS_LABEL;

pub const HISTORY_LABEL: &str = "history";

/// Read by the dashboard before its own scripts run (see its App.svelte).
const IN_APP_SCRIPT: &str = if cfg!(target_os = "macos") {
    "window.__UNA_APP__ = { platform: 'macos' };"
} else {
    "window.__UNA_APP__ = { platform: 'other' };"
};

/// Servers whose page has already been allowed to drag the window.
static DRAGGABLE: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Open the history on whichever server answers, or say that none does.
pub fn open(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let base = match handle.try_state::<AppState>() {
            Some(state) => state.live_server().await.ok(),
            None => None,
        };
        let main = handle.clone();
        let _ = handle.run_on_main_thread(move || show(&main, base));
    });
}

fn show(app: &AppHandle, base: Option<String>) {
    let Some(url) = target(app, base.as_deref()) else {
        return;
    };
    if let Some(base) = &base {
        allow_dragging(app, base);
    }
    if let Some(window) = app.get_webview_window(HISTORY_LABEL) {
        // Already up on this server: leave it where the user is. Otherwise
        // start over at Home, which also picks up anything new.
        let same_server = window
            .url()
            .map(|u| u.origin() == url.origin())
            .unwrap_or(false);
        if !(window.is_visible().unwrap_or(false) && same_server) {
            let _ = window.navigate(url);
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    match create(app, url) {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => tracing::warn!("could not open the history window: {e}"),
    }
}

/// The dashboard's Home on `base`, or the local "can't reach it" page.
fn target(app: &AppHandle, base: Option<&str>) -> Option<tauri::Url> {
    match base {
        Some(base) => format!("{}/#/home", base.trim_end_matches('/')).parse().ok(),
        // Wherever the app's own pages are served from (tauri://localhost in
        // a build, the dev server in development), as the settings page knows.
        None => app
            .get_webview_window(SETTINGS_LABEL)?
            .url()
            .ok()?
            .join("history.html")
            .ok(),
    }
}

fn create(app: &AppHandle, url: tauri::Url) -> tauri::Result<tauri::WebviewWindow> {
    let builder = WebviewWindowBuilder::new(app, HISTORY_LABEL, WebviewUrl::External(url))
        .title("Dictation history")
        .inner_size(1120.0, 760.0)
        .min_inner_size(760.0, 520.0)
        .initialization_script(IN_APP_SCRIPT)
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
            if let Some(window) = handle.get_webview_window(HISTORY_LABEL) {
                let _ = window.hide();
            }
        }
    });
    Ok(window)
}

/// Let the dashboard on `base` drag the window by its top strip, and nothing
/// else: a remote page gets no IPC at all unless a capability names it.
fn allow_dragging(app: &AppHandle, base: &str) {
    let origin = base.trim_end_matches('/').to_string();
    let mut allowed = DRAGGABLE.lock().unwrap();
    if allowed.contains(&origin) {
        return;
    }
    let capability = CapabilityBuilder::new(format!("history-drag-{}", allowed.len()))
        .remote(origin.clone())
        .window(HISTORY_LABEL)
        .permission("core:window:allow-start-dragging");
    match app.add_capability(capability) {
        Ok(()) => {
            allowed.insert(origin);
        }
        Err(e) => tracing::warn!("history window can't be dragged from {origin}: {e}"),
    }
}
