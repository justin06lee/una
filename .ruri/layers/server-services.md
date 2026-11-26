# Una — Server services

One layer of Una's stack, owning `server/src/una_server/services/, server/src/una_server/memory.py, server/deploy/, server/tests/test_cleaner.py, server/src/una_server/tools/merge_data.py, server/tests/test_merge_data.py`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer server-services` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer server-services add|set|drop <section> …`. Where it and the code disagree, the code is right.

This layer implements the server-side dictation pipeline: 16kHz audio ingest, faster-whisper transcription, Ollama text cleanup, active-model management, single-slot training-job launching, run eligibility, idle autotraining, and mDNS discovery. It is built as small stateless helpers plus three stateful owners (Transcriber/ModelManager, Cleaner, JobManager/Discovery) coordinated by the API layer under one inference lock. Newcomers must grasp the degradation contract: transcription is authoritative and cleanup/training must never fail or block dictation.

## Where to change what

- **Audio ingest and resampling:** server/src/una_server/services/storage.py
- **Speech-to-text transcription:** server/src/una_server/services/transcriber.py, server/src/una_server/services/model_manager.py
- **Active-model resolution and hot-swap:** server/src/una_server/services/model_manager.py, server/src/una_server/services/transcriber.py
- **LLM transcript cleanup:** server/src/una_server/services/cleaner.py, server/src/una_server/services/prompts.py
- **Cleanup timeout budget:** server/src/una_server/services/cleaner.py, server/deploy/jetson/una.toml
- **Cleanup prompt and per-app tone:** server/src/una_server/services/prompts.py, server/src/una_server/services/cleaner.py
- **Training job launching:** server/src/una_server/services/jobs.py, server/src/una_server/services/runs.py
- **Run eligibility counting and launch:** server/src/una_server/services/runs.py, server/src/una_server/services/jobs.py
- **Idle auto-training trigger:** server/src/una_server/services/autotrain.py, server/src/una_server/services/runs.py
- **LAN discovery advertisement:** server/src/una_server/services/discovery.py
- **Moving data between servers (snapshot + idempotent merge):** server/src/una_server/tools/merge_data.py
- **Server install on x86 Linux without Docker (tenet):** server/deploy/linux/install.sh, server/deploy/linux/una.toml

## How it works

- **Dictation transcribe and clean:** storage.py decodes upload to float32 mono 16kHz and persists WAV → ModelManager lock serializes inference and hot-swaps → Transcriber runs faster-whisper in a thread and returns text plus language → Cleaner calls Ollama with system prompt from prompts.py, strips <think> tags, and bounds the call by a timeout that scales with word count → Safety check rejects output that collapses the dictation; on cleanup failure, timeout, or rejection, return raw transcript with applied=false
- **Model hot-swap:** ModelManager looks up models row and resolves ct2_path or hf_source → Acquire inference lock to drain in-flight transcription → Free old Transcriber and GC before loading to avoid double VRAM → Construct new Transcriber for requested model and set active_model_id
- **Idle auto-training:** autotrain.py ticks every 60s and re-reads training.auto config → Check dictation-idle time exceeds auto_idle_minutes → Check no active JobManager process and no active training_runs row → Check eligible minutes threshold and FRESH_SQL shows pairs newer than last run → Launch run via runs.launch_run so unchanged or persistently failing data never retries

## Key files

- server/src/una_server/services/transcriber.py — owns one loaded faster-whisper model; thread-based transcribe
- server/src/una_server/services/model_manager.py — resolves active model row, serializes inference and swaps with free-then-load
- server/src/una_server/services/cleaner.py — Ollama cleanup client with failure counting, skip-window, warmup, and length-scaled timeout
- server/src/una_server/services/prompts.py — builds cleanup system prompt with dictionary clause and app-name tone
- server/src/una_server/services/storage.py — decodes arbitrary audio to 16k mono, computes duration, saves YYYY/MM WAVs
- server/src/una_server/services/jobs.py — single-slot training subprocess launcher with active-status guard and finish callback
- server/src/una_server/services/runs.py — eligible-minutes, style, and fresh-pair counts plus shared run launcher
- server/src/una_server/services/autotrain.py — idle-triggered auto-training loop gated on freshness and inactivity
- server/src/una_server/services/discovery.py — advertises _una._tcp mDNS service with LAN address and /v1 property
- server/src/una_server/services/__init__.py — package export for services
- server/deploy/jetson/ — install.sh (rootless Jetson server: builds GPU CTranslate2, uv sync on managed Python, Ollama 0.34.0, systemd user units), una.service, ollama.service, una.toml (8 GB shared-memory defaults and per-word cleanup timeout); midna served una with it until 2026-10-21
- server/src/una_server/tools/merge_data.py — stdlib-only snapshot (sqlite online backup + referenced audio/registered models/run dirs) and re-runnable merge by primary key: target wins, newer updated_at or a source delete flows over, paths re-rooted to the target's layout, files never replaced, una.db backed up to data/backups first
- server/deploy/linux/ — install.sh (rootless x86_64 NVIDIA server: uv sync with cuda12 extra minus nvidia-cudnn-cu12, Ollama 0.34.0 amd64, systemd user units; UNA_NO_START=1 installs without touching the GPU), una.service, ollama.service, una.toml (6 GB card: int8_float16 Whisper + llama3.2:3b, pause_serving, batch 2 x 8); runs the live server on tenet since 2026-10-21

## Rules and traps

- Dictation never fails because cleanup failed: Cleaner must degrade to raw text with applied=false, using consecutive-failure skip window.
- Cleanup timeout must scale with dictation length: a flat limit sends long dictations to raw text, because the cleanup model writes about 15 words per second.
- Warm the cleanup model into VRAM before real requests; cold-load blows the timeout budget and forces raw fallback.
- Hold ModelManager lock for both inference and swaps, and free-then-load models to avoid double VRAM.
- Never run inference on the event loop: faster-whisper transcribe runs in a thread via transcribe_sync.
- Single training job only: check ACTIVE_STATUSES and JobManager.active before launch; the runner subprocess owns its training_runs row.
- Autotraining must require fresh pairs newer than the last run attempt, plus idle time and eligible minutes, so it never loops on unchanged data or retries a persistently failing run.
- Cleanup output that shrinks a long dictation to a short line fails the safety check and is discarded in favor of the raw transcript.
- Never move a server by copying server/data over another's: it replaces the target's una.db. Use tools/merge_data.py (snapshot on the source, merge on the target), and take a last delta snapshot at cutover.
- deploy/*/una.toml is caught by .gitignore's una.toml pattern: add it with git add -f or the installer ships without its defaults.

## What it talks to

- Server API — routes call storage, ModelManager/Transcriber, Cleaner, runs, jobs, and discovery via AppState and config
- Training loop — jobs.py spawns the runner subprocess and runs.py supplies eligibility counts and launch; finish callback reloads ASR
- Runtime — faster-whisper CUDA model, Ollama service, and SQLite/WAV store provide inference, cleanup, and persistence
- Client core — discovery.py advertises _una._tcp so LAN clients auto-discover the /v1 endpoint
- Dashboard — shows the applied flag and fallback reason from cleanup results, so the raw transcript is always visible
