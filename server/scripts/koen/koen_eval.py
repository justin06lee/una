"""Score a Whisper (CTranslate2) model on EN / KO / mixed manifests: WER, CER, MER, script fidelity.

    python koen_eval.py --model large-v3-turbo --manifest ko.tsv en.tsv --language ko \
        --device cpu --compute-type int8 --limit 120 --out results/turbo-ko

A manifest is a TSV with a header: id, path, text, set (en | ko | mixed). Decoding matches
una's gate (training/evaluate.py): greedy, temperature 0, no previous-text conditioning, no
VAD. --language takes en, ko, auto (Whisper's own detection over every language), route
(detection between en and ko only, ko when P(ko) / (P(ko) + P(en)) >= --ko-threshold) or
koen (both language tokens in the prompt, <|ko|><|en|>, which Peng et al. 2023 found helps
Whisper on code-switched speech with no training; clips over 30 s are cut to 30 s).
"""

from __future__ import annotations

import argparse
import csv
import json
import random
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))  # also under python -I
import koen_metrics as m


def load_manifests(paths: list[Path], limit: int | None, seed: int) -> list[dict]:
    rows: list[dict] = []
    for path in paths:
        with path.open(newline="", encoding="utf-8") as fh:
            part = list(csv.DictReader(fh, delimiter="\t"))
        if limit and len(part) > limit:
            part = random.Random(seed).sample(part, limit)
        rows += part
    return rows


def route_language(model, audio, threshold: float) -> tuple[str, float]:
    _lang, _prob, probs = model.detect_language(audio)
    p = dict(probs)
    ko, en = p.get("ko", 0.0), p.get("en", 0.0)
    share = ko / (ko + en) if ko + en else 0.0
    return ("ko" if share >= threshold else "en"), share


def decode_with_tokens(model, audio, languages: list[str], max_new_tokens: int) -> str:
    """Greedy decode of one <=30 s clip with a hand-built prompt (several language tokens)."""
    from faster_whisper.audio import pad_or_trim

    features = pad_or_trim(model.feature_extractor(audio))
    tok = model.hf_tokenizer
    prompt = [tok.token_to_id(t) for t in
              ["<|startoftranscript|>", *(f"<|{lang}|>" for lang in languages),
               "<|transcribe|>", "<|notimestamps|>"]]
    result = model.model.generate(
        model.encode(features), [prompt], beam_size=1, max_length=len(prompt) + max_new_tokens,
        suppress_blank=True, suppress_tokens=[-1],
    )[0]
    eot = tok.token_to_id("<|endoftext|>")
    return tok.decode([t for t in result.sequences_ids[0] if t < eot]).strip()


def summarize(rows: list[dict]) -> dict:
    out: dict = {}
    for name in sorted({r["set"] for r in rows}):
        part = [r for r in rows if r["set"] == name]
        refs, hyps = [r["text"] for r in part], [r["hyp"] for r in part]
        bad, scored = m.script_fidelity(refs, hyps)
        used: dict[str, int] = {}
        for r in part:
            used[r["lang_used"]] = used.get(r["lang_used"], 0) + 1
        out[name] = {
            "n": len(part),
            "wer": round(m.wer(refs, hyps).rate, 2),
            "cer": round(m.cer(refs, hyps).rate, 2),
            "mer": round(m.mer(refs, hyps).rate, 2),
            "script_violation_utts": bad,
            "script_violation_pct": round(100.0 * bad / scored, 1) if scored else 0.0,
            "hyp_with_hangul_pct": round(100.0 * sum(m.has_hangul(h) for h in hyps) / len(part), 1),
            "language_used": used,
            "seconds_per_utt": round(sum(float(r["seconds"]) for r in part) / len(part), 2),
        }
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--model", required=True, help="CT2 dir or faster-whisper alias")
    ap.add_argument("--manifest", type=Path, nargs="+", required=True)
    ap.add_argument("--language", choices=["en", "ko", "auto", "route", "koen"], required=True)
    ap.add_argument("--ko-threshold", type=float, default=0.5)
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--compute-type", default="int8")
    ap.add_argument("--threads", type=int, default=0)
    ap.add_argument("--limit", type=int, default=None, help="per manifest, sampled with --seed")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--max-new-tokens", type=int, default=224, help="caps hallucination loops")
    ap.add_argument("--out", type=Path, required=True, help="writes <out>.tsv and <out>.json")
    args = ap.parse_args(argv)

    from faster_whisper import WhisperModel

    rows = load_manifests(args.manifest, args.limit, args.seed)
    model = WhisperModel(args.model, device=args.device, compute_type=args.compute_type,
                         cpu_threads=args.threads)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    for i, row in enumerate(rows, 1):
        audio = m.load_audio(row["path"])
        start = time.monotonic()
        share = ""
        if args.language == "koen":
            row["hyp"] = decode_with_tokens(model, audio, ["ko", "en"], args.max_new_tokens)
            row["lang_used"] = "ko+en"
        else:
            if args.language == "route":
                language, share = route_language(model, audio, args.ko_threshold)
            else:
                language = None if args.language == "auto" else args.language
            segments, info = model.transcribe(
                audio, language=language, beam_size=1, temperature=0.0,
                condition_on_previous_text=False, vad_filter=False, word_timestamps=False,
                max_new_tokens=args.max_new_tokens,
            )
            row["hyp"] = " ".join(s.text.strip() for s in segments).strip()
            row["lang_used"] = language or info.language
        row["p_ko_share"] = f"{share:.3f}" if share != "" else ""
        row["seconds"] = f"{time.monotonic() - start:.2f}"
        if i % 20 == 0:
            print(f"{i}/{len(rows)}", file=sys.stderr, flush=True)

    fields = ["id", "set", "lang_used", "p_ko_share", "seconds", "text", "hyp", "path"]
    with args.out.with_suffix(".tsv").open("w", newline="", encoding="utf-8") as fh:
        writer = csv.DictWriter(fh, fieldnames=fields, delimiter="\t", extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)
    summary = {"model": args.model, "language": args.language, "ko_threshold": args.ko_threshold,
               "device": args.device, "compute_type": args.compute_type, "sets": summarize(rows)}
    args.out.with_suffix(".json").write_text(json.dumps(summary, ensure_ascii=False, indent=2))
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
