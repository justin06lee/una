//! Una desktop app: menu-bar dictation client built on tauri v2.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod commands;
mod effects;
mod events;
mod hotkey;
mod tray;
mod windows;

use std::sync::{Arc, Mutex, OnceLock, RwLock};

use tauri::Manager;
use una_core::audio::{AudioEngine, AudioResult, AudioSettings};
use una_core::net::ApiClient;
use una_core::state::{Command, Controller, Event, HotkeyMode, Snapshot};

use app_state::AppState;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = una_core::config::load_or_create().unwrap_or_else(|e| {
        tracing::warn!("could not load config ({e}); using defaults");
        una_core::config::Config::default()
    });

    // The audio worker needs to report results into the controller, but the
    // controller is spawned later (it needs tauri's async runtime): bridge
    // through a OnceLock.
    let controller_cell: Arc<OnceLock<una_core::state::ControllerHandle>> =
        Arc::new(OnceLock::new());

    let audio_settings = AudioSettings {
        input_device: config.audio.input_device.clone(),
        prefer_builtin: config.audio.prefer_builtin,
    };
    let audio_cell = controller_cell.clone();
    let engine = Arc::new(AudioEngine::spawn(
        audio_settings,
        Box::new(move |result| {
            let Some(controller) = audio_cell.get() else {
                return;
            };
            match result {
                AudioResult::Finalized(f) => controller.event(Event::AudioFinalized {
                    session: f.session,
                    wav: f.wav,
                    duration: f.duration,
                }),
                AudioResult::Failed { session, message } => controller.event(Event::AudioFailed {
                    session,
                    message,
                    at: std::time::Instant::now(),
                }),
            }
        }),
    ));

    let mode = HotkeyMode::parse(&config.hotkey.mode);
    let config = Arc::new(RwLock::new(config));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch just brings up settings.
            windows::show_settings(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    let Some(state) = app.try_state::<AppState>() else {
                        return;
                    };
                    match event.state() {
                        tauri_plugin_global_shortcut::ShortcutState::Pressed => {
                            state.controller.command(Command::HotkeyDown);
                        }
                        tauri_plugin_global_shortcut::ShortcutState::Released => {
                            state.controller.command(Command::HotkeyUp);
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::discover_servers,
            commands::health_check,
            commands::permissions_status,
            commands::prompt_accessibility,
            commands::retry_last,
            commands::test_record,
            commands::audio_devices,
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();

            // Windows are created hidden up front.
            windows::create_hud(&handle)?;
            windows::create_settings(&handle)?;

            // Controller actor with the tauri effect runner.
            let runner = effects::TauriEffects::new(
                handle.clone(),
                engine.clone(),
                ApiClient::new(),
                config.clone(),
            );
            // Controller::spawn needs a tokio runtime context; setup runs on
            // the main thread, so enter tauri's runtime for the spawn.
            let controller = tauri::async_runtime::block_on(async {
                Controller::spawn(
                    mode,
                    Box::new(runner),
                    Box::new(|| {
                        let (_, wav) = una_core::spool::latest().ok().flatten()?;
                        let duration = una_core::audio::wav_duration(&wav)?;
                        Some((wav, duration))
                    }),
                )
            });
            let _ = controller_cell.set(controller.clone());

            let state = AppState {
                controller: controller.clone(),
                engine: engine.clone(),
                api: ApiClient::new(),
                config: config.clone(),
                tray_toggle: Mutex::new(None),
                last_snapshot: Arc::new(Mutex::new(Snapshot::Idle)),
            };
            app.manage(state);

            // Tray, hotkey, IPC socket, event forwarders.
            tray::setup(&handle)?;
            let binding = config.read().unwrap().hotkey.binding.clone();
            if let Err(e) = hotkey::register(&handle, &binding) {
                tracing::warn!("could not register hotkey {binding:?}: {e}");
            }
            events::spawn_forwarders(handle.clone());
            #[cfg(unix)]
            {
                let ipc_controller = controller.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = una_core::ipc::serve(ipc_controller).await {
                        tracing::warn!("ipc server exited: {e}");
                    }
                });
            }

            // Ask for mic access on first run so the first dictation doesn't
            // stall on the permission dialog.
            let perms = una_platform::permissions();
            if perms.mic() == una_platform::PermissionState::Undetermined {
                perms.request_mic();
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running una");
}
