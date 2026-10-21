"""Metrics for the Korean / Korean-English code-switching Whisper work (docs/korean-codeswitch.md).

Pure stdlib + rapidfuzz, so it runs anywhere. Every metric normalizes both sides the same way:

- EN: WER over una's own normalization (training/filters.normalize_text), as the existing gate.
- KO: CER over Hangul syllables and Latin letters with all spaces removed; Korean spacing is
  loose enough that word-level scores mostly measure where the spaces went.
- Mixed: MER (mixed error rate) over tokens that are one Hangul syllable or one Latin word, the
  usual unit for code-switched ASR, plus a script check: a reference span that comes back in the
  other script was translated, transliterated or romanized, which this work must never do.
"""

from __future__ import annotations

import re
import unicodedata
from collections.abc import Sequence
from dataclasses import dataclass

from rapidfuzz.distance import Levenshtein

HANGUL = "hangul"
LATIN = "latin"
DIGIT = "digit"

_HANGUL_RE = re.compile(r"[가-힣ᄀ-ᇿ㄰-㆏]")
# FLEURS-style glosses: 비슈케크(Bishkek) is said once, in Korean.
_GLOSS_RE = re.compile(r"\([^)]*\)")
# The same as una_server.training.filters (kept in step by hand: this runs outside the venv).
_PUNCT_RE = re.compile(r"[^\w\s']", re.UNICODE)
_WS_RE = re.compile(r"\s+")
_TOKEN_RE = re.compile(r"[가-힣ᄀ-ᇿ㄰-㆏]|[a-z]+(?:'[a-z]+)*|\d+")


def normalize_text(text: str) -> str:
    """una's metric normalization: NFKC, lowercase, punctuation to spaces."""
    text = unicodedata.normalize("NFKC", text).lower()
    text = _PUNCT_RE.sub(" ", text)
    return _WS_RE.sub(" ", text).strip()


def normalize_reference(text: str) -> str:
    """References only: drop parenthesized glosses the speaker never said."""
    return normalize_text(_GLOSS_RE.sub(" ", text))


def script_of(token: str) -> str:
    if _HANGUL_RE.match(token):
        return HANGUL
    if token[:1].isdigit():
        return DIGIT
    return LATIN


def has_hangul(text: str) -> bool:
    return bool(_HANGUL_RE.search(text))


def mixed_tokens(text: str) -> list[str]:
    """One token per Hangul syllable, per Latin word, per digit run (already normalized text)."""
    return _TOKEN_RE.findall(text)


def _chars(text: str) -> str:
    return "".join(mixed_tokens(text))


@dataclass
class Score:
    errors: int
    total: int

    @property
    def rate(self) -> float:
        return 100.0 * self.errors / self.total if self.total else 0.0


def _corpus(refs: Sequence[str], hyps: Sequence[str], unit) -> Score:
    errors = total = 0
    for ref, hyp in zip(refs, hyps):
        r = unit(normalize_reference(ref))
        if not r:
            continue
        errors += Levenshtein.distance(r, unit(normalize_text(hyp)))
        total += len(r)
    return Score(errors, total)


def wer(refs: Sequence[str], hyps: Sequence[str]) -> Score:
    return _corpus(refs, hyps, str.split)


def cer(refs: Sequence[str], hyps: Sequence[str]) -> Score:
    return _corpus(refs, hyps, _chars)


def mer(refs: Sequence[str], hyps: Sequence[str]) -> Score:
    return _corpus(refs, hyps, mixed_tokens)


def script_violations(ref: str, hyp: str) -> int:
    """Reference Hangul/Latin tokens that came back only in the other script.

    Aligns the mixed tokens; in each substituted stretch, a reference token counts when the
    hypothesis side has the other script and none of its own (회의 -> "meeting", "meeting" ->
    미팅, 안녕 -> "annyeong"). Plain misrecognitions within one script don't count.
    """
    r = mixed_tokens(normalize_reference(ref))
    h = mixed_tokens(normalize_text(hyp))
    count = 0
    for op in Levenshtein.opcodes(r, h):
        if op.tag != "replace":
            continue
        hyp_scripts = {script_of(t) for t in h[op.dest_start:op.dest_end]}
        for token in r[op.src_start:op.src_end]:
            own = script_of(token)
            if own == DIGIT:
                continue
            other = LATIN if own == HANGUL else HANGUL
            if other in hyp_scripts and own not in hyp_scripts:
                count += 1
    return count


def script_fidelity(refs: Sequence[str], hyps: Sequence[str]) -> tuple[int, int]:
    """(utterances with any violation, utterances scored)."""
    bad = scored = 0
    for ref, hyp in zip(refs, hyps):
        if not mixed_tokens(normalize_reference(ref)):
            continue
        scored += 1
        bad += script_violations(ref, hyp) > 0
    return bad, scored


def load_audio(path: str):
    """16 kHz mono float32. soundfile first: some PyAV releases break faster-whisper's decode_audio."""
    import soundfile as sf
    audio, rate = sf.read(path, dtype="float32", always_2d=False)
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    if rate != 16000:
        from faster_whisper import decode_audio
        return decode_audio(path, sampling_rate=16000)
    return audio
