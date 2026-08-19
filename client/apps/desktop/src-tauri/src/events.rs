//! Forward controller snapshots and audio levels to the webviews.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use una_core::state::Snapshot;

use crate::app_state::AppState;
use crate::{tray, windows};

#[derive(Serialize, Clone)]
struct LevelPayload {
    rms: f32,
    peak: f32,
}

pub fn spawn_forwarders(app: AppHandle) {
    let state = app.state::<AppState>();
    let mut snapshots = state.controller.subscribe();
    let mut levels = state.engine.levels();
    let last_snapshot = state.last_snapshot.clone();
    let config = state.config.clone();

    // State changes -> "state-changed" + tray label + HUD interactivity.
    let state_app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match snapshots.recv().await {
                Ok(snapshot) => {
                    let entered_done = {
                        let mut last = last_snapshot.lock().unwrap();
                        let was_done = matches!(*last, Snapshot::Done { .. });
                        let was_error = matches!(*last, Snapshot::Error { .. });
                        *last = snapshot.clone();
                        match snapshot {
                            Snapshot::Done { .. } if !was_done => Some(crate::sounds::Cue::Done),
                            Snapshot::Error { .. } if !was_error => Some(crate::sounds::Cue::Error),
                            _ => None,
                        }
                    };
                    if let Some(cue) = entered_done {
                        if config.read().unwrap().ui.sounds {
                            crate::sounds::play(cue);
                        }
                    }
                    let recording = matches!(snapshot, Snapshot::Recording { .. });
                    tray::set_recording(&state_app, recording);
                    // The HUD is click-through except when showing an error
                    // (so the Retry button works).
                    let interactive = matches!(snapshot, Snapshot::Error { .. });
                    windows::set_hud_interactive(&state_app, interactive);
                    let _ = state_app.emit("state-changed", &snapshot);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Audio levels -> "audio-level" (already ~30Hz from the meter).
    let level_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while levels.changed().await.is_ok() {
            let frame = *levels.borrow_and_update();
            let _ = level_app.emit(
                "audio-level",
                LevelPayload {
                    rms: frame.rms,
                    peak: frame.peak,
                },
            );
        }
    });
}
