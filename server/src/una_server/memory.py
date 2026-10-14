"""Handing memory back after big loads. On a Jetson the CPU and GPU share it, so what one
load keeps after it's done is what the cleanup model can't have."""

from __future__ import annotations

import ctypes


def release_freed_memory() -> None:
    """Hand memory freed while loading a model back to the OS.

    CTranslate2 reads the weights through malloc on several threads and frees them once
    they are on the GPU, but glibc keeps what was freed in its per-thread arenas: ~760 MB
    for large-v3-turbo.
    """
    try:
        ctypes.CDLL("libc.so.6").malloc_trim(0)
    except (OSError, AttributeError):
        pass  # not glibc (macOS, musl): nothing to hand back this way
