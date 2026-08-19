"""Model registry: list, promote/rollback (both are 'activate')."""

from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends

from ..schemas import ModelInfo
from ..state import AppState
from .deps import get_state

router = APIRouter(tags=["models"])

State = Annotated[AppState, Depends(get_state)]


def _model(row) -> ModelInfo:
    return ModelInfo(
        id=row["id"],
        kind=row["kind"],
        parent_model_id=row["parent_model_id"],
        training_run_id=row["training_run_id"],
        eval_wer=row["eval_wer"],
        is_active=bool(row["is_active"]),
        created_at=row["created_at"],
        notes=row["notes"],
    )


@router.get("/models", response_model=list[ModelInfo])
async def list_models(state: State) -> list[ModelInfo]:
    async with state.db.execute("SELECT * FROM models ORDER BY created_at DESC") as cur:
        return [_model(r) for r in await cur.fetchall()]


@router.post("/models/{model_id}/activate", response_model=ModelInfo)
async def activate_model(state: State, model_id: str) -> ModelInfo:
    await state.models.swap(model_id)
    async with state.db.execute("SELECT * FROM models WHERE id = ?", (model_id,)) as cur:
        return _model(await cur.fetchone())
