"""Data prep for the Korean / code-switching work (docs/korean-codeswitch.md). CPU and disk only.

    python koen_data.py fleurs ~/una-koen/fleurs/ko_kr test ko > ko_test.tsv
    python koen_data.py zeroth ~/una-koen/zeroth/test_data_01 ko > zeroth_test.tsv
    python koen_data.py hike ~/una-koen/hike/test.parquet > hike.tsv
    python koen_data.py catalog --server-dir ~/una/server --lid large-v3-turbo > dictations_ko.tsv

`fleurs`, `zeroth` and `hike` write manifests in koen_eval.py's format (id, path, text, set).
`catalog` reads una's database read-only and lists every dictation with Hangul in any of its
texts and, with --lid, Whisper's P(ko) share for its audio: una has been forcing
language="en", so Korean speech was stored as an English translation and only the audio says
it was Korean.
"""

from __future__ import annotations

import argparse
import csv
import sqlite3
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))  # also under python -I
import koen_metrics as m

FIELDS = ["id", "path", "text", "set"]


def _writer(fields: list[str]) -> csv.DictWriter:
    writer = csv.DictWriter(sys.stdout, fieldnames=fields, delimiter="\t", extrasaction="ignore")
    writer.writeheader()
    return writer


def _unquote(field: str) -> str:
    if len(field) >= 2 and field[0] == field[-1] == '"':
        field = field[1:-1]
    return field.replace('""', '"')


def fleurs(args: argparse.Namespace) -> None:
    """FLEURS <lang>/<split>.tsv: id, file, raw transcription, transcription, ... (no quoting rules
    to trust: some normalized fields have unbalanced quotes, so split on tabs only)."""
    root = Path(args.dir).expanduser()
    writer = _writer(FIELDS)
    for line in (root / f"{args.split}.tsv").read_text(encoding="utf-8").splitlines():
        cols = line.split("\t")
        audio = root / args.split / cols[1]
        if audio.exists():
            writer.writerow({"id": f"fleurs-{cols[1].removesuffix('.wav')}", "path": str(audio),
                             "text": _unquote(cols[2]), "set": args.set})


def zeroth(args: argparse.Namespace) -> None:
    """Zeroth-Korean (LibriSpeech layout): <spk>/<utt>.flac beside <spk>_*.trans.txt lines 'utt text'."""
    writer = _writer(FIELDS)
    for trans in sorted(Path(args.dir).expanduser().rglob("*.trans.txt")):
        for line in trans.read_text(encoding="utf-8").splitlines():
            utt, _, text = line.partition(" ")
            audio = trans.parent / f"{utt}.flac"
            if audio.exists() and text.strip():
                writer.writerow({"id": f"zeroth-{utt}", "path": str(audio), "text": text.strip(),
                                 "set": args.set})


def hike(args: argparse.Namespace) -> None:
    """HiKE (thetaone-ai/HiKE, Apache-2.0): the parquet's audio written out beside it."""
    import pyarrow.parquet as pq

    parquet = Path(args.parquet).expanduser()
    out = parquet.parent / "audio"
    out.mkdir(exist_ok=True)
    writer = _writer(FIELDS + ["cs_level", "category"])
    for row in pq.read_table(parquet).to_pylist():
        suffix = Path(row["audio"].get("path") or "x.wav").suffix or ".wav"
        audio = out / f"{row['sample_id']}{suffix}"
        if not audio.exists():
            audio.write_bytes(row["audio"]["bytes"])
        writer.writerow({"id": f"hike-{row['sample_id']}", "path": str(audio), "text": row["text"],
                         "set": args.set, "cs_level": row["cs_level"], "category": row["category"]})


CATALOG_SQL = """
SELECT d.id, d.created_at, d.duration_ms, d.audio_path, d.language, d.deleted, d.eval_holdout,
       d.raw_text, d.cleaned_text, c.action, c.corrected_text, c.polished_text
FROM dictations d LEFT JOIN corrections c ON c.dictation_id = d.id
ORDER BY d.created_at
"""


def catalog(args: argparse.Namespace) -> None:
    server = Path(args.server_dir).expanduser()
    db = sqlite3.connect(f"file:{server / 'data' / 'una.db'}?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    rows = [dict(r) for r in db.execute(CATALOG_SQL)]
    texts = ("raw_text", "cleaned_text", "corrected_text", "polished_text")

    model = None
    if args.lid:
        from faster_whisper import WhisperModel
        model = WhisperModel(args.lid, device="cpu", compute_type="int8", cpu_threads=args.threads)

    fields = ["id", "created_at", "duration_ms", "deleted", "eval_holdout", "hangul_in_text",
              "p_ko_share", "action", "raw_text", "corrected_text", "polished_text", "audio_path"]
    writer = _writer(fields)
    listed = 0
    for row in rows:
        row["hangul_in_text"] = int(any(row[k] and m.has_hangul(row[k]) for k in texts))
        row["p_ko_share"] = ""
        audio = Path(row["audio_path"] or "")
        audio = audio if audio.is_absolute() else server / audio
        if model is not None and audio.exists() and (row["duration_ms"] or 0) >= 1000:
            _lang, _p, probs = model.detect_language(m.load_audio(str(audio)))
            p = dict(probs)
            ko, en = p.get("ko", 0.0), p.get("en", 0.0)
            row["p_ko_share"] = f"{ko / (ko + en):.3f}" if ko + en else "0"
        is_ko = row["hangul_in_text"] or (row["p_ko_share"] and float(row["p_ko_share"]) >= args.min_share)
        if is_ko or args.all:
            writer.writerow(row)
            listed += 1
    print(f"{len(rows)} dictations, {listed} listed", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("fleurs")
    f.add_argument("dir"); f.add_argument("split"); f.add_argument("set")
    f.set_defaults(func=fleurs)
    z = sub.add_parser("zeroth")
    z.add_argument("dir"); z.add_argument("set")
    z.set_defaults(func=zeroth)
    h = sub.add_parser("hike")
    h.add_argument("parquet"); h.add_argument("set", nargs="?", default="mixed")
    h.set_defaults(func=hike)
    c = sub.add_parser("catalog")
    c.add_argument("--server-dir", required=True)
    c.add_argument("--lid", help="faster-whisper model for language ID on the audio (CPU)")
    c.add_argument("--min-share", type=float, default=0.3, help="list audio with P(ko) share >= this")
    c.add_argument("--threads", type=int, default=2)
    c.add_argument("--all", action="store_true", help="list every dictation, not just Korean ones")
    c.set_defaults(func=catalog)
    args = ap.parse_args(argv)
    args.func(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
