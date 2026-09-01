//! The controller's effect runner: bridges FSM effects to the audio engine,
//! HTTP uploads, text injection, and the HUD window.

use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use una_core::audio::AudioEngine;
use una_core::config::Config;
use una_core::endpoint::EndpointResolver;
use una_core::net::{ApiClient, DictationRequest};
use una_core::state::{ControllerHandle, EffectRunner, ErrKind, Event};

use crate::app_state::AppState;
use crate::corrections;
use crate::windows;

pub struct TauriEffects {
    app: AppHandle,
    engine: Arc<AudioEngine>,
    api: ApiClient,
    config: Arc<RwLock<Config>>,
    endpoints: Arc<EndpointResolver>,
}

impl TauriEffects {
    pub fn new(
        app: AppHandle,
        engine: Arc<AudioEngine>,
        api: ApiClient,
        config: Arc<RwLock<Config>>,
        endpoints: Arc<EndpointResolver>,
    ) -> Self {
        Self {
            app,
            engine,
            api,
            config,
            endpoints,
        }
    }

    fn controller(&self) -> Option<ControllerHandle> {
        self.app
            .try_state::<AppState>()
            .map(|s| s.controller.clone())
    }
}

impl EffectRunner for TauriEffects {
    fn show_hud(&mut self) {
        windows::show_hud(&self.app);
    }

    fn hide_hud(&mut self) {
        windows::hide_hud(&self.app);
    }

    fn start_recording(&mut self, session: u64) {
        self.engine.start(session);
    }

    fn stop_recording(&mut self, session: u64) {
        self.engine.stop(session);
    }

    fn cancel_recording(&mut self, session: u64) {
        self.engine.cancel(session);
    }

    fn upload(&mut self, session: u64, wav: Vec<u8>, _duration: Duration) {
        let Some(controller) = self.controller() else {
            return;
        };
        let api = self.api.clone();
        let config = self.config.clone();
        let endpoints = self.endpoints.clone();
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let (urls, autodiscover) = {
                let cfg = config.read().unwrap();
                (cfg.server.urls.clone(), cfg.server.autodiscover)
            };

            let Some(base) = endpoints.resolve(&urls, autodiscover).await else {
                let _ = una_core::spool::save(&wav);
                controller.event(Event::UploadErr {
                    session,
                    kind: ErrKind::Connect,
                    message: no_server_message(&urls, autodiscover),
                    retryable: true,
                    at: Instant::now(),
                });
                return;
            };

            let app_name =
                tauri::async_runtime::spawn_blocking(|| una_platform::frontmost().current())
                    .await
                    .ok()
                    .flatten();

            let request = DictationRequest::new(wav, app_name);
            let mut result = api.dictate(&base, &request).await;

            // A transport failure usually means the cached endpoint is no
            // longer the right one — the laptop moved between the home LAN and
            // a remote network. Re-probe and, if that turns up a *different*
            // endpoint, send the same utterance there. The server dedupes on
            // utterance_id, so a retry can never produce a second dictation.
            if matches!(&result, Err(e) if e.retryable()) {
                endpoints.invalidate();
                if let Some(next) = endpoints.resolve(&urls, autodiscover).await {
                    if next != base {
                        tracing::info!("endpoint moved {base} -> {next}; retrying dictation");
                        result = api.dictate(&next, &request).await;
                    }
                }
            }

            match result {
                Ok(resp) => {
                    // A retried dictation just landed; drop its spool entry.
                    match una_core::spool::remove_latest_if_matches(&request.wav) {
                        Ok(true) => tracing::debug!("removed spooled retry after success"),
                        Ok(false) => {}
                        Err(e) => tracing::warn!("spool cleanup failed: {e}"),
                    }
                    // The insert effect is the only place that learns the
                    // paste actually landed, but only the response carries the
                    // dictation id a correction has to be filed against.
                    if let Some(id) = resp.id.clone() {
                        if let Some(state) = app.try_state::<AppState>() {
                            *state.last_dictation.lock().unwrap() = Some((id, resp.text.clone()));
                        }
                    }
                    controller.event(Event::UploadOk {
                        session,
                        text: resp.text,
                    });
                }
                Err(err) => {
                    tracing::warn!("dictation upload failed: {err}");
                    if matches!(err.kind(), ErrKind::Connect | ErrKind::Timeout) {
                        endpoints.invalidate();
                    }
                    if let Err(e) = una_core::spool::save(&request.wav) {
                        tracing::warn!("could not spool failed dictation: {e}");
                    }
                    controller.event(Event::UploadErr {
                        session,
                        kind: err.kind(),
                        message: err.to_string(),
                        retryable: err.retryable(),
                        at: Instant::now(),
                    });
                }
            }
        });
    }

    fn inject(&mut self, session: u64, text: String) {
        let Some(controller) = self.controller() else {
            return;
        };
        let config = self.config.clone();
        let app = self.app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let (restore_clipboard, restore_delay_ms, overrides) = {
                let cfg = config.read().unwrap();
                (
                    cfg.insert.restore_clipboard,
                    cfg.insert.restore_delay_ms,
                    cfg.insert.paste_overrides.clone(),
                )
            };
            // Per-app paste chord override (Linux terminals).
            let paste_combo = una_platform::frontmost().current().and_then(|app| {
                let lower = app.to_lowercase();
                overrides
                    .iter()
                    .find(|(name, _)| lower.contains(&name.to_lowercase()))
                    .map(|(_, combo)| combo.clone())
            });
            let opts = una_platform::InjectOptions {
                restore_clipboard,
                restore_delay: Duration::from_millis(restore_delay_ms),
                paste_combo,
            };
            match injector().inject(&text, &opts) {
                Ok(_outcome) => {
                    // The text is in front of the user now: start watching for
                    // the edits that make it a training pair.
                    if let Some(id) = dictation_id_for(&app, &text) {
                        corrections::on_inserted(&app, id, text.clone());
                    }
                    controller.event(Event::InsertOk {
                        session,
                        at: Instant::now(),
                    });
                }
                Err(e) => {
                    controller.event(Event::InsertErr {
                        session,
                        message: e.to_string(),
                        at: Instant::now(),
                    });
                }
            }
        });
    }
}

/// The id of the dictation that produced `text`, consuming the record so a
/// retry or a manual paste of the same string can't file a second correction.
///
/// Matching on the text (rather than threading the id through the state
/// machine) keeps the FSM free of server concepts, and is exact: this is the
/// same String the upload handed over.
fn dictation_id_for(app: &AppHandle, text: &str) -> Option<String> {
    let state = app.try_state::<AppState>()?;
    let mut slot = state.last_dictation.lock().unwrap();
    match slot.as_ref() {
        Some((_, pasted)) if pasted == text => slot.take().map(|(id, _)| id),
        _ => None,
    }
}

/// Cached injector (probing/setup can be repeated cheaply, but there is no
/// reason to rebuild it per dictation).
fn injector() -> &'static dyn una_platform::TextInjector {
    static INJECTOR: OnceLock<Box<dyn una_platform::TextInjector>> = OnceLock::new();
    INJECTOR.get_or_init(una_platform::injector).as_ref()
}

/// What to tell the user when no configured endpoint answered.
fn no_server_message(urls: &[String], autodiscover: bool) -> String {
    if urls.is_empty() && !autodiscover {
        "No una server configured — add one in Settings".into()
    } else if urls.is_empty() {
        "No una server found on this network".into()
    } else if urls.len() == 1 {
        "Can't reach your una server".into()
    } else {
        format!("Can't reach your una server (tried {} addresses)", urls.len())
    }
}
