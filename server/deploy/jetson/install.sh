#!/usr/bin/env bash
# una's server on an NVIDIA Jetson (Orin, JetPack 7), installed without root.
#
# PyPI's CTranslate2 wheels for aarch64 are CPU-only (faster-whisper would run Whisper
# on the CPU), and a Jetson ships without the CUDA toolkit or g++. So this builds
# CTranslate2 for the GPU with nvcc and g++ from conda-forge, next to everything else
# under ~/.local and ~/.cache, and runs the server and Ollama as systemd user services.
# Re-run it after `git pull`: what is already done is skipped.
#
#   server/deploy/jetson/install.sh
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
server=$(cd "$here/../.." && pwd)
share=$HOME/.local/share/una
cache=$HOME/.cache/una
uv=$HOME/.local/bin/uv
micromamba=$HOME/.local/bin/micromamba
build_env=$share/build-env
# 0.34.1 dropped ADAPTER from `ollama create` (ollama#14969), which is how a trained
# style model is made; models made before keep working on later versions.
ollama_version=${UNA_OLLAMA_VERSION:-0.34.0}
cuda_arch=${UNA_CUDA_ARCH:-8.7}  # Orin; nvcc 12.9 has no Thor (11.0)
export MAMBA_ROOT_PREFIX=$HOME/.local/share/mamba

step() { printf '\n==> %s\n' "$*"; }

reinstall_cudnn=()

# Build the CTranslate2 that uv.lock pins, so the rest of the lock still agrees with it.
ct2_version=$(python3 - "$server/uv.lock" <<'EOF'
import sys, tomllib
lock = tomllib.load(open(sys.argv[1], "rb"))
print(next(p["version"] for p in lock["package"] if p["name"] == "ctranslate2"))
EOF
)
ct2_prefix=$share/ctranslate2-$ct2_version
ct2_wheels=$cache/wheels

if [ ! -x "$uv" ]; then
  step "uv"
  curl -LsSf https://astral.sh/uv/install.sh | env UV_NO_MODIFY_PATH=1 sh
fi

if ! ls "$ct2_wheels"/ctranslate2-"$ct2_version"-*.whl >/dev/null 2>&1; then
  if [ ! -x "$build_env/bin/nvcc" ]; then
    step "build tools: nvcc 12.9, cuBLAS headers, g++ 13, cmake (conda-forge, ~3 GB)"
    if [ ! -x "$micromamba" ]; then
      curl -Ls https://micro.mamba.pm/api/micromamba/linux-aarch64/latest \
        | tar -xj -C "$HOME/.local" bin/micromamba
    fi
    "$micromamba" create -y -p "$build_env" -c conda-forge \
      cuda-version=12.9 cuda-nvcc cuda-cudart-dev cuda-cudart-static libcublas-dev \
      cuda-cccl libcurand-dev cmake ninja gxx_linux-aarch64=13 gcc_linux-aarch64=13
  fi

  step "CTranslate2 $ct2_version for CUDA sm_${cuda_arch/./} (about 40 minutes)"
  src=$cache/ctranslate2-$ct2_version
  if [ ! -d "$src" ]; then
    git clone --depth 1 --branch "v$ct2_version" --recurse-submodules --shallow-submodules \
      https://github.com/OpenNMT/CTranslate2.git "$src"
  fi
  # The toolkit root must be the env itself: targets/sbsa-linux/bin/nvcc is a symlink,
  # and nvcc run through it looks for cicc in the wrong place. CUDA_PATH is where
  # FindCUDA then finds the headers. A system python3.12 has no Python.h, hence
  # --managed-python for the wheel. No cuDNN: Whisper's two convolutions run on
  # CTranslate2's own kernel, as in its official wheels.
  CUDA_PATH=$build_env/targets/sbsa-linux "$micromamba" run -p "$build_env" bash -c "
    set -e
    cmake -S '$src' -B '$src/build' -G Ninja -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_INSTALL_PREFIX='$ct2_prefix' \
      -DBUILD_CLI=OFF -DWITH_MKL=OFF -DWITH_RUY=ON -DOPENMP_RUNTIME=COMP \
      -DWITH_CUDA=ON -DWITH_CUDNN=OFF -DCUDA_DYNAMIC_LOADING=ON \
      -DCUDA_ARCH_LIST='$cuda_arch' -DCUDA_TOOLKIT_ROOT_DIR='$build_env'
    # nvcc takes ~1.5 GB a file and memory is shared with the GPU: keep it to 3 at once
    ninja -C '$src/build' -j3
    ninja -C '$src/build' install
    cd '$src/python'
    cp ../README.md .
    CTRANSLATE2_ROOT='$ct2_prefix' LDFLAGS='-Wl,-rpath,$ct2_prefix/lib' \
      LDSHARED=\"\$CXX -shared\" '$uv' build --wheel --managed-python --python 3.12 \
      --out-dir '$ct2_wheels'
  "
fi

if [ ! -x "$HOME/.bun/bin/bun" ]; then
  step "bun (builds the dashboard)"
  curl -fsSL https://bun.sh/install | bash
fi

step "dashboard"
(cd "$server/dashboard" && "$HOME/.bun/bin/bun" install --frozen-lockfile && "$HOME/.bun/bin/bun" run build)

step "python environment"
# The cuda12 extra brings cuBLAS 12 for CTranslate2; torch brings its own CUDA 13 libs.
# Not its cuDNN: this CTranslate2 doesn't use one, and nvidia-cudnn-cu12 unpacks over
# torch's cuDNN 9 for CUDA 13 in nvidia/cudnn/lib, which breaks every convolution in
# training (CUDNN_STATUS_SUBLIBRARY_VERSION_MISMATCH).
venv_python=$server/.venv/bin/python
if [ -x "$venv_python" ] && "$uv" pip show --python "$venv_python" nvidia-cudnn-cu12 >/dev/null 2>&1; then
  "$uv" pip uninstall --python "$venv_python" nvidia-cudnn-cu12
  reinstall_cudnn=(--reinstall-package nvidia-cudnn-cu13)
fi
# uv sync removes the CTranslate2 built here (it isn't the locked wheel) and would
# install PyPI's CPU-only one: skip it, then put ours back. uv's own Python, not the
# system's: Triton (torch's) compiles a helper against Python.h, which Ubuntu only
# ships in python3.12-dev.
(cd "$server" && "$uv" sync --managed-python --python 3.12 --extra training --extra cuda12 \
  --no-dev --no-install-package ctranslate2 --no-install-package nvidia-cudnn-cu12 \
  "${reinstall_cudnn[@]}")
"$uv" pip install --python "$venv_python" --no-deps --reinstall \
  "$(ls "$ct2_wheels"/ctranslate2-"$ct2_version"-*.whl | head -1)"

ollama=$share/ollama/bin/ollama
if ! "$ollama" --version 2>/dev/null | grep -q "$ollama_version"; then
  step "Ollama $ollama_version"
  rm -rf "$share/ollama" && mkdir -p "$share/ollama"
  curl -fL "https://github.com/ollama/ollama/releases/download/v$ollama_version/ollama-linux-arm64.tar.zst" \
    | tar --zstd -x -C "$share/ollama"
fi

if [ ! -f "$server/una.toml" ]; then
  step "una.toml (Jetson defaults)"
  cp "$here/una.toml" "$server/una.toml"
fi

step "services"
units=$HOME/.config/systemd/user
mkdir -p "$units"
for unit in una ollama; do
  sed -e "s|@SERVER@|$server|g" -e "s|@SHARE@|$share|g" "$here/$unit.service" > "$units/$unit.service"
done
systemctl --user daemon-reload
systemctl --user enable ollama.service
systemctl --user restart ollama.service  # picks up a changed unit; it's back in seconds

cleanup_model=$(python3 - "$server/una.toml" <<'EOF'
import sys, tomllib
print(tomllib.load(open(sys.argv[1], "rb")).get("cleanup", {}).get("model", "qwen3:8b"))
EOF
)
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
