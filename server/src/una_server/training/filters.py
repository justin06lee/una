"""Training-pair eligibility. Pure functions — shared by the correction API and the dataset builder.

The core noise defense: a correction that diverges too far from the raw transcript is a content
rewrite, not a transcription fix, and would teach the model to paraphrase. Filter it out.
"""

from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass

from rapidfuzz.distance import Levenshtein

MIN_DURATION_MS = 1_000
MAX_DURATION_MS = 30_000

_PUNCT_RE = re.compile(r"[^\w\s']", re.UNICODE)
_WS_RE = re.compile(r"\s+")


def normalize_text(text: str) -> str:
    """Whisper-style light normalization: casing, punctuation, and whitespace only.

    Applied identically to both sides of every comparison, so it only needs to be
    consistent, not identical to OpenAI's EnglishTextNormalizer.
    """
    text = unicodedata.normalize("NFKC", text).lower()
    text = _PUNCT_RE.sub(" ", text)
    return _WS_RE.sub(" ", text).strip()


def norm_edit_distance(raw: str, corrected: str) -> float:
    """Normalized Levenshtein distance in [0, 1] over normalized text."""
    a, b = normalize_text(raw), normalize_text(corrected)
    if not a and not b:
        return 0.0
    return Levenshtein.normalized_distance(a, b)


@dataclass
class EligibilityResult:
    eligible: bool
    reason: str | None  # populated when ineligible
    distance: float | None


def check_pair(
    *,
    action: str,
    raw_text: str,
    corrected_text: str | None,
    duration_ms: int,
    max_edit_distance: float,
) -> EligibilityResult:
    if action == "excluded":
        return EligibilityResult(False, "excluded by user", None)
    if action == "skipped":
        return EligibilityResult(False, "skipped (unreviewed)", None)

    text = raw_text if action == "accepted" else (corrected_text or "")
    distance = 0.0 if action == "accepted" else norm_edit_distance(raw_text, text)

    if not normalize_text(text):
        return EligibilityResult(False, "empty transcript", distance)
    if duration_ms < MIN_DURATION_MS:
        return EligibilityResult(False, f"too short ({duration_ms}ms < {MIN_DURATION_MS}ms)", distance)
    if duration_ms > MAX_DURATION_MS:
        return EligibilityResult(False, f"too long ({duration_ms}ms > {MAX_DURATION_MS}ms)", distance)
    if distance > max_edit_distance:
        return EligibilityResult(
            False,
            f"edit distance {distance:.2f} > {max_edit_distance:.2f} (likely content rewrite)",
            distance,
        )
    return EligibilityResult(True, None, distance)


def training_text(action: str, raw_text: str, corrected_text: str | None) -> str:
    """The target transcript a pair contributes: raw for 'accepted', the correction for 'edited'."""
    return raw_text if action == "accepted" else (corrected_text or raw_text)
