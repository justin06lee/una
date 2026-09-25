"""Shared application state attached to app.state.una."""

from __future__ import annotations

from dataclasses import dataclass, field

import aiosqlite

from .config import Config
from .services.cleaner import Cleaner
from .services.discovery import Discovery
from .services.jobs import JobManager
from .services.model_manager import ModelManager
from .services.teacher import Teacher


@dataclass
class AppState:
    config: Config
    db: aiosqlite.Connection
    models: ModelManager
    cleaner: Cleaner
    jobs: JobManager
    discovery: Discovery | None = None
    teacher: Teacher | None = None
    last_dictation_at: float = field(default=0.0)
