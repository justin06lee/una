# Una — Training loop

One layer of Una's stack, owning `server/src/una_server/training/, server/tests/test_filters.py, server/tests/test_style_training.py, server/tests/test_training_logic.py, server/tests/test_training_runs.py, server/scripts/koen/, docs/korean-codeswitch.md`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer training-loop` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer training-loop add|set|drop <section> …`. Where it and the code disagree, the code is right.

This layer fine-tunes two models from user corrections: Whisper (LoRA) for transcription and a cleanup LLM (QLoRA) for polishing style. Each side has a parallel pipeline (dataset build, train adapter, convert, evaluate, WER/edit-distance-gated promote) run as a spawned subprocess that owns its training_runs row via sync sqlite3. Newcomers must grasp that promotion only happens on a measured improvement over baseline by a margin on enough holdout samples, and that heavy ML imports must stay lazy so the serving venv stays importable.

## Where to change what

- **ASR training run lifecycle:** server/src/una_server/training/runner.py, server/src/una_server/training/dataset.py, server/src/una_server/training/train_lora.py
- **Style training run lifecycle:** server/src/una_server/training/style_runner.py, server/src/una_server/training/style_dataset.py, server/src/una_server/training/train_style_lora.py
- **Correction eligibility filtering:** server/src/una_server/training/filters.py
- **ASR dataset building:** server/src/una_server/training/dataset.py, server/src/una_server/training/filters.py
- **Style dataset building:** server/src/una_server/training/style_dataset.py, server/src/una_server/training/filters.py
- **ASR WER evaluation:** server/src/una_server/training/evaluate.py, server/src/una_server/training/filters.py
- **Style output evaluation:** server/src/una_server/training/style_eval.py, server/src/una_server/training/style_dataset.py
- **Adapter merge and CTranslate2 conversion:** server/src/una_server/training/convert.py
- **ASR WER-gated promotion and pruning:** server/src/una_server/training/promote.py
- **Style promotion and Ollama model creation:** server/src/una_server/training/style_promote.py, server/src/una_server/training/style_eval.py
- **LoRA hyperparameters (Whisper):** server/src/una_server/training/train_lora.py, server/src/una_server/training/runner.py
- **QLoRA hyperparameters (style LLM):** server/src/una_server/training/train_style_lora.py, server/src/una_server/training/style_runner.py
- **Korean / code-switch eval prep (planned, awaiting approval):** docs/korean-codeswitch.md, server/scripts/koen/koen_data.py, server/scripts/koen/koen_eval.py, server/scripts/koen/koen_metrics.py

## How it works

- **ASR run (runner.py):** JobManager spawns python -m una_server.training.runner --run-id → building: dataset.build_datasets selects eligible correction pairs → training: train_lora.train_adapter writes run_dir/adapter → converting: convert.merge_adapter + CTranslate2 convert + smoke test → evaluating: evaluate scores baseline vs candidate WER via CTranslate2 → promoted|rejected: promote.should_promote gates registry insert and pruning
- **Style run (style_runner.py):** JobManager spawns style_runner in same single GPU slot → building: style_dataset.build_style_datasets selects raw->polished pairs → training: train_style_lora.train_style_adapter writes run_dir/adapter → converting: style_promote.create_ollama_model via ollama create → evaluating: style_eval queries baseline and candidate via /api/chat, mean norm edit distance → promoted|rejected: should_promote_style gates live switch

## Key files

- server/src/una_server/training/runner.py — ASR run entrypoint, stage order, sqlite row ownership, progress mapping
- server/src/una_server/training/style_runner.py — style run entrypoint mirroring runner.py, reuses RunContext/connect
- server/src/una_server/training/filters.py — pure eligibility: normalize_text, norm_edit_distance, check_pair, duration bounds
- server/src/una_server/training/dataset.py — ASR Sample and build_datasets from corrections
- server/src/una_server/training/style_dataset.py — StyleSample, messages_for, build_style_datasets
- server/src/una_server/training/train_lora.py — Whisper LoRA training on q_proj+v_proj
- server/src/una_server/training/train_style_lora.py — cleanup-LLM QLoRA (4-bit on CUDA) on q/k/v/o_proj
- server/src/una_server/training/convert.py — merge adapter, CTranslate2 convert, copy tokenizer files, smoke test
- server/src/una_server/training/evaluate.py — greedy CTranslate2 baseline-vs-candidate WER via jiwer
- server/src/una_server/training/promote.py — should_promote gate, new_model_id, prune_finetuned
- server/src/una_server/memory.py — release_freed_memory (malloc_trim) after every Whisper/CT2 load; ~760 MB per large-v3-turbo load stays in glibc arenas otherwise
- server/scripts/koen/koen_metrics.py — WER, CER, MER and script-violation scoring for Korean/code-switch tests
- server/scripts/koen/koen_eval.py — scores a CT2 model per manifest under en/ko/auto/route/ko+en settings
- server/scripts/koen/koen_data.py — FLEURS/Zeroth/HiKE manifests and read-only DB catalogue with audio LID
- server/scripts/koen/fetch_public.sh — downloads the open Korean test sets
- docs/korean-codeswitch.md — Korean/code-switch plan, gates, baselines and approval list

## Rules and traps

- Keep heavy imports (torch/transformers/peft/soundfile/faster-whisper) inside functions so the serving venv can import these modules.
- Stage order is fixed: building -> training -> converting -> evaluating -> promoted|rejected; eval must score converted models through the serving path (CTranslate2 / Ollama chat).
- Runner sqlite uses plain sync sqlite3 with busy_timeout and foreign_keys OFF; dangling dictation->model references after pruning are accepted, do not enable FK.
- Promotion requires beating baseline by >= margin on >= min_samples; never promote on training loss or vibes.
- Pure-logic tests must pass without torch/transformers/peft/ollama; do not add heavy imports at module top level that break them.
- The training stage runs in a spawned child via runner.in_training_process (progress via its own RunContext, PR_SET_PDEATHSIG ties it to the runner): torch keeps ~3 GB per process that empty_cache can't free, and the eval stages need it (Jetson).
- Korean/code-switch data lives outside the una checkout (~/una-koen on tenet), never inside ~/una.
- Korean/code-switch scripts read the una database read-only (mode=ro) and never write to it.
- Korean eval runs need GPU time on tenet only after the running voice job finishes and dictation is paused, with the user's approval.
- Proposed Korean mixed-script gate (not yet approved): HiKE MER must be 20 or less and under 5% of sentences may keep a wrong-script span.
- Korean CER must not rise on either Korean test set; FLEURS English WER allowance (+0.3 or strict 0) is still the user's call.

## What it talks to

- Server services — JobManager spawns runner/style_runner into single GPU slot; model_manager source resolution mirrored in evaluate.resolve_model_source
- Server API — correction endpoints and training-run launch/guard feed eligible pairs and run rows; _guard_unseen_transcript and filters.check_pair share eligibility
- Server config/db — Config hyperparams/thresholds and SQLite WAL + migrations define run rows, dictations, corrections
- Dashboard training — Training page reads run status and WER charts produced here
- Runtime — needs CUDA VRAM, ollama CLI reachable, CTranslate2; CPU path is fp32 dev/test only
- Remote host tenet — holds the ~/una-koen test-set copy and the read-only koen runs; the Mac holds the mirrored baseline outputs under scratch/una-koen
