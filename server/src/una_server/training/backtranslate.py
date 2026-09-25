"""Back-translation: turn text the user typed into (spoken -> written) cleanup pairs.

The cleanup model's job is to turn a Whisper transcript into what the user would have
typed. Text the user really typed is the best possible target for that — so an LLM is
asked to run the job backwards: given what they wrote, what would Whisper have produced
had they said it instead? Thousands of such pairs teach the model the user's style
before they have dictated much at all.

Pure prompt construction and reply validation live here so the server (when it has an
LLM) and the import tool (on the machine holding the writing) share them.
"""

from __future__ import annotations

import hashlib
import json
import logging
import re
import zlib
from dataclasses import dataclass

from .filters import norm_edit_distance, normalize_text

log = logging.getLogger(__name__)

SYSTEM = """\
You make training data for one person's private dictation system. They speak; Whisper \
transcribes it; a small model then turns Whisper's transcript into the text they would \
have typed. You are given things they actually typed. For each item write two things.

"target": their text with accidental typos fixed and nothing else changed. Fix only slips \
of the keyboard — transposed, missing, doubled or stray letters ("proejcts" -> "projects", \
"FOr" -> "For"). Keep everything deliberate exactly as it is: lowercase, shorthand and \
slang (yk, lowk, idk, u, ur, tho, wat, ngl, type shi), fragments, run-ons, missing \
apostrophes, their punctuation habits, CAPS for emphasis, emoji, file paths, commands and \
code. Never reword, reorder, add or drop words.

"spoken": what Whisper would output if they had said the same thing out loud instead of \
typing it.
- Shorthand is said the way people say it: "yk" -> "you know", "idk" -> "I don't know", \
"u" -> "you", "lowk" -> "lowkey", "w/" -> "with", "->" -> "to" or "then".
- Paths, commands, flags, identifiers and symbols are spoken aloud ("src slash main dot \
rs", "dash dash force"), and Whisper writes down what it hears — sometimes cleanly, \
sometimes as ordinary words.
- Speech is not clean. Add some of what people really say — "um", "uh", "like", "so", \
"I mean", a repeated word, a restarted phrase — more in long items, none in some short \
ones. In about one item in four, add a self-correction: invent a plausible wrong first \
choice that they then take back ("send it Tuesday, no wait, Wednesday"). The target \
stays exactly their text, with only the final choice.
- Whisper writes ordinary sentences: capitalized, punctuated, numbers as digits, and it \
often leaves out "um" and "uh" by itself. It sometimes mishears rare names and jargon as \
common words ("Tauri" -> "Tory", "Wispr" -> "Whisper", "Llama" -> "LAMA"); do that only \
where a real recognizer plausibly would.

Reply with only a JSON array with one object per item, in the same order: \
[{"id": "...", "target": "...", "spoken": "..."}]"""


@dataclass
class Item:
    id: str
    text: str


@dataclass
class Pair:
    id: str
    target: str
    spoken: str


def content_hash(text: str) -> str:
    """Stable identity of a piece of writing: whitespace-insensitive sha256."""
    return hashlib.sha256(" ".join(text.split()).encode()).hexdigest()


def is_holdout(key: str) -> bool:
    """Deterministic ~10% holdout, like dictations' eval split."""
    return zlib.crc32(key.encode()) % 10 == 0


def build_prompt(items: list[Item], calibration: list[str]) -> str:
    lines = []
    if calibration:
        lines.append("Real Whisper transcripts of this person, for calibration:")
        lines += [f"- {text}" for text in calibration]
        lines.append("")
    lines.append("Items:")
    lines.append(json.dumps([{"id": item.id, "text": item.text} for item in items], ensure_ascii=False))
    return "\n".join(lines)


# A target must be the writing with typos fixed — more change than this is a rewrite.
MAX_TARGET_DRIFT = 0.15
# Spoken text legitimately drifts (shorthand spelled out, fillers), but not this far.
MAX_SPOKEN_DRIFT = 0.8


def parse_reply(reply: object, items: list[Item]) -> list[Pair]:
    """Validated pairs from the LLM's reply; items it botched are left out."""
    if not isinstance(reply, list):
        return []
    by_id = {item.id: item for item in items}
    pairs: list[Pair] = []
    for entry in reply:
        if not isinstance(entry, dict):
            continue
        item = by_id.get(str(entry.get("id")))
        target, spoken = entry.get("target"), entry.get("spoken")
        if item is None or not isinstance(target, str) or not isinstance(spoken, str):
            continue
        target, spoken = target.strip(), spoken.strip()
        if not target or not spoken:
            continue
        if norm_edit_distance(item.text, target) > MAX_TARGET_DRIFT or _capitals(target) > _capitals(
            item.text
        ):
            # It rewrote the text, or "fixed" deliberate lowercase — the comparison above
            # can't see casing, and casing is style. The original is the safer target.
            target = item.text
        if norm_edit_distance(target, spoken) > MAX_SPOKEN_DRIFT or _overlap(target, spoken) < 0.34:
            continue
        pairs.append(Pair(item.id, target, spoken))
    return pairs


def _overlap(target: str, spoken: str) -> float:
    """Share of the target's longer words that the spoken version also has.

    Edit distance alone is a poor judge on short texts, where spelling out shorthand
    ("idk" -> "I don't know") moves it a lot; an unrelated sentence shares no words.
    """
    content = {w for w in normalize_text(target).split() if len(w) >= 3}
    if not content:
        return 1.0
    return len(content & set(normalize_text(spoken).split())) / len(content)


def _capitals(text: str) -> int:
    return sum(1 for char in text if char.isupper())


# ---------------------------------------------------------------------------
# Splitting writing into dictation-sized pieces
# ---------------------------------------------------------------------------

_SENTENCE_END = re.compile(r"(?<=[.!?])\s+")
MAX_WORDS = 90  # about 30 s of speech, and well inside the trainer's sequence budget


def chunk(text: str, max_words: int = MAX_WORDS) -> list[str]:
    """Paragraphs, and long paragraphs split at sentence ends, each <= max_words."""
    pieces: list[str] = []
    for paragraph in re.split(r"\n\s*\n", text):
        paragraph = " ".join(paragraph.split())
        if not paragraph:
            continue
        if len(paragraph.split()) <= max_words:
            pieces.append(paragraph)
            continue
        current: list[str] = []
        for sentence in _SENTENCE_END.split(paragraph):
            words = sentence.split()
            if current and len(current) + len(words) > max_words:
                pieces.append(" ".join(current))
                current = []
            current += words
            while len(current) > max_words:  # one enormous sentence
                pieces.append(" ".join(current[:max_words]))
                current = current[max_words:]
        if current:
            pieces.append(" ".join(current))
    return pieces
