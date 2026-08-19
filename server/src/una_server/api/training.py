"""Training run lifecycle + eligibility stats."""

from __future__ import annotations

from pathlib import Path
from typing import Annotated

from fastapi import APIRouter, Depends
from fastapi.responses import PlainTextResponse

from ..errors import NotFound
from ..schemas import Eligibility, StartRunRequest, TrainingRun
from ..services import runs
from ..state import AppState
from .deps import get_state

router = APIRouter(prefix="/training", tags=["training"])

State = Annotated[AppState, Depends(get_state)]


@router.get("/eligibility", response_model=Eligibility)
async def eligibility(state: State) -> Eligibility:
    pairs, minutes = await runs.eligible_counts(state)
    async with state.db.execute(runs.STYLE_SQL) as cur:
        style = await cur.fetchone()
    threshold = state.config.training.threshold_minutes
    style_threshold = state.config.training.style_threshold_pairs
    return Eligibility(
        eligible_pairs=pairs,
        eligible_minutes=round(minutes, 2),
        threshold_minutes=threshold,
        ready=minutes >= threshold,
        style_pairs=style["n"],
        style_threshold_pairs=style_threshold,
        style_ready=style["n"] >= style_threshold,
    )


def _run(row) -> TrainingRun:
    return TrainingRun(
        id=row["id"],
        kind=row["kind"],
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
async def start_run(state: State, body: StartRunRequest | None = None) -> TrainingRun:
    kind = body.kind if body is not None else "asr"
    return _run(await runs.launch_run(state, kind))


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
