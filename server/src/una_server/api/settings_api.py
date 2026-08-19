"""Runtime-mutable settings, persisted in the settings table, overlaid on una.toml at read time."""

from __future__ import annotations

import json
from typing import Annotated

from fastapi import APIRouter, Depends

from ..errors import BadRequest
from ..state import AppState
from .deps import get_state

router = APIRouter(tags=["settings"])

State = Annotated[AppState, Depends(get_state)]

# key -> (validator, applier). Appliers mutate live config so changes take effect immediately.
MUTABLE_KEYS = {
    "cleanup.enabled": bool,
    "cleanup.model": str,
    "cleanup.timeout_s": float,
    "training.threshold_minutes": float,
    "training.auto": bool,
    "training.auto_idle_minutes": float,
    "training.max_edit_distance": float,
}


def _apply(state: AppState, key: str, value) -> None:
    section, attr = key.split(".", 1)
    setattr(getattr(state.config, section), attr, value)


async def apply_stored_settings(state: AppState) -> None:
    """Called at startup: overlay persisted settings onto the loaded config."""
    async with state.db.execute("SELECT key, value FROM settings") as cur:
        rows = await cur.fetchall()
    for row in rows:
        if row["key"] in MUTABLE_KEYS:
            _apply(state, row["key"], json.loads(row["value"]))


@router.get("/settings")
async def get_settings(state: State) -> dict:
    return {
        key: getattr(getattr(state.config, key.split(".")[0]), key.split(".", 1)[1])
        for key in MUTABLE_KEYS
    }


@router.put("/settings")
async def put_settings(state: State, body: dict) -> dict:
    for key, value in body.items():
        if key not in MUTABLE_KEYS:
            raise BadRequest(f"unknown or immutable setting: {key}")
        expected = MUTABLE_KEYS[key]
        if expected is float and isinstance(value, int):
            value = float(value)
        if not isinstance(value, expected):
            raise BadRequest(f"{key} must be {expected.__name__}")
        _apply(state, key, value)
        await state.db.execute(
            "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, json.dumps(value)),
        )
    await state.db.commit()
    return await get_settings(state)
