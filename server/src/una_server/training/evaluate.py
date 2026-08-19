"""Baseline-vs-candidate WER, both through CTranslate2 with identical greedy decode settings.

WER values are percentages (0-100), computed with jiwer over normalize_text-normalized pairs.
Models are loaded one at a time and freed between them to avoid holding double VRAM.
"""

from __future__ import annotations

import gc
import logging
from collections.abc import Sequence

from ..config import AsrConfig
from .dataset import Sample
from .filters import normalize_text

log = logging.getLogger(__name__)


def resolve_model_source(model_id: str, ct2_path: str | None, hf_source: str | None) -> str:
    """Mirror services.model_manager._resolve_source for plain column values."""
    if ct2_path:
        return ct2_path
    return hf_source or model_id.removeprefix("base:")


def compute_wer(references: Sequence[str], hypotheses: Sequence[str]) -> float:
    """Corpus WER in percent over normalized text; pairs with an empty reference are dropped."""
    import jiwer

    refs: list[str] = []
    hyps: list[str] = []
    for reference, hypothesis in zip(references, hypotheses):
        reference = normalize_text(reference)
        if not reference:
            continue
        refs.append(reference)
        hyps.append(normalize_text(hypothesis))
    if not refs:
        raise ValueError("no non-empty references to score")
    return 100.0 * jiwer.wer(refs, hyps)


def transcribe_all(source: str, samples: Sequence[Sample], cfg: AsrConfig) -> list[str]:
    """Greedy transcription of every sample: beam 1, temperature 0, no prompt, no VAD."""
    from faster_whisper import WhisperModel  # deferred: heavy import

    model = WhisperModel(source, device=cfg.device, compute_type=cfg.compute_type)
    hypotheses: list[str] = []
    try:
        for sample in samples:
            segments, _info = model.transcribe(
                sample.audio_path,
                language=cfg.language,
                beam_size=1,
                temperature=0.0,
                condition_on_previous_text=False,
                vad_filter=False,
                word_timestamps=False,
            )
            hypotheses.append(" ".join(seg.text.strip() for seg in segments).strip())
    finally:
        del model
        gc.collect()
    return hypotheses


def evaluate_models(
    baseline_source: str,
    candidate_source: str,
    eval_samples: Sequence[Sample],
    cfg: AsrConfig,
) -> tuple[float, float]:
    """Return (wer_baseline, wer_candidate) in percent over the eval set."""
    references = [sample.text for sample in eval_samples]
    log.info("evaluating baseline %s on %d samples", baseline_source, len(eval_samples))
    wer_baseline = compute_wer(references, transcribe_all(baseline_source, eval_samples, cfg))
    log.info("baseline WER %.2f; evaluating candidate %s", wer_baseline, candidate_source)
    wer_candidate = compute_wer(references, transcribe_all(candidate_source, eval_samples, cfg))
    log.info("candidate WER %.2f", wer_candidate)
    return wer_baseline, wer_candidate
