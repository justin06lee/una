"""Style model evaluation through Ollama — the actual serving path.

Both the baseline and the candidate are queried via /api/chat exactly as the
cleaner does at inference (temperature 0 for determinism), and scored by mean
normalized edit distance between output and the polished target. Lower is
better. Models are evaluated one at a time so Ollama holds a single model.
"""

from __future__ import annotations

import logging
import re
from collections.abc import Sequence

import httpx

from ..config import CleanupConfig
from .filters import norm_edit_distance
from .style_dataset import StyleSample, messages_for

log = logging.getLogger(__name__)

THINK_RE = re.compile(r"<think>.*?</think>", re.DOTALL)


def _generate(
    client: httpx.Client, model: str, cleanup_cfg: CleanupConfig, sample: StyleSample
) -> str:
    payload = {
        "model": model,
        "stream": False,
        "keep_alive": "5m",
        "options": {
            "temperature": 0.0,
            "num_predict": max(64, len(sample.raw_text) * 2),
        },
        "messages": messages_for(cleanup_cfg, sample.raw_text, sample.app_name),
    }
    resp = client.post("/api/chat", json=payload)
    resp.raise_for_status()
    content = resp.json()["message"]["content"]
    return THINK_RE.sub("", content).strip().strip('"')


def mean_distance(
    ollama_url: str,
    model: str,
    samples: Sequence[StyleSample],
    cleanup_cfg: CleanupConfig,
    *,
    timeout_s: float = 120.0,
) -> float:
    """Mean normalized edit distance of `model`'s outputs to the polished targets."""
    total = 0.0
    with httpx.Client(base_url=ollama_url, timeout=timeout_s) as client:
        for sample in samples:
            output = _generate(client, model, cleanup_cfg, sample)
            total += norm_edit_distance(output, sample.polished_text)
    distance = total / len(samples)
    log.info("style eval: %s -> mean distance %.4f over %d samples", model, distance, len(samples))
    return distance


def smoke_test(
    ollama_url: str, model: str, sample: StyleSample, cleanup_cfg: CleanupConfig
) -> None:
    """The candidate must produce non-empty output before it is worth evaluating."""
    with httpx.Client(base_url=ollama_url, timeout=300.0) as client:  # first load is slow
        output = _generate(client, model, cleanup_cfg, sample)
    if not output.strip():
        raise RuntimeError(f"style model {model} produced empty output in smoke test")
    log.info("style smoke test passed: %r", output[:120])
