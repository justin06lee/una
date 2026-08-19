//! The controller's effect runner: bridges FSM effects to the audio engine,
//! HTTP uploads, text injection, and the HUD window.

use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use una_core::audio::AudioEngine;
use una_core::config::Config;
use una_core::net::{ApiClient, DictationRequest};
use una_core::state::{ControllerHandle, EffectRunner, ErrKind, Event};

use crate::app_state::AppState;
use crate::windows;

pub struct TauriEffects {
    app: AppHandle,
    engine: Arc<AudioEngine>,
    api: ApiClient,
    config: Arc<RwLock<Config>>,
}

impl TauriEffects {
    pub fn new(
        app: AppHandle,
        engine: Arc<AudioEngine>,
        api: ApiClient,
        config: Arc<RwLock<Config>>,
    ) -> Self {
        Self {
            app,
            engine,
            api,
            config,
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
        tauri::async_runtime::spawn(async move {
            let (manual_url, autodiscover) = {
                let cfg = config.read().unwrap();
                (cfg.server.url.trim().to_string(), cfg.server.autodiscover)
            };

            // Resolve the server: manual URL always wins; otherwise the cached
            // mDNS result, browsing only when the cache is empty (a 2s browse
            // per dictation is real latency).
            let base = if !manual_url.is_empty() {
                Some(manual_url)
            } else if autodiscover {
                match discovered_url() {
                    Some(url) => Some(url),
                    None => {
                        let found = tauri::async_runtime::spawn_blocking(|| {
                            una_core::discovery::discover(Duration::from_secs(2))
                        })
                        .await
                        .ok()
                        .and_then(|list| list.into_iter().next())
                        .map(|s| s.url);
                        if let Some(url) = &found {
                            set_discovered_url(Some(url.clone()));
                        }
                        found
                    }
                }
            } else {
                None
            };

            let Some(base) = base else {
                let _ = una_core::spool::save(&wav);
                controller.event(Event::UploadErr {
                    session,
                    kind: ErrKind::Connect,
                    message: "No una server configured or discovered".into(),
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
            match api.dictate(&base, &request).await {
                Ok(resp) => {
                    // A retried dictation just landed; drop its spool entry.
                    match una_core::spool::remove_latest_if_matches(&request.wav) {
                        Ok(true) => tracing::debug!("removed spooled retry after success"),
                        Ok(false) => {}
                        Err(e) => tracing::warn!("spool cleanup failed: {e}"),
                    }
                    controller.event(Event::UploadOk {
                        session,
                        text: resp.text,
                    });
                }
                Err(err) => {
                    tracing::warn!("dictation upload failed: {err}");
                    if matches!(err.kind(), ErrKind::Connect | ErrKind::Timeout) {
                        // The cached server may have moved; re-browse next time.
                        set_discovered_url(None);
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

/// Cached injector (probing/setup can be repeated cheaply, but there is no
/// reason to rebuild it per dictation).
fn injector() -> &'static dyn una_platform::TextInjector {
    static INJECTOR: OnceLock<Box<dyn una_platform::TextInjector>> = OnceLock::new();
    INJECTOR.get_or_init(una_platform::injector).as_ref()
}

/// Last mDNS-discovered server URL, invalidated when a connection fails.
fn discovery_cache() -> &'static Mutex<Option<String>> {
    static CACHE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn discovered_url() -> Option<String> {
    discovery_cache().lock().unwrap().clone()
}

fn set_discovered_url(url: Option<String>) {
    *discovery_cache().lock().unwrap() = url;
}
