"""Single-slot training job launcher. The runner subprocess owns its training_runs row."""

from __future__ import annotations

import asyncio
import logging
import os
import signal
import sys

import aiosqlite

from ..db import utcnow
from ..errors import RunActive

log = logging.getLogger(__name__)

ACTIVE_STATUSES = ("queued", "building", "training", "evaluating", "converting")


class JobManager:
    def __init__(self, db: aiosqlite.Connection, on_finished=None):
        self.db = db
        self.process: asyncio.subprocess.Process | None = None
        self._watcher: asyncio.Task | None = None
        self.on_finished = on_finished  # async callback, e.g. reload ASR after pause_serving

    @property
    def active(self) -> bool:
        return self.process is not None and self.process.returncode is None

    async def has_active_run(self) -> bool:
        placeholders = ",".join("?" * len(ACTIVE_STATUSES))
        async with self.db.execute(
            f"SELECT COUNT(*) AS n FROM training_runs WHERE status IN ({placeholders})",
            ACTIVE_STATUSES,
        ) as cur:
            row = await cur.fetchone()
        return row["n"] > 0

    async def start(self, run_id: str) -> None:
        if self.active or await self.has_active_run():
            raise RunActive("a training run is already active")
        self.process = await asyncio.create_subprocess_exec(
            sys.executable,
            "-m",
            "una_server.training.runner",
            "--run-id",
            run_id,
            env={**os.environ},
        )
        await self.db.execute(
            "UPDATE training_runs SET pid = ?, started_at = ? WHERE id = ?",
            (self.process.pid, utcnow(), run_id),
        )
        await self.db.commit()
        self._watcher = asyncio.create_task(self._watch(run_id))

    async def _watch(self, run_id: str) -> None:
        assert self.process is not None
        code = await self.process.wait()
        if code != 0:
            # Runner normally records its own terminal status; cover hard crashes.
            await self.db.execute(
                """UPDATE training_runs SET status = 'failed',
                   error = COALESCE(error, 'runner exited with code ' || ?),
                   finished_at = COALESCE(finished_at, ?)
                   WHERE id = ? AND status NOT IN ('promoted', 'rejected', 'failed', 'cancelled')""",
                (str(code), utcnow(), run_id),
            )
            await self.db.commit()
            log.error("training run %s exited with code %s", run_id, code)
        self.process = None
        if self.on_finished is not None:
            try:
                await self.on_finished()
            except Exception:
                log.exception("post-training callback failed")

    async def cancel(self, run_id: str) -> None:
        if self.process is not None and self.process.returncode is None:
            self.process.send_signal(signal.SIGTERM)
        await self.db.execute(
            """UPDATE training_runs SET status = 'cancelled', finished_at = ?
               WHERE id = ? AND status NOT IN ('promoted', 'rejected', 'failed')""",
            (utcnow(), run_id),
        )
        await self.db.commit()
