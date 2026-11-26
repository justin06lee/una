# Una — Client core

One layer of Una's stack, owning `client/crates/una-core/`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer client-core` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer client-core add|set|drop <section> …`. Where it and the code disagree, the code is right.

Shared platform-agnostic Rust core for the desktop dictation client: dictation state machine plus audio capture, server HTTP client, endpoint selection, config, spool, discovery, and IPC. Built around a pure deterministic FSM in state.rs with a tokio Controller actor executing Effects; audio runs on a dedicated cpal worker thread resampled to 16kHz mono. Newcomers must grasp that state.rs owns all timing/transitions and everything else is an effect runner around it.

## Where to change what

- **Dictation state machine and controller:** client/crates/una-core/src/state.rs
- **Microphone capture and 16kHz WAV encoding:** client/crates/una-core/src/audio.rs
- **Server HTTP client (dictation upload, health, correction):** client/crates/una-core/src/net.rs
- **Endpoint selection and caching:** client/crates/una-core/src/endpoint.rs
- **TOML config and hotkey bindings:** client/crates/una-core/src/config.rs
- **Post-paste correction capture:** client/crates/una-core/src/correction.rs
- **Failed-upload spool and retry:** client/crates/una-core/src/spool.rs
- **mDNS server discovery:** client/crates/una-core/src/discovery.rs
- **Unix-socket IPC for CLI/keybinds:** client/crates/una-core/src/ipc.rs

## How it works

- **Dictation utterance:** Controller receives Command (Toggle/Start) via mpsc or ipc socket → step() advances Machine and emits Effects with at-timestamps → EffectRunner starts audio worker capture with warm stream reuse → Audio finalized to 16kHz mono WAV, level frames on watch channel → EndpointResolver picks cached/probed base URL → ApiClient POSTs multipart dictation with utterance_id, one retry on connect/reset only → On failure spool WAV to cache dir; Snapshot broadcast on each change
- **Endpoint resolution:** Collect ordered candidate URLs (manual URL wins over discovery) → Probe candidates concurrently with per-candidate timeout → First health success wins and is cached for CACHE_TTL → Steady-state dictations use lock-read cached endpoint → Invalidate cache on upload failure for roaming
- **Correction capture:** Record pasted span range and post-paste text → Accessibility path recovers edited span via edited_region_text or returns None if outside span → Keystroke path tracks span length via EditCounter, invalidated by clicks/arrows/undo → PUT correction to /v1/dictations/{id}/correction for training pair

## Key files

- client/crates/una-core/src/state.rs — pure deterministic dictation FSM plus tokio Controller actor, Snapshots, timing constants
- client/crates/una-core/src/audio.rs — cpal capture worker, 16kHz resampling, level metering, WAV encoding, warm keepalive
- client/crates/una-core/src/net.rs — server API client: multipart upload, health, correction PUT, timeouts and retry rule
- client/crates/una-core/src/endpoint.rs — concurrent candidate probing, cached winner, TTL and invalidate
- client/crates/una-core/src/config.rs — TOML config, server/discovery settings, Binding combo vs native keycode
- client/crates/una-core/src/correction.rs — edited_region_text and EditCounter pure correction logic
- client/crates/una-core/src/spool.rs — cache-dir WAV spool, newest KEEP=5 pruning, duplicate suppression
- client/crates/una-core/src/discovery.rs — _una._tcp.local mDNS browse, DiscoveredServer with version/api TXT
- client/crates/una-core/src/ipc.rs — Unix-socket newline-JSON toggle/start/stop/cancel server
- client/crates/una-core/src/lib.rs — crate root re-exporting Controller, Machine, Effects, EndpointResolver

## Rules and traps

- Never put timing or IO inside step(): all timestamps travel in Events; caller executes Effects.
- Discard utterances under MIN_UTTERANCE (300ms); auto-finalize at MAX_UTTERANCE (5min); Done dismisses after DONE_DISMISS (900ms).
- Audio must be 16kHz mono s16le WAV (TARGET_SAMPLE_RATE); cpal stream stays on its worker thread, never Send.
- Retry uploads exactly once on connect/reset with same utterance_id; never retry on HTTP status errors.
- Manually configured server URL always beats autodiscovery; discovery only when url empty and autodiscover true.
- Spool keeps newest 5 only and must not stack duplicates of a re-spooled retry.

## What it talks to

- Desktop shell — drives Controller via Commands and executes Effects (hotkey, tray, windows, sounds, paste)
- Platform inject — performs text paste/reading that correction.rs measures and the FSM triggers
- CLI — drives dictation remotely through unix-socket IPC toggle/start/stop/cancel
- Server API — target of net.rs multipart dictation, health checks, and correction PUTs
- Server services — consumes uploaded WAVs; discovery TXT version/api describes its capability
