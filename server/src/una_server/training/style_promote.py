"""Style model promotion: gate decision, Ollama model creation, live switch."""

from __future__ import annotations

import logging
import shutil
import subprocess
from pathlib import Path

import httpx

log = logging.getLogger(__name__)


def should_promote_style(
    dist_baseline: float,
    dist_candidate: float,
    n_eval: int,
    margin: float,
    min_samples: int,
) -> tuple[bool, str]:
    """Promote only when the candidate's mean edit distance improves by >= margin."""
    if n_eval < min_samples:
        return False, f"eval set too small ({n_eval} < {min_samples})"
    delta = dist_baseline - dist_candidate
    if delta >= margin:
        return True, f"distance improved by {delta:.4f} (>= {margin})"
    return False, f"distance improved by only {delta:.4f} (< {margin})"


def probe_gate(baseline_failures: int, candidate_failures: int) -> tuple[bool, str]:
    """A candidate may not answer more of the reply probes than the model it replaces."""
    if candidate_failures > baseline_failures:
        return False, (
            f"answers {candidate_failures} reply probes instead of cleaning them "
            f"(current model: {baseline_failures})"
        )
    return True, f"reply probes: {candidate_failures} answered (current model: {baseline_failures})"


def new_style_model_name(run_id: str) -> str:
    return f"una-style-{run_id[-4:].lower()}"


def create_ollama_model(name: str, base: str, adapter_dir: Path, run_dir: Path) -> None:
    """`ollama create` a model layering the LoRA adapter over the quantized base.

    Requires the ollama CLI reachable from the server process (bare-metal deploys).
    In Docker, run the training on a host with the CLI or create the model by hand:
    `ollama create <name> -f <run_dir>/Modelfile`.
    """
    if shutil.which("ollama") is None:
        raise RuntimeError(
            "ollama CLI not found; create the model manually with "
            f"`ollama create {name} -f {run_dir}/Modelfile`"
        )
    modelfile = run_dir / "Modelfile"
    modelfile.write_text(f"FROM {base}\nADAPTER {adapter_dir.resolve()}\n")
    result = subprocess.run(
        ["ollama", "create", name, "-f", str(modelfile)],
        capture_output=True, text=True, timeout=600, check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"ollama create failed ({result.returncode}): {result.stderr.strip()[:500]}"
        )
    log.info("created ollama model %s (FROM %s + adapter)", name, base)


def switch_cleanup_model(port: int, name: str) -> None:
    """Point the live cleaner at the promoted model via the settings API (persisted)."""
    url = f"http://127.0.0.1:{port}/v1/settings"
    try:
        response = httpx.put(url, json={"cleanup.model": name}, timeout=30.0)
        response.raise_for_status()
        log.info("cleanup.model switched to %s", name)
    except httpx.HTTPError as exc:
        log.warning(
            "settings switch failed (%s); %s exists in ollama — set cleanup.model "
            "to it from the dashboard", exc, name,
        )
