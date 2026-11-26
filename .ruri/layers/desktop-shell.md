# Una — Desktop shell

One layer of Una's stack, owning `client/apps/desktop/src-tauri/, client/scripts/gen_icons.py, client/apps/desktop/linux/`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer desktop-shell` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer desktop-shell add|set|drop <section> …`. Where it and the code disagree, the code is right.

The desktop shell is the Tauri v2 Rust host for the menu-bar dictation client: it owns app startup, shared AppState, Tauri commands, and bridging of una-core controller effects to windows, audio, network, and injection. It now owns two windows again — the history window as the main home window (Home/Review/Dictionary/Training/Insights plus a Settings button) and the smaller settings window — plus separate Review Dictations and HUD/correction windows. A newcomer must grasp that all dictation logic lives in una-core's FSM; this layer only runs its EffectRunner, forwards snapshots, and manages windows.

## Where to change what

- **Dictation effect execution (record/upload/inject/HUD):** client/apps/desktop/src-tauri/src/effects.rs, client/apps/desktop/src-tauri/src/main.rs, client/apps/desktop/src-tauri/src/app_state.rs
- **History main window (Home/Review/Dictionary/Training/Insights + Settings button):** client/apps/desktop/src-tauri/src/windows.rs, client/apps/desktop/src-tauri/src/history.rs, client/apps/desktop/src-tauri/src/commands.rs, client/apps/desktop/src-tauri/src/main.rs
- **Settings window (smaller Una window, ⌘, shortcut):** client/apps/desktop/src-tauri/src/windows.rs, client/apps/desktop/src-tauri/src/commands.rs, client/apps/desktop/src-tauri/src/main.rs
- **Correction learning flow (accepted/edited/fix window):** client/apps/desktop/src-tauri/src/corrections.rs, client/apps/desktop/src-tauri/src/commands.rs, client/apps/desktop/src-tauri/src/windows.rs, client/apps/desktop/src-tauri/src/app_state.rs
- **HUD window:** client/apps/desktop/src-tauri/src/windows.rs, client/apps/desktop/src-tauri/src/events.rs
- **Settings tabs and config changes:** client/apps/desktop/src-tauri/src/commands.rs, client/apps/desktop/src-tauri/src/windows.rs, client/apps/desktop/src-tauri/src/hotkey.rs
- **Global hotkey / push-to-talk binding:** client/apps/desktop/src-tauri/src/hotkey.rs, client/apps/desktop/src-tauri/src/main.rs, client/apps/desktop/src-tauri/src/commands.rs
- **Tray icon and menu:** client/apps/desktop/src-tauri/src/tray.rs, client/apps/desktop/src-tauri/src/events.rs
- **State snapshot and audio-level forwarding:** client/apps/desktop/src-tauri/src/events.rs, client/apps/desktop/src-tauri/src/windows.rs, client/apps/desktop/src-tauri/src/tray.rs
- **Feedback sounds:** client/apps/desktop/src-tauri/src/sounds.rs, client/apps/desktop/src-tauri/src/effects.rs
- **App icons:** client/scripts/gen_icons.py

## How it works

- **Dictation turn:** Hotkey press or tray toggle sends Command to controller (main.rs hotkey handler, tray.rs, commands.rs) → Controller FSM emits effects; TauriEffects runs them: audio engine, endpoint resolve, ApiClient upload (effects.rs) → State snapshots forwarded as state-changed events to HUD webview, tray label, HUD interactivity (events.rs) → On success Done cue plays, text injected, last_dictation stored and correction watch armed (effects.rs, corrections.rs, app_state.rs)
- **Correction after paste:** Upload result sets last_dictation; insert effect consumes it once paste lands (app_state.rs, effects.rs) → Watch window: no edit key yields accepted signal; edit in readable app reads field back and diffs pasted span yielding edited (corrections.rs) → Unreadable-app edit opens focused correction window (windows.rs accessory-mode flip) → Correction command reports result to server for training set (commands.rs, corrections.rs)
- **Settings rebind:** Webview calls set_config command with validation of hotkey mode, hud_mode, server URLs (commands.rs) → Persisted config updates shared RwLock and EndpointResolver (app_state.rs, commands.rs) → hotkey::apply tears down old registration and registers combo or native backend (hotkey.rs) → Windows adjust HUD visibility mode and tray launch-at-login state (windows.rs, tray.rs)

## Key files

- client/apps/desktop/src-tauri/src/main.rs — app startup, controller spawn, plugin and hotkey wiring, history-window-on-launch
- client/apps/desktop/src-tauri/src/effects.rs — EffectRunner bridging FSM effects to audio, HTTP, inject, HUD
- client/apps/desktop/src-tauri/src/windows.rs — history main window plus smaller settings, HUD and correction/fix window creation and focus rules
- client/apps/desktop/src-tauri/src/history.rs — history main window server-page loading, navigation handshake and offline handling
- client/apps/desktop/src-tauri/src/corrections.rs — post-paste accepted/edited/fix-window training-signal watcher
- client/apps/desktop/src-tauri/src/commands.rs — Tauri commands for settings, HUD, history/settings windows, and correction webviews
- client/apps/desktop/src-tauri/src/app_state.rs — shared controller, engine, api, config, endpoints, pending slots
- client/apps/desktop/src-tauri/src/events.rs — snapshot and level forwarders to webviews, tray, HUD
- client/apps/desktop/src-tauri/src/hotkey.rs — combo vs native backend selection and re-registration
- client/apps/desktop/src-tauri/src/tray.rs — menu-bar icon, menu items opening history/settings pages, autostart and opener actions

## Rules and traps

- Never put dictation state machine logic here; it lives in una-core state — this layer only runs effects and forwards snapshots.
- Validate config in set_config before persisting (hotkey binding, hold/toggle/hybrid mode, pill/flash hud_mode).
- HUD window must stay fixed-size, frameless, transparent, always-on-top, click-through anchored bottom-center; visual states are CSS only.
- Correction window must flip accessory mode out and back so a menu-bar app can take keyboard focus.
- Sounds are best-effort and silent on failure; never surface them as errors.
- Launching Una or its Dock icon opens the history main window; the smaller settings window opens only via its history sidebar Settings button, ⌘,, or tray Settings…; Review Dictations stays its own window.

## What it talks to

- Client core — drives its Controller FSM, AudioEngine, Config, EndpointResolver, ApiClient from effects and commands
- Desktop windows — shell creates the windows and feeds them state-changed and level events; webviews call back via commands
- Platform inject — text injection and field read-back for paste and edited-correction diffing
- Server API — uploads dictations and reports accepted/edited corrections that become training data
- Server dashboard — history window embeds it from AppState live_server() with navigation handshake and offline fallback; build via build.rs and icons from gen_icons.py
