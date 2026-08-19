"""Promotion gate + registry housekeeping. Decision logic is pure so it can be unit-tested."""

from __future__ import annotations

import logging
import shutil
import sqlite3
from collections.abc import Sequence
from datetime import UTC, date, datetime

log = logging.getLogger(__name__)


def should_promote(
    wer_baseline: float,
    wer_candidate: float,
    n_eval: int,
    margin: float,
    min_samples: int,
) -> tuple[bool, str]:
    """Promote iff the candidate beats the baseline by at least `margin` WER points
    (WER expressed in percent) on at least `min_samples` eval samples.
    """
    if n_eval < min_samples:
        return False, f"too few eval samples ({n_eval} < {min_samples})"
    improvement = wer_baseline - wer_candidate
    if improvement < margin:
        return False, (
            f"WER improvement {improvement:.2f} below margin {margin:.2f} "
            f"(baseline {wer_baseline:.2f} -> candidate {wer_candidate:.2f})"
        )
    return True, (
        f"WER improved by {improvement:.2f} points "
        f"(baseline {wer_baseline:.2f} -> candidate {wer_candidate:.2f}, n_eval={n_eval})"
    )


def new_model_id(run_id: str, on: date | None = None) -> str:
    """Registry id for a run's converted model, e.g. 'ft:2026-08-18-a1b2'."""
    return f"ft:{(on or datetime.now(UTC).date()).isoformat()}-{run_id[-4:].lower()}"


def prunable_model_ids(models: Sequence[tuple[str, str]], keep: int) -> list[str]:
    """Given (id, created_at) rows for non-active fine-tuned models, return the ids beyond
    the newest `keep`, newest first."""
    ranked = sorted(models, key=lambda model: model[1], reverse=True)
    return [model_id for model_id, _created_at in ranked[max(keep, 0):]]


def prune_finetuned(db: sqlite3.Connection, keep: int) -> list[str]:
    """Delete CT2 dir + models row for old non-active fine-tuned models; never the active one.

    The caller's connection must leave foreign_keys OFF (sqlite's default): old dictations may
    still reference a pruned model id, and those dangling references are accepted.
    """
    rows = db.execute(
        "SELECT id, created_at, ct2_path FROM models WHERE kind = 'finetuned' AND is_active = 0"
    ).fetchall()
    by_id = {row["id"]: row for row in rows}
    doomed = prunable_model_ids([(row["id"], row["created_at"]) for row in rows], keep)
    for model_id in doomed:
        ct2_path = by_id[model_id]["ct2_path"]
        if ct2_path:
            shutil.rmtree(ct2_path, ignore_errors=True)
        db.execute("DELETE FROM models WHERE id = ?", (model_id,))
        log.info("pruned fine-tuned model %s (%s)", model_id, ct2_path)
    db.commit()
    return doomed
