# Una — Server API

One layer of Una's stack, owning `server/src/una_server/api/, server/src/una_server/main.py, server/src/una_server/config.py, server/src/una_server/db.py, server/src/una_server/schemas.py, server/src/una_server/state.py, server/src/una_server/errors.py, server/migrations/, server/src/una_server/__init__.py, server/tests/conftest.py, server/tests/test_api.py, server/tests/test_stats.py`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer server-api` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer server-api add|set|drop <section> …`. Where it and the code disagree, the code is right.

The Server API layer is the FastAPI /v1 boundary for Una: routers in `api/`, Pydantic contracts in `schemas.py`, app wiring in `main.py`, config in `config.py`, shared handles in `state.py`, SQLite access in `db.py`, and typed errors in `errors.py`. A newcomer must grasp: routers are thin over services/storage/DB via `AppState`, all responses are schema-typed, and config is `una.toml` overlaid by persisted settings at startup.

## Where to change what

- **Dictation ingest and history:** server/src/una_server/api/dictations.py, server/src/una_server/schemas.py, server/src/una_server/db.py
- **Correction and review queue:** server/src/una_server/api/dictations.py, server/src/una_server/schemas.py
- **Personal dictionary CRUD:** server/src/una_server/api/dictionary.py, server/src/una_server/schemas.py
- **Runtime settings:** server/src/una_server/api/settings_api.py, server/src/una_server/config.py, server/src/una_server/main.py
- **Health and usage stats:** server/src/una_server/api/system.py, server/src/una_server/schemas.py
- **Training runs and eligibility:** server/src/una_server/api/training.py, server/src/una_server/schemas.py
- **Model list and activate:** server/src/una_server/api/model_registry.py, server/src/una_server/schemas.py
- **Configuration and env overrides:** server/src/una_server/config.py, server/src/una_server/api/settings_api.py
- **App factory and lifespan:** server/src/una_server/main.py, server/src/una_server/state.py, server/src/una_server/errors.py
- **Error responses:** server/src/una_server/errors.py, server/src/una_server/main.py

## How it works

- **Dictation ingest:** POST /v1/dictations multipart audio + app_name/utterance_id hits api/dictations.py → Dedupe on utterance_id; build Whisper initial_prompt from active dictionary phrases → Transcribe via state.transcriber, clean via state.cleaner when enabled → Persist dictation row + wav via services.storage; freeze eval_holdout by ID hash → Return DictationResponse with timings, asr_model, cleanup flags
- **Correction flow:** POST correction with corrected text for a dictation id → Validate and run training eligibility check (check_pair / edit distance) → Store corrected/polished text, eligibility, and review action in DB → Expose via detail, list, and review-queue endpoints
- **Startup:** load_config from una.toml + UNA__SECTION__KEY env overrides → connect SQLite with WAL mode, run migrations/ → apply_stored_settings overlay onto live config → Seed registry, load/warm ASR, start jobs/discovery; install error handlers

## Key files

- server/src/una_server/api/dictations.py — dictation ingest, history, corrections, review queue
- server/src/una_server/main.py — app factory, lifespan, router and static mounting
- server/src/una_server/schemas.py — Pydantic request/response contract models
- server/src/una_server/config.py — una.toml + env-override config models
- server/src/una_server/state.py — shared AppState handles (db, config, models, jobs)
- server/src/una_server/db.py — aiosqlite connect, WAL pragmas, migration runner
- server/src/una_server/api/system.py — health and stats aggregation
- server/src/una_server/api/settings_api.py — mutable settings with live appliers
- server/src/una_server/errors.py — typed errors to uniform JSON responses
- server/src/una_server/api/training.py — training run lifecycle and eligibility

## Rules and traps

- Never put sounds-like hints in the Whisper initial_prompt; bare phrases only, hints go to the cleaner.
- Only keys in settings_api MUTABLE_KEYS may be changed at runtime; other config needs restart/file change.
- Eval holdout is frozen at insert via is_eval_holdout ID hash; do not recompute or reassign later.
- Training-in-progress must return 503 (TrainingInProgress) on conflicting dictation paths.
- DB must stay WAL mode with busy_timeout and foreign_keys on so the training subprocess can write.
- Raise typed UnaError subclasses, never raw HTTPException, to keep the {error:{code,message}} shape.

## What it talks to

- Server services — routers call transcriber, cleaner, storage, jobs, runs, and model manager via AppState
- Training loop — runs as a separate short-lived process that writes to the same SQLite file; dictations/corrections write eligibility fields and training.py reads eligible counts
- Dashboard API client and UI kit — typed fetch client consumes schemas.py contracts over /v1
- Runtime — binds LAN-only unauthenticated port 8100 with data_dir for SQLite/WAV store
- Client core — dictation audio and app_name originate from the Rust core spool/endpoints path
