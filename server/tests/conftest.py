import asyncio
import io

import numpy as np
import pytest
import soundfile as sf
from fastapi.testclient import TestClient

from una_server.config import Config, ServerConfig
from una_server.main import create_app
from una_server.services.transcriber import TranscribeResult


class FakeTranscriber:
    """Stands in for the faster-whisper wrapper; returns a canned transcript."""

    def __init__(self, text="um so this is a test dictation"):
        self.text = text
        self.model = object()
        self.last_initial_prompt = None

    async def transcribe(self, samples, *, language=None, initial_prompt=None, use_vad=True):
        self.last_initial_prompt = initial_prompt
        return TranscribeResult(text=self.text, language=language or "en")


class FakeProc:
    """Stands in for a runner subprocess; exits 0 immediately."""

    pid = 4242

    def __init__(self):
        self.returncode = None

    async def wait(self):
        self.returncode = 0
        return 0


@pytest.fixture
def fake_runner(monkeypatch):
    async def fake_exec(*args, **kwargs):
        return FakeProc()

    monkeypatch.setattr(asyncio, "create_subprocess_exec", fake_exec)


@pytest.fixture
def client(tmp_path):
    config = Config(server=ServerConfig(data_dir=tmp_path / "data"))
    config.cleanup.enabled = False
    config.discovery.mdns = False
    app = create_app(config, load_model=False)
    with TestClient(app) as test_client:
        state = app.state.una
        state.models.transcriber = FakeTranscriber()
        state.models.active_model_id = "base:large-v3-turbo"
        yield test_client


def wav_bytes(seconds=2.0, sr=16000):
    t = np.linspace(0, seconds, int(seconds * sr), endpoint=False)
    audio = (0.1 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)
    buf = io.BytesIO()
    sf.write(buf, audio, sr, format="WAV", subtype="PCM_16")
    return buf.getvalue()
