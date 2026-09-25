"""Ollama cleanup with a hard degradation contract: dictation never fails because cleanup failed."""

from __future__ import annotations

import logging
import re
import time
from dataclasses import dataclass

import httpx

from ..config import CleanupConfig
from ..training.filters import norm_edit_distance
from .prompts import build_system_prompt

log = logging.getLogger(__name__)

THINK_RE = re.compile(r"<think>.*?</think>", re.DOTALL)


@dataclass
class CleanupResult:
    text: str
    applied: bool
    error: str | None
    elapsed_ms: int
    model: str | None


class Cleaner:
    def __init__(self, cfg: CleanupConfig):
        self.cfg = cfg
        self.client = httpx.AsyncClient(base_url=cfg.ollama_url, timeout=cfg.timeout_s)
        self._consecutive_failures = 0
        self._skip_until = 0.0

    async def aclose(self) -> None:
        await self.client.aclose()

    async def warm(self) -> None:
        """Load the cleanup model into VRAM and keep it resident so real requests
        never pay the cold-load (which blows the timeout budget and degrades to raw)."""
        if not self.cfg.enabled:
            return
        payload = {
            "model": self.cfg.model,
            "stream": False,
            "keep_alive": self.cfg.keep_alive,
            "messages": [{"role": "user", "content": "ok"}],
            "options": {"num_predict": 1},
        }
        try:
            await self.client.post("/api/chat", json=payload, timeout=180.0)
            log.info("cleanup model %s warmed", self.cfg.model)
        except httpx.HTTPError as exc:
            log.warning("cleanup model warm failed: %s", type(exc).__name__)

    async def keep_warm(self, interval_s: float = 900.0) -> None:
        """Background loop: re-warm before keep_alive expires so latency stays flat."""
        import asyncio

        while True:
            await self.warm()
            await asyncio.sleep(interval_s)

    async def ping(self) -> bool:
        try:
            resp = await self.client.get("/api/version", timeout=1.5)
            return resp.status_code == 200
        except httpx.HTTPError:
            return False

    def _sane(self, raw: str, cleaned: str) -> bool:
        if not cleaned.strip():
            return False
        ratio = len(cleaned) / max(len(raw), 1)
        if not 0.4 <= ratio <= 2.5:
            return False
        # A small model handed a question or an instruction sometimes answers it
        # ("I think there may be some confusion — this is a text cleaning service…").
        # The length can look fine; the words won't.
        return norm_edit_distance(raw, cleaned) <= self.cfg.max_divergence

    async def clean(self, raw_text: str, phrases: list[str], app_name: str | None) -> CleanupResult:
        start = time.monotonic()

        def fail(err: str) -> CleanupResult:
            self._consecutive_failures += 1
            if self._consecutive_failures >= 3:
                self._skip_until = time.monotonic() + 60
                log.warning("cleanup circuit breaker open for 60s (%s)", err)
            return CleanupResult(raw_text, False, err, int((time.monotonic() - start) * 1000), self.cfg.model)

        if not self.cfg.enabled or not raw_text.strip():
            return CleanupResult(raw_text, False, None, 0, None)
        if time.monotonic() < self._skip_until:
            return CleanupResult(raw_text, False, "circuit_open", 0, None)

        payload = {
            "model": self.cfg.model,
            "stream": False,
            "keep_alive": self.cfg.keep_alive,
            "think": False,
            "options": {
                "temperature": 0.1,
                "num_predict": max(64, len(raw_text) * 2),
            },
            "messages": [
                {"role": "system", "content": build_system_prompt(self.cfg, phrases, app_name)},
                {"role": "user", "content": raw_text},
            ],
        }
        try:
            resp = await self.client.post("/api/chat", json=payload)
        except httpx.HTTPError as exc:
            return fail(f"ollama unreachable: {type(exc).__name__}")
        if resp.status_code != 200:
            # Older Ollama rejects unknown fields like "think" — retry once without it.
            if "think" in payload:
                payload.pop("think")
                try:
                    resp = await self.client.post("/api/chat", json=payload)
                except httpx.HTTPError as exc:
                    return fail(f"ollama unreachable: {type(exc).__name__}")
            if resp.status_code != 200:
                return fail(f"ollama status {resp.status_code}")

        try:
            content = resp.json()["message"]["content"]
        except (KeyError, ValueError) as exc:
            return fail(f"bad ollama response: {exc}")
        cleaned = THINK_RE.sub("", content).strip().strip('"')
        if not self._sane(raw_text, cleaned):
            return fail("cleanup output failed sanity check")

        self._consecutive_failures = 0
        return CleanupResult(cleaned, True, None, int((time.monotonic() - start) * 1000), self.cfg.model)
