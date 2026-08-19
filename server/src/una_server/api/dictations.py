"""Dictation ingest (the money path), history, corrections, review queue."""

from __future__ import annotations

import time
from pathlib import Path
from typing import Annotated

from fastapi import APIRouter, Depends, File, Form, UploadFile
from fastapi.responses import FileResponse
from ulid import ULID

from ..db import is_eval_holdout, utcnow
from ..errors import BadRequest, NotFound, TrainingInProgress
from ..schemas import (
    CorrectionRequest,
    CorrectionResponse,
    DictationDetail,
    DictationList,
    DictationResponse,
    DictationSummary,
    Timings,
)
from ..services import storage
from ..state import AppState
from ..training.filters import check_pair
from .deps import get_state

router = APIRouter(tags=["dictations"])

State = Annotated[AppState, Depends(get_state)]


async def _active_phrases(state: AppState) -> tuple[list[str], list[str]]:
    """(bare phrases for Whisper biasing, phrases with sounds-like hints for the cleaner).

    Sounds-like hints stay out of the Whisper initial_prompt — its hard character
    budget is for vocabulary, and prompt terms can be hallucinated verbatim — but
    they help the cleanup LLM fix predictable mishearings.
    """
    async with state.db.execute(
        "SELECT phrase, sounds_like FROM dictionary_entries WHERE active = 1 "
        "ORDER BY hit_count DESC, created_at ASC"
    ) as cur:
        rows = await cur.fetchall()
    phrases = [row["phrase"] for row in rows]
    hinted = [
        f'{row["phrase"]} (often misheard as "{row["sounds_like"]}")'
        if row["sounds_like"]
        else row["phrase"]
        for row in rows
    ]
    return phrases, hinted


async def _bump_hit_counts(state: AppState, phrases: list[str], text: str) -> None:
    lowered = text.lower()
    hits = [p for p in phrases if p.lower() in lowered]
    for phrase in hits:
        await state.db.execute(
            "UPDATE dictionary_entries SET hit_count = hit_count + 1 WHERE phrase = ?", (phrase,)
        )
    if hits:
        await state.db.commit()


def _row_to_response(row) -> DictationResponse:
    text = row["cleaned_text"] if row["cleanup_applied"] else row["raw_text"]
    return DictationResponse(
        id=row["id"],
        text=text,
        raw_text=row["raw_text"],
        cleaned_text=row["cleaned_text"],
        cleanup_applied=bool(row["cleanup_applied"]),
        cleanup_error=row["cleanup_error"],
        duration_ms=row["duration_ms"],
        timings=Timings(
            transcribe_ms=row["transcribe_ms"] or 0,
            cleanup_ms=row["cleanup_ms"],
            total_ms=(row["transcribe_ms"] or 0) + (row["cleanup_ms"] or 0),
        ),
        asr_model=row["asr_model_id"] or "unknown",
        llm_model=row["llm_model"],
    )


@router.post("/dictations", response_model=DictationResponse)
async def create_dictation(
    state: State,
    audio: UploadFile = File(...),
    app_name: str | None = Form(None),
    clean: bool = Form(True),
    language: str | None = Form(None),
    utterance_id: str | None = Form(None),
    client: str | None = Form(None),
) -> DictationResponse:
    started = time.monotonic()

    # Idempotent retry: same utterance_id -> return the stored result.
    if utterance_id:
        async with state.db.execute(
            "SELECT * FROM dictations WHERE utterance_id = ? AND deleted = 0", (utterance_id,)
        ) as cur:
            existing = await cur.fetchone()
        if existing is not None:
            return _row_to_response(existing)

    if not state.models.loaded:
        raise TrainingInProgress("ASR model unloaded (training in progress); retry shortly")

    samples = storage.decode_to_16k_mono(await audio.read())
    duration = storage.duration_ms(samples)

    phrases, hinted_phrases = await _active_phrases(state)
    from ..services.prompts import build_initial_prompt

    initial_prompt = build_initial_prompt(phrases, state.config.asr.initial_prompt_max_chars)

    t0 = time.monotonic()
    async with state.models.lock:
        result = await state.models.transcriber.transcribe(
            samples, language=language, initial_prompt=initial_prompt
        )
    transcribe_ms = int((time.monotonic() - t0) * 1000)

    cleanup = None
    if clean and result.text.strip():
        cleanup = await state.cleaner.clean(result.text, hinted_phrases, app_name)

    dictation_id = str(ULID())
    audio_path = storage.save_wav(state.config.audio_dir, dictation_id, samples)

    await state.db.execute(
        """INSERT INTO dictations
           (id, created_at, app_name, audio_path, duration_ms, sample_rate, language,
            raw_text, cleaned_text, cleanup_applied, cleanup_error, asr_model_id, llm_model,
            transcribe_ms, cleanup_ms, utterance_id, eval_holdout, client)
           VALUES (?, ?, ?, ?, ?, 16000, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
        (
            dictation_id,
            utcnow(),
            app_name,
            str(audio_path),
            duration,
            result.language,
            result.text,
            cleanup.text if cleanup and cleanup.applied else None,
            1 if cleanup and cleanup.applied else 0,
            cleanup.error if cleanup else None,
            state.models.active_model_id,
            cleanup.model if cleanup else None,
            transcribe_ms,
            cleanup.elapsed_ms if cleanup else None,
            utterance_id,
            1 if is_eval_holdout(dictation_id) else 0,
            client,
        ),
    )
    await state.db.commit()
    await _bump_hit_counts(state, phrases, result.text)
    state.last_dictation_at = time.monotonic()

    final_text = cleanup.text if cleanup and cleanup.applied else result.text
    return DictationResponse(
        id=dictation_id,
        text=final_text,
        raw_text=result.text,
        cleaned_text=cleanup.text if cleanup and cleanup.applied else None,
        cleanup_applied=bool(cleanup and cleanup.applied),
        cleanup_error=cleanup.error if cleanup else None,
        duration_ms=duration,
        timings=Timings(
            transcribe_ms=transcribe_ms,
            cleanup_ms=cleanup.elapsed_ms if cleanup else None,
            total_ms=int((time.monotonic() - started) * 1000),
        ),
        asr_model=state.models.active_model_id or "unknown",
        llm_model=cleanup.model if cleanup else None,
    )


def _summary(row) -> DictationSummary:
    return DictationSummary(
        id=row["id"],
        created_at=row["created_at"],
        app_name=row["app_name"],
        duration_ms=row["duration_ms"],
        text=(row["cleaned_text"] if row["cleanup_applied"] else row["raw_text"]) or "",
        reviewed=row["review_action"] is not None and row["review_action"] != "skipped",
        review_action=row["review_action"],
    )


LIST_SQL = """
SELECT d.*, c.action AS review_action, c.corrected_text, c.polished_text, c.training_eligible, c.eligibility_reason
FROM dictations d LEFT JOIN corrections c ON c.dictation_id = d.id
WHERE d.deleted = 0
"""


@router.get("/dictations", response_model=DictationList)
async def list_dictations(
    state: State,
    limit: int = 50,
    cursor: str | None = None,
    q: str | None = None,
    app: str | None = None,
    reviewed: bool | None = None,
) -> DictationList:
    sql, params = LIST_SQL, []
    if cursor:
        sql += " AND d.id < ?"
        params.append(cursor)
    if q:
        sql += " AND (d.raw_text LIKE ? OR d.cleaned_text LIKE ? OR c.corrected_text LIKE ?)"
        params += [f"%{q}%"] * 3
    if app:
        sql += " AND d.app_name = ?"
        params.append(app)
    if reviewed is True:
        sql += " AND c.action IS NOT NULL AND c.action != 'skipped'"
    elif reviewed is False:
        sql += " AND (c.action IS NULL OR c.action = 'skipped')"
    sql += " ORDER BY d.id DESC LIMIT ?"
    params.append(min(limit, 200) + 1)

    async with state.db.execute(sql, params) as cur:
        rows = await cur.fetchall()
    items = [_summary(r) for r in rows[: min(limit, 200)]]
    next_cursor = items[-1].id if len(rows) > min(limit, 200) else None
    return DictationList(items=items, next_cursor=next_cursor)


@router.get("/review/next", response_model=DictationDetail | None)
async def review_next(state: State, after: str | None = None):
    sql = LIST_SQL + " AND (c.action IS NULL OR c.action = 'skipped')"
    params: list = []
    if after:
        sql += " AND d.id > ?"
        params.append(after)
    sql += " ORDER BY d.id ASC LIMIT 1"
    async with state.db.execute(sql, params) as cur:
        row = await cur.fetchone()
    return _detail(row) if row else None


def _detail(row) -> DictationDetail:
    return DictationDetail(
        **_summary(row).model_dump(),
        raw_text=row["raw_text"],
        cleaned_text=row["cleaned_text"],
        cleanup_applied=bool(row["cleanup_applied"]),
        asr_model=row["asr_model_id"],
        llm_model=row["llm_model"],
        language=row["language"],
        corrected_text=row["corrected_text"],
        polished_text=row["polished_text"],
        training_eligible=bool(row["training_eligible"]) if row["training_eligible"] is not None else None,
        eligibility_reason=row["eligibility_reason"],
        eval_holdout=bool(row["eval_holdout"]),
    )


async def _fetch_detail_row(state: AppState, dictation_id: str):
    async with state.db.execute(LIST_SQL + " AND d.id = ?", (dictation_id,)) as cur:
        row = await cur.fetchone()
    if row is None:
        raise NotFound(f"dictation {dictation_id} not found")
    return row


@router.get("/dictations/{dictation_id}", response_model=DictationDetail)
async def get_dictation(state: State, dictation_id: str) -> DictationDetail:
    return _detail(await _fetch_detail_row(state, dictation_id))


@router.get("/dictations/{dictation_id}/audio")
async def get_audio(state: State, dictation_id: str) -> FileResponse:
    row = await _fetch_detail_row(state, dictation_id)
    path = Path(row["audio_path"])
    if not path.exists():
        raise NotFound("audio file missing")
    return FileResponse(path, media_type="audio/wav")


@router.put("/dictations/{dictation_id}/correction", response_model=CorrectionResponse)
async def put_correction(
    state: State, dictation_id: str, body: CorrectionRequest
) -> CorrectionResponse:
    row = await _fetch_detail_row(state, dictation_id)
    if body.action == "edited" and not (body.corrected_text or "").strip():
        raise BadRequest("action 'edited' requires corrected_text")

    corrected = row["raw_text"] if body.action == "accepted" else body.corrected_text
    # only touch the stored style pair when the request explicitly sends the field —
    # a skip/exclude that omits it must not wipe an earlier polish
    polished_provided = "polished_text" in body.model_fields_set
    polished = (body.polished_text or "").strip() or None
    result = check_pair(
        action=body.action,
        raw_text=row["raw_text"],
        corrected_text=corrected,
        duration_ms=row["duration_ms"],
        max_edit_distance=state.config.training.max_edit_distance,
    )
    now = utcnow()
    await state.db.execute(
        """INSERT INTO corrections
           (id, dictation_id, corrected_text, polished_text, action, norm_edit_distance,
            training_eligible, eligibility_reason, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(dictation_id) DO UPDATE SET
             corrected_text = excluded.corrected_text,
             polished_text = CASE WHEN ? THEN excluded.polished_text
                             ELSE corrections.polished_text END,
             action = excluded.action,
             norm_edit_distance = excluded.norm_edit_distance,
             training_eligible = excluded.training_eligible,
             eligibility_reason = excluded.eligibility_reason,
             updated_at = excluded.updated_at""",
        (
            str(ULID()),
            dictation_id,
            corrected,
            polished,
            body.action,
            result.distance,
            1 if result.eligible else 0,
            result.reason,
            now,
            now,
            1 if polished_provided else 0,
        ),
    )
    await state.db.commit()
    if not polished_provided:
        async with state.db.execute(
            "SELECT polished_text FROM corrections WHERE dictation_id = ?", (dictation_id,)
        ) as cur:
            polished = (await cur.fetchone())["polished_text"]
    return CorrectionResponse(
        dictation_id=dictation_id,
        action=body.action,
        norm_edit_distance=result.distance,
        training_eligible=result.eligible,
        eligibility_reason=result.reason,
        polished_text=polished,
    )


@router.delete("/dictations/{dictation_id}", status_code=204)
async def delete_dictation(state: State, dictation_id: str) -> None:
    row = await _fetch_detail_row(state, dictation_id)
    await state.db.execute("UPDATE dictations SET deleted = 1 WHERE id = ?", (dictation_id,))
    await state.db.commit()
    path = Path(row["audio_path"])
    if path.exists():
        path.unlink()
