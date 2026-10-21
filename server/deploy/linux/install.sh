#!/usr/bin/env bash
# una's server on an x86_64 Linux PC with an NVIDIA card, installed without root.
#
# The bare-metal twin of docker-compose.yml, for a box without Docker or sudo: the
# Python environment from uv.lock, Ollama 0.34.0 under ~/.local/share/una, and the
# server and Ollama as systemd user services. PyPI's CTranslate2 wheels for x86_64 run
# on the GPU as they are, so unlike the Jetson nothing is compiled here.
# Re-run it after `git pull`: what is already done is skipped.
#
#   server/deploy/linux/install.sh
#   UNA_NO_START=1 server/deploy/linux/install.sh   # install, but leave the GPU alone
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
server=$(cd "$here/../.." && pwd)
share=$HOME/.local/share/una
uv=$HOME/.local/bin/uv
# 0.34.1 dropped ADAPTER from `ollama create` (ollama#14969), which is how a trained
# style model is made; models made before keep working on later versions.
ollama_version=${UNA_OLLAMA_VERSION:-0.34.0}
no_start=${UNA_NO_START:-}

step() { printf '\n==> %s\n' "$*"; }

if [ ! -x "$uv" ]; then
  step "uv"
  curl -LsSf https://astral.sh/uv/install.sh | env UV_NO_MODIFY_PATH=1 sh
fi

bun=$(command -v bun || echo "$HOME/.bun/bin/bun")  # a distro's package will do
if [ ! -x "$bun" ]; then
  step "bun (builds the dashboard)"
  curl -fsSL https://bun.sh/install | bash
fi

step "dashboard"
(cd "$server/dashboard" && "$bun" install --frozen-lockfile && "$bun" run build)

step "python environment"
# The cuda12 extra brings cuBLAS 12 for CTranslate2; torch brings its own CUDA 13 libs.
# Not the extra's cuDNN: CTranslate2's wheels are built without one, and
# nvidia-cudnn-cu12 unpacks over torch's cuDNN 9 for CUDA 13 in nvidia/cudnn/lib, which
# breaks every convolution in training (CUDNN_STATUS_SUBLIBRARY_VERSION_MISMATCH).
# uv's own Python 3.12: the system one may be newer than una supports, or lack Python.h,
# which Triton (torch's) compiles a helper against.
venv_python=$server/.venv/bin/python
reinstall_cudnn=()
if [ -x "$venv_python" ] && "$uv" pip show --python "$venv_python" nvidia-cudnn-cu12 >/dev/null 2>&1; then
  "$uv" pip uninstall --python "$venv_python" nvidia-cudnn-cu12
  reinstall_cudnn=(--reinstall-package nvidia-cudnn-cu13)
fi
(cd "$server" && "$uv" sync --managed-python --python 3.12 --extra training --extra cuda12 \
  --no-dev --no-install-package nvidia-cudnn-cu12 "${reinstall_cudnn[@]}")

ollama=$share/ollama/bin/ollama
if ! "$ollama" --version 2>/dev/null | grep -q "$ollama_version"; then
  step "Ollama $ollama_version"
  rm -rf "$share/ollama" && mkdir -p "$share/ollama"
  curl -fL "https://github.com/ollama/ollama/releases/download/v$ollama_version/ollama-linux-amd64.tar.zst" \
    | tar --zstd -x -C "$share/ollama"
fi

if [ ! -f "$server/una.toml" ]; then
  step "una.toml (6 GB card defaults)"
  cp "$here/una.toml" "$server/una.toml"
fi

step "services"
units=$HOME/.config/systemd/user
mkdir -p "$units"
for unit in una ollama; do
  sed -e "s|@SERVER@|$server|g" -e "s|@SHARE@|$share|g" "$here/$unit.service" > "$units/$unit.service"
done
systemctl --user daemon-reload

cleanup_model=$(python3 - "$server/una.toml" <<'EOF'
import sys, tomllib
print(tomllib.load(open(sys.argv[1], "rb")).get("cleanup", {}).get("model", "qwen3:8b"))
EOF
)

if [ -n "$no_start" ]; then
  echo
  echo "Installed; nothing started, so the GPU is untouched. When it is free:"
  echo "  systemctl --user enable --now ollama.service"
  echo "  $ollama pull $cleanup_model"
  echo "  systemctl --user enable --now una.service"
  exit 0
fi

systemctl --user enable ollama.service
systemctl --user restart ollama.service  # picks up a changed unit; it's back in seconds
step "cleanup model $cleanup_model"
until "$ollama" list >/dev/null 2>&1; do sleep 1; done
"$ollama" pull "$cleanup_model"

systemctl --user enable una.service
systemctl --user restart una.service

if [ "$(loginctl show-user "$USER" -p Linger --value 2>/dev/null)" != yes ]; then
  echo
  echo "The services stop when you log out. To keep them running, once:"
  echo "  sudo loginctl enable-linger $USER"
fi
echo
echo "una is starting on http://$(hostname).local:8100 (the first start downloads Whisper)."
echo "Logs: journalctl --user -u una -f"
