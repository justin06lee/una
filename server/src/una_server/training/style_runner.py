"""Style training runner: `python -m una_server.training.style_runner --run-id <ULID>`.

Spawned by services.jobs.JobManager (same single GPU slot as ASR runs); owns its
training_runs row via sync sqlite3. Mirrors runner.py's stage order:
building -> training (SFT, then DPO) -> converting (ollama create) -> evaluating ->
promoted|rejected.

wer_baseline / wer_candidate store the style metric — mean normalized edit
distance in [0, 1] between model output and polished target (lower is better) — on
confirmed dictations when there are enough, else on the holdout of the user's own
writing (eval_set says which). A candidate must also not answer more of the reply
probes than the current model. `notes` records what went into the run.
"""

from __future__ import annotations

import argparse
import json
import logging
import random
import shutil
import sqlite3
import sys
from pathlib import Path

import httpx

from ..config import Config, load_config
from ..db import utcnow
from . import style_dataset, style_eval, style_promote
from .runner import RunContext, _install_sigterm_handler, _setup_logging, connect

log = logging.getLogger("una_server.training.style_runner")


def _hyperparams(row: sqlite3.Row, cfg: Config) -> dict:
    """The run's stored style_* settings over the current config's (older runs lack some)."""
    defaults = style_settings(cfg.training)
    stored = json.loads(row["hyperparams_json"]) if row["hyperparams_json"] else {}
    return {**defaults, **stored}


def style_settings(training) -> dict:
    return {k: v for k, v in training.model_dump().items() if k.startswith("style_")}


def _unload_ollama_models(cfg: Config) -> None:
    """Best-effort: ask Ollama to release resident models so QLoRA gets the VRAM."""
    try:
        with httpx.Client(base_url=cfg.cleanup.ollama_url, timeout=10.0) as client:
            resp = client.get("/api/ps")
            for model in resp.json().get("models", []):
                client.post(
                    "/api/generate", json={"model": model["name"], "keep_alive": 0}
                )
                log.info("asked ollama to unload %s", model["name"])
    except (httpx.HTTPError, ValueError):
        log.info("could not query ollama for resident models (continuing)")


def _cleanup(run_dir: Path) -> None:
    """Drop checkpoints; keep adapter/, the Modelfile and its ollama-adapter/, and train.log."""
    for checkpoint in run_dir.glob("checkpoint-*"):
        shutil.rmtree(checkpoint, ignore_errors=True)


def _choose_eval(split: style_dataset.StyleSplit, min_samples: int):
    """Judge on confirmed dictations when there are enough; else the writing holdout."""
    if len(split.eval) >= min_samples:
        return "confirmed", split.eval
    if len(split.writing_eval) >= min_samples:
        return "writing", split.writing_eval
    raise RuntimeError(
        f"not enough held-out pairs to judge a candidate: {len(split.eval)} confirmed, "
        f"{len(split.writing_eval)} from your writing (need {min_samples})"
    )


def _on_policy_preferences(
    cfg: Config, model: str, split: style_dataset.StyleSplit, limit: int
) -> list[style_dataset.PreferencePair]:
    """Your writing preferred over what the current model makes of its spoken version.

    Shows the model its own habits — formal capitals where you'd write lowercase, fillers
    left in, answers instead of cleanups — next to what you would have written.
    """
    pool = [s for s in split.train if s.origin == "writing"]
    random.Random(0).shuffle(pool)
    pool = pool[:limit]
    if not pool:
        return []
    produced = style_eval.outputs(cfg.cleanup.ollama_url, model, pool, cfg.cleanup)
    return [
        style_dataset.PreferencePair(s.raw_text, s.polished_text, out, s.app_name, "on-policy")
        for s, out in zip(pool, produced, strict=True)
        if out.strip() and style_dataset.differs(s.polished_text, out)
    ]


def _execute(run: RunContext, cfg: Config, row: sqlite3.Row, run_dir: Path) -> None:
    from . import train_style_lora  # heavy deps live behind this module

    hp = _hyperparams(row, cfg)
    log.info("style run %s starting with hyperparams %s", run.run_id, hp)
    baseline_model = row["base_model_id"] or cfg.cleanup.model
    notes: list[str] = []

    # -- building -------------------------------------------------------------
    run.update(status="building", progress=0.02)
    split = style_dataset.build_style_datasets(
        run.conn,
        use_writing=bool(hp["style_use_writing"]),
        use_silver=bool(hp["style_use_silver"]),
        gold_repeat=int(hp["style_gold_repeat"]),
        augment=bool(hp.get("style_augment_disfluency", False)),
    )
    if not split.train:
        raise RuntimeError("no style training pairs (confirm some in Review, or import your writing)")
    eval_set, eval_samples = _choose_eval(split, int(hp["style_min_eval_samples"]))
    counts = split.counts()
    notes.append("train: " + ", ".join(f"{n} {origin}" for origin, n in sorted(counts.items())))
    notes.append(f"eval: {len(eval_samples)} {eval_set}")
    run.update(n_train=len(split.train), n_eval=len(eval_samples), eval_set=eval_set,
               notes="; ".join(notes))

    preferences: list[style_dataset.PreferencePair] = []
    if hp["style_dpo"]:
        preferences = list(split.preferences)
        if int(hp["style_dpo_synthetic"]) > 0:
            run.update(progress=0.04)
            preferences += _on_policy_preferences(
                cfg, baseline_model, split, int(hp["style_dpo_synthetic"])
            )
        if len(preferences) < int(hp["style_dpo_min_pairs"]):
            notes.append(f"dpo: skipped ({len(preferences)} preference pairs)")
            preferences = []

    # -- training -------------------------------------------------------------
    _unload_ollama_models(cfg)
    run.update(status="training", progress=0.1)
    last_progress = 0.1

    def on_progress(fraction: float) -> None:
        nonlocal last_progress
        progress = 0.1 + 0.6 * min(max(fraction, 0.0), 1.0)
        if progress - last_progress >= 0.005:
            last_progress = progress
            run.update(progress=round(progress, 4))

    adapter_dir, dpo_stats = train_style_lora.train_style_adapter(
        split.train, run_dir, hp, cfg.cleanup,
        device=cfg.asr.device, on_progress=on_progress, preferences=preferences,
    )
    if dpo_stats.get("error"):
        notes.append(f"dpo: failed, kept the supervised adapter ({dpo_stats['error']})")
    elif dpo_stats:
        origins: dict[str, int] = {}
        for pair in preferences:
            origins[pair.origin] = origins.get(pair.origin, 0) + 1
        notes.append(
            "dpo: " + ", ".join(f"{n} {o}" for o, n in sorted(origins.items()))
            + f" (preferred {dpo_stats.get('preferred_rate')}, loss {dpo_stats.get('final_loss')})"
        )
    run.update(notes="; ".join(notes))

    # -- converting: layer the adapter into an ollama model -------------------
    run.update(status="converting", progress=0.75)
    model_name = style_promote.new_style_model_name(run.run_id)
    style_promote.create_ollama_model(
        model_name, hp["style_ollama_base"], adapter_dir, run_dir
    )
    style_eval.smoke_test(
        cfg.cleanup.ollama_url, model_name, split.train[0], cfg.cleanup
    )

    # -- evaluating (both models through ollama, the real serving path) -------
    run.update(status="evaluating", progress=0.85)
    dist_baseline = style_eval.mean_distance(
        cfg.cleanup.ollama_url, baseline_model, eval_samples, cfg.cleanup
    )
    probes_baseline = style_eval.reply_failures(cfg.cleanup.ollama_url, baseline_model, cfg.cleanup)
    dist_candidate = style_eval.mean_distance(
        cfg.cleanup.ollama_url, model_name, eval_samples, cfg.cleanup
    )
    probes_candidate = style_eval.reply_failures(cfg.cleanup.ollama_url, model_name, cfg.cleanup)
    run.update(wer_baseline=round(dist_baseline, 4), wer_candidate=round(dist_candidate, 4))

    # -- promotion gate -------------------------------------------------------
    ok, reason = style_promote.should_promote_style(
        dist_baseline, dist_candidate, len(eval_samples),
        cfg.training.style_promotion_margin, cfg.training.style_min_eval_samples,
    )
    probes_ok, probes_reason = style_promote.probe_gate(
        len(probes_baseline), len(probes_candidate)
    )
    notes.append(probes_reason)
    if ok and not probes_ok:
        ok, reason = False, probes_reason
    log.info("style promotion decision: %s — %s", "promote" if ok else "reject", reason)
    notes.append(("promoted: " if ok else "rejected: ") + reason)
    if ok:
        run.update(
            status="promoted", produced_model_id=model_name, progress=1.0,
            finished_at=utcnow(), notes="; ".join(notes),
        )
        style_promote.switch_cleanup_model(cfg.server.port, model_name)
    else:
        # keep the ollama model + adapter for inspection; the cleaner is untouched
        run.update(
            status="rejected", produced_model_id=model_name, progress=1.0,
            finished_at=utcnow(), notes="; ".join(notes),
        )

    _cleanup(run_dir)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="una style fine-tuning runner")
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args(argv)

    cfg = load_config()
    conn = connect(cfg.db_path)
    row = conn.execute("SELECT * FROM training_runs WHERE id = ?", (args.run_id,)).fetchone()
    if row is None:
        print(f"training run {args.run_id} not found in {cfg.db_path}", file=sys.stderr)
        return 1

    run_dir = cfg.runs_dir / args.run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    _setup_logging(Path(row["log_path"]) if row["log_path"] else run_dir / "train.log")
    _install_sigterm_handler(cfg.db_path, args.run_id)

    run = RunContext(conn, args.run_id)
    try:
        _execute(run, cfg, row, run_dir)
        return 0
    except Exception as exc:
        log.exception("style run %s failed", args.run_id)
        run.update(status="failed", error=str(exc), finished_at=utcnow())
        return 1
    finally:
        conn.close()


if __name__ == "__main__":
    sys.exit(main())
