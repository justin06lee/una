"""Audio ingest: decode arbitrary WAV, resample to 16k mono, persist to disk."""

from __future__ import annotations

import io
from datetime import datetime, timezone
from pathlib import Path

import numpy as np
import soundfile as sf
import soxr

from ..errors import AudioInvalid

TARGET_SR = 16000


def decode_to_16k_mono(data: bytes) -> np.ndarray:
    """Bytes of any soundfile-readable audio -> float32 mono 16kHz [-1, 1]."""
    try:
        audio, sr = sf.read(io.BytesIO(data), dtype="float32", always_2d=True)
    except Exception as exc:
        raise AudioInvalid(f"could not decode audio: {exc}") from exc
    if audio.shape[0] == 0:
        raise AudioInvalid("empty audio")
    mono = audio.mean(axis=1)
    if sr != TARGET_SR:
        mono = soxr.resample(mono, sr, TARGET_SR)
    return np.ascontiguousarray(mono, dtype=np.float32)


def duration_ms(samples: np.ndarray) -> int:
    return int(len(samples) * 1000 / TARGET_SR)


def save_wav(audio_dir: Path, dictation_id: str, samples: np.ndarray) -> Path:
    now = datetime.now(timezone.utc)
    out_dir = audio_dir / f"{now:%Y}" / f"{now:%m}"
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / f"{dictation_id}.wav"
    sf.write(path, samples, TARGET_SR, subtype="PCM_16")
    return path
