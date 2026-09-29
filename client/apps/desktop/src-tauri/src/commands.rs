//! Tauri commands invoked by the settings, HUD, correction, and review webviews.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use una_core::audio::{AudioEngine, AudioSettings};
use una_core::config::Config;
use una_core::discovery::DiscoveredServer;
use una_core::state::{Command, HotkeyMode, Snapshot};

use crate::app_state::AppState;
use crate::corrections;
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
    if config.fixup.model.trim().is_empty() {
        return Err("pick a model for Fix up".into());
    }
    if !config.fixup.effort.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("invalid effort {:?}", config.fixup.effort));
    }
    for url in &config.server.urls {
        let url = url.trim();
        if url.is_empty() {
            continue;
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(format!("server address {url:?} must start with http:// or https://"));
        }
    }
    if config.insert.restore_delay_ms > 10_000 {
        return Err("restore delay must be at most 10000 ms".into());
    }

    let mut config = config;
    config.normalize();
    config.hotkey.binding = hotkey::canonical_binding(&config.hotkey.binding);

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
    if previous.server != config.server {
        // The candidate list changed; stop using whatever was resolved from it.
        state.endpoints.invalidate();
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
    // No explicit URL: report on whichever endpoint the client would actually
    // use right now, so the dot in Settings matches dictation behaviour.
    let base = match url.filter(|u| !u.trim().is_empty()) {
        Some(u) => u,
        None => {
            let cfg = state.config_snapshot();
            state
                .endpoints
                .resolve(&cfg.server.urls, cfg.server.autodiscover)
                .await
                .ok_or_else(|| {
                    if cfg.server.urls.is_empty() {
                        "no server configured".to_string()
                    } else {
                        "none of your server addresses answered".to_string()
                    }
                })?
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

#[derive(Serialize)]
pub struct EndpointStatus {
    url: String,
    reachable: bool,
    /// Round-trip time of the probe, for picking the better address.
    ms: u64,
}

/// Probe every configured address so the settings window can show which ones
/// work from where you are right now.
#[tauri::command]
pub async fn probe_endpoints(state: State<'_, AppState>) -> Result<Vec<EndpointStatus>, String> {
    let cfg = state.config_snapshot();
    let api = state.api.clone();
    let mut out = Vec::with_capacity(cfg.server.urls.len());
    // Sequential on purpose: a handful of addresses, and the per-endpoint
    // timings are only comparable when they don't contend for the uplink.
    for url in cfg.server.urls {
        let started = std::time::Instant::now();
        let reachable = api
            .reachable(&url, una_core::endpoint::PROBE_TIMEOUT)
            .await;
        out.push(EndpointStatus {
            url,
            reachable,
            ms: started.elapsed().as_millis() as u64,
        });
    }
    Ok(out)
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

/// What macOS itself does on a Fn / 🌐 press — System Settings > Keyboard >
/// "Press 🌐 key to": 0 Do Nothing, 1 Change Input Source, 2 Show Emoji &
/// Symbols, 3 Start Dictation. None when never set (or not macOS). macOS
/// acts on the key below any event tap, so una can't stop it; the settings
/// window warns when Fn is the hotkey and this isn't 0.
#[tauri::command]
pub async fn fn_key_action() -> Option<u8> {
    #[cfg(target_os = "macos")]
    {
        // `defaults` goes through cfprefsd, so a change made in System
        // Settings a moment ago is already visible.
        tauri::async_runtime::spawn_blocking(|| {
            let out = std::process::Command::new("/usr/bin/defaults")
                .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            String::from_utf8_lossy(&out.stdout).trim().parse().ok()
        })
        .await
        .ok()
        .flatten()
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// Open System Settings > Keyboard, where "Press 🌐 key to" lives.
#[tauri::command]
pub fn open_keyboard_settings(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_url(
            "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
            None::<&str>,
        )
        .map_err(|e| e.to_string())
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

// ---------------------------------------------------------------------------
// Correction window
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct PendingCorrection {
    /// The text una pasted, for the editor to open with.
    text: String,
    /// Where it went, so the window can say what it is about to rewrite.
    app_name: Option<String>,
    /// Whether submitting can put the corrected text back, or only record it.
    can_write_back: bool,
    /// What the ASR heard, before cleanup: the "What you said" field. None
    /// when neither memory nor the server has it, and the field is left out.
    raw_text: Option<String>,
}

/// What the correction window should be showing, if anything.
#[tauri::command]
pub async fn correction_pending(
    state: State<'_, AppState>,
) -> Result<Option<PendingCorrection>, String> {
    let Some((id, known_raw)) = state
        .pending_correction
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| (p.dictation_id.clone(), p.raw_text.clone()))
    else {
        return Ok(None);
    };
    let raw_text = match known_raw {
        Some(raw) => Some(raw),
        None => {
            let recent = state
                .recent_dictation
                .lock()
                .unwrap()
                .as_ref()
                .filter(|r| r.id == id)
                .map(|r| r.raw_text.clone());
            match recent {
                Some(raw) => raw,
                None => fetch_raw_text(&state, &id).await,
            }
        }
    };
    let mut slot = state.pending_correction.lock().unwrap();
    Ok(slot.as_mut().filter(|p| p.dictation_id == id).map(|p| {
        p.raw_text.clone_from(&raw_text);
        PendingCorrection {
            text: p.inserted.clone(),
            app_name: p.app_name.clone(),
            can_write_back: p.app_pid.is_some() && p.span_len.is_some(),
            raw_text,
        }
    }))
}

/// An older dictation's raw transcript, from the server.
async fn fetch_raw_text(state: &AppState, id: &str) -> Option<String> {
    let base = live_server(state).await.ok()?;
    let detail = state
        .api
        .get_json(&base, &format!("/v1/dictations/{id}"))
        .await
        .ok()?;
    detail.get("raw_text")?.as_str().map(str::to_string)
}

/// The recording behind the correction being edited (WAV bytes), so the user
/// can hear what they actually said. Straight from memory for the latest
/// dictation; from the server for an older one.
#[tauri::command]
pub async fn correction_audio(state: State<'_, AppState>) -> Result<tauri::ipc::Response, String> {
    let id = state
        .pending_correction
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.dictation_id.clone())
        .ok_or("nothing to correct")?;
    let cached = state
        .recent_dictation
        .lock()
        .unwrap()
        .as_ref()
        .filter(|r| r.id == id)
        .map(|r| r.wav.clone());
    if let Some(wav) = cached {
        return Ok(tauri::ipc::Response::new(wav.as_ref().clone()));
    }
    let base = live_server(&state).await?;
    let bytes = state
        .api
        .get_bytes(&base, &format!("/v1/dictations/{id}/audio"))
        .await
        .map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// File the user's fix as training pairs and put the written version back
/// into the app it came from.
///
/// `said` is the "What you said" field, present when the window showed the
/// raw transcript: then both targets are filed at once, the transcript for
/// the voice model and the wording for the style model. Without it, the fix
/// is to the pasted text only, a style target.
///
/// The pairs are recorded first: a write-back that fails (the app quit, the
/// caret moved) must not cost the training data, which is the point of the
/// whole exercise.
#[tauri::command]
pub async fn correction_submit(
    app: AppHandle,
    text: String,
    said: Option<String>,
) -> Result<bool, String> {
    let (pending, restore_clipboard) = {
        let state = app.state::<AppState>();
        let pending = state.pending_correction.lock().unwrap().take();
        let restore = state.config.read().unwrap().insert.restore_clipboard;
        (pending, restore)
    };
    let Some(pending) = pending else {
        return Err("nothing to correct".into());
    };
    windows::hide_correction(&app);

    let changed = text != pending.inserted;
    match (said, pending.raw_text.clone()) {
        (Some(said), Some(raw)) if !said.trim().is_empty() => {
            let said = (said.trim() != raw.trim()).then(|| said.trim().to_string());
            // An unchanged wording is the cleanup model's own output: filing
            // it as a style target would train the model on itself.
            let written = changed.then(|| text.trim().to_string());
            corrections::submit_with_transcript(&app, &pending.dictation_id, said, written).await;
        }
        _ => {
            let action = una_core::correction::classify(&pending.inserted, &text);
            corrections::submit_with_source(
                &app,
                &pending.dictation_id,
                action,
                text.clone(),
                "popup",
            )
            .await;
        }
    }
    let pid = pending.app_pid;
    if !changed {
        // Nothing to rewrite in the app; just go back to it.
        windows::return_focus(pid);
        return Ok(false);
    }
    let written = corrections::write_back(pending, text, restore_clipboard).await;
    if !written {
        // Writing back brings the app forward itself; otherwise do it here.
        windows::return_focus(pid);
    }
    Ok(written)
}

/// Close the window without recording anything.
#[tauri::command]
pub fn correction_dismiss(app: AppHandle, state: State<'_, AppState>) {
    let pid = state
        .pending_correction
        .lock()
        .unwrap()
        .take()
        .and_then(|p| p.app_pid);
    windows::hide_correction(&app);
    windows::return_focus(pid);
}

// ---------------------------------------------------------------------------
// Review window
// ---------------------------------------------------------------------------

async fn live_server(state: &AppState) -> Result<String, String> {
    state.live_server().await
}

/// Unreviewed dictations with the teacher's guesses, flagged ones first.
/// Passed through as the server's JSON — the window reads it directly.
#[tauri::command]
pub async fn review_queue(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<serde_json::Value, String> {
    let base = live_server(&state).await?;
    let path = format!("/v1/review/queue?limit={}", limit.unwrap_or(20).clamp(1, 100));
    state.api.get_json(&base, &path).await.map_err(|e| e.to_string())
}

/// A dictation's audio (WAV), as raw bytes: the webview turns it into a blob
/// for its player, so the server never has to be reachable from the webview.
#[tauri::command]
pub async fn review_audio(
    state: State<'_, AppState>,
    id: String,
) -> Result<tauri::ipc::Response, String> {
    let base = live_server(&state).await?;
    let bytes = state
        .api
        .get_bytes(&base, &format!("/v1/dictations/{id}/audio"))
        .await
        .map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// File the review of one dictation: the literal transcript's verdict and, when
/// given, how it should read.
#[tauri::command]
pub async fn review_submit(
    state: State<'_, AppState>,
    id: String,
    action: String,
    corrected_text: Option<String>,
    polished_text: Option<String>,
) -> Result<una_core::net::CorrectionResponse, String> {
    let base = live_server(&state).await?;
    let body = una_core::net::CorrectionRequest {
        action,
        corrected_text,
        polished_text,
        source: "review".into(),
        transcript_shown: true,
    };
    state
        .api
        .put_correction(&base, &id, &body)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn review_close(app: AppHandle) {
    windows::hide_review(&app);
}

// ---------------------------------------------------------------------------
// Fix up (yagami)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct FixedUp {
    text: String,
    /// The model that wrote it, as picked in settings.
    model: String,
}

/// Rewrite what was said as clean text, with the model picked in settings,
/// through yagami on this machine (started here if it isn't running).
/// Settings' "Try it" passes its own `model` and `effort`, since what's on
/// screen may not have been saved yet.
#[tauri::command]
pub async fn fixup_text(
    state: State<'_, AppState>,
    text: String,
    model: Option<String>,
    effort: Option<String>,
) -> Result<FixedUp, String> {
    if text.trim().is_empty() {
        return Err("Nothing to fix up yet".into());
    }
    let (model, effort) = {
        let cfg = state.config.read().unwrap();
        (
            model.unwrap_or_else(|| cfg.fixup.model.clone()),
            effort.unwrap_or_else(|| cfg.fixup.effort.clone()),
        )
    };
    let fixed = una_core::yagami::fix_up(&model, Some(&effort), &text)
        .await
        .map_err(|e| e.to_string())?;
    Ok(FixedUp { text: fixed, model })
}

/// The agent CLIs on this machine and the models they offer, for settings.
#[tauri::command]
pub async fn yagami_inventory() -> Result<una_core::yagami::Inventory, String> {
    let client = una_core::yagami::Yagami::connect()
        .await
        .map_err(|e| e.to_string())?;
    client.inventory().await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// History window
// ---------------------------------------------------------------------------

/// Try the server again, from the history window's "can't reach it" page.
#[tauri::command]
pub fn open_history(app: AppHandle) {
    crate::history::open(&app);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    windows::show_settings(&app);
}
