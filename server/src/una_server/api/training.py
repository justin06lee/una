"""Training run lifecycle + eligibility stats."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Annotated

from fastapi import APIRouter, Depends
from fastapi.responses import PlainTextResponse
from ulid import ULID

from ..errors import NotFound
from ..schemas import Eligibility, TrainingRun
from ..state import AppState
from .deps import get_state

router = APIRouter(prefix="/training", tags=["training"])

State = Annotated[AppState, Depends(get_state)]

ELIGIBLE_SQL = """
SELECT COUNT(*) AS n, COALESCE(SUM(d.duration_ms), 0) AS total_ms
FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.training_eligible = 1 AND d.deleted = 0 AND d.eval_holdout = 0
"""


STYLE_SQL = """
SELECT COUNT(*) AS n FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.polished_text IS NOT NULL AND d.deleted = 0
"""


@router.get("/eligibility", response_model=Eligibility)
async def eligibility(state: State) -> Eligibility:
    async with state.db.execute(ELIGIBLE_SQL) as cur:
        row = await cur.fetchone()
    async with state.db.execute(STYLE_SQL) as cur:
        style = await cur.fetchone()
    minutes = row["total_ms"] / 60_000
    threshold = state.config.training.threshold_minutes
    return Eligibility(
        eligible_pairs=row["n"],
        eligible_minutes=round(minutes, 2),
        threshold_minutes=threshold,
        ready=minutes >= threshold,
        style_pairs=style["n"],
    )


def _run(row) -> TrainingRun:
    return TrainingRun(
        id=row["id"],
        status=row["status"],
        started_at=row["started_at"],
        finished_at=row["finished_at"],
        base_model_id=row["base_model_id"],
        produced_model_id=row["produced_model_id"],
        n_train=row["n_train"],
        n_eval=row["n_eval"],
        wer_baseline=row["wer_baseline"],
        wer_candidate=row["wer_candidate"],
        error=row["error"],
        progress=row["progress"],
    )


@router.get("/runs", response_model=list[TrainingRun])
async def list_runs(state: State) -> list[TrainingRun]:
    async with state.db.execute("SELECT * FROM training_runs ORDER BY id DESC LIMIT 50") as cur:
        return [_run(r) for r in await cur.fetchall()]


@router.post("/runs", response_model=TrainingRun, status_code=201)
async def start_run(state: State) -> TrainingRun:
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
    async with state.db.execute("SELECT * FROM training_runs WHERE id = ?", (run_id,)) as cur:
        return _run(await cur.fetchone())


async def _fetch_run(state: AppState, run_id: str):
    async with state.db.execute("SELECT * FROM training_runs WHERE id = ?", (run_id,)) as cur:
        row = await cur.fetchone()
    if row is None:
        raise NotFound(f"training run {run_id} not found")
    return row


@router.get("/runs/{run_id}", response_model=TrainingRun)
async def get_run(state: State, run_id: str) -> TrainingRun:
    return _run(await _fetch_run(state, run_id))


@router.get("/runs/{run_id}/log", response_class=PlainTextResponse)
async def run_log(state: State, run_id: str, tail: int = 200) -> str:
    row = await _fetch_run(state, run_id)
    path = Path(row["log_path"]) if row["log_path"] else None
    if path is None or not path.exists():
        return ""
    lines = path.read_text(errors="replace").splitlines()
    return "\n".join(lines[-max(1, min(tail, 5000)):])


@router.post("/runs/{run_id}/cancel", response_model=TrainingRun)
async def cancel_run(state: State, run_id: str) -> TrainingRun:
    await _fetch_run(state, run_id)
    await state.jobs.cancel(run_id)
    if state.config.training.pause_serving and not state.models.loaded:
        await state.models.load_active()
    return _run(await _fetch_run(state, run_id))
