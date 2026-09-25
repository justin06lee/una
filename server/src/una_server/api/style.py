"""The style corpus: the user's own writing, back-translated into cleanup training pairs."""

from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends
from ulid import ULID

from ..db import utcnow
from ..errors import BadRequest
from ..schemas import (
    Calibration,
    CorpusKnown,
    CorpusKnownRequest,
    CorpusStats,
    CorpusUpload,
    CorpusUploadResult,
)
from ..state import AppState
from ..training.backtranslate import content_hash, is_holdout
from .deps import get_state

router = APIRouter(prefix="/style", tags=["style"])

State = Annotated[AppState, Depends(get_state)]


@router.post("/corpus", response_model=CorpusUploadResult)
async def upload_corpus(state: State, body: CorpusUpload) -> CorpusUploadResult:
    """Add writing samples. Idempotent on the text; a later upload may fill in a
    back-translation for a sample that arrived without one, never overwrite one."""
    if len(body.items) > 1000:
        raise BadRequest("at most 1000 items per upload")
    result = CorpusUploadResult()
    now = utcnow()
    for item in body.items:
        written = item.written_text.strip()
        if not written:
            continue
        key = content_hash(written)
        finished = bool(item.spoken_text and item.target_text)
        async with state.db.execute(
            "SELECT id, spoken_text FROM style_corpus WHERE content_hash = ?", (key,)
        ) as cur:
            existing = await cur.fetchone()
        if existing is None:
            await state.db.execute(
                """INSERT INTO style_corpus
                   (id, source, app_name, written_text, content_hash, target_text, spoken_text,
                    generator_model, eval_holdout, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                (
                    str(ULID()), item.source, item.app_name, written, key,
                    item.target_text if finished else None,
                    item.spoken_text if finished else None,
                    item.generator_model if finished else None,
                    1 if is_holdout(key) else 0, now, now,
                ),
            )
            result.added += 1
        elif finished and existing["spoken_text"] is None:
            await state.db.execute(
                """UPDATE style_corpus SET target_text = ?, spoken_text = ?, generator_model = ?,
                   updated_at = ? WHERE id = ?""",
                (item.target_text, item.spoken_text, item.generator_model, now, existing["id"]),
            )
            result.updated += 1
        else:
            result.unchanged += 1
    await state.db.commit()
    return result


@router.post("/corpus/known", response_model=CorpusKnown)
async def known(state: State, body: CorpusKnownRequest) -> CorpusKnown:
    finished: list[str] = []
    for start in range(0, len(body.hashes), 500):
        chunk = body.hashes[start:start + 500]
        placeholders = ",".join("?" * len(chunk))
        async with state.db.execute(
            f"SELECT content_hash FROM style_corpus WHERE spoken_text IS NOT NULL "
            f"AND content_hash IN ({placeholders})",
            chunk,
        ) as cur:
            finished += [row["content_hash"] for row in await cur.fetchall()]
    return CorpusKnown(finished=finished)


@router.get("/corpus/stats", response_model=CorpusStats)
async def corpus_stats(state: State) -> CorpusStats:
    async with state.db.execute(
        """SELECT COUNT(*) AS samples, COUNT(spoken_text) AS finished,
                  COALESCE(SUM(CASE WHEN spoken_text IS NOT NULL THEN eval_holdout END), 0)
                    AS holdout
           FROM style_corpus"""
    ) as cur:
        row = await cur.fetchone()
    return CorpusStats(samples=row["samples"], finished=row["finished"], holdout=row["holdout"])


@router.get("/calibration", response_model=Calibration)
async def calibration(state: State, n: int = 6) -> Calibration:
    """A few real transcripts, so back-translation can match how Whisper writes this voice."""
    async with state.db.execute(
        """SELECT raw_text FROM dictations
           WHERE deleted = 0 AND LENGTH(raw_text) - LENGTH(REPLACE(raw_text, ' ', '')) >= 7
           ORDER BY id DESC LIMIT ?""",
        (max(0, min(n, 20)),),
    ) as cur:
        return Calibration(transcripts=[row["raw_text"] for row in await cur.fetchall()])
