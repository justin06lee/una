//! Shared app state managed by tauri.

use std::sync::{Arc, Mutex, RwLock};

use una_core::audio::AudioEngine;
use una_core::config::Config;
use una_core::endpoint::EndpointResolver;
use una_core::net::ApiClient;
use una_core::state::{ControllerHandle, Snapshot};

use crate::corrections::{Pending, PendingSlot};

pub struct AppState {
    pub controller: ControllerHandle,
    pub engine: Arc<AudioEngine>,
    pub api: ApiClient,
    pub config: Arc<RwLock<Config>>,
    /// Shared endpoint chooser, so the settings window and the dictation path
    /// agree on which server address is currently live.
    pub endpoints: Arc<EndpointResolver>,
    /// Tray "Start/Stop Dictation" item, for live relabeling.
    pub tray_toggle: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    pub last_snapshot: Arc<Mutex<Snapshot>>,
    /// The dictation whose text is about to be pasted: (id, text). Set when
    /// the upload returns, consumed by the insert effect, which is the only
    /// place that knows the paste actually landed.
    pub last_dictation: Mutex<Option<(String, String)>>,
    /// The correction the window is currently editing.
    pub pending_correction: PendingSlot,
    /// The most recent paste, kept after its watch window closes so "Fix Last
    /// Dictation…" can still file a correction against it minutes later.
    pub recent_paste: Mutex<Option<Pending>>,
}

impl AppState {
    pub fn config_snapshot(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    /// Base URL for opening the dashboard: whichever endpoint is currently
    /// live, else the first one configured.
    pub fn dashboard_url(&self) -> Option<String> {
        self.endpoints
            .cached()
            .or_else(|| self.config.read().unwrap().server.urls.first().cloned())
    }
}
