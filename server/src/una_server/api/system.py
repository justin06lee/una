"""Health + usage statistics."""

from __future__ import annotations

import shutil
import subprocess
from datetime import UTC, date, datetime, timedelta
from itertools import pairwise
from typing import Annotated

from fastapi import APIRouter, Depends

from ..schemas import (
    Health,
    Stats,
    StatsApp,
    StatsCleanup,
    StatsDay,
    StatsReview,
    StatsStreak,
    StatsTotals,
    TeacherStatus,
    WerPoint,
)
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
            capture_output=True, text=True, timeout=2, check=False,
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


def _word_count(expr: str) -> str:
    """SQL word count for a text expression.

    Counts space-delimited runs after flattening newlines and collapsing double
    spaces. Transcripts are single-spaced prose, so the error is negligible and
    the whole aggregation stays inside SQLite rather than pulling every
    transcript into Python.
    """
    flat = (
        "trim(replace(replace(replace(replace("
        f"coalesce({expr}, ''), char(10), ' '), char(13), ' '), '  ', ' '), '  ', ' '))"
    )
    return (
        f"CASE WHEN {flat} = '' THEN 0 "
        f"ELSE length({flat}) - length(replace({flat}, ' ', '')) + 1 END"
    )


# The text the user actually received: cleaned when cleanup succeeded, else raw.
FINAL_TEXT = "CASE WHEN d.cleanup_applied = 1 THEN d.cleaned_text ELSE d.raw_text END"
FINAL_WORDS = _word_count(FINAL_TEXT)
RAW_WORDS = _word_count("d.raw_text")
CLEANED_WORDS = _word_count("d.cleaned_text")


def _streaks(days: list[str], today: date) -> StatsStreak:
    """Current and longest run of consecutive UTC days with at least one dictation.

    The current streak counts back from today, tolerating a gap of one day so a
    streak isn't reported as broken before the user has dictated today.
    """
    if not days:
        return StatsStreak(current=0, longest=0)
    parsed = sorted({date.fromisoformat(d) for d in days})

    longest = run = 1
    for previous, current in pairwise(parsed):
        run = run + 1 if current - previous == timedelta(days=1) else 1
        longest = max(longest, run)

    latest = parsed[-1]
    if (today - latest).days > 1:
        return StatsStreak(current=0, longest=longest)
    current_streak = 1
    seen = set(parsed)
    cursor = latest
    while cursor - timedelta(days=1) in seen:
        cursor -= timedelta(days=1)
        current_streak += 1
    return StatsStreak(current=current_streak, longest=longest)


@router.get("/stats", response_model=Stats)
async def stats(state: State) -> Stats:
    async with state.db.execute(
        f"""SELECT substr(d.created_at, 1, 10) AS day, COUNT(*) AS n,
                   COALESCE(SUM(d.duration_ms), 0) AS ms,
                   COALESCE(SUM({FINAL_WORDS}), 0) AS words
            FROM dictations d WHERE d.deleted = 0
            GROUP BY day ORDER BY day DESC LIMIT 400"""
    ) as cur:
        per_day = [StatsDay(**dict(r)) for r in await cur.fetchall()]

    async with state.db.execute(
        f"""SELECT COALESCE(NULLIF(TRIM(d.app_name), ''), 'Unknown') AS app,
                   COUNT(*) AS n, COALESCE(SUM(d.duration_ms), 0) AS ms,
                   COALESCE(SUM({FINAL_WORDS}), 0) AS words
            FROM dictations d WHERE d.deleted = 0
            GROUP BY app ORDER BY words DESC, n DESC LIMIT 12"""
    ) as cur:
        by_app = [StatsApp(**dict(r)) for r in await cur.fetchall()]

    async with state.db.execute(
        f"""SELECT COUNT(*) AS dictations, COALESCE(SUM(d.duration_ms), 0) AS ms,
                   COALESCE(SUM({FINAL_WORDS}), 0) AS words
            FROM dictations d WHERE d.deleted = 0"""
    ) as cur:
        totals_row = await cur.fetchone()

    # Filler words and false starts the cleanup pass stripped out. Clamped at 0
    # per row so a cleanup that legitimately expanded the text can't subtract.
    async with state.db.execute(
        f"""SELECT COUNT(*) AS applied,
                   COALESCE(SUM(MAX({RAW_WORDS} - {CLEANED_WORDS}, 0)), 0) AS words_removed
            FROM dictations d WHERE d.deleted = 0 AND d.cleanup_applied = 1"""
    ) as cur:
        cleanup_row = await cur.fetchone()

    async with state.db.execute(
        "SELECT COALESCE(SUM(hit_count), 0) AS hits FROM dictionary_entries"
    ) as cur:
        dictionary_hits = (await cur.fetchone())["hits"]

    async with state.db.execute(
        """SELECT
             SUM(CASE WHEN c.action IS NULL OR c.action = 'skipped' THEN 1 ELSE 0 END) AS backlog,
             SUM(CASE WHEN c.action IS NOT NULL AND c.action != 'skipped' THEN 1 ELSE 0 END) AS reviewed,
             SUM(CASE WHEN c.training_eligible = 1 THEN 1 ELSE 0 END) AS eligible
           FROM dictations d LEFT JOIN corrections c ON c.dictation_id = d.id
           WHERE d.deleted = 0"""
    ) as cur:
        review_row = await cur.fetchone()

    async with state.db.execute(
        """SELECT id, finished_at, wer_baseline, wer_candidate, status
           FROM training_runs WHERE wer_candidate IS NOT NULL AND kind = 'asr'
           ORDER BY id ASC"""
    ) as cur:
        wer_series = [WerPoint(**dict(r)) for r in await cur.fetchall()]

    total_ms = totals_row["ms"]
    total_words = totals_row["words"]
    minutes = total_ms / 60_000
    totals = StatsTotals(
        dictations=totals_row["dictations"],
        words=total_words,
        ms=total_ms,
        avg_wpm=round(total_words / minutes, 1) if minutes > 0 else 0.0,
        days_active=len(per_day),
    )

    return Stats(
        totals=totals,
        streak=_streaks([d.day for d in per_day], datetime.now(UTC).date()),
        per_day=per_day,
        by_app=by_app,
        cleanup=StatsCleanup(
            applied=cleanup_row["applied"],
            words_removed=cleanup_row["words_removed"],
            dictionary_hits=dictionary_hits,
        ),
        review=StatsReview(
            backlog=review_row["backlog"] or 0,
            reviewed=review_row["reviewed"] or 0,
            eligible=review_row["eligible"] or 0,
        ),
        wer_series=wer_series,
    )


@router.get("/teacher", response_model=TeacherStatus)
async def teacher_status(state: State) -> TeacherStatus:
    teacher = state.teacher
    if teacher is None:
        return TeacherStatus(enabled=False)
    async with state.db.execute(
        "SELECT COUNT(*) AS n FROM teacher_labels WHERE status = 'partial'"
    ) as cur:
        partial = (await cur.fetchone())["n"]
    return TeacherStatus(
        enabled=True,
        second_asr_model=teacher.cfg.second_asr_model if teacher.second else None,
        llm_model=teacher.cfg.llm_model if teacher.llm else None,
        unlabeled=await teacher.pending(),
        partial=partial,
        last_error=teacher.last_error,
    )
