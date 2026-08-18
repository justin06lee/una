//! Shared app state managed by tauri.

use std::sync::{Arc, Mutex, RwLock};

use una_core::audio::AudioEngine;
use una_core::config::Config;
use una_core::net::ApiClient;
use una_core::state::{ControllerHandle, Snapshot};

pub struct AppState {
    pub controller: ControllerHandle,
    pub engine: Arc<AudioEngine>,
    pub api: ApiClient,
    pub config: Arc<RwLock<Config>>,
    /// Tray "Start/Stop Dictation" item, for live relabeling.
    pub tray_toggle: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    pub last_snapshot: Arc<Mutex<Snapshot>>,
}

impl AppState {
    pub fn config_snapshot(&self) -> Config {
        self.config.read().unwrap().clone()
    }
}
