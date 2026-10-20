"""Move one una server's history into another's without losing either.

    # on the old server: a consistent copy, safe while una runs
    python -m una_server.tools.merge_data snapshot data ~/una-snapshot
    # on the new one, from server/, after bringing the snapshot over
    python -m una_server.tools.merge_data merge ~/una-snapshot data --dry-run
    python -m una_server.tools.merge_data merge ~/una-snapshot data

`snapshot` copies una.db with SQLite's online backup, not by copying the live file,
along with the recordings, the registered fine-tuned Whisper models and the training-run
folders the database points at. A rejected run's model folder isn't registered, so it
stays behind.

`merge` adds what the target lacks, row by row, matched by primary key. Ids are ULIDs,
so two servers never mint the same one. It never deletes anything, and where both sides
have a row the target's wins. The exceptions are changes made on the source after an
earlier merge: a correction or a writing pair with a newer updated_at, and a dictation
deleted there. Paths are rewritten to the target's layout (Docker stores /data/...,
bare metal data/...), and the files come along without overwriting any the target already
has. Merging the same snapshot twice changes nothing, and a newer snapshot brings just
what is new, so the last one can be merged again at cutover. The target's una.db is
backed up into data/backups/ before each merge.

Standard library only, so it also runs piped into a Python that lacks una's packages:
    ssh old 'cd ~/una/server && python3 - snapshot data ~/una-snapshot' < merge_data.py
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import sqlite3
import sys
from collections import Counter
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath
from urllib.parse import quote

TABLES = (
    "models", "training_runs", "dictations", "corrections", "teacher_labels",
    "dictionary_entries", "settings", "style_corpus",
)
# Same rule as una_server.db.MIGRATIONS_DIR. Missing when run from stdin.
try:
    MIGRATIONS_DIR: Path | None = Path(__file__).resolve().parents[3] / "migrations"
except NameError:
    MIGRATIONS_DIR = None


def uri(path: Path, mode: str = "ro") -> str:
    return f"file:{quote(str(path.resolve()))}?mode={mode}"


def utcnow() -> str:
    return datetime.now(UTC).isoformat()


def stamp() -> str:
    return datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ")


# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------


def under(stored: str, anchor: str) -> PurePosixPath | None:
    """The part of a stored path from its data folder down: 'audio/2026/10/x.wav'.

    Stored paths are relative ('data/audio/...') on bare metal and absolute
    ('/data/audio/...', '/home/u/una/server/data/models/...') elsewhere; the last
    component named `anchor` is the one just under the data folder.
    """
    parts = PurePosixPath(stored).parts
    for i in range(len(parts) - 1, -1, -1):
        if parts[i] == anchor:
            return PurePosixPath(*parts[i:])
    return None


def prefix_of(stored: str, anchor: str) -> str | None:
    """'data' for 'data/audio/2026/10/x.wav', '/data' for '/data/audio/...'."""
    rel = under(stored, anchor)
    if rel is None:
        return None
    parts = PurePosixPath(stored).parts
    head = parts[: len(parts) - len(rel.parts)]
    return str(PurePosixPath(*head)) if head else ""


def join(prefix: str, rel: PurePosixPath) -> str:
    return str(PurePosixPath(prefix) / rel) if prefix else str(rel)


class Layout:
    """How the target server writes paths, read from rows it already has.

    audio_path and log_path keep whatever data_dir the server was given (relative on
    bare metal); ct2_path is always absolute. With no rows to learn from, `root` (the
    data dir as the target server sees it) decides, resolved for ct2_path.
    """

    def __init__(self, conn: sqlite3.Connection, root: str):
        def seen(sql: str, anchor: str) -> str | None:
            row = conn.execute(sql).fetchone()
            return prefix_of(row[0], anchor) if row else None

        self.audio = seen(
            "SELECT audio_path FROM dictations ORDER BY created_at DESC LIMIT 1", "audio"
        )
        self.models = seen(
            "SELECT ct2_path FROM models WHERE ct2_path IS NOT NULL LIMIT 1", "models"
        )
        self.runs = seen(
            "SELECT log_path FROM training_runs WHERE log_path IS NOT NULL LIMIT 1", "runs"
        )
        self.audio = self.audio if self.audio is not None else root
        self.runs = self.runs if self.runs is not None else root
        if self.models is None:
            self.models = root if root.startswith("/") else str(Path(root).resolve())


# ---------------------------------------------------------------------------
# Snapshot
# ---------------------------------------------------------------------------


def backup_db(src: Path, dst: Path) -> None:
    """SQLite's online backup: consistent even while the server is writing."""
    dst.parent.mkdir(parents=True, exist_ok=True)
    tmp = dst.with_name(dst.name + ".tmp")
    tmp.unlink(missing_ok=True)
    source = sqlite3.connect(src)
    target = sqlite3.connect(tmp)
    try:
        source.backup(target)
        # a copy in WAL mode can't be opened read-only without its -shm file
        target.execute("PRAGMA journal_mode=DELETE")
    finally:
        target.close()
        source.close()
    os.replace(tmp, dst)


def copy_file(src: Path, dst: Path) -> str:
    """'copied', 'present' (same size already there) or 'conflict' (left alone)."""
    if dst.exists():
        return "present" if dst.stat().st_size == src.stat().st_size else "conflict"
    dst.parent.mkdir(parents=True, exist_ok=True)
    tmp = dst.with_name(dst.name + ".part")
    shutil.copy2(src, tmp)
    os.replace(tmp, dst)
    return "copied"


def copy_tree(src: Path, dst: Path) -> Counter:
    tally: Counter = Counter()
    for path in sorted(src.rglob("*")):
        if path.is_file():
            tally[copy_file(path, dst / path.relative_to(src))] += 1
    return tally


def referenced_files(conn: sqlite3.Connection, schema: str = "main"):
    """(kind, path under the data folder) for every file the database points at."""
    for (path,) in conn.execute(f"SELECT audio_path FROM {schema}.dictations"):
        rel = under(path, "audio")
        if rel is not None:
            yield "audio", rel
    for (path,) in conn.execute(
        f"SELECT ct2_path FROM {schema}.models WHERE ct2_path IS NOT NULL"
    ):
        rel = under(path, "models")
        if rel is not None:
            yield "model", rel
    for (run_id,) in conn.execute(f"SELECT id FROM {schema}.training_runs"):
        yield "run", PurePosixPath("runs", run_id)


def snapshot(data: Path, out: Path) -> dict:
    backup_db(data / "una.db", out / "una.db")
    conn = sqlite3.connect(uri(out / "una.db"), uri=True)
    tally: Counter = Counter()
    try:
        for kind, rel in referenced_files(conn):
            src = data / rel
            if not src.exists():
                tally[f"{kind} missing"] += 1  # deleted dictations lose their audio
                continue
            if src.is_dir():
                for outcome, n in copy_tree(src, out / rel).items():
                    tally[f"{kind} files {outcome}"] += n
            else:
                tally[f"{kind} {copy_file(src, out / rel)}"] += 1
        info = {
            "host": socket.gethostname(),
            "data_dir": str(data.resolve()),
            "taken_at": utcnow(),
            "counts": counts(conn),
            "files": dict(sorted(tally.items())),
        }
    finally:
        conn.close()
    (out / "snapshot.json").write_text(json.dumps(info, indent=2) + "\n")
    return info


# ---------------------------------------------------------------------------
# Merge
# ---------------------------------------------------------------------------


def counts(conn: sqlite3.Connection, schema: str = "main") -> dict[str, int]:
    have = {
        r[0] for r in conn.execute(f"SELECT name FROM {schema}.sqlite_master WHERE type='table'")
    }
    return {
        t: conn.execute(f"SELECT count(*) FROM {schema}.{t}").fetchone()[0]
        for t in TABLES if t in have
    }


def migrations(conn: sqlite3.Connection, schema: str) -> set[str]:
    try:
        return {r[0] for r in conn.execute(f"SELECT version FROM {schema}.schema_migrations")}
    except sqlite3.OperationalError:
        return set()


def apply_migrations(conn: sqlite3.Connection, wanted: set[str], migrations_dir: Path) -> list:
    """Bring the target up to the source's schema, the way the server does at start."""
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations "
        "(version TEXT PRIMARY KEY, applied_at TEXT NOT NULL)"
    )
    applied = migrations(conn, "main")
    done = []
    for path in sorted(migrations_dir.glob("*.sql")):
        if path.stem in applied or not (wanted - applied):
            continue
        conn.executescript(path.read_text())
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)",
            (path.stem, utcnow()),
        )
        conn.commit()
        applied.add(path.stem)
        done.append(path.stem)
    if wanted - applied:
        raise SystemExit(f"target can't be migrated to {sorted(wanted - applied)}")
    return done


def columns(conn: sqlite3.Connection, schema: str, table: str) -> list[str]:
    return [r[1] for r in conn.execute(f"PRAGMA {schema}.table_info({table})")]


class Merger:
    def __init__(self, conn: sqlite3.Connection, layout: Layout):
        self.conn = conn
        self.layout = layout
        self.tally: dict[str, Counter] = {t: Counter() for t in TABLES}
        self.notes: list[str] = []

    # -- helpers -------------------------------------------------------------

    def shared(self, table: str) -> list[str]:
        mine = columns(self.conn, "main", table)
        theirs = set(columns(self.conn, "src", table))
        return [c for c in mine if c in theirs]

    def rows(self, table: str, order: str = "rowid"):
        cols = self.shared(table)
        cur = self.conn.execute(f"SELECT {', '.join(cols)} FROM src.{table} ORDER BY {order}")
        for values in cur.fetchall():
            yield dict(zip(cols, values, strict=True))

    def one(self, sql: str, *args):
        return self.conn.execute(sql, args).fetchone()

    def insert(self, table: str, row: dict) -> None:
        cols = list(row)
        self.conn.execute(
            f"INSERT INTO main.{table} ({', '.join(cols)}) VALUES ({', '.join('?' * len(cols))})",
            [row[c] for c in cols],
        )

    def update(self, table: str, row: dict, key: str, keep: tuple = ()) -> None:
        cols = [c for c in row if c != key and c not in keep]
        self.conn.execute(
            f"UPDATE main.{table} SET {', '.join(f'{c} = ?' for c in cols)} WHERE {key} = ?",
            [*(row[c] for c in cols), row[key]],
        )

    def rewrite(self, value: str | None, anchor: str, prefix: str) -> str | None:
        if not value:
            return value
        rel = under(value, anchor)
        return join(prefix, rel) if rel is not None else value

    # -- tables, in foreign-key order ----------------------------------------

    def models(self) -> None:
        for row in self.rows("models", "created_at"):
            if self.one("SELECT 1 FROM main.models WHERE id = ?", row["id"]):
                self.tally["models"]["kept target"] += 1
                continue
            row["is_active"] = 0  # the target keeps serving what it serves
            row["ct2_path"] = self.rewrite(row.get("ct2_path"), "models", self.layout.models)
            if row.get("parent_model_id") and not self.one(
                "SELECT 1 FROM main.models WHERE id = ?", row["parent_model_id"]
            ):
                row["parent_model_id"] = None
            self.insert("models", row)
            self.tally["models"]["added"] += 1

    def training_runs(self) -> None:
        for row in self.rows("training_runs", "started_at"):
            if self.one("SELECT 1 FROM main.training_runs WHERE id = ?", row["id"]):
                self.tally["training_runs"]["kept target"] += 1
                continue
            row["log_path"] = self.rewrite(row.get("log_path"), "runs", self.layout.runs)
            row["pid"] = None
            if row.get("base_model_id") and not self.one(
                "SELECT 1 FROM main.models WHERE id = ?", row["base_model_id"]
            ):
                row["base_model_id"] = None
            self.insert("training_runs", row)
            self.tally["training_runs"]["added"] += 1

    def dictations(self) -> None:
        for row in self.rows("dictations", "created_at"):
            have = self.one("SELECT deleted FROM main.dictations WHERE id = ?", row["id"])
            if have is not None:
                if row.get("deleted") and not have[0]:
                    # deleted on the source after an earlier merge: follow it (a soft
                    # delete, so the recording stays on disk)
                    self.conn.execute("UPDATE main.dictations SET deleted = 1 WHERE id = ?",
                                      (row["id"],))
                    self.tally["dictations"]["deleted since"] += 1
                else:
                    self.tally["dictations"]["already there"] += 1
                continue
            if row.get("utterance_id") and self.one(
                "SELECT 1 FROM main.dictations WHERE utterance_id = ?", row["utterance_id"]
            ):
                self.tally["dictations"]["skipped: same utterance under another id"] += 1
                self.notes.append(f"dictation {row['id']}: its utterance is already on the target")
                continue
            row["audio_path"] = self.rewrite(row["audio_path"], "audio", self.layout.audio)
            if row.get("asr_model_id") and not self.one(
                "SELECT 1 FROM main.models WHERE id = ?", row["asr_model_id"]
            ):
                row["asr_model_id"] = None
            self.insert("dictations", row)
            self.tally["dictations"]["added"] += 1

    def _by_timestamp(self, table: str, row: dict, key: str, have, keep: tuple = ()) -> None:
        """A row both sides have: the source's wins only if it changed later."""
        if (row.get("updated_at") or "") > (have["updated_at"] or ""):
            self.update(table, {**row, key: have[key]}, key, keep)
            self.tally[table]["updated (newer on source)"] += 1
        else:
            self.tally[table]["kept target"] += 1

    def corrections(self) -> None:
        for row in self.rows("corrections", "created_at"):
            if not self.one("SELECT 1 FROM main.dictations WHERE id = ?", row["dictation_id"]):
                self.tally["corrections"]["skipped: dictation not merged"] += 1
                continue
            have = self.one(
                "SELECT id, updated_at FROM main.corrections WHERE dictation_id = ?",
                row["dictation_id"],
            )
            if have is None:
                self.insert("corrections", row)
                self.tally["corrections"]["added"] += 1
            else:
                self._by_timestamp("corrections", row, "id", have, keep=("created_at",))

    def teacher_labels(self) -> None:
        for row in self.rows("teacher_labels", "created_at"):
            if not self.one("SELECT 1 FROM main.dictations WHERE id = ?", row["dictation_id"]):
                self.tally["teacher_labels"]["skipped: dictation not merged"] += 1
                continue
            have = self.one(
                "SELECT dictation_id, updated_at FROM main.teacher_labels "
                "WHERE dictation_id = ?", row["dictation_id"],
            )
            if have is None:
                self.insert("teacher_labels", row)
                self.tally["teacher_labels"]["added"] += 1
            else:
                self._by_timestamp("teacher_labels", row, "dictation_id", have,
                                   keep=("created_at",))

    def dictionary_entries(self) -> None:
        for row in self.rows("dictionary_entries", "created_at"):
            by_id = self.one(
                "SELECT phrase, hit_count FROM main.dictionary_entries WHERE id = ?", row["id"]
            )
            if by_id is not None:
                if (row.get("hit_count") or 0) > (by_id[1] or 0):
                    self.conn.execute(
                        "UPDATE main.dictionary_entries SET hit_count = ? WHERE id = ?",
                        (row["hit_count"], row["id"]),
                    )
                    self.tally["dictionary_entries"]["hit count raised"] += 1
                else:
                    self.tally["dictionary_entries"]["kept target"] += 1
            elif self.one("SELECT 1 FROM main.dictionary_entries WHERE phrase = ?", row["phrase"]):
                self.tally["dictionary_entries"]["kept target (same phrase)"] += 1
            else:
                self.insert("dictionary_entries", row)
                self.tally["dictionary_entries"]["added"] += 1

    def settings(self) -> None:
        for row in self.rows("settings", "key"):
            have = self.one("SELECT value FROM main.settings WHERE key = ?", row["key"])
            if have is None:
                self.insert("settings", row)
                self.tally["settings"]["added"] += 1
            elif have[0] == row["value"]:
                self.tally["settings"]["same"] += 1
            else:
                self.tally["settings"]["kept target"] += 1
                self.notes.append(
                    f"setting {row['key']}: kept target {have[0]!r}, source had {row['value']!r}"
                )

    def style_corpus(self) -> None:
        for row in self.rows("style_corpus", "created_at"):
            have = self.one(
                "SELECT id, updated_at FROM main.style_corpus WHERE id = ?", row["id"]
            )
            if have is not None:
                self._by_timestamp("style_corpus", row, "id", have, keep=("created_at",))
            elif self.one(
                "SELECT 1 FROM main.style_corpus WHERE content_hash = ?", row["content_hash"]
            ):
                self.tally["style_corpus"]["kept target (same text)"] += 1
            else:
                self.insert("style_corpus", row)
                self.tally["style_corpus"]["added"] += 1

    def run(self) -> None:
        present = {
            r[0] for r in self.conn.execute("SELECT name FROM src.sqlite_master WHERE type='table'")
        }
        for table in TABLES:
            if table in present:
                getattr(self, table)()


def copy_files(snap: Path, target: Path) -> Counter:
    """Bring the snapshot's files over; never replace one the target already has."""
    conn = sqlite3.connect(uri(snap / "una.db"), uri=True)
    tally: Counter = Counter()
    try:
        for kind, rel in referenced_files(conn):
            src = snap / rel
            if not src.exists():
                continue
            if src.is_dir():
                for outcome, n in copy_tree(src, target / rel).items():
                    tally[f"{kind} files {outcome}"] += n
            else:
                tally[f"{kind} {copy_file(src, target / rel)}"] += 1
    finally:
        conn.close()
    return tally


def merge(snap: Path, target: Path, *, root: str | None = None, dry_run: bool = False,
          migrations_dir: Path | None = None) -> dict:
    db_path = target / "una.db"
    if not db_path.exists():
        raise SystemExit(f"{db_path} doesn't exist: start una there once so it makes one")
    conn = sqlite3.connect(uri(db_path, "rw"), uri=True, isolation_level=None)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA busy_timeout=5000")
    conn.execute("PRAGMA foreign_keys=ON")
    conn.execute("ATTACH DATABASE ? AS src", (uri(snap / "una.db"),))
    report: dict = {"before": counts(conn)}
    try:
        missing = migrations(conn, "src") - migrations(conn, "main")
        if missing:
            if dry_run:
                raise SystemExit(f"target lacks migrations {sorted(missing)}; start una there "
                                 "with this checkout once (or merge without --dry-run)")
            mdir = migrations_dir or MIGRATIONS_DIR
            if mdir is None or not mdir.is_dir():
                raise SystemExit(f"target lacks migrations {sorted(missing)}; pass --migrations")
            report["migrated"] = apply_migrations(conn, missing, mdir)
        layout = Layout(conn, root if root is not None else str(target))
        report["layout"] = {"audio": layout.audio, "models": layout.models, "runs": layout.runs}
        if not dry_run:
            backups = target / "backups"
            backups.mkdir(exist_ok=True)
            backup = backups / f"una-before-merge-{stamp()}.db"
            backup_db(db_path, backup)
            report["backup"] = str(backup)
            report["files"] = dict(sorted(copy_files(snap, target).items()))
        merger = Merger(conn, layout)
        conn.execute("BEGIN IMMEDIATE")
        try:
            broken = {tuple(r) for r in conn.execute("PRAGMA main.foreign_key_check")}
            merger.run()
            problems = {tuple(r) for r in conn.execute("PRAGMA main.foreign_key_check")} - broken
            if problems:
                raise RuntimeError(f"the merge broke foreign keys: {sorted(problems)[:5]}")
        except BaseException:
            conn.execute("ROLLBACK")
            raise
        conn.execute("ROLLBACK" if dry_run else "COMMIT")
        report["rows"] = {t: dict(c) for t, c in merger.tally.items() if c}
        report["notes"] = merger.notes
        report["after"] = report["before"] if dry_run else counts(conn)
        report["dry_run"] = dry_run
    finally:
        conn.close()
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    snap = sub.add_parser("snapshot", help="copy a server's data, safe while it runs")
    snap.add_argument("data", type=Path, help="the server's data folder (server/data)")
    snap.add_argument("out", type=Path, help="where the snapshot goes (reused next time)")
    mrg = sub.add_parser("merge", help="merge a snapshot into a server's data")
    mrg.add_argument("snapshot", type=Path)
    mrg.add_argument("data", type=Path, help="the target server's data folder")
    mrg.add_argument(
        "--stored-root",
        help="the data folder as the target server names it, when there are no rows to "
             "learn it from (Docker: /data; default: DATA as given)",
    )
    mrg.add_argument("--migrations", type=Path, help="migrations folder (default: the repo's)")
    mrg.add_argument("--dry-run", action="store_true", help="report, change nothing")
    args = parser.parse_args(argv)
    if args.cmd == "snapshot":
        info = snapshot(args.data, args.out)
    else:
        info = merge(args.snapshot, args.data, root=args.stored_root, dry_run=args.dry_run,
                     migrations_dir=args.migrations)
    print(json.dumps(info, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
