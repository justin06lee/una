"""Pydantic API contract models."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel


class Timings(BaseModel):
    transcribe_ms: int
    cleanup_ms: int | None = None
    total_ms: int


class DictationResponse(BaseModel):
    id: str
    text: str
    raw_text: str
    cleaned_text: str | None = None
    cleanup_applied: bool
    cleanup_error: str | None = None
    duration_ms: int
    timings: Timings
    asr_model: str
    llm_model: str | None = None


class DictationSummary(BaseModel):
    id: str
    created_at: str
    app_name: str | None
    duration_ms: int
    text: str
    reviewed: bool
    review_action: str | None = None


class DictationDetail(DictationSummary):
    raw_text: str
    cleaned_text: str | None
    cleanup_applied: bool
    asr_model: str | None
    llm_model: str | None
    language: str | None
    corrected_text: str | None = None
    polished_text: str | None = None
    training_eligible: bool | None = None
    eligibility_reason: str | None = None
    eval_holdout: bool


class DictationList(BaseModel):
    items: list[DictationSummary]
    next_cursor: str | None = None


class CorrectionRequest(BaseModel):
    corrected_text: str | None = None
    action: Literal["accepted", "edited", "skipped", "excluded"]
    polished_text: str | None = None  # preferred final rendering -> style-LLM training pair


class CorrectionResponse(BaseModel):
    dictation_id: str
    action: str
    norm_edit_distance: float | None
    training_eligible: bool
    eligibility_reason: str | None
    polished_text: str | None = None


class DictionaryEntry(BaseModel):
    id: str
    phrase: str
    sounds_like: str | None = None
    notes: str | None = None
    active: bool = True
    hit_count: int = 0


class DictionaryCreate(BaseModel):
    phrase: str
    sounds_like: str | None = None
    notes: str | None = None


class DictionaryPatch(BaseModel):
    phrase: str | None = None
    sounds_like: str | None = None
    notes: str | None = None
    active: bool | None = None


class Eligibility(BaseModel):
    eligible_pairs: int
    eligible_minutes: float
    threshold_minutes: float
    ready: bool
    style_pairs: int = 0
    style_threshold_pairs: int = 0
    style_ready: bool = False


class StartRunRequest(BaseModel):
    kind: Literal["asr", "style"] = "asr"


class TrainingRun(BaseModel):
    id: str
    kind: str = "asr"
    status: str
    started_at: str | None
    finished_at: str | None
    base_model_id: str | None
    produced_model_id: str | None
    n_train: int | None
    n_eval: int | None
    wer_baseline: float | None
    wer_candidate: float | None
    error: str | None
    progress: float


class ModelInfo(BaseModel):
    id: str
    kind: str
    parent_model_id: str | None
    training_run_id: str | None
    eval_wer: float | None
    is_active: bool
    created_at: str
    notes: str | None


class Health(BaseModel):
    status: str
    asr_model: str | None
    asr_model_loaded: bool
    ollama: str
    training_active: bool
    gpu: dict | None = None


class StatsTotals(BaseModel):
    dictations: int
    words: int
    ms: int
    """Dictation speed: words spoken per minute of recorded audio."""
    avg_wpm: float
    days_active: int


class StatsStreak(BaseModel):
    current: int
    longest: int


class StatsDay(BaseModel):
    day: str
    n: int
    ms: int
    words: int


class StatsApp(BaseModel):
    app: str
    n: int
    ms: int
    words: int


class StatsCleanup(BaseModel):
    """What the cleanup pass and the dictionary did for you."""

    applied: int
    words_removed: int
    dictionary_hits: int


class StatsReview(BaseModel):
    backlog: int
    reviewed: int
    eligible: int


class WerPoint(BaseModel):
    id: str
    finished_at: str | None
    wer_baseline: float | None
    wer_candidate: float | None
    status: str


class Stats(BaseModel):
    totals: StatsTotals
    streak: StatsStreak
    per_day: list[StatsDay]
    by_app: list[StatsApp]
    cleanup: StatsCleanup
    review: StatsReview
    wer_series: list[WerPoint]
