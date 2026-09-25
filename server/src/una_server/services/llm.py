"""Anthropic Messages API access for the teacher. Optional, and never on the dictation path.

Works against the real API with a key, or against yagami — a local server that fronts a
signed-in Claude Code with the same API — so a Claude subscription can stand in for a key.
yagami ignores `max_tokens` and structured-output settings, so replies are asked for as
plain JSON and parsed leniently here rather than relying on either.
"""

from __future__ import annotations

import json
import logging
import os
from pathlib import Path
from typing import Any

import anthropic

log = logging.getLogger(__name__)


class LlmError(Exception):
    """The LLM could not be reached, refused, or answered with something unusable."""


def _yagami_key() -> str | None:
    config_dir = Path(os.environ.get("YAGAMI_CONFIG_DIR", Path.home() / ".config" / "yagami"))
    try:
        keys = json.loads((config_dir / "config.json").read_text()).get("apiKeys") or []
    except (OSError, ValueError):
        return None
    return keys[0] if keys else None


def resolve_api_key(explicit: str) -> str | None:
    """The configured key, else ANTHROPIC_API_KEY, else the local yagami server's key."""
    return explicit or os.environ.get("ANTHROPIC_API_KEY") or _yagami_key()


def extract_json(text: str) -> Any:
    """The first JSON object or array in a model reply, ignoring prose and code fences."""
    decoder = json.JSONDecoder()
    for index, char in enumerate(text):
        if char not in "{[":
            continue
        try:
            value, _ = decoder.raw_decode(text[index:])
        except ValueError:
            continue
        return value
    raise LlmError(f"no JSON in reply: {text[:200]!r}")


class Llm:
    def __init__(self, base_url: str, api_key: str, model: str, timeout_s: float):
        self.model = model
        self.client = anthropic.AsyncAnthropic(
            base_url=base_url or None,
            api_key=resolve_api_key(api_key),
            timeout=timeout_s,
            max_retries=1,
        )

    async def aclose(self) -> None:
        await self.client.close()

    async def json_reply(self, system: str, user: str) -> Any:
        """One turn; the reply's JSON payload. Raises LlmError on any failure."""
        try:
            response = await self.client.messages.create(
                model=self.model,
                max_tokens=16000,
                system=system,
                messages=[{"role": "user", "content": user}],
            )
        except anthropic.AuthenticationError as exc:
            raise LlmError(f"not authorized: {exc.message}") from exc
        except anthropic.RateLimitError as exc:
            raise LlmError("rate limited") from exc
        except anthropic.APIStatusError as exc:
            raise LlmError(f"status {exc.status_code}: {exc.message}"[:300]) from exc
        except anthropic.APIConnectionError as exc:
            raise LlmError(f"unreachable: {exc}") from exc
        if response.stop_reason == "refusal":
            raise LlmError("the model declined")
        text = "".join(block.text for block in response.content if block.type == "text")
        return extract_json(text)
