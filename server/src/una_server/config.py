"""Configuration: una.toml + UNA__SECTION__KEY env overrides (env wins)."""

from __future__ import annotations

import os
import tomllib
from functools import lru_cache
from pathlib import Path

from pydantic import BaseModel
from pydantic_settings import BaseSettings, SettingsConfigDict


class ServerConfig(BaseModel):
    # LAN-only, unauthenticated service. Never expose this bind to the internet.
    bind: str = "0.0.0.0"
    port: int = 8100
    data_dir: Path = Path("./data")


class AsrConfig(BaseModel):
    model: str = "large-v3-turbo"
    device: str = "cuda"
    compute_type: str = "float16"
    language: str | None = "en"
    beam_size: int = 1
    vad_min_silence_ms: int = 300
    initial_prompt_max_chars: int = 180


# How the user writes to coding agents, where most dictation goes. Style pairs from
# their own writing to agents are trained under this tone, so it has to be the one
# those apps get at inference too.
AGENT_TONE = (
    "casual, the way they type to coding agents: lowercase is fine, keep their shorthand "
    "and slang; file paths, commands and code names exact"
)


class CleanupConfig(BaseModel):
    enabled: bool = True
    ollama_url: str = "http://localhost:11434"
    model: str = "qwen3:8b"
    timeout_s: float = 3.0
    keep_alive: str = "30m"
    # app-name substring (lowercased) -> tone hint; first match wins
    tone_map: dict[str, str] = {
        "slack": "casual chat; lowercase is fine",
        "discord": "casual chat; lowercase is fine",
        "messages": "casual chat; lowercase is fine",
        "mail": "professional prose",
        "pages": "professional prose",
        "docs": "professional prose",
        "claude code": AGENT_TONE,
        "codex": AGENT_TONE,
        "ruri": AGENT_TONE,
        "alacritty": AGENT_TONE,
        "ghostty": AGENT_TONE,
        "kitty": AGENT_TONE,
        "wezterm": AGENT_TONE,
        "terminal": "verbatim; do not alter punctuation inside commands",
        "iterm": "verbatim; do not alter punctuation inside commands",
    }
    default_tone: str = "neutral"
    # Cleanup output further than this (normalized edit distance) from the transcript
    # is a reply, not a cleanup — the model answered the dictation instead of cleaning
    # it — and is thrown away in favour of the raw transcript.
    max_divergence: float = 0.65


class TrainingConfig(BaseModel):
    threshold_minutes: float = 30.0
    auto: bool = False
    auto_idle_minutes: float = 30.0
    pause_serving: bool = False
    max_edit_distance: float = 0.30
    promotion_margin_wer: float = 0.5
    min_eval_samples: int = 40
    base_hf_model: str = "openai/whisper-large-v3-turbo"
    lora_r: int = 32
    lora_alpha: int = 64
    lora_dropout: float = 0.05
    learning_rate: float = 1e-3
    epochs: float = 3.0
    batch_size: int = 8
    grad_accum: int = 2
    keep_finetuned_models: int = 3
    # Style model (cleanup LLM) fine-tuning on (raw -> polished) pairs.
    style_threshold_pairs: int = 50
    style_base_hf_model: str = "unsloth/Llama-3.2-3B-Instruct"  # ungated Llama mirror
    style_ollama_base: str = "llama3.2:3b"  # FROM line of the generated Modelfile
    style_lora_r: int = 16
    style_lora_alpha: int = 32
    style_lora_dropout: float = 0.05
    style_learning_rate: float = 2e-4
    style_epochs: float = 2.0
    style_batch_size: int = 1
    style_grad_accum: int = 8
    style_max_seq_len: int = 768  # the system prompt alone is ~220 tokens
    style_promotion_margin: float = 0.02  # mean normalized edit distance must improve by this
    style_min_eval_samples: int = 20
    # Your own writing, back-translated into (spoken -> written) pairs; ready to train
    # once this many are finished. See docs/personal-model.md.
    style_use_writing: bool = True
    style_writing_threshold: int = 200
    # The teacher's unconfirmed polished guesses, as extra lower-trust training pairs.
    style_use_silver: bool = True
    # Confirmed pairs are the real thing: repeat them this many times in training.
    style_gold_repeat: int = 3
    # After supervised training, DPO on (preferred, dispreferred) outputs: your edits
    # against what was pasted, and your writing against what the current model makes
    # of its spoken version (up to style_dpo_synthetic of those).
    style_dpo: bool = True
    style_dpo_min_pairs: int = 20
    style_dpo_synthetic: int = 150
    style_dpo_beta: float = 0.1
    style_dpo_learning_rate: float = 2e-5
    style_dpo_epochs: float = 1.0


class TeacherConfig(BaseModel):
    """An optional second opinion on every dictation, used to pre-fill review.

    Off by default; with it off una behaves exactly as before. Nothing here is on
    the dictation path — it runs in the background after the text is pasted.
    """

    enabled: bool = False
    # A second, slower ASR pass over the stored audio. It isn't waited on, so it can
    # afford the full large-v3 decoder and beam search. CPU by default so it never
    # competes with the serving model for VRAM on a small card.
    second_asr: bool = True
    second_asr_model: str = "large-v3"
    second_asr_device: str = "cpu"
    second_asr_compute_type: str = "int8"
    second_asr_cpu_threads: int = 4
    second_asr_beam_size: int = 5
    # An Anthropic Messages API endpoint that reconciles the two transcripts into a
    # literal and a polished guess. Empty = skip this part. Point it at yagami to use
    # a signed-in Claude Code instead of an API key.
    llm_base_url: str = ""
    # Falls back to ANTHROPIC_API_KEY, then to the first key in yagami's config.
    llm_api_key: str = ""
    llm_model: str = "claude-haiku-4-5"
    llm_timeout_s: float = 180.0
    # Also label dictations recorded before the teacher was turned on.
    backfill: bool = True
    # Seconds between checks for new work when there is none.
    interval_s: float = 20.0


class DiscoveryConfig(BaseModel):
    mdns: bool = True


class Config(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="UNA__", env_nested_delimiter="__")

    server: ServerConfig = ServerConfig()
    asr: AsrConfig = AsrConfig()
    cleanup: CleanupConfig = CleanupConfig()
    training: TrainingConfig = TrainingConfig()
    teacher: TeacherConfig = TeacherConfig()
    discovery: DiscoveryConfig = DiscoveryConfig()

    @classmethod
    def settings_customise_sources(cls, settings_cls, init_settings, env_settings, dotenv_settings, file_secret_settings):
        # env must override una.toml values (which arrive as init kwargs)
        return (env_settings, init_settings, dotenv_settings, file_secret_settings)

    @property
    def audio_dir(self) -> Path:
        return self.server.data_dir / "audio"

    @property
    def models_dir(self) -> Path:
        return self.server.data_dir / "models"

    @property
    def runs_dir(self) -> Path:
        return self.server.data_dir / "runs"

    @property
    def db_path(self) -> Path:
        return self.server.data_dir / "una.db"


def load_config(path: str | os.PathLike | None = None) -> Config:
    """Load una.toml (path from arg or UNA_CONFIG, default ./una.toml) with env overrides."""
    config_path = Path(path or os.environ.get("UNA_CONFIG", "una.toml"))
    file_values: dict = {}
    if config_path.exists():
        with open(config_path, "rb") as f:
            file_values = tomllib.load(f)
    # BaseSettings gives env vars precedence over init kwargs by default.
    return Config(**file_values)


@lru_cache(maxsize=1)
def get_config() -> Config:
    return load_config()
