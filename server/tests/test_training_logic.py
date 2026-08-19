"""Pure-logic tests for the training pipeline. Must pass without torch/transformers/peft."""

import sqlite3
import sys
from datetime import date
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from una_server.db import MIGRATIONS_DIR
from una_server.training import convert
from una_server.training.dataset import build_datasets
from una_server.training.promote import (
    new_model_id,
    prunable_model_ids,
    prune_finetuned,
    should_promote,
)

TS = "2026-08-18T00:00:00+00:00"


# -- should_promote ---------------------------------------------------------------


def test_promote_when_better_by_margin():
    ok, reason = should_promote(10.0, 9.0, 100, 0.5, 40)
    assert ok
    assert "improved" in reason


def test_promote_exactly_at_margin_and_min_samples():
    ok, _reason = should_promote(10.0, 9.5, 40, 0.5, 40)
    assert ok


def test_reject_better_but_under_margin():
    ok, reason = should_promote(10.0, 9.8, 100, 0.5, 40)
    assert not ok
    assert "margin" in reason


def test_reject_when_worse():
    ok, reason = should_promote(10.0, 11.0, 100, 0.5, 40)
    assert not ok
    assert "margin" in reason


def test_reject_too_few_eval_samples():
    ok, reason = should_promote(10.0, 5.0, 39, 0.5, 40)
    assert not ok
    assert "too few eval samples" in reason
    assert "39 < 40" in reason


# -- model ids and pruning selection ----------------------------------------------


def test_new_model_id_format():
    assert new_model_id("01JD3EXAMPLERUNIDABCDWXYZ", on=date(2026, 8, 18)) == "ft:2026-08-18-wxyz"


def test_prunable_keeps_newest():
    rows = [
        ("m1", "2026-01-01T00:00:00+00:00"),
        ("m3", "2026-03-01T00:00:00+00:00"),
        ("m2", "2026-02-01T00:00:00+00:00"),
    ]
    assert prunable_model_ids(rows, keep=2) == ["m1"]
    assert prunable_model_ids(rows, keep=0) == ["m3", "m2", "m1"]
    assert prunable_model_ids(rows, keep=5) == []
    assert prunable_model_ids([], keep=3) == []


# -- migration-backed fixtures ----------------------------------------------------


def _make_db() -> sqlite3.Connection:
    conn = sqlite3.connect(":memory:")
    conn.row_factory = sqlite3.Row
    for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
        conn.executescript(path.read_text())
    return conn


def _write_wav(path: Path, seconds: float = 1.0) -> None:
    sr = 16000
    t = np.linspace(0, seconds, int(seconds * sr), endpoint=False)
    audio = (0.1 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)
    path.parent.mkdir(parents=True, exist_ok=True)
    sf.write(path, audio, sr, subtype="PCM_16")


def _insert_pair(
    conn: sqlite3.Connection,
    tmp_path: Path,
    *,
    did: str,
    action: str = "accepted",
    raw: str = "hello world",
    corrected: str | None = None,
    eligible: int = 1,
    holdout: int = 0,
    deleted: int = 0,
    missing: bool = False,
    duration_ms: int = 5000,
) -> None:
    audio_path = tmp_path / f"{did}.wav"
    if not missing:
        _write_wav(audio_path)
    conn.execute(
        "INSERT INTO dictations (id, created_at, audio_path, duration_ms, raw_text, "
        "eval_holdout, deleted) VALUES (?, ?, ?, ?, ?, ?, ?)",
        (did, TS, str(audio_path), duration_ms, raw, holdout, deleted),
    )
    conn.execute(
        "INSERT INTO corrections (id, dictation_id, corrected_text, action, training_eligible, "
        "created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        (f"c-{did}", did, corrected, action, eligible, TS, TS),
    )
    conn.commit()


# -- dataset selection ------------------------------------------------------------


def test_build_datasets_split_and_targets(tmp_path):
    conn = _make_db()
    _insert_pair(conn, tmp_path, did="d1")  # train: accepted -> raw text
    _insert_pair(
        conn, tmp_path, did="d2", action="edited", raw="helo world", corrected="Hello, world!"
    )  # train: edited -> corrected text, punctuation/casing kept
    _insert_pair(conn, tmp_path, did="d3", holdout=1)  # eval holdout
    _insert_pair(conn, tmp_path, did="d4", eligible=0)  # ineligible: skipped
    _insert_pair(conn, tmp_path, did="d5", deleted=1)  # deleted dictation: skipped
    _insert_pair(conn, tmp_path, did="d6", missing=True)  # audio file gone: skipped

    split = build_datasets(conn)

    assert {Path(s.audio_path).stem for s in split.train} == {"d1", "d2"}
    assert {Path(s.audio_path).stem for s in split.eval} == {"d3"}
    assert split.n_missing_audio == 1
    d1 = next(s for s in split.train if Path(s.audio_path).stem == "d1")
    d2 = next(s for s in split.train if Path(s.audio_path).stem == "d2")
    assert d1.text == "hello world"
    assert d2.text == "Hello, world!"
    assert split.train_minutes == pytest.approx(10_000 / 60_000)


def test_build_datasets_empty_db(tmp_path):
    split = build_datasets(_make_db())
    assert split.train == [] and split.eval == [] and split.n_missing_audio == 0


# -- pruning against the real schema ----------------------------------------------


def _insert_model(conn, model_id, kind, ct2_path, is_active, created_at):
    conn.execute(
        "INSERT INTO models (id, kind, ct2_path, is_active, created_at) VALUES (?, ?, ?, ?, ?)",
        (model_id, kind, ct2_path, is_active, created_at),
    )
    conn.commit()


def test_prune_finetuned_deletes_oldest_never_active(tmp_path):
    conn = _make_db()
    _insert_model(conn, "base:turbo", "base", None, 0, "2025-12-01T00:00:00+00:00")
    for name, active, created in [
        ("m1", 0, "2026-01-01T00:00:00+00:00"),
        ("m2", 0, "2026-02-01T00:00:00+00:00"),
        ("m3", 1, "2026-03-01T00:00:00+00:00"),  # active: untouchable even though older than m4
        ("m4", 0, "2026-04-01T00:00:00+00:00"),
    ]:
        model_dir = tmp_path / name
        model_dir.mkdir()
        (model_dir / "model.bin").write_bytes(b"stub")
        _insert_model(conn, name, "finetuned", str(model_dir), active, created)

    doomed = prune_finetuned(conn, keep=2)

    assert doomed == ["m1"]
    assert not (tmp_path / "m1").exists()
    assert (tmp_path / "m2").exists() and (tmp_path / "m3").exists() and (tmp_path / "m4").exists()
    remaining = {row["id"] for row in conn.execute("SELECT id FROM models")}
    assert remaining == {"base:turbo", "m2", "m3", "m4"}


def test_prune_finetuned_noop_when_under_limit(tmp_path):
    conn = _make_db()
    _insert_model(conn, "m1", "finetuned", str(tmp_path / "m1"), 0, TS)
    assert prune_finetuned(conn, keep=3) == []
    assert conn.execute("SELECT COUNT(*) AS n FROM models").fetchone()["n"] == 1


# -- converter command ------------------------------------------------------------


def test_converter_command_module_fallback(tmp_path, monkeypatch):
    monkeypatch.setattr(convert.shutil, "which", lambda _name: None)
    merged = tmp_path / "merged"
    merged.mkdir()
    (merged / "tokenizer.json").write_text("{}")
    cmd = convert.converter_command(merged, tmp_path / "out")
    assert cmd[:3] == [sys.executable, "-m", "ctranslate2.converters.transformers"]
    assert cmd[cmd.index("--quantization") + 1] == "float16"
    assert "--copy_files" in cmd and "tokenizer.json" in cmd
    assert "preprocessor_config.json" not in cmd  # only files that exist are copied


def test_converter_command_prefers_console_script(tmp_path, monkeypatch):
    script = "/opt/venv/bin/ct2-transformers-converter"
    monkeypatch.setattr(convert.shutil, "which", lambda _name: script)
    cmd = convert.converter_command(tmp_path / "merged", tmp_path / "out")
    assert cmd[0] == script
    assert cmd[cmd.index("--model") + 1] == str(tmp_path / "merged")
    assert cmd[cmd.index("--output_dir") + 1] == str(tmp_path / "out")


def test_ensure_feature_extractor_config(tmp_path):
    merged = tmp_path / "merged"
    out = tmp_path / "ct2"
    merged.mkdir()
    out.mkdir()
    # no source file -> no-op
    convert.ensure_feature_extractor_config(merged, out)
    assert not (out / "preprocessor_config.json").exists()
    (merged / "preprocessor_config.json").write_text('{"feature_size": 128}')
    convert.ensure_feature_extractor_config(merged, out)
    assert (out / "preprocessor_config.json").read_text() == '{"feature_size": 128}'
    # already present -> left alone
    (out / "preprocessor_config.json").write_text("keep")
    convert.ensure_feature_extractor_config(merged, out)
    assert (out / "preprocessor_config.json").read_text() == "keep"
