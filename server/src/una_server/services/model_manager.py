"""Active-model resolution and hot-swap. One inference lock; free-then-load to avoid double VRAM."""

from __future__ import annotations

import asyncio
import gc
import logging
from pathlib import Path

import aiosqlite

from ..config import AsrConfig
from ..errors import UnaError
from .transcriber import Transcriber

log = logging.getLogger(__name__)


def _resolve_source(row: aiosqlite.Row) -> str:
    if row["ct2_path"]:
        return row["ct2_path"]
    return row["hf_source"] or row["id"].removeprefix("base:")


class ModelManager:
    def __init__(self, cfg: AsrConfig, db: aiosqlite.Connection):
        self.cfg = cfg
        self.db = db
        self.lock = asyncio.Lock()  # serializes inference AND swaps
        self.transcriber: Transcriber | None = None
        self.active_model_id: str | None = None

    @property
    def loaded(self) -> bool:
        return self.transcriber is not None

    async def _fetch_model_row(self, model_id: str) -> aiosqlite.Row:
        async with self.db.execute("SELECT * FROM models WHERE id = ?", (model_id,)) as cur:
            row = await cur.fetchone()
        if row is None:
            raise UnaError(f"model not found: {model_id}", code="NOT_FOUND", status=404)
        return row

    async def load_active(self) -> None:
        async with self.db.execute("SELECT * FROM models WHERE is_active = 1") as cur:
            row = await cur.fetchone()
        if row is None:
            raise UnaError("no active model in registry")
        source = _resolve_source(row)
        log.info("loading ASR model %s (%s)", row["id"], source)
        self.transcriber = await asyncio.to_thread(Transcriber, self.cfg, source)
        await asyncio.to_thread(self.transcriber.warmup)
        self.active_model_id = row["id"]
        log.info("ASR model %s ready", row["id"])

    async def unload(self) -> None:
        async with self.lock:
            self._free()

    def _free(self) -> None:
        if self.transcriber is not None:
            del self.transcriber.model
            self.transcriber = None
            gc.collect()

    async def swap(self, model_id: str) -> None:
        """Activate model_id: free current, load new, flip registry. Reload previous on failure."""
        row = await self._fetch_model_row(model_id)
        if row["ct2_path"] and not Path(row["ct2_path"]).exists():
            raise UnaError(f"model files missing: {row['ct2_path']}", code="MODEL_FILES_MISSING", status=409)
        previous_id = self.active_model_id
        async with self.lock:
            self._free()
            source = _resolve_source(row)
            try:
                self.transcriber = await asyncio.to_thread(Transcriber, self.cfg, source)
                await asyncio.to_thread(self.transcriber.warmup)
            except Exception as exc:
                log.error("failed to load %s: %s — reloading previous model", model_id, exc)
                if previous_id:
                    prev = await self._fetch_model_row(previous_id)
                    self.transcriber = await asyncio.to_thread(
                        Transcriber, self.cfg, _resolve_source(prev)
                    )
                raise UnaError(f"failed to load model {model_id}: {exc}", code="MODEL_LOAD_FAILED") from exc
            self.active_model_id = model_id
        await self.db.execute("UPDATE models SET is_active = 0 WHERE is_active = 1")
        await self.db.execute("UPDATE models SET is_active = 1 WHERE id = ?", (model_id,))
        await self.db.commit()
        log.info("active ASR model is now %s", model_id)
