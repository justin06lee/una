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


async def launch_run(state: AppState):
    """Create a training_runs row, start the runner subprocess, pause serving if configured.

    Returns the inserted row. Raises RunActive if a run is already going.
    """
    run_id = str(ULID())
    hyper = {
        "base_hf_model": state.config.training.base_hf_model,
        "lora_r": state.config.training.lora_r,
        "lora_alpha": state.config.training.lora_alpha,
        "lora_dropout": state.config.training.lora_dropout,
        "learning_rate": state.config.training.learning_rate,
        "epochs": state.config.training.epochs,
        "batch_size": state.config.training.batch_size,
        "grad_accum": state.config.training.grad_accum,
    }
    log_path = state.config.runs_dir / run_id / "train.log"
    await state.db.execute(
        """INSERT INTO training_runs (id, status, base_model_id, hyperparams_json, log_path)
           VALUES (?, 'queued', ?, ?, ?)""",
        (run_id, state.models.active_model_id, json.dumps(hyper), str(log_path)),
    )
    await state.db.commit()
    try:
        await state.jobs.start(run_id)  # raises RunActive if one is already going
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
