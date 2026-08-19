"""Style training runner: `python -m una_server.training.style_runner --run-id <ULID>`.

Spawned by services.jobs.JobManager (same single GPU slot as ASR runs); owns its
training_runs row via sync sqlite3. Mirrors runner.py's stage order:
building -> training -> converting (ollama create) -> evaluating -> promoted|rejected.

wer_baseline / wer_candidate store the style metric — mean normalized edit
distance in [0, 1] between model output and polished target (lower is better).
"""

from __future__ import annotations

import argparse
import json
import logging
import shutil
import sqlite3
import sys
from pathlib import Path

import httpx

from ..config import Config, load_config
from ..db import utcnow
from . import style_dataset, style_eval, style_promote
from .runner import RunContext, _install_sigterm_handler, _setup_logging, connect

log = logging.getLogger("una_server.training.style_runner")


def _hyperparams(row: sqlite3.Row, cfg: Config) -> dict:
    training = cfg.training
    defaults = {
        "style_base_hf_model": training.style_base_hf_model,
        "style_ollama_base": training.style_ollama_base,
        "style_lora_r": training.style_lora_r,
        "style_lora_alpha": training.style_lora_alpha,
        "style_lora_dropout": training.style_lora_dropout,
        "style_learning_rate": training.style_learning_rate,
        "style_epochs": training.style_epochs,
        "style_batch_size": training.style_batch_size,
        "style_grad_accum": training.style_grad_accum,
        "style_max_seq_len": training.style_max_seq_len,
    }
    stored = json.loads(row["hyperparams_json"]) if row["hyperparams_json"] else {}
    return {**defaults, **stored}


def _unload_ollama_models(cfg: Config) -> None:
    """Best-effort: ask Ollama to release resident models so QLoRA gets the VRAM."""
    try:
        with httpx.Client(base_url=cfg.cleanup.ollama_url, timeout=10.0) as client:
            resp = client.get("/api/ps")
            for model in resp.json().get("models", []):
                client.post(
                    "/api/generate", json={"model": model["name"], "keep_alive": 0}
                )
                log.info("asked ollama to unload %s", model["name"])
    except (httpx.HTTPError, ValueError):
        log.info("could not query ollama for resident models (continuing)")


def _cleanup(run_dir: Path) -> None:
    """Drop checkpoints; keep adapter/, Modelfile, and train.log for inspection."""
    for checkpoint in run_dir.glob("checkpoint-*"):
        shutil.rmtree(checkpoint, ignore_errors=True)


def _execute(run: RunContext, cfg: Config, row: sqlite3.Row, run_dir: Path) -> None:
    from . import train_style_lora  # heavy deps live behind this module

    hp = _hyperparams(row, cfg)
    log.info("style run %s starting with hyperparams %s", run.run_id, hp)

    # -- building -------------------------------------------------------------
    run.update(status="building", progress=0.02)
    split = style_dataset.build_style_datasets(run.conn)
    if not split.train:
        raise RuntimeError("no style training pairs (add Final text in Review)")
    run.update(n_train=len(split.train), n_eval=len(split.eval))

    # -- training -------------------------------------------------------------
    _unload_ollama_models(cfg)
    run.update(status="training", progress=0.1)
    last_progress = 0.1

    def on_progress(fraction: float) -> None:
        nonlocal last_progress
        progress = 0.1 + 0.6 * min(max(fraction, 0.0), 1.0)
        if progress - last_progress >= 0.005:
            last_progress = progress
            run.update(progress=round(progress, 4))

    adapter_dir = train_style_lora.train_style_adapter(
        split.train, run_dir, hp, cfg.cleanup,
        device=cfg.asr.device, on_progress=on_progress,
    )

    # -- converting: layer the adapter into an ollama model -------------------
    run.update(status="converting", progress=0.75)
    model_name = style_promote.new_style_model_name(run.run_id)
    style_promote.create_ollama_model(
        model_name, hp["style_ollama_base"], adapter_dir, run_dir
    )
    style_eval.smoke_test(
        cfg.cleanup.ollama_url, model_name, split.train[0], cfg.cleanup
    )

    # -- evaluating (both models through ollama, the real serving path) -------
    run.update(status="evaluating", progress=0.85)
    baseline_model = row["base_model_id"] or cfg.cleanup.model
    dist_baseline = dist_candidate = None
    if split.eval:
        dist_baseline = style_eval.mean_distance(
            cfg.cleanup.ollama_url, baseline_model, split.eval, cfg.cleanup
        )
        dist_candidate = style_eval.mean_distance(
            cfg.cleanup.ollama_url, model_name, split.eval, cfg.cleanup
        )
        run.update(
            wer_baseline=round(dist_baseline, 4), wer_candidate=round(dist_candidate, 4)
        )

    # -- promotion gate -------------------------------------------------------
    ok, reason = style_promote.should_promote_style(
        dist_baseline if dist_baseline is not None else 0.0,
        dist_candidate if dist_candidate is not None else 0.0,
        len(split.eval),
        cfg.training.style_promotion_margin,
        cfg.training.style_min_eval_samples,
    )
    log.info("style promotion decision: %s — %s", "promote" if ok else "reject", reason)
    if ok:
        run.update(
            status="promoted", produced_model_id=model_name, progress=1.0,
            finished_at=utcnow(),
        )
        style_promote.switch_cleanup_model(cfg.server.port, model_name)
    else:
        # keep the ollama model + adapter for inspection; the cleaner is untouched
        run.update(
            status="rejected", produced_model_id=model_name, progress=1.0,
            finished_at=utcnow(),
        )

    _cleanup(run_dir)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="una style fine-tuning runner")
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args(argv)

    cfg = load_config()
    conn = connect(cfg.db_path)
    row = conn.execute("SELECT * FROM training_runs WHERE id = ?", (args.run_id,)).fetchone()
    if row is None:
        print(f"training run {args.run_id} not found in {cfg.db_path}", file=sys.stderr)
        return 1

    run_dir = cfg.runs_dir / args.run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    _setup_logging(Path(row["log_path"]) if row["log_path"] else run_dir / "train.log")
    _install_sigterm_handler(cfg.db_path, args.run_id)

    run = RunContext(conn, args.run_id)
    try:
        _execute(run, cfg, row, run_dir)
        return 0
    except Exception as exc:
        log.exception("style run %s failed", args.run_id)
        run.update(status="failed", error=str(exc), finished_at=utcnow())
        return 1
    finally:
        conn.close()


if __name__ == "__main__":
    sys.exit(main())
