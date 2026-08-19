"""Idle-triggered automatic training.

A background loop that, when `training.auto` is on, launches a fine-tune once
the machine has been dictation-idle for `training.auto_idle_minutes`, enough
eligible audio has accumulated, no run is active, and at least one eligible
pair is newer than the last run attempt (so unchanged data is never retrained
and a persistently failing run is never auto-retried).

The loop is always spawned; it re-reads config every tick so flipping
`training.auto` in Settings takes effect without a restart.
"""

from __future__ import annotations

import asyncio
import logging
import time

from ..state import AppState
from . import runs

log = logging.getLogger(__name__)

INTERVAL_S = 60.0


async def maybe_start(state: AppState, *, launch=runs.launch_run) -> bool:
    """Start a run if every auto-training condition holds. Returns True if started."""
    cfg = state.config.training
    if not cfg.auto:
        return False
    idle_s = time.monotonic() - state.last_dictation_at
    if idle_s < cfg.auto_idle_minutes * 60.0:
        return False
    if state.jobs.active or await state.jobs.has_active_run():
        return False
    _, minutes = await runs.eligible_counts(state)
    if minutes < cfg.threshold_minutes:
        return False
    fresh = await runs.fresh_pair_count(state)
    if fresh == 0:
        return False
    row = await launch(state)
    log.info(
        "auto-training: started run %s (%.1f eligible minutes, %d new pairs, idle %.0fs)",
        row["id"], minutes, fresh, idle_s,
    )
    return True


async def loop(state: AppState, interval_s: float = INTERVAL_S) -> None:
    while True:
        await asyncio.sleep(interval_s)
        try:
            await maybe_start(state)
        except Exception:
            log.exception("auto-training check failed")
