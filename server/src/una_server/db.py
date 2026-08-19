"""aiosqlite connection + migration runner. WAL mode so the training subprocess can write too."""

from __future__ import annotations

import zlib
from datetime import datetime, timezone
from pathlib import Path

import aiosqlite

MIGRATIONS_DIR = Path(__file__).resolve().parent.parent.parent / "migrations"


def utcnow() -> str:
    return datetime.now(timezone.utc).isoformat()


def is_eval_holdout(dictation_id: str) -> bool:
    """Deterministic ~10% eval split, frozen at insert time by hashing the ID."""
    return zlib.crc32(dictation_id.encode()) % 10 == 0


async def connect(db_path: Path) -> aiosqlite.Connection:
    db_path.parent.mkdir(parents=True, exist_ok=True)
    db = await aiosqlite.connect(db_path)
    db.row_factory = aiosqlite.Row
    await db.execute("PRAGMA journal_mode=WAL")
    await db.execute("PRAGMA busy_timeout=5000")
    await db.execute("PRAGMA foreign_keys=ON")
    return db


async def migrate(db: aiosqlite.Connection) -> None:
    await db.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version TEXT PRIMARY KEY, applied_at TEXT NOT NULL)"
    )
    await db.commit()
    async with db.execute("SELECT version FROM schema_migrations") as cur:
        applied = {row["version"] async for row in cur}
    for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
        version = path.stem
        if version in applied:
            continue
        await db.executescript(path.read_text())
        await db.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)",
            (version, utcnow()),
        )
        await db.commit()


async def ensure_base_model(db: aiosqlite.Connection, model_alias: str) -> str:
    """Seed/activate the configured base model row; returns the active model id."""
    base_id = f"base:{model_alias}"
    async with db.execute("SELECT id FROM models WHERE id = ?", (base_id,)) as cur:
        row = await cur.fetchone()
    if row is None:
        await db.execute(
            "INSERT INTO models (id, kind, hf_source, is_active, created_at, notes) VALUES (?, 'base', ?, 0, ?, 'seeded from config')",
            (base_id, model_alias, utcnow()),
        )
    async with db.execute("SELECT id FROM models WHERE is_active = 1") as cur:
        active = await cur.fetchone()
    if active is None:
        await db.execute("UPDATE models SET is_active = 1 WHERE id = ?", (base_id,))
        await db.commit()
        return base_id
    await db.commit()
    return active["id"]
