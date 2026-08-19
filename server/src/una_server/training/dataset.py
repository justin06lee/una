"""Dataset assembly: eligible correction pairs -> train/eval sample lists.

Pure stdlib + filters so the runner (and tests) can build datasets without torch installed.
Targets keep punctuation/casing — normalization is applied only when computing metrics.
"""

from __future__ import annotations

import logging
import sqlite3
from dataclasses import dataclass, field
from pathlib import Path

from .filters import training_text

log = logging.getLogger(__name__)

PAIRS_SQL = """
SELECT c.corrected_text, c.action, d.raw_text, d.audio_path, d.duration_ms, d.eval_holdout
FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.training_eligible = 1 AND d.deleted = 0
"""


@dataclass
class Sample:
    """One (audio, target transcript) training pair."""

    audio_path: str
    text: str
    duration_ms: int


@dataclass
class DatasetSplit:
    train: list[Sample] = field(default_factory=list)
    eval: list[Sample] = field(default_factory=list)
    n_missing_audio: int = 0

    @property
    def train_minutes(self) -> float:
        return sum(sample.duration_ms for sample in self.train) / 60_000


def build_datasets(db: sqlite3.Connection) -> DatasetSplit:
    """Split eligible pairs on the frozen eval_holdout flag; skip rows whose WAV is gone.

    The connection must have row_factory = sqlite3.Row.
    """
    split = DatasetSplit()
    for row in db.execute(PAIRS_SQL):
        if not Path(row["audio_path"]).exists():
            split.n_missing_audio += 1
            continue
        sample = Sample(
            audio_path=row["audio_path"],
            text=training_text(row["action"], row["raw_text"], row["corrected_text"]),
            duration_ms=row["duration_ms"],
        )
        (split.eval if row["eval_holdout"] else split.train).append(sample)
    if split.n_missing_audio:
        log.warning("skipped %d eligible pairs with missing audio files", split.n_missing_audio)
    log.info(
        "dataset: %d train / %d eval pairs (%.1f train minutes)",
        len(split.train), len(split.eval), split.train_minutes,
    )
    return split
