"""Merging one server's data into another's: nothing lost, nothing overwritten, re-runnable."""

import sqlite3
from pathlib import Path

import pytest

from una_server.db import MIGRATIONS_DIR, is_eval_holdout
from una_server.tools import merge_data as md

TS = "2026-10-01T00:00:00+00:00"
LATER = "2026-10-05T00:00:00+00:00"


def make_db(data: Path, *, upto: str | None = None) -> sqlite3.Connection:
    data.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(data / "una.db")
    conn.execute("PRAGMA journal_mode=WAL")  # as the server runs it
    conn.execute("CREATE TABLE schema_migrations (version TEXT PRIMARY KEY, applied_at TEXT)")
    for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
        if upto and path.stem > upto:
            break
        conn.executescript(path.read_text())
        conn.execute("INSERT INTO schema_migrations VALUES (?, ?)", (path.stem, TS))
    conn.commit()
    return conn


def add_model(conn, model_id, *, ct2_path=None, active=0, kind="finetuned"):
    conn.execute(
        "INSERT INTO models (id, kind, ct2_path, hf_source, is_active, created_at) "
        "VALUES (?, ?, ?, ?, ?, ?)",
        (model_id, kind, ct2_path, None if ct2_path else "large-v3-turbo", active, TS),
    )
    conn.commit()


def add_dictation(conn, data: Path, prefix: str, did: str, *, deleted=0, audio=b"RIFF....",
                  created=TS, model="base:large-v3-turbo"):
    rel = f"audio/2026/10/{did}.wav"
    if audio is not None:
        (data / rel).parent.mkdir(parents=True, exist_ok=True)
        (data / rel).write_bytes(audio)
    conn.execute(
        "INSERT INTO dictations (id, created_at, audio_path, duration_ms, raw_text, "
        "asr_model_id, utterance_id, eval_holdout, deleted) VALUES (?, ?, ?, 1000, ?, ?, ?, ?, ?)",
        (did, created, f"{prefix}/{rel}", f"text {did}", model, f"utt-{did}",
         int(is_eval_holdout(did)), deleted),
    )
    conn.commit()


def add_correction(conn, cid, did, text, *, updated=TS):
    conn.execute(
        "INSERT INTO corrections (id, dictation_id, corrected_text, action, created_at, "
        "updated_at, source) VALUES (?, ?, ?, 'edited', ?, ?, 'popup')",
        (cid, did, text, TS, updated),
    )
    conn.commit()


@pytest.fixture
def servers(tmp_path):
    """tenet-like target (Docker paths, a promoted model) and a midna-like source."""
    tgt_data, src_data = tmp_path / "tenet" / "data", tmp_path / "midna" / "data"
    tgt, src = make_db(tgt_data), make_db(src_data)

    add_model(tgt, "base:large-v3-turbo", kind="base")
    (tgt_data / "models" / "ft-tenet").mkdir(parents=True)
    (tgt_data / "models" / "ft-tenet" / "model.bin").write_bytes(b"tenet")
    add_model(tgt, "ft-tenet", ct2_path="/data/models/ft-tenet", active=1)
    add_dictation(tgt, tgt_data, "/data", "01TENET0000000000000000001", model="ft-tenet")
    add_correction(tgt, "01TC1", "01TENET0000000000000000001", "tenet fix")
    tgt.execute("INSERT INTO settings VALUES ('cleanup.model', 'una-style-abcd')")
    tgt.commit()

    add_model(src, "base:large-v3-turbo", kind="base", active=1)
    abs_models = "/home/midna/una/server/data/models"
    (src_data / "models" / "ft-midna").mkdir(parents=True)
    (src_data / "models" / "ft-midna" / "model.bin").write_bytes(b"midna")
    add_model(src, "ft-midna", ct2_path=f"{abs_models}/ft-midna")
    # a rejected run's model: on disk, never registered
    (src_data / "models" / "ft-rejected").mkdir()
    (src_data / "models" / "ft-rejected" / "model.bin").write_bytes(b"nope")
    for n in (1, 2):
        add_dictation(src, src_data, "data", f"01MIDNA000000000000000000{n}")
    add_dictation(src, src_data, "data", "01MIDNA0000000000000000009", deleted=1, audio=None)
    add_correction(src, "01MC1", "01MIDNA0000000000000000001", "midna fix")
    src.execute("INSERT INTO settings VALUES ('cleanup.model', 'llama3.2:3b')")
    src.execute("INSERT INTO settings VALUES ('training.auto', 'true')")
    src.commit()
    tgt.close()
    return tgt_data, src, src_data, tmp_path / "snap"


def rows(data: Path, sql: str):
    conn = sqlite3.connect(data / "una.db")
    try:
        return conn.execute(sql).fetchall()
    finally:
        conn.close()


def test_paths_are_cut_at_the_data_folder():
    assert str(md.under("data/audio/2026/10/x.wav", "audio")) == "audio/2026/10/x.wav"
    assert md.prefix_of("data/audio/2026/10/x.wav", "audio") == "data"
    assert md.prefix_of("/data/audio/2026/10/x.wav", "audio") == "/data"
    assert md.prefix_of("/home/u/audio/una/server/data/audio/2026/10/x.wav", "audio") == (
        "/home/u/audio/una/server/data"
    )
    assert md.prefix_of("/srv/models/ft-1", "models") == "/srv"
    assert md.under("elsewhere/x.wav", "audio") is None


def test_merge_adds_the_source_and_keeps_the_target(servers):
    tgt_data, _src, src_data, snap = servers
    info = md.snapshot(src_data, snap)
    assert info["counts"]["dictations"] == 3
    assert not (snap / "models" / "ft-rejected").exists()

    report = md.merge(snap, tgt_data)
    assert report["before"]["dictations"] == 1
    assert report["after"]["dictations"] == 4
    assert report["after"]["corrections"] == 2
    assert report["layout"] == {"audio": "/data", "models": "/data", "runs": str(tgt_data)}
    assert Path(report["backup"]).exists()

    # recordings land in the target's layout, and come along
    paths = dict(rows(tgt_data, "SELECT id, audio_path FROM dictations"))
    assert paths["01MIDNA0000000000000000001"] == (
        "/data/audio/2026/10/01MIDNA0000000000000000001.wav"
    )
    assert (tgt_data / "audio/2026/10/01MIDNA0000000000000000002.wav").exists()
    # the target keeps serving its own model; the source's registered one comes along
    models = {r[0]: r[1:] for r in rows(tgt_data, "SELECT id, ct2_path, is_active FROM models")}
    assert models["ft-tenet"] == ("/data/models/ft-tenet", 1)
    assert models["ft-midna"] == ("/data/models/ft-midna", 0)
    assert (tgt_data / "models/ft-midna/model.bin").read_bytes() == b"midna"
    assert not (tgt_data / "models" / "ft-rejected").exists()
    # settings: the target's win, new ones come over, and the conflict is reported
    settings = dict(rows(tgt_data, "SELECT key, value FROM settings"))
    assert settings == {"cleanup.model": "una-style-abcd", "training.auto": "true"}
    assert any("cleanup.model" in n for n in report["notes"])
    # the eval holdout is a hash of the id, so it survives
    holdout = rows(tgt_data, "SELECT id, eval_holdout FROM dictations")
    assert all(bool(h) == is_eval_holdout(i) for i, h in holdout)


def test_merging_again_changes_nothing(servers):
    tgt_data, _src, src_data, snap = servers
    md.snapshot(src_data, snap)
    md.merge(snap, tgt_data)
    before = rows(tgt_data, "SELECT * FROM dictations ORDER BY id")
    report = md.merge(snap, tgt_data)
    assert report["after"] == report["before"]
    assert not any("added" in tally for tally in report["rows"].values())
    assert rows(tgt_data, "SELECT * FROM dictations ORDER BY id") == before
    assert report["files"] == {"audio present": 2, "model files present": 1}


def test_a_newer_snapshot_brings_only_what_changed(servers):
    tgt_data, src, src_data, snap = servers
    md.snapshot(src_data, snap)
    md.merge(snap, tgt_data)

    # on the source, after the first merge: a new dictation, an edit, a deletion
    add_dictation(src, src_data, "data", "01MIDNA0000000000000000003", created=LATER)
    add_correction(src, "01MC3", "01MIDNA0000000000000000003", "new")
    src.execute("UPDATE corrections SET corrected_text = 'edited later', updated_at = ? "
                "WHERE id = '01MC1'", (LATER,))
    src.execute("UPDATE dictations SET deleted = 1 WHERE id = '01MIDNA0000000000000000002'")
    src.commit()
    md.snapshot(src_data, snap)
    report = md.merge(snap, tgt_data)

    assert report["rows"]["dictations"] == {"added": 1, "already there": 2, "deleted since": 1}
    assert report["rows"]["corrections"] == {"added": 1, "updated (newer on source)": 1}
    fixes = dict(rows(tgt_data, "SELECT id, corrected_text FROM corrections"))
    assert fixes["01MC1"] == "edited later"
    assert fixes["01TC1"] == "tenet fix"
    deleted = dict(rows(tgt_data, "SELECT id, deleted FROM dictations"))
    assert deleted["01MIDNA0000000000000000002"] == 1


def test_an_edit_made_on_the_target_since_wins(servers):
    tgt_data, _src, src_data, snap = servers
    md.snapshot(src_data, snap)
    md.merge(snap, tgt_data)
    conn = sqlite3.connect(tgt_data / "una.db")
    conn.execute("UPDATE corrections SET corrected_text = 'fixed on tenet', updated_at = ? "
                 "WHERE id = '01MC1'", (LATER,))
    conn.commit()
    conn.close()
    report = md.merge(snap, tgt_data)
    assert report["rows"]["corrections"]["kept target"] == 1
    assert rows(tgt_data, "SELECT corrected_text FROM corrections WHERE id = '01MC1'") == [
        ("fixed on tenet",)
    ]


def test_dry_run_changes_nothing(servers):
    tgt_data, _src, src_data, snap = servers
    md.snapshot(src_data, snap)
    report = md.merge(snap, tgt_data, dry_run=True)
    assert report["rows"]["dictations"]["added"] == 3  # the deleted one too, still deleted
    assert rows(tgt_data, "SELECT count(*) FROM dictations") == [(1,)]
    assert not (tgt_data / "audio/2026/10/01MIDNA0000000000000000001.wav").exists()
    assert not (tgt_data / "backups").exists()


def test_a_file_already_on_the_target_is_never_replaced(servers):
    tgt_data, _src, src_data, snap = servers
    md.snapshot(src_data, snap)
    clash = tgt_data / "audio/2026/10/01MIDNA0000000000000000001.wav"
    clash.parent.mkdir(parents=True, exist_ok=True)
    clash.write_bytes(b"something else entirely")
    report = md.merge(snap, tgt_data)
    assert report["files"]["audio conflict"] == 1
    assert clash.read_bytes() == b"something else entirely"


def test_bare_metal_target_and_an_older_schema(tmp_path, servers):
    _, _src, src_data, snap = servers
    tgt_data = tmp_path / "fresh" / "data"
    make_db(tgt_data, upto="007_teacher").close()
    md.snapshot(src_data, snap)
    report = md.merge(snap, tgt_data, root="data")
    assert report["migrated"] == ["008_style_corpus"]
    assert report["layout"]["audio"] == "data"
    assert report["layout"]["models"] == str(Path("data").resolve())
    paths = [r[0] for r in rows(tgt_data, "SELECT audio_path FROM dictations")]
    assert all(p.startswith("data/audio/2026/10/") for p in paths)


def test_a_snapshot_changes_nothing_on_the_source(tmp_path):
    data = tmp_path / "data"
    live = make_db(data)  # held open: a running server, with rows only in its WAL
    live.execute("PRAGMA wal_autocheckpoint=0")
    add_dictation(live, data, "data", "01LIVE00000000000000000001")
    md.snapshot(data, tmp_path / "snap-live")
    assert rows(tmp_path / "snap-live", "SELECT id FROM dictations") == [
        ("01LIVE00000000000000000001",)
    ]
    live.close()  # a stopped server: una.db alone, no -wal or -shm

    before = {p.name: p.stat().st_mtime_ns for p in data.iterdir()}
    md.snapshot(data, tmp_path / "snap-stopped")
    assert {p.name: p.stat().st_mtime_ns for p in data.iterdir()} == before
    assert rows(tmp_path / "snap-stopped", "SELECT count(*) FROM dictations") == [(1,)]
