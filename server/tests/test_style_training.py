"""Style-trainer logic tests. Must pass without torch/transformers/peft/ollama."""

import sqlite3

from una_server.config import CleanupConfig
from una_server.db import MIGRATIONS_DIR
from una_server.training.style_dataset import build_style_datasets, messages_for
from una_server.training.style_promote import new_style_model_name, should_promote_style

TS = "2026-08-18T00:00:00+00:00"


def _make_db() -> sqlite3.Connection:
    conn = sqlite3.connect(":memory:")
    conn.row_factory = sqlite3.Row
    for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
        conn.executescript(path.read_text())
    return conn


def _insert_pair(
    conn: sqlite3.Connection,
    did: str,
    raw: str,
    polished: str | None,
    *,
    holdout: int = 0,
    deleted: int = 0,
    app_name: str | None = "Slack",
) -> None:
    conn.execute(
        """INSERT INTO dictations (id, created_at, app_name, audio_path, duration_ms,
           raw_text, eval_holdout, deleted)
           VALUES (?, ?, ?, '/nonexistent.wav', 2000, ?, ?, ?)""",
        (did, TS, app_name, raw, holdout, deleted),
    )
    conn.execute(
        """INSERT INTO corrections (id, dictation_id, action, polished_text,
           training_eligible, created_at, updated_at)
           VALUES (?, ?, 'accepted', ?, 1, ?, ?)""",
        (f"c-{did}", did, polished, TS, TS),
    )


# -- dataset ----------------------------------------------------------------------


def test_style_split_on_holdout_and_filters():
    conn = _make_db()
    _insert_pair(conn, "d1", "um hello there", "Hello there!")
    _insert_pair(conn, "d2", "so anyway", "Anyway —", holdout=1)
    _insert_pair(conn, "d3", "no polish here", None)
    _insert_pair(conn, "d4", "deleted row", "Deleted.", deleted=1)
    _insert_pair(conn, "d5", "   ", "Whitespace raw is dropped.")

    split = build_style_datasets(conn)
    assert [s.raw_text for s in split.train] == ["um hello there"]
    assert [s.polished_text for s in split.eval] == ["Anyway —"]
    assert split.train[0].app_name == "Slack"


def test_messages_match_cleaner_inference_shape():
    cfg = CleanupConfig()
    messages = messages_for(cfg, "um send it monday", "Slack")
    assert [m["role"] for m in messages] == ["system", "user"]
    assert messages[1]["content"] == "um send it monday"
    # tone_map default maps slack -> casual tone; the prompt must reflect it
    assert cfg.tone_map["slack"] in messages[0]["content"]


# -- promotion gate ---------------------------------------------------------------


def test_style_promote_on_margin():
    ok, reason = should_promote_style(0.20, 0.15, 50, 0.02, 20)
    assert ok
    assert "improved" in reason


def test_style_reject_under_margin_or_worse():
    ok, _ = should_promote_style(0.20, 0.19, 50, 0.02, 20)
    assert not ok
    ok, _ = should_promote_style(0.20, 0.25, 50, 0.02, 20)
    assert not ok


def test_style_reject_small_eval_set():
    ok, reason = should_promote_style(0.5, 0.1, 19, 0.02, 20)
    assert not ok
    assert "19 < 20" in reason


def test_style_model_name_format():
    assert new_style_model_name("01JEXAMPLERUNID0ABCD") == "una-style-abcd"


# -- API --------------------------------------------------------------------------


def test_style_run_api_and_eligibility(client, fake_runner):
    elig = client.get("/v1/training/eligibility").json()
    assert elig["style_pairs"] == 0
    assert elig["style_ready"] is False
    assert elig["style_threshold_pairs"] > 0

    resp = client.post("/v1/training/runs", json={"kind": "style"})
    assert resp.status_code == 201, resp.text
    run = resp.json()
    assert run["kind"] == "style"
    # style runs resolve their baseline (cleanup.model) at execution time
    assert run["base_model_id"] is None
