"""Import your own writing as training data for the cleanup model.

    uv run python -m una_server.tools.writing --server http://tenet.local:8100 \\
        --llm http://127.0.0.1:8787            # optional: back-translate here

Reads what you have typed to coding agents on this machine — Claude Code and Codex
histories — keeps what looks typed by you rather than pasted or generated, splits it into
dictation-sized pieces, and uploads them to the una server. With --llm (an Anthropic
Messages API endpoint, e.g. a local yagami) each piece is back-translated here into what
Whisper would have heard had you said it; without it, the server's teacher does that
once it has an LLM. Idempotent: pieces the server already has finished are skipped.

`make import-writing` wraps this.
"""

from __future__ import annotations

import argparse
import asyncio
import collections
import json
import os
import re
import sys
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

import httpx

from ..services.llm import Llm, LlmError
from ..training import backtranslate as bt

# ---------------------------------------------------------------------------
# Harvesting
# ---------------------------------------------------------------------------


@dataclass
class Sample:
    source: str
    app_name: str
    text: str


_MARKERS = re.compile(
    r"\[(?:external unsupported block: [^\]]*|image #\d+|Image #\d+|Pasted text #\d+[^\]]*)\]"
)
_SECRET = re.compile(
    r"sk-[A-Za-z0-9_-]{16,}|ygm_[A-Za-z0-9]{8,}|gh[pousr]_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}"
    r"|-----BEGIN|xox[abposr]-|eyJ[A-Za-z0-9_-]{20,}\.|[A-Za-z0-9+/]{40,}={0,2}"
)
_SKIP_PREFIXES = (
    "<", "/", "!", "Caveat:", "This session is being continued", "[Request interrupted",
)
_SKIP_CONTAINS = ("```", "<system-reminder>", "tool_use_id", '{"', "Traceback (most recent")
# Prompts that harnesses and smoke tests send, not the user.
_TEST_PROMPT = re.compile(
    r"reply with (?:the single word|only|exactly)|haiku about|count (?:slowly )?from \d"
    r"|one number per line|say (?:only|just) ",
    re.IGNORECASE,
)
_BULLET = re.compile(r"^\s*(?:[-*•]|\d+[.)])\s+", re.MULTILINE)
_HEADING = re.compile(r"^#{1,6}\s", re.MULTILINE)


def looks_typed(text: str) -> bool:
    """Whether a message reads as something the user typed, not pasted or generated."""
    if len(text) < 12 or len(text) > 3000:
        return False
    if text.startswith(_SKIP_PREFIXES) or any(s in text for s in _SKIP_CONTAINS):
        return False
    if _SECRET.search(text) or _TEST_PROMPT.search(text):
        return False
    if text.count("\n") > 15 or len(_BULLET.findall(text)) >= 4 or _HEADING.search(text):
        return False
    letters = sum(1 for char in text if char.isalpha())
    return letters / len(text) >= 0.6


def _claude_code(root: Path) -> Iterator[str]:
    for path in sorted(root.glob("*/*.jsonl")):
        for line in path.open(errors="ignore"):
            try:
                entry = json.loads(line)
            except ValueError:
                continue
            if entry.get("type") != "user" or entry.get("isMeta") or entry.get("isSidechain"):
                continue
            if entry.get("isCompactSummary") or entry.get("toolUseResult") is not None:
                continue
            content = (entry.get("message") or {}).get("content")
            if isinstance(content, list):
                content = "\n".join(
                    block.get("text", "")
                    for block in content
                    if isinstance(block, dict) and block.get("type") == "text"
                )
            if isinstance(content, str):
                yield content


def _codex(root: Path) -> Iterator[str]:
    for path in sorted(root.rglob("*.jsonl")):
        for line in path.open(errors="ignore"):
            try:
                entry = json.loads(line)
            except ValueError:
                continue
            payload = entry.get("payload") or {}
            if entry.get("type") == "event_msg" and payload.get("type") == "user_message":
                message = payload.get("message")
                if isinstance(message, str):
                    yield message


SOURCES = {
    # source id: (reader, default location, the app the writing was typed in)
    "claude-code": (_claude_code, Path.home() / ".claude" / "projects", "Claude Code"),
    "codex": (_codex, Path.home() / ".codex" / "sessions", "Codex"),
}


def harvest(sources: list[str]) -> list[Sample]:
    """Every dictation-sized piece of typed writing across the chosen sources."""
    raw: list[tuple[str, str, str]] = []
    for source in sources:
        reader, root, app_name = SOURCES[source]
        if not root.exists():
            continue
        for message in reader(root):
            text = _MARKERS.sub("", message).strip()
            if looks_typed(text):
                raw.append((source, app_name, text))
    # The same text sent three or more times is a harness or a script, not a person.
    counts = collections.Counter(text for _, _, text in raw)
    samples: list[Sample] = []
    seen: set[str] = set()
    for source, app_name, text in raw:
        if counts[text] >= 3:
            continue
        for piece in bt.chunk(text):
            key = bt.content_hash(piece)
            if len(piece.split()) < 3 or key in seen:
                continue
            seen.add(key)
            samples.append(Sample(source, app_name, piece))
    return samples


# ---------------------------------------------------------------------------
# Back-translation + upload
# ---------------------------------------------------------------------------


async def translate(
    llm: Llm, samples: list[Sample], calibration: list[str], *, batch: int, parallel: int
) -> dict[str, bt.Pair]:
    """content hash -> pair, for every sample the LLM handled well."""
    gate = asyncio.Semaphore(parallel)
    done: dict[str, bt.Pair] = {}
    batches = [samples[i:i + batch] for i in range(0, len(samples), batch)]
    finished = 0

    async def run(group: list[Sample]) -> None:
        nonlocal finished
        items = [bt.Item(bt.content_hash(s.text)[:12], s.text) for s in group]
        async with gate:
            try:
                reply = await llm.json_reply(bt.SYSTEM, bt.build_prompt(items, calibration))
            except LlmError as exc:
                print(f"  batch failed: {exc}", file=sys.stderr)
                reply = []
        for pair in bt.parse_reply(reply, items):
            done[pair.id] = pair
        finished += 1
        print(f"  back-translated {finished}/{len(batches)} batches ({len(done)} pairs)", flush=True)

    await asyncio.gather(*(run(group) for group in batches))
    return done


def upload(server: str, samples: list[Sample], pairs: dict[str, bt.Pair], model: str | None):
    items = []
    for sample in samples:
        pair = pairs.get(bt.content_hash(sample.text)[:12])
        items.append({
            "source": sample.source,
            "app_name": sample.app_name,
            "written_text": sample.text,
            "target_text": pair.target if pair else None,
            "spoken_text": pair.spoken if pair else None,
            "generator_model": model if pair else None,
        })
    totals = collections.Counter()
    with httpx.Client(base_url=server, timeout=60.0) as client:
        for i in range(0, len(items), 100):
            resp = client.post("/v1/style/corpus", json={"items": items[i:i + 100]})
            resp.raise_for_status()
            totals.update(resp.json())
    return totals


async def main_async(args: argparse.Namespace) -> int:
    samples = harvest(args.source)
    words = sum(len(s.text.split()) for s in samples)
    print(f"found {len(samples)} pieces of your writing ({words} words)")
    if args.limit:
        samples = samples[: args.limit]
    if args.dry_run:
        for sample in samples[:15]:
            print(f"  [{sample.source}] {sample.text[:140]}")
        return 0

    with httpx.Client(base_url=args.server, timeout=30.0) as client:
        known = client.post(
            "/v1/style/corpus/known",
            json={"hashes": [bt.content_hash(s.text) for s in samples]},
        )
        known.raise_for_status()
        finished = set(known.json()["finished"])
        calibration = client.get("/v1/style/calibration").json()["transcripts"]
    todo = [s for s in samples if bt.content_hash(s.text) not in finished]
    print(f"{len(samples) - len(todo)} already on the server; {len(todo)} to go")
    if not todo:
        return 0

    pairs: dict[str, bt.Pair] = {}
    if args.llm:
        llm = Llm(args.llm, "", args.llm_model, timeout_s=300.0)
        try:
            pairs = await translate(llm, todo, calibration, batch=args.batch, parallel=args.parallel)
        finally:
            await llm.aclose()
    totals = upload(args.server, todo, pairs, args.llm_model if args.llm else None)
    print(f"uploaded: {dict(totals)}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--server", default=os.environ.get("UNA_SERVER", "http://tenet.local:8100"),
        help="una server URL (default: $UNA_SERVER, else tenet.local)",
    )
    parser.add_argument("--llm", default="", help="Messages API URL to back-translate with here")
    parser.add_argument("--llm-model", default="claude-haiku-4-5")
    parser.add_argument(
        "--source", action="append", choices=sorted(SOURCES),
        help="where to read writing from (default: all)",
    )
    parser.add_argument("--batch", type=int, default=8, help="pieces per LLM call")
    parser.add_argument("--parallel", type=int, default=3, help="LLM calls in flight")
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--dry-run", action="store_true", help="show what would be imported")
    args = parser.parse_args(argv)
    args.source = args.source or sorted(SOURCES)
    return asyncio.run(main_async(args))


if __name__ == "__main__":
    sys.exit(main())
