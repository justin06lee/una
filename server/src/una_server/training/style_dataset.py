"""Style dataset assembly: (raw transcript -> polished text) chat pairs.

Pure stdlib so the runner (and tests) can build datasets without torch installed.
Each sample carries the dictation's app_name so training reconstructs the same
system prompt the cleaner uses at inference time.
"""

from __future__ import annotations

import logging
import sqlite3
from dataclasses import dataclass, field

from ..config import CleanupConfig
from ..services.prompts import build_system_prompt

log = logging.getLogger(__name__)

STYLE_PAIRS_SQL = """
SELECT d.raw_text, c.polished_text, d.app_name, d.eval_holdout
FROM corrections c JOIN dictations d ON d.id = c.dictation_id
WHERE c.polished_text IS NOT NULL AND d.deleted = 0
"""


@dataclass
class StyleSample:
    """One (raw dictation, polished output) pair plus its prompt context."""

    raw_text: str
    polished_text: str
    app_name: str | None


@dataclass
class StyleSplit:
    train: list[StyleSample] = field(default_factory=list)
    eval: list[StyleSample] = field(default_factory=list)


def messages_for(cleanup_cfg: CleanupConfig, raw_text: str, app_name: str | None) -> list[dict]:
    """The exact chat the cleaner sends at inference (dictionary omitted: it varies over time)."""
    return [
        {"role": "system", "content": build_system_prompt(cleanup_cfg, [], app_name)},
        {"role": "user", "content": raw_text},
    ]


def build_style_datasets(db: sqlite3.Connection) -> StyleSplit:
    """Split polished pairs on the frozen eval_holdout flag.

    The connection must have row_factory = sqlite3.Row.
    """
    split = StyleSplit()
    for row in db.execute(STYLE_PAIRS_SQL):
        raw = (row["raw_text"] or "").strip()
        polished = (row["polished_text"] or "").strip()
        if not raw or not polished:
            continue
        sample = StyleSample(
            raw_text=raw, polished_text=polished, app_name=row["app_name"]
        )
        (split.eval if row["eval_holdout"] else split.train).append(sample)
    log.info("style dataset: %d train / %d eval pairs", len(split.train), len(split.eval))
    return split
