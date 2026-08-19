"""Typed errors -> uniform {"error": {"code", "message"}} JSON responses."""

from __future__ import annotations

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse


class UnaError(Exception):
    code = "INTERNAL"
    status = 500

    def __init__(self, message: str = "", *, code: str | None = None, status: int | None = None):
        super().__init__(message or self.code)
        self.message = message or self.code
        if code:
            self.code = code
        if status:
            self.status = status


class NotFound(UnaError):
    code = "NOT_FOUND"
    status = 404


class AudioInvalid(UnaError):
    code = "AUDIO_INVALID"
    status = 400


class AsrFailed(UnaError):
    code = "ASR_FAILED"
    status = 500


class RunActive(UnaError):
    code = "RUN_ACTIVE"
    status = 409


class TrainingInProgress(UnaError):
    code = "TRAINING_IN_PROGRESS"
    status = 503


class BadRequest(UnaError):
    code = "BAD_REQUEST"
    status = 400


def install_handlers(app: FastAPI) -> None:
    @app.exception_handler(UnaError)
    async def una_error_handler(_request: Request, exc: UnaError) -> JSONResponse:
        return JSONResponse(
            status_code=exc.status,
            content={"error": {"code": exc.code, "message": exc.message}},
        )
