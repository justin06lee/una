"""Training runner: `python -m una_server.training.runner --run-id <ULID>`.

Spawned by services.jobs.JobManager; owns its training_runs row via plain sync sqlite3
(the server keeps the database in WAL mode, so both sides can write). Heavy ML modules are
imported lazily inside the run so this module stays importable in the serving venv.

Stage order: building -> training -> converting -> evaluating -> promoted|rejected.
Conversion runs before the final eval so both models are scored through CTranslate2.
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import shutil
import signal
import sqlite3
import sys
from pathlib import Path

import httpx

from ..config import Config, load_config
from ..db import utcnow
from . import dataset, promote

log = logging.getLogger("una_server.training.runner")


def connect(db_path: Path) -> sqlite3.Connection:
    # foreign_keys stays OFF (sqlite's default): pruning deletes models rows that old
    # dictations may still reference, and those dangling references are accepted.
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA busy_timeout=5000")
    return conn


class RunContext:
    """Tiny UPDATE helper for the run's row; every stage reports through it."""

    def __init__(self, conn: sqlite3.Connection, run_id: str):
        self.conn = conn
        self.run_id = run_id

    def update(self, **fields: object) -> None:
        assignments = ", ".join(f"{name} = ?" for name in fields)
        self.conn.execute(
            f"UPDATE training_runs SET {assignments} WHERE id = ?",
            (*fields.values(), self.run_id),
        )
        self.conn.commit()


def _install_sigterm_handler(db_path: Path, run_id: str) -> None:
    def handler(_signum, _frame):
        log.info("SIGTERM received; marking run %s cancelled", run_id)
        conn = sqlite3.connect(db_path)  # fresh connection: the main one may be mid-query
        try:
            conn.execute("PRAGMA busy_timeout=5000")
            conn.execute(
                "UPDATE training_runs SET status = 'cancelled', finished_at = ? "
                "WHERE id = ? AND status NOT IN ('promoted', 'rejected', 'failed')",
                (utcnow(), run_id),
            )
            conn.commit()
        finally:
            conn.close()
        os._exit(0)

    signal.signal(signal.SIGTERM, handler)


def _setup_logging(log_path: Path) -> None:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    formatter = logging.Formatter("%(asctime)s %(levelname)s %(name)s: %(message)s")
    root = logging.getLogger()
    root.setLevel(logging.INFO)
    for handler in (logging.FileHandler(log_path), logging.StreamHandler(sys.stderr)):
        handler.setFormatter(formatter)
        root.addHandler(handler)


def _hyperparams(row: sqlite3.Row, cfg: Config) -> dict:
    training = cfg.training
    defaults = {
        "base_hf_model": training.base_hf_model,
        "lora_r": training.lora_r,
        "lora_alpha": training.lora_alpha,
        "lora_dropout": training.lora_dropout,
        "learning_rate": training.learning_rate,
        "epochs": training.epochs,
        "batch_size": training.batch_size,
        "grad_accum": training.grad_accum,
    }
    stored = json.loads(row["hyperparams_json"]) if row["hyperparams_json"] else {}
    return {**defaults, **stored}


def _baseline_row(run: RunContext, base_model_id: str | None) -> sqlite3.Row:
    row = run.conn.execute("SELECT * FROM models WHERE is_active = 1").fetchone()
    if row is None and base_model_id:
        row = run.conn.execute(
            "SELECT * FROM models WHERE id = ?", (base_model_id,)
        ).fetchone()
    if row is None:
        raise RuntimeError("no active model to evaluate against")
    return row


def _register_model(
    run: RunContext, model_id: str, ct2_dir: Path, hf_source: str,
    parent_model_id: str, eval_wer: float | None,
) -> None:
    run.conn.execute(
        """INSERT INTO models (id, kind, ct2_path, hf_source, parent_model_id, training_run_id,
           eval_wer, is_active, created_at)
           VALUES (?, 'finetuned', ?, ?, ?, ?, ?, 0, ?)""",
        (model_id, str(ct2_dir), hf_source, parent_model_id, run.run_id, eval_wer, utcnow()),
    )
    run.conn.commit()


def _activate(cfg: Config, model_id: str) -> None:
    url = f"http://127.0.0.1:{cfg.server.port}/v1/models/{model_id}/activate"
    try:
        response = httpx.post(url, timeout=120.0)  # model load takes seconds
        response.raise_for_status()
        log.info("activated %s", model_id)
    except httpx.HTTPError as exc:
        log.warning(
            "activate call failed (%s); %s is registered but inactive — "
            "activate it from the dashboard", exc, model_id,
        )


def _cleanup(run_dir: Path) -> None:
    """Drop the huge intermediates; keep adapter/ and train.log for inspection."""
    shutil.rmtree(run_dir / "merged", ignore_errors=True)
    for checkpoint in run_dir.glob("checkpoint-*"):
        shutil.rmtree(checkpoint, ignore_errors=True)


def _execute(run: RunContext, cfg: Config, row: sqlite3.Row, run_dir: Path) -> None:
    from . import convert, evaluate, train_lora  # heavy deps live behind these modules

    hp = _hyperparams(row, cfg)
    log.info("run %s starting with hyperparams %s", run.run_id, hp)

    # -- building -------------------------------------------------------------
    run.update(status="building", progress=0.02)
    split = dataset.build_datasets(run.conn)
    if not split.train:
        raise RuntimeError("no eligible training samples")
    run.update(
        n_train=len(split.train),
        n_eval=len(split.eval),
        train_minutes=round(split.train_minutes, 2),
    )

    # -- training -------------------------------------------------------------
    run.update(status="training", progress=0.1)
    last_progress = 0.1

    def on_progress(fraction: float) -> None:
        nonlocal last_progress
        progress = 0.1 + 0.6 * min(max(fraction, 0.0), 1.0)
        if progress - last_progress >= 0.005:
            last_progress = progress
            run.update(progress=round(progress, 4))

    adapter_dir = train_lora.train_adapter(
        split.train, run_dir, hp,
        device=cfg.asr.device, language=cfg.asr.language, on_progress=on_progress,
    )

    # -- converting (before eval, so both models are scored through CT2) ------
    run.update(status="converting", progress=0.72)
    merged_dir = convert.merge_adapter(
        hp["base_hf_model"], adapter_dir, run_dir / "merged", cfg.asr.device
    )
    model_id = promote.new_model_id(run.run_id)
    ct2_dir = (cfg.models_dir / model_id).resolve()
    convert.convert_to_ct2(merged_dir, ct2_dir)
    run.update(progress=0.8)
    convert.smoke_test(ct2_dir, split.train[0].audio_path, cfg.asr)

    # -- evaluating -----------------------------------------------------------
    run.update(status="evaluating", progress=0.85)
    baseline = _baseline_row(run, row["base_model_id"])
    baseline_source = evaluate.resolve_model_source(
        baseline["id"], baseline["ct2_path"], baseline["hf_source"]
    )
    wer_baseline = wer_candidate = None
    if split.eval:
        wer_baseline, wer_candidate = evaluate.evaluate_models(
            baseline_source, str(ct2_dir), split.eval, cfg.asr
        )
        run.update(wer_baseline=round(wer_baseline, 3), wer_candidate=round(wer_candidate, 3))

    # -- promotion gate -------------------------------------------------------
    ok, reason = promote.should_promote(
        wer_baseline if wer_baseline is not None else 0.0,
        wer_candidate if wer_candidate is not None else 0.0,
        len(split.eval),
        cfg.training.promotion_margin_wer,
        cfg.training.min_eval_samples,
    )
    log.info("promotion decision: %s — %s", "promote" if ok else "reject", reason)
    if ok:
        _register_model(run, model_id, ct2_dir, hp["base_hf_model"], baseline["id"], wer_candidate)
        run.update(
            status="promoted", produced_model_id=model_id, progress=1.0, finished_at=utcnow()
        )
        _activate(cfg, model_id)
        pruned = promote.prune_finetuned(run.conn, cfg.training.keep_finetuned_models)
        if pruned:
            log.info("pruned %d old fine-tuned model(s): %s", len(pruned), ", ".join(pruned))
    else:
        # keep runs/<id> and the converted CT2 dir for inspection; no models row
        run.update(status="rejected", progress=1.0, finished_at=utcnow())

    _cleanup(run_dir)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="una fine-tuning runner")
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
        log.exception("training run %s failed", args.run_id)
        run.update(status="failed", error=str(exc), finished_at=utcnow())
        return 1
    finally:
        conn.close()


if __name__ == "__main__":
    sys.exit(main())
