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


class DisagreementSpan(BaseModel):
    """Characters [start, end) of raw_text the teacher's second ASR pass heard as `alt`;
    t0/t1 are where that is in the audio, in seconds."""

    start: int
    end: int
    alt: str
    t0: float | None = None
    t1: float | None = None


class TeacherLabel(BaseModel):
    """The teacher's second opinion on a dictation, for pre-filling review."""

    status: Literal["done", "partial", "failed"]
    second_text: str | None = None
    second_model: str | None = None
    disagreements: list[DisagreementSpan] = []
    literal_guess: str | None = None
    polished_guess: str | None = None
    llm_model: str | None = None
    llm_error: str | None = None
    needs_review: bool = True
    error: str | None = None


class DictationDetail(DictationSummary):
    raw_text: str
    cleaned_text: str | None
    cleanup_applied: bool
    # The pasted cleanup reads like a reply to the dictation rather than a cleanup of it.
    cleanup_diverged: bool = False
    asr_model: str | None
    llm_model: str | None
    language: str | None
    corrected_text: str | None = None
    polished_text: str | None = None
    training_eligible: bool | None = None
    eligibility_reason: str | None = None
    correction_source: str | None = None
    eval_holdout: bool
    teacher: TeacherLabel | None = None


class ReviewQueue(BaseModel):
    """Unreviewed dictations, the ones worth a look first."""

    items: list[DictationDetail]
    pending: int  # unreviewed dictations in total
    needs_review: int  # of those, how many the teacher flagged


class TeacherStatus(BaseModel):
    enabled: bool
    second_asr_model: str | None = None
    llm_model: str | None = None
    unlabeled: int = 0
    partial: int = 0
    last_error: str | None = None


class DictationList(BaseModel):
    items: list[DictationSummary]
    next_cursor: str | None = None


class CorrectionRequest(BaseModel):
    corrected_text: str | None = None
    # accepted/edited/skipped/excluded judge the literal transcript (the ASR pair).
    # "polished" carries only the final text and says nothing about what was said:
    # it is what an edit to the pasted text means, since what got pasted was the
    # cleaned version, not the transcript.
    action: Literal["accepted", "edited", "skipped", "excluded", "polished"]
    polished_text: str | None = None  # preferred final rendering -> style-LLM training pair
    source: Literal["review", "auto", "popup"] = "review"


class CorrectionResponse(BaseModel):
    dictation_id: str
    action: str
    norm_edit_distance: float | None
    training_eligible: bool
    eligibility_reason: str | None
    polished_text: str | None = None
    source: str = "review"


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
    # back-translated pieces of the user's own writing, and how many make a run worthwhile
    style_writing_pairs: int = 0
    style_writing_threshold: int = 0


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
    eval_set: str | None = None  # style runs: 'confirmed' dictations, or the 'writing' holdout
    notes: str | None = None


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


class CorpusItem(BaseModel):
    """A piece of the user's own writing, optionally already back-translated."""

    source: str
    app_name: str | None = None
    written_text: str
    target_text: str | None = None
    spoken_text: str | None = None
    generator_model: str | None = None


class CorpusUpload(BaseModel):
    items: list[CorpusItem]
    # Replace finished back-translations too (after the method improves).
    replace: bool = False


class CorpusUploadResult(BaseModel):
    added: int = 0
    updated: int = 0
    unchanged: int = 0


class CorpusKnownRequest(BaseModel):
    hashes: list[str]


class CorpusKnown(BaseModel):
    finished: list[str]  # hashes the server already holds with a back-translation


class Calibration(BaseModel):
    transcripts: list[str]


class CorpusStats(BaseModel):
    samples: int
    finished: int
    holdout: int
