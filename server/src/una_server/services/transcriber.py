"""faster-whisper wrapper. Inference runs in a thread; callers hold the model manager's lock."""

from __future__ import annotations

import asyncio
import logging
from dataclasses import dataclass

import numpy as np

from ..config import AsrConfig
from ..errors import AsrFailed


@dataclass
class TranscribeResult:
    text: str
    language: str | None


class Transcriber:
    """Owns one loaded faster-whisper model."""

    def __init__(self, cfg: AsrConfig, model_path_or_alias: str):
        from faster_whisper import WhisperModel  # deferred: heavy import

        self.cfg = cfg
        self.source = model_path_or_alias
        self.model = WhisperModel(
            model_path_or_alias, device=cfg.device, compute_type=cfg.compute_type
        )

    def transcribe_sync(
        self,
        samples: np.ndarray,
        *,
        language: str | None = None,
        initial_prompt: str | None = None,
        use_vad: bool = True,
    ) -> TranscribeResult:
        try:
            segments, info = self.model.transcribe(
                samples,
                language=language or self.cfg.language,
                beam_size=self.cfg.beam_size,
                temperature=0.0,
                condition_on_previous_text=False,
                vad_filter=use_vad,
                vad_parameters={"min_silence_duration_ms": self.cfg.vad_min_silence_ms},
                word_timestamps=False,
                initial_prompt=initial_prompt,
            )
            text = " ".join(seg.text.strip() for seg in segments).strip()
        except Exception as exc:
            raise AsrFailed(f"transcription failed: {exc}") from exc
        return TranscribeResult(text=text, language=getattr(info, "language", None))

    async def transcribe(self, samples: np.ndarray, **kwargs) -> TranscribeResult:
        return await asyncio.to_thread(self.transcribe_sync, samples, **kwargs)

    def warmup(self) -> None:
        silence = np.zeros(16000, dtype=np.float32)
        try:
            self.transcribe_sync(silence, use_vad=False)
        except AsrFailed as exc:
            # don't block startup, but a failing warmup means every request will fail
            # (e.g. missing CUDA libraries) — make it impossible to miss in the logs
            logging.getLogger(__name__).error("ASR warmup FAILED — serving will not work: %s", exc)
