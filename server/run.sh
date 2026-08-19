#!/usr/bin/env bash
# Launcher used by una.service on bare-metal deploys.
#
# torch >= 2.13 ships CUDA-13 libs but ctranslate2 needs the cu12 wheels
# (installed via the `cuda12` extra); put every nvidia wheel's lib dir on
# LD_LIBRARY_PATH so both find what they link against.
set -euo pipefail
cd "$(dirname "$0")"

nvlibs=""
for dir in .venv/lib/python*/site-packages/nvidia/*/lib; do
  [ -d "$dir" ] && nvlibs="${nvlibs:+$nvlibs:}$PWD/$dir"
done
export LD_LIBRARY_PATH="${nvlibs}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

exec "$HOME/.local/bin/uv" run --no-sync uvicorn una_server.main:app --host 0.0.0.0 --port 8100
