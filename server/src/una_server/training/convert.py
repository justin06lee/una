"""Merge the LoRA adapter into the HF base, convert to CTranslate2, and smoke-test the result.

Heavy deps (torch/peft/transformers/faster-whisper model load) are imported inside functions.
"""

from __future__ import annotations

import gc
import logging
import shutil
import subprocess
import sys
from pathlib import Path

from ..config import AsrConfig

log = logging.getLogger(__name__)

# faster-whisper needs these next to model.bin to load a local dir (128-mel models especially).
COPYABLE_FILES = ("tokenizer.json", "preprocessor_config.json")


def merge_adapter(base_hf_model: str, adapter_dir: Path, merged_dir: Path, device: str) -> Path:
    """Load the HF base, apply + merge the adapter, save a full HF model (plus processor files)."""
    import torch
    from peft import PeftModel
    from transformers import WhisperForConditionalGeneration, WhisperProcessor

    dtype = torch.float16 if device == "cuda" and torch.cuda.is_available() else torch.float32
    log.info("merging adapter %s into %s (%s)", adapter_dir, base_hf_model, dtype)
    model = WhisperForConditionalGeneration.from_pretrained(base_hf_model, torch_dtype=dtype)
    model = PeftModel.from_pretrained(model, str(adapter_dir))
    merged = model.merge_and_unload()
    merged_dir.mkdir(parents=True, exist_ok=True)
    merged.save_pretrained(str(merged_dir), safe_serialization=True)
    # tokenizer.json + preprocessor_config.json ride along for the CT2 converter / faster-whisper
    processor = WhisperProcessor.from_pretrained(base_hf_model)
    processor.save_pretrained(str(merged_dir))
    # Newer transformers save the processor without preprocessor_config.json;
    # faster-whisper reads the mel-bin count from it (128 for v3 models), so
    # save the feature extractor explicitly — without it the converted model is
    # fed 80-mel features and every transcription fails.
    processor.feature_extractor.save_pretrained(str(merged_dir))
    del model, merged
    gc.collect()
    return merged_dir


def converter_command(merged_dir: Path, out_dir: Path) -> list[str]:
    """ctranslate2 console script if installed, else `python -m` module fallback."""
    script = shutil.which("ct2-transformers-converter")
    base = [script] if script else [sys.executable, "-m", "ctranslate2.converters.transformers"]
    cmd = [
        *base,
        "--model", str(merged_dir),
        "--output_dir", str(out_dir),
        "--quantization", "float16",
        "--force",  # overwrite leftovers from a crashed earlier attempt of the same run
    ]
    copy_files = [name for name in COPYABLE_FILES if (merged_dir / name).exists()]
    if copy_files:
        cmd += ["--copy_files", *copy_files]
    return cmd


def ensure_feature_extractor_config(merged_dir: Path, out_dir: Path) -> None:
    """Guarantee preprocessor_config.json sits next to model.bin.

    Belt and suspenders for the --copy_files path: if the converter did not
    copy it, faster-whisper falls back to 80 mel bins and a v3 (128-mel)
    model rejects every input.
    """
    src = merged_dir / "preprocessor_config.json"
    dst = out_dir / "preprocessor_config.json"
    if src.exists() and not dst.exists():
        shutil.copy2(src, dst)


def convert_to_ct2(merged_dir: Path, out_dir: Path) -> Path:
    out_dir.parent.mkdir(parents=True, exist_ok=True)
    cmd = converter_command(merged_dir, out_dir)
    log.info("converting to CTranslate2: %s", " ".join(cmd))
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise RuntimeError(
            f"ct2 conversion failed with code {proc.returncode}: {proc.stderr.strip()[-2000:]}"
        )
    ensure_feature_extractor_config(merged_dir, out_dir)
    return out_dir


def smoke_test(ct2_dir: Path, wav_path: str, cfg: AsrConfig) -> str:
    """Load the converted model with faster-whisper and transcribe one training WAV.

    Empty/whitespace output means the conversion is broken: raise so the run aborts
    before any promotion decision.
    """
    from faster_whisper import WhisperModel  # deferred: heavy import

    log.info("smoke testing %s on %s", ct2_dir, wav_path)
    model = WhisperModel(str(ct2_dir), device=cfg.device, compute_type=cfg.compute_type)
    try:
        segments, _info = model.transcribe(
            wav_path,
            language=cfg.language,
            beam_size=1,
            temperature=0.0,
            condition_on_previous_text=False,
            vad_filter=False,
        )
        text = " ".join(seg.text.strip() for seg in segments).strip()
    finally:
        del model
        gc.collect()
    if not text:
        raise RuntimeError(f"smoke test of {ct2_dir} produced empty transcription")
    log.info("smoke test transcription: %r", text)
    return text
