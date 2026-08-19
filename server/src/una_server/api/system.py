"""Health + stats."""

from __future__ import annotations

import shutil
import subprocess
from typing import Annotated

from fastapi import APIRouter, Depends

from ..schemas import Health
from ..state import AppState
from .deps import get_state

router = APIRouter(tags=["system"])

State = Annotated[AppState, Depends(get_state)]


def _gpu_info() -> dict | None:
    if not shutil.which("nvidia-smi"):
        return None
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=name,memory.free", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=2,
        )
        name, free = out.stdout.strip().split("\n")[0].split(", ")
        return {"name": name, "vram_free_mb": int(free)}
    except Exception:
        return None


@router.get("/health", response_model=Health)
async def health(state: State) -> Health:
    return Health(
        status="ok" if state.models.loaded else "degraded",
        asr_model=state.models.active_model_id,
        asr_model_loaded=state.models.loaded,
        ollama="ok" if await state.cleaner.ping() else "unreachable",
        training_active=state.jobs.active,
        gpu=_gpu_info(),
    )


@router.get("/stats")
async def stats(state: State) -> dict:
    async with state.db.execute(
        """SELECT substr(created_at, 1, 10) AS day, COUNT(*) AS n, SUM(duration_ms) AS ms
           FROM dictations WHERE deleted = 0 GROUP BY day ORDER BY day DESC LIMIT 60"""
    ) as cur:
        per_day = [dict(r) for r in await cur.fetchall()]
    async with state.db.execute(
        """SELECT COUNT(*) AS n FROM dictations d
           LEFT JOIN corrections c ON c.dictation_id = d.id
           WHERE d.deleted = 0 AND (c.action IS NULL OR c.action = 'skipped')"""
    ) as cur:
        backlog = (await cur.fetchone())["n"]
    async with state.db.execute(
        """SELECT id, finished_at, wer_baseline, wer_candidate, status
           FROM training_runs WHERE wer_candidate IS NOT NULL AND kind = 'asr'
           ORDER BY id ASC"""
    ) as cur:
        wer_series = [dict(r) for r in await cur.fetchall()]
    return {"per_day": per_day, "review_backlog": backlog, "wer_series": wer_series}
