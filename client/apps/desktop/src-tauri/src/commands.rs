//! Tauri commands invoked by the settings/HUD webviews.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, State};
use una_core::audio::{AudioEngine, AudioSettings};
use una_core::config::Config;
use una_core::discovery::DiscoveredServer;
use una_core::state::{Command, HotkeyMode, Snapshot};

use crate::app_state::AppState;
use crate::{hotkey, windows};

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.config_snapshot()
}

#[tauri::command]
pub fn set_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: Config,
) -> Result<(), String> {
    // Validate before persisting.
    hotkey::validate_binding(&config.hotkey.binding)?;
    if !matches!(config.hotkey.mode.as_str(), "hold" | "toggle" | "hybrid") {
        return Err(format!("invalid hotkey mode {:?}", config.hotkey.mode));
    }
    if !matches!(config.ui.hud_mode.as_str(), "pill" | "flash") {
        return Err(format!("invalid hud mode {:?}", config.ui.hud_mode));
    }
    let url = config.server.url.trim();
    if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("server URL must start with http:// or https://".into());
    }
    if config.insert.restore_delay_ms > 10_000 {
        return Err("restore delay must be at most 10000 ms".into());
    }

    let previous = state.config_snapshot();
    una_core::config::save(&config).map_err(|e| e.to_string())?;
    *state.config.write().unwrap() = config.clone();

    // Apply live changes.
    if previous.hotkey.binding != config.hotkey.binding {
        hotkey::apply(&app, &config.hotkey.binding)?;
    }
    if previous.ui.hud_mode != config.ui.hud_mode {
        windows::apply_hud_mode(&app, config.ui.hud_mode != "flash");
    }
    if previous.hotkey.mode != config.hotkey.mode {
        state
            .controller
            .command(Command::SetMode(HotkeyMode::parse(&config.hotkey.mode)));
    }
    if previous.audio != config.audio {
        state.engine.set_settings(AudioSettings {
            input_device: config.audio.input_device.clone(),
            prefer_builtin: config.audio.prefer_builtin,
        });
    }
    if previous.general.launch_at_login != config.general.launch_at_login {
        use tauri_plugin_autostart::ManagerExt as _;
        let autolaunch = app.autolaunch();
        let result = if config.general.launch_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if let Err(e) = result {
            tracing::warn!("could not apply launch-at-login: {e}");
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn discover_servers() -> Vec<DiscoveredServer> {
    tauri::async_runtime::spawn_blocking(|| una_core::discovery::discover(Duration::from_secs(2)))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn health_check(
    state: State<'_, AppState>,
    url: Option<String>,
) -> Result<serde_json::Value, String> {
    let base = match url.filter(|u| !u.trim().is_empty()) {
        Some(u) => u,
        None => {
            let configured = state.config_snapshot().server.url.trim().to_string();
            if configured.is_empty() {
                return Err("no server configured".into());
            }
            configured
        }
    };
    let health = state.api.health(&base).await.map_err(|e| e.to_string())?;
    serde_json::to_value(health).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct PermissionsStatus {
    mic: una_platform::PermissionState,
    accessibility: una_platform::PermissionState,
    secure_input: bool,
    probe: una_platform::InjectProbe,
}

#[tauri::command]
pub async fn permissions_status() -> Result<PermissionsStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let perms = una_platform::permissions();
        let probe = una_platform::injector().probe();
        #[cfg(target_os = "macos")]
        let secure_input = una_platform::macos::secure_input_active();
        #[cfg(not(target_os = "macos"))]
        let secure_input = false;
        PermissionsStatus {
            mic: perms.mic(),
            accessibility: perms.accessibility(),
            secure_input,
            probe,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn prompt_accessibility() {
    una_platform::permissions().prompt_accessibility();
}

#[tauri::command]
pub fn retry_last(state: State<'_, AppState>) -> Result<(), String> {
    match una_core::spool::latest() {
        Ok(Some(_)) => {
            state.controller.command(Command::RetryLast);
            Ok(())
        }
        Ok(None) => Err("nothing to retry: the spool is empty".into()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn audio_devices() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(AudioEngine::input_devices)
        .await
        .unwrap_or_default()
}

#[derive(Serialize)]
pub struct CapturedHotkey {
    /// Binding string to store: "native:<keycode>:<Name>" or a combo.
    pub binding: String,
    /// Human-readable key name ("Fn", "Right ⌘", "F5", "A", …).
    pub name: String,
    pub keycode: Option<u32>,
    pub is_modifier: bool,
    /// True when the binding uses the native event-tap backend (single key,
    /// consumed system-wide); false for combo bindings.
    pub is_native: bool,
}

/// Whether native single-key capture (the event-tap backend) exists here.
#[tauri::command]
pub fn hotkey_capture_supported() -> bool {
    cfg!(target_os = "macos")
}

/// Arm the event tap in one-shot mode and wait (up to 20s) for the next key
/// press. Bare modifiers resolve on release; a non-modifier key pressed with
/// modifiers held resolves to the combo form when it is expressible.
#[tauri::command]
pub async fn capture_hotkey(app: AppHandle) -> Result<CapturedHotkey, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_global_shortcut::GlobalShortcutExt as _;

        let tap = hotkey::ensure_tap(&app)?;
        // Suspend combo shortcuts so pressing the current hotkey mid-capture
        // records it instead of starting a dictation.
        let _ = app.global_shortcut().unregister_all();

        let result =
            tauri::async_runtime::spawn_blocking(move || tap.capture_next(Duration::from_secs(20)))
                .await
                .map_err(|e| e.to_string())?;

        // Restore the currently-configured binding; if the user adopts the
        // captured one, the subsequent set_config re-applies again.
        {
            use tauri::Manager as _;
            let current = app.state::<AppState>().config_snapshot().hotkey.binding;
            if let Err(e) = hotkey::apply(&app, &current) {
                tracing::warn!("could not re-apply hotkey after capture: {e}");
            }
        }

        let captured = result.map_err(|e| e.to_string())?;
        // Prefer the combo form when modifiers were held and the plugin can
        // express it; otherwise fall back to the native single-key form.
        let binding = match captured.combo.as_deref() {
            Some(combo) if hotkey::parse_combo(combo).is_ok() => combo.to_string(),
            _ => format!("native:{}:{}", captured.keycode, captured.name),
        };
        let is_native = binding.starts_with("native:");
        Ok(CapturedHotkey {
            binding,
            name: captured.name,
            keycode: Some(captured.keycode as u32),
            is_modifier: captured.is_modifier,
            is_native,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err(
            "Native key capture is only available on macOS. Type a key combo instead, or \
             bind a compositor shortcut to run `una toggle`."
                .into(),
        )
    }
}

/// Abort a pending capture_hotkey (it returns a \"cancelled\" error).
#[tauri::command]
pub fn cancel_hotkey_capture() {
    #[cfg(target_os = "macos")]
    hotkey::cancel_capture();
}

#[derive(Serialize)]
pub struct TestRecordResult {
    ok: bool,
    max_rms: f32,
    max_peak: f32,
}

/// Capture 2 seconds from the configured input and report peak levels.
/// The audio is discarded. Refused while a dictation is in flight.
#[tauri::command]
pub async fn test_record(state: State<'_, AppState>) -> Result<TestRecordResult, String> {
    const TEST_SESSION: u64 = u64::MAX;
    if !matches!(*state.last_snapshot.lock().unwrap(), Snapshot::Idle) {
        return Err("a dictation is in progress".into());
    }
    let engine = state.engine.clone();
    let mut levels = engine.levels();
    engine.start(TEST_SESSION);

    let mut max_rms: f32 = 0.0;
    let mut max_peak: f32 = 0.0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(120), levels.changed()).await {
            Ok(Ok(())) => {
                let frame = *levels.borrow_and_update();
                max_rms = max_rms.max(frame.rms);
                max_peak = max_peak.max(frame.peak);
            }
            _ => {}
        }
    }
    engine.cancel(TEST_SESSION);

    Ok(TestRecordResult {
        ok: max_peak > 0.02,
        max_rms,
        max_peak,
    })
}
