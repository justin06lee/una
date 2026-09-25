//! Menu-bar tray icon and menu.

use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;
use una_core::state::Command;

use crate::app_state::AppState;
use crate::windows;

const ID_TOGGLE: &str = "toggle-dictation";
const ID_RETRY: &str = "retry-last";
const ID_FIX: &str = "fix-last";
const ID_REVIEW: &str = "review";
const ID_SETTINGS: &str = "settings";
const ID_HISTORY: &str = "history";
const ID_LAUNCH: &str = "launch-at-login";
const ID_QUIT: &str = "quit";

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let launch_enabled = state.config_snapshot().general.launch_at_login;

    let toggle = MenuItem::with_id(app, ID_TOGGLE, "Start Dictation", true, None::<&str>)?;
    let retry = MenuItem::with_id(app, ID_RETRY, "Retry Last Dictation", true, None::<&str>)?;
    let fix = MenuItem::with_id(app, ID_FIX, "Fix Last Dictation…", true, None::<&str>)?;
    let review = MenuItem::with_id(app, ID_REVIEW, "Review Dictations…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, ID_SETTINGS, "Settings…", true, None::<&str>)?;
    let history = MenuItem::with_id(app, ID_HISTORY, "Dictation History…", true, None::<&str>)?;
    let launch = CheckMenuItem::with_id(
        app,
        ID_LAUNCH,
        "Launch at Login",
        true,
        launch_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, ID_QUIT, "Quit Una", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&toggle)
        .item(&retry)
        .item(&fix)
        .item(&review)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&settings)
        .item(&history)
        .item(&launch)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&quit)
        .build()?;

    *state.tray_toggle.lock().unwrap() = Some(toggle);

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id("una-tray")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Una — dictation")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
        .build(app)?;

    Ok(())
}

fn on_menu_event(app: &AppHandle, id: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    match id {
        ID_TOGGLE => state.controller.command(Command::Toggle),
        ID_RETRY => match una_core::spool::latest() {
            Ok(Some(_)) => state.controller.command(Command::RetryLast),
            _ => tracing::info!("retry requested but the spool is empty"),
        },
        ID_FIX => {
            if !crate::corrections::open_for_recent(app) {
                tracing::info!("fix requested but nothing has been dictated yet");
            }
        }
        ID_REVIEW => windows::show_review(app),
        ID_SETTINGS => windows::show_settings(app),
        ID_HISTORY => {
            match state.dashboard_url() {
                // No server configured: settings is the useful destination.
                None => windows::show_settings(app),
                Some(url) => {
                    let home = format!("{}/#/home", url.trim_end_matches('/'));
                    if let Err(e) = app.opener().open_url(home, None::<&str>) {
                        tracing::warn!("could not open the dashboard: {e}");
                    }
                }
            }
        }
        ID_LAUNCH => {
            let autolaunch = app.autolaunch();
            let currently = autolaunch.is_enabled().unwrap_or(false);
            let desired = !currently;
            let result = if desired {
                autolaunch.enable()
            } else {
                autolaunch.disable()
            };
            match result {
                Ok(()) => {
                    let mut cfg = state.config.write().unwrap();
                    cfg.general.launch_at_login = desired;
                    let cfg_clone = cfg.clone();
                    drop(cfg);
                    if let Err(e) = una_core::config::save(&cfg_clone) {
                        tracing::warn!("could not save config: {e}");
                    }
                }
                Err(e) => tracing::warn!("could not toggle launch-at-login: {e}"),
            }
        }
        ID_QUIT => app.exit(0),
        other => tracing::debug!("unhandled tray menu item: {other}"),
    }
}

/// Relabel the Start/Stop item to match the current state.
pub fn set_recording(app: &AppHandle, recording: bool) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let guard = state.tray_toggle.lock().unwrap();
    if let Some(item) = guard.as_ref() {
        let _ = item.set_text(if recording {
            "Stop Dictation"
        } else {
            "Start Dictation"
        });
    }
}
