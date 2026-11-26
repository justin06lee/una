# Una — architecture

The shape of this project, for a model that has never seen it: the stack it is built as, top to bottom, how the parts connect, where things are, how to run it. This is the index. Each layer has a sheet of its own in `.ruri/layers/` — where to change what inside it, how it works, its key files, its traps — so read the one for the layer you are about to work in (`ruri layer <slug>` prints it), and leave the rest. Where the work stands — git, decisions, what worked and what didn't, what's open — is in catchup.md beside this file.
Don't edit this file: ruri writes it. `ruri architecture` prints it with every line numbered, and once you have read it that way you can put right what your work changed — `ruri architecture add|set|drop <section> …`. The user corrects it on the architecture page.

Read from the repo at a0ccf13; folded forward from finished turns since. Where it and the code disagree, the code is right.

Una is a self-hosted Wispr Flow replacement: hold a hotkey anywhere, speak, and clean text lands in the focused app. Everything runs on your own hardware and it LoRA-fine-tunes Whisper on your voice the longer you use it.

## The stack, top to bottom

1. **Desktop windows** — Svelte 5 Tauri windows: HUD, settings, correction editor, review window (`client/apps/desktop/ui/`) → `.ruri/layers/desktop-windows.md`
2. **Desktop shell** — Tauri Rust shell: commands, hotkey, tray, windows, sounds, correction flow (`client/apps/desktop/src-tauri/, client/scripts/, client/apps/desktop/linux/ · 1 file`) → `.ruri/layers/desktop-shell.md`
3. **Client core** — Shared Rust core: 16kHz audio, spool, endpoints, net, discovery, state (`client/crates/una-core/`) → `.ruri/layers/client-core.md`
4. **Platform inject** — OS text injection and reading: macOS AX/eventtap, Linux X11/Wayland (`client/crates/una-platform/`) → `.ruri/layers/platform-inject.md`
5. **CLI** — una CLI binary wrapping the core (`client/crates/una-cli/`) → `.ruri/layers/cli.md`
6. **Server API** — FastAPI routes, schemas, config, state, SQLite access (`server/src/una_server/api/, server/src/una_server/, server/migrations/ +1 · 10 files`) → `.ruri/layers/server-api.md`
7. **Server services** — Dictation pipeline: faster-whisper transcriber, Ollama cleaner, storage, jobs, discovery (`server/src/una_server/services/, server/src/una_server/, server/deploy/ +2 · 4 files`) → `.ruri/layers/server-services.md`
8. **Training loop** — LoRA/QLoRA dataset, training, eval, WER-gated promote, autotrain (`server/src/una_server/training/, server/tests/, server/scripts/koen/ +1 · 5 files`) → `.ruri/layers/training-loop.md`
9. **Dashboard shell and settings** — App frame, hash routing, theming, landing page and settings form with Vite build. (`server/dashboard/, server/dashboard/src/, server/dashboard/src/lib/ +1 · 12 files`) → `.ruri/layers/dashboard-shell-settings.md`
10. **Dashboard home and insights** — Home feed and usage-insights pages with activity charts and stat tiles. (`server/dashboard/src/pages/, server/dashboard/src/lib/ · 5 files`) → `.ruri/layers/dashboard-home-insights.md`
11. **Dashboard review queue** — Review page for listening, editing transcripts, teacher guess pre-fill and disagreement chips, and viewing word diffs. (`server/dashboard/src/pages/, server/dashboard/src/lib/ · 3 files`) → `.ruri/layers/dashboard-review-queue.md`
12. **Dashboard dictionary and training** — Dictionary entries plus training runs with WER charts and run status. (`server/dashboard/src/pages/, server/dashboard/src/lib/ · 5 files`) → `.ruri/layers/dashboard-dictionary-training.md`
13. **iOS app** — Swift/SwiftUI, iOS 18+, no third-party packages; build with DEVELOPER_DIR=/Volumes/T7/Stockpile/Xcode.app/Contents/Developer   [client/apps/ios/] (`client/apps/ios/`) → `.ruri/layers/ios-app.md`

## How it flows

- **Dictation:** global hotkey (client hotkey.rs) → record 16kHz (una-core audio.rs) → POST wav (una-core net.rs/endpoint.rs) → server_api dictations.py → transcriber.py faster-whisper → cleaner.py Ollama → paste-inject (una-platform) → SQLite + WAV storage.py
- **Correction capture:** pasted text watch (una-platform axtext/eventtap) → correction window (Correction.svelte) → commands.rs/corrections.rs → server_api dictations.py → Review page final-text target → training pairs (filters.py/dataset.py)
- **Voice self-improvement:** Training page (Training.svelte) → runner.py train_lora.py → convert.py to CTranslate2 → evaluate.py WER vs production → promote.py hot-swap/rollback → model_registry.py
- **iPhone dictation:** Keyboard mic key -> Darwin notification + App Group flag (UnaKit MicRemote/Handoff) → Record tab or keyboard handoff records (client/apps/ios/Una, 16kHz WAV) → UnaKit endpoint race and spool upload → server_api dictations.py → transcriber.py faster-whisper → cleaner.py Ollama → app passes transcript to keyboard, which inserts it (space-aware)

## Where things are

- client/apps/desktop/ — Tauri shell (src-tauri: hotkey, tray, HUD/settings windows, commands) and Svelte UI (ui: HUD, unified Una window with embedded dashboard)
- client/crates/una-core/ — audio, spool, net, discovery, config, state
- client/crates/una-platform/ — macOS/Linux text read + paste injection
- client/crates/una-cli/ — una CLI
- server/src/una_server/api/ — FastAPI routes: dictations, review, training, registry
- server/src/una_server/services/ — transcriber, cleaner, storage, jobs, discovery
- server/src/una_server/training/ — LoRA + style datasets, eval, promote
- server/dashboard/src/pages/ — Home, Review, Dictionary, Training, Insights, Settings
- server/migrations/ — SQLite migrations
- server/tests/ — API, filters, stats, training tests
- docs/ — remote-access, iphone.md (incl. free-team limits), learning, macOS/Linux install, Korean code-switch plan
- server/deploy/ + docker-compose.yml — rootless server installs (jetson, linux), compose with Ollama
- server/scripts/koen/ — Korean code-switch eval: metrics, scoring, test-set builds, data fetch
- server/src/una_server/tools/merge_data.py — snapshot one server's data and merge it into another's (re-runnable, never overwrites)
- client/apps/ios/Una/ + UnaKeyboard/ + UnaKit/ — SwiftUI app, keyboard extension, Swift package; mic-key handoff via App Group, UnaKit Handoff.localBase falls back to app storage without it
- client/apps/ios/Config/ — Una.xcconfig (bundle ID, App Group) and Una-Info.plist (export-compliance key): the places to change for app identity and plist settings

## How to run it

- docker compose up -d --build — server :8100 + Ollama :11434
- docker exec -it una-ollama-1 ollama pull qwen3:8b — fetch cleanup model
- make — build + install + launch the client: Una.app on macOS; on Linux una-desktop + una in ~/.local/bin with an app-grid launcher (golden path)
- make dmg — distributable dmg, revealed in Finder
- make update — refresh installed client
- make ui-build && make client-dev — client dev loop (needs tauri-cli v2)
- server-dev: cd server && uv run uvicorn una_server.main:app --reload — CPU hacking
- server-test: cd server && uv run pytest -q / client-test: cd client && cargo test -p una-core
- Linux client test harness (Jetson): virtual mic via pw-loopback + PIPEWIRE_NODE=una-test-mic, a GTK paste-target window and xdotool on a private Xvfb (:99 is shared and other chats steal focus)
- Jetson server (midna, retired from serving 2026-10-21): server/deploy/jetson/install.sh — no root; re-run after git pull; never plain 'uv sync' there (drops the GPU CTranslate2); see docs/jetson-server.md
- Live server: tenet (x86, 6 GB GTX 1660 Ti), ~/una via server/deploy/linux/install.sh as systemd user units una + ollama; clients use http://192.168.1.172:8100 and http://tenet.makima:8100; re-run install.sh after git pull, never plain 'uv sync'; logs: journalctl --user -u una

## What it does

- Push-to-talk dictation from any app via global hotkey
- Local Whisper transcription via faster-whisper with CUDA
- Local LLM cleanup for filler, punctuation, and per-app tone
- One-click Fix-up polish via yagami LLM with model settings
- Automatic correction capture from pasted text for training pairs
- Review queue and correction editor with audio playback
- Custom dictionary injected into decoding and cleanup prompts
- LoRA voice fine-tuning of Whisper with WER-gated hot-swap
- QLoRA style fine-tuning of cleanup LLM on polished pairs
- Personal style LoRA with Claude teacher drafts
- Model registry with evaluation history and one-click rollback
- Dashboard for transcripts, dictionary, training, and usage insights
- History main window with Settings button for smaller window
- Racing multi-address server list with makima and mDNS discovery
- Hold, toggle, hybrid modes with left/right modifiers, shortcut abort, tray and HUD
- iPhone app on TestFlight: record, upload, corrections, keyboard mic dictation
