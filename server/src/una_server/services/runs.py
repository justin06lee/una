"""Shared training-run launching + eligibility counts (used by the API and auto-training)."""

from __future__ import annotations

import json

from ulid import ULID

from ..state import AppState

ELIGIBLE_SQL = """
SELECT COUNT(*) AS n, COALESCE(SUM(d.duration_ms), 0) AS total_ms
FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.training_eligible = 1 AND d.deleted = 0 AND d.eval_holdout = 0
"""

STYLE_SQL = """
SELECT COUNT(*) AS n FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.polished_text IS NOT NULL AND d.deleted = 0
"""

WRITING_SQL = """
SELECT COUNT(*) AS n FROM style_corpus WHERE spoken_text IS NOT NULL AND target_text IS NOT NULL
"""

# Eligible pairs whose correction is newer than the last run attempt (any outcome).
# Auto-training only fires when this is non-zero, so it neither retrains in a loop
# on unchanged data nor hammers the GPU retrying a persistently failing run.
FRESH_SQL = """
SELECT COUNT(*) AS n
FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.training_eligible = 1 AND d.deleted = 0 AND d.eval_holdout = 0
  AND c.updated_at > COALESCE((SELECT MAX(started_at) FROM training_runs), '')
"""


async def eligible_counts(state: AppState) -> tuple[int, float]:
    """(eligible pair count, eligible minutes)."""
    async with state.db.execute(ELIGIBLE_SQL) as cur:
        row = await cur.fetchone()
    return row["n"], row["total_ms"] / 60_000


async def fresh_pair_count(state: AppState) -> int:
    async with state.db.execute(FRESH_SQL) as cur:
        row = await cur.fetchone()
    return row["n"]


RUNNER_MODULES = {
    "asr": "una_server.training.runner",
    "style": "una_server.training.style_runner",
}


def _hyperparams(state: AppState, kind: str) -> dict:
    training = state.config.training
    if kind == "style":
        return {k: v for k, v in training.model_dump().items() if k.startswith("style_")}
    return {
        "base_hf_model": training.base_hf_model,
        "lora_r": training.lora_r,
        "lora_alpha": training.lora_alpha,
        "lora_dropout": training.lora_dropout,
        "learning_rate": training.learning_rate,
        "epochs": training.epochs,
        "batch_size": training.batch_size,
        "grad_accum": training.grad_accum,
    }


async def launch_run(state: AppState, kind: str = "asr"):
    """Create a training_runs row, start the runner subprocess, pause serving if configured.

    Returns the inserted row. Raises RunActive if a run is already going. The single
    job slot is shared across kinds — one GPU, one run at a time.
    """
    run_id = str(ULID())
    # base_model_id references the ASR models table; style runs resolve their
    # baseline (cleanup.model) at execution time instead.
    base_model_id = None if kind == "style" else state.models.active_model_id
    log_path = state.config.runs_dir / run_id / "train.log"
    await state.db.execute(
        """INSERT INTO training_runs (id, kind, status, base_model_id, hyperparams_json, log_path)
           VALUES (?, ?, 'queued', ?, ?, ?)""",
        (run_id, kind, base_model_id, json.dumps(_hyperparams(state, kind)), str(log_path)),
    )
    await state.db.commit()
    try:
        # raises RunActive if one is already going
        await state.jobs.start(run_id, module=RUNNER_MODULES[kind])
    except Exception:
        await state.db.execute("DELETE FROM training_runs WHERE id = ?", (run_id,))
        await state.db.commit()
        raise
    if state.config.training.pause_serving:
        await state.models.unload()
    async with state.db.execute(
        "SELECT * FROM training_runs WHERE id = ?", (run_id,)
    ) as cur:
        return await cur.fetchone()
