"""Style dataset assembly: (transcript -> how it should read) chat pairs for the cleanup LLM.

Pure stdlib so the runner (and tests) can build datasets without torch installed.
Each sample carries an app_name so training reconstructs the same system prompt the
cleaner uses at inference time.

Pairs come from three places, in decreasing order of trust:

- confirmed: a dictation's polished text as the user gave or confirmed it. The only
  source of held-out eval pairs once there are enough of them, and repeated in training.
- writing:   the user's own writing, back-translated into what Whisper would have heard
  (training/backtranslate.py). Its frozen holdout stands in as the eval set until there
  are enough confirmed pairs.
- silver:    the teacher's polished guess for dictations nobody has confirmed. Train only.

Preference pairs for DPO come from confirmed polished texts that differ from what was
pasted: the user's version is preferred over the model's.
"""

from __future__ import annotations

import logging
import random
import sqlite3
import zlib
from dataclasses import dataclass, field

from rapidfuzz.distance import Levenshtein

from ..config import CleanupConfig
from ..services.prompts import build_system_prompt

log = logging.getLogger(__name__)

STYLE_PAIRS_SQL = """
SELECT d.raw_text, c.polished_text, d.app_name, d.eval_holdout
FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.polished_text IS NOT NULL AND d.deleted = 0 AND c.action != 'excluded'
"""

SILVER_SQL = """
SELECT d.raw_text, t.polished_guess, d.app_name
FROM teacher_labels t JOIN dictations d ON d.id = t.dictation_id
LEFT JOIN corrections c ON c.dictation_id = d.id
WHERE t.polished_guess IS NOT NULL AND d.deleted = 0 AND d.eval_holdout = 0
  AND c.polished_text IS NULL AND COALESCE(c.action, '') != 'excluded'
"""

WRITING_SQL = """
SELECT id, spoken_text, target_text, app_name, eval_holdout FROM style_corpus
WHERE spoken_text IS NOT NULL AND target_text IS NOT NULL
"""

PREFERENCE_SQL = """
SELECT d.raw_text, c.polished_text, d.cleaned_text, d.app_name
FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.polished_text IS NOT NULL AND d.cleanup_applied = 1 AND d.cleaned_text IS NOT NULL
  AND d.deleted = 0 AND d.eval_holdout = 0 AND c.action != 'excluded'
"""

# Below this (case- and punctuation-sensitive) difference, two outputs are the same answer.
MIN_PREFERENCE_GAP = 0.02


@dataclass
class StyleSample:
    """One (transcript, polished output) pair plus its prompt context."""

    raw_text: str
    polished_text: str
    app_name: str | None
    origin: str = "confirmed"  # confirmed | writing | silver


@dataclass
class PreferencePair:
    """For one transcript: an output to prefer over another."""

    raw_text: str
    chosen: str
    rejected: str
    app_name: str | None
    origin: str  # edit (the user's fix vs what was pasted) | on-policy (writing vs the model)


@dataclass
class StyleSplit:
    train: list[StyleSample] = field(default_factory=list)
    eval: list[StyleSample] = field(default_factory=list)  # confirmed holdout
    writing_eval: list[StyleSample] = field(default_factory=list)  # back-translated holdout
    preferences: list[PreferencePair] = field(default_factory=list)

    def counts(self) -> dict[str, int]:
        out: dict[str, int] = {}
        for sample in self.train:
            out[sample.origin] = out.get(sample.origin, 0) + 1
        return out


def messages_for(cleanup_cfg: CleanupConfig, raw_text: str, app_name: str | None) -> list[dict]:
    """The exact chat the cleaner sends at inference (dictionary omitted: it varies over time)."""
    return [
        {"role": "system", "content": build_system_prompt(cleanup_cfg, [], app_name)},
        {"role": "user", "content": raw_text},
    ]


def disfluent(spoken: str, key: str) -> str:
    """The spoken side of a writing pair with the stumbles real dictation has.

    Whisper writes down this user's "um"s, "uh"s and doubled words, and the cleanup
    model has to learn to drop them — but a back-translation doesn't always put enough
    in. So some are added here, deterministically per pair (same text every run), and
    about a quarter of pairs are left as they are.
    """
    rng = random.Random(zlib.crc32(key.encode()))
    words = spoken.split()
    if len(words) < 4 or rng.random() < 0.25:
        return spoken
    out: list[str] = []
    for word in words:
        roll = rng.random()
        if roll < 0.05:
            out.append(rng.choice(("um,", "uh,", "um", "uh")))
        out.append(word)
        if roll > 0.97 and word.isalpha() and len(word) <= 5:
            out.append(word)  # "the the"
    if rng.random() < 0.35:
        first = out[0]
        if first != "I" and not first.isupper():
            out[0] = first[:1].lower() + first[1:]
        out.insert(0, rng.choice(("Um,", "Uh,", "So, um,", "Um, so")))
    return " ".join(out)


def differs(a: str, b: str) -> bool:
    return Levenshtein.normalized_distance(a.strip(), b.strip()) >= MIN_PREFERENCE_GAP


def build_style_datasets(
    db: sqlite3.Connection,
    *,
    use_writing: bool = True,
    use_silver: bool = True,
    gold_repeat: int = 1,
    augment: bool = False,
) -> StyleSplit:
    """Assemble every source. The connection must have row_factory = sqlite3.Row."""
    split = StyleSplit()
    for row in db.execute(STYLE_PAIRS_SQL):
        raw = (row["raw_text"] or "").strip()
        polished = (row["polished_text"] or "").strip()
        if not raw or not polished:
            continue
        sample = StyleSample(raw, polished, row["app_name"], "confirmed")
        if row["eval_holdout"]:
            split.eval.append(sample)
        else:
            split.train += [sample] * max(1, gold_repeat)

    if use_writing:
        for row in db.execute(WRITING_SQL):
            spoken = row["spoken_text"].strip()
            if augment:
                spoken = disfluent(spoken, row["id"])
            sample = StyleSample(spoken, row["target_text"].strip(), row["app_name"], "writing")
            (split.writing_eval if row["eval_holdout"] else split.train).append(sample)

    if use_silver:
        for row in db.execute(SILVER_SQL):
            raw = (row["raw_text"] or "").strip()
            if raw:
                split.train.append(
                    StyleSample(raw, row["polished_guess"].strip(), row["app_name"], "silver")
                )

    for row in db.execute(PREFERENCE_SQL):
        raw = (row["raw_text"] or "").strip()
        chosen, rejected = row["polished_text"].strip(), row["cleaned_text"].strip()
        if raw and chosen and differs(chosen, rejected):
            split.preferences.append(PreferencePair(raw, chosen, rejected, row["app_name"], "edit"))

    log.info(
        "style dataset: train %s / eval %d confirmed + %d writing / %d preference pairs",
        split.counts(), len(split.eval), len(split.writing_eval), len(split.preferences),
    )
    return split
