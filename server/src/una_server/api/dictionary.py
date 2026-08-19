"""Personal dictionary CRUD — feeds Whisper initial_prompt and the cleanup prompt."""

from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends
from ulid import ULID

from ..db import utcnow
from ..errors import BadRequest, NotFound
from ..schemas import DictionaryCreate, DictionaryEntry, DictionaryPatch
from ..state import AppState
from .deps import get_state

router = APIRouter(tags=["dictionary"])

State = Annotated[AppState, Depends(get_state)]


def _entry(row) -> DictionaryEntry:
    return DictionaryEntry(
        id=row["id"],
        phrase=row["phrase"],
        sounds_like=row["sounds_like"],
        notes=row["notes"],
        active=bool(row["active"]),
        hit_count=row["hit_count"],
    )


@router.get("/dictionary", response_model=list[DictionaryEntry])
async def list_entries(state: State) -> list[DictionaryEntry]:
    async with state.db.execute(
        "SELECT * FROM dictionary_entries ORDER BY hit_count DESC, created_at ASC"
    ) as cur:
        return [_entry(r) for r in await cur.fetchall()]


@router.post("/dictionary", response_model=DictionaryEntry, status_code=201)
async def create_entry(state: State, body: DictionaryCreate) -> DictionaryEntry:
    phrase = body.phrase.strip()
    if not phrase:
        raise BadRequest("phrase must be non-empty")
    async with state.db.execute(
        "SELECT id FROM dictionary_entries WHERE phrase = ?", (phrase,)
    ) as cur:
        if await cur.fetchone():
            raise BadRequest(f"phrase already exists: {phrase}")
    entry_id = str(ULID())
    await state.db.execute(
        "INSERT INTO dictionary_entries (id, phrase, sounds_like, notes, created_at) VALUES (?, ?, ?, ?, ?)",
        (entry_id, phrase, body.sounds_like, body.notes, utcnow()),
    )
    await state.db.commit()
    async with state.db.execute("SELECT * FROM dictionary_entries WHERE id = ?", (entry_id,)) as cur:
        return _entry(await cur.fetchone())


@router.patch("/dictionary/{entry_id}", response_model=DictionaryEntry)
async def patch_entry(state: State, entry_id: str, body: DictionaryPatch) -> DictionaryEntry:
    async with state.db.execute("SELECT * FROM dictionary_entries WHERE id = ?", (entry_id,)) as cur:
        row = await cur.fetchone()
    if row is None:
        raise NotFound(f"dictionary entry {entry_id} not found")
    updates = {k: v for k, v in body.model_dump(exclude_unset=True).items()}
    if "active" in updates:
        updates["active"] = 1 if updates["active"] else 0
    if updates:
        sets = ", ".join(f"{k} = ?" for k in updates)
        await state.db.execute(
            f"UPDATE dictionary_entries SET {sets} WHERE id = ?", (*updates.values(), entry_id)
        )
        await state.db.commit()
    async with state.db.execute("SELECT * FROM dictionary_entries WHERE id = ?", (entry_id,)) as cur:
        return _entry(await cur.fetchone())


@router.delete("/dictionary/{entry_id}", status_code=204)
async def delete_entry(state: State, entry_id: str) -> None:
    cur = await state.db.execute("DELETE FROM dictionary_entries WHERE id = ?", (entry_id,))
    await state.db.commit()
    if cur.rowcount == 0:
        raise NotFound(f"dictionary entry {entry_id} not found")
