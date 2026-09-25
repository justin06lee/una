"""App factory. Lifespan: DB migrate -> seed registry -> load + warmup ASR -> mDNS."""

from __future__ import annotations

import asyncio
import logging
import time
from contextlib import asynccontextmanager
from pathlib import Path

from fastapi import FastAPI
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from . import db as dbmod
from .api import router as v1_router
from .api.settings_api import apply_stored_settings
from .config import Config, load_config
from .errors import install_handlers
from .services import autotrain, teacher
from .services.cleaner import Cleaner
from .services.discovery import Discovery
from .services.jobs import JobManager
from .services.model_manager import ModelManager
from .state import AppState

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
log = logging.getLogger(__name__)

WEB_DIR = Path(__file__).resolve().parent / "web"


def create_app(config: Config | None = None, *, load_model: bool = True) -> FastAPI:
    cfg = config or load_config()

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        cfg.server.data_dir.mkdir(parents=True, exist_ok=True)
        db = await dbmod.connect(cfg.db_path)
        await dbmod.migrate(db)
        await dbmod.ensure_base_model(db, cfg.asr.model)

        state = AppState(
            config=cfg,
            db=db,
            models=ModelManager(cfg.asr, db),
            cleaner=Cleaner(cfg.cleanup),
            jobs=JobManager(db),
        )

        async def reload_after_training():
            # pause_serving unloads the ASR model during a run; a promoted model is loaded
            # by the runner's activate call, so only reload when serving was left empty.
            if load_model and not state.models.loaded:
                await state.models.load_active()

        if cfg.teacher.enabled:
            state.teacher = teacher.Teacher(cfg.teacher, cfg.cleanup, db, cfg.asr.language)
        state.jobs.on_finished = reload_after_training
        await apply_stored_settings(state)
        # Boot counts as activity so a restart gets a full idle grace period
        # before auto-training may pause serving.
        state.last_dictation_at = time.monotonic()
        app.state.una = state

        if load_model:
            await state.models.load_active()
        warm_task = None
        if load_model and cfg.cleanup.enabled:
            warm_task = asyncio.create_task(
                state.cleaner.keep_warm(paused=lambda: state.jobs.active)
            )
        auto_task = None
        if load_model:
            auto_task = asyncio.create_task(autotrain.loop(state))
        teacher_task = None
        if load_model and state.teacher is not None:
            teacher_task = asyncio.create_task(teacher.loop(state))
        if cfg.discovery.mdns:
            state.discovery = Discovery(cfg.server.port)
            # zeroconf's sync API must not run on the event loop thread (EventLoopBlocked)
            await asyncio.to_thread(state.discovery.start)

        log.info("una server ready on %s:%d (LAN-only: do not expose to the internet)",
                 cfg.server.bind, cfg.server.port)
        yield

        if auto_task is not None:
            auto_task.cancel()
        if teacher_task is not None:
            teacher_task.cancel()
        if state.teacher is not None:
            await state.teacher.aclose()
        if warm_task is not None:
            warm_task.cancel()
        if state.discovery:
            await asyncio.to_thread(state.discovery.stop)
        await state.cleaner.aclose()
        await db.close()

    app = FastAPI(title="una", version="0.1.0", lifespan=lifespan)
    install_handlers(app)
    app.include_router(v1_router)

    if WEB_DIR.exists() and (WEB_DIR / "index.html").exists():
        app.mount("/assets", StaticFiles(directory=WEB_DIR / "assets"), name="assets")

        @app.get("/{path:path}", include_in_schema=False)
        async def spa(path: str):
            candidate = WEB_DIR / path
            if path and candidate.is_file() and candidate.resolve().is_relative_to(WEB_DIR):
                return FileResponse(candidate)
            return FileResponse(WEB_DIR / "index.html")

    return app


app = create_app()
