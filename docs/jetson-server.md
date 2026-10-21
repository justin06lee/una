# una's server on a Jetson

A Jetson Orin Nano (8 GB) can be the whole una server: Whisper on its GPU, the
cleanup model in Ollama, and both kinds of training. Tested on an Orin Nano Super
with JetPack 7 (L4T R39, Ubuntu 24.04, CUDA 13.2 driver).

midna, the Jetson this page was written on, served una from 2026-10-14 to 2026-10-21.
una now runs on tenet, an x86 PC with a 6 GB card (see
[`server/deploy/linux/install.sh`](../server/deploy/linux/install.sh)), and midna's data
was merged into tenet's. The page stays as the recipe for a Jetson.

```sh
git clone https://github.com/justin06lee/una.git ~/una
~/una/server/deploy/jetson/install.sh
```

No root needed: everything goes under `~/.local` and `~/.cache`, and the server
and Ollama run as systemd user services. The first run takes about an hour, most
of it compiling CTranslate2. After a `git pull`, run it again; it skips what is
already done and restarts the server.

## What it does, and why

- **CTranslate2 built for the GPU.** faster-whisper runs on CTranslate2, and PyPI's
  aarch64 wheels of it are CPU-only. A Jetson has no CUDA toolkit and no g++ either,
  so the script gets nvcc 12.9, the cuBLAS headers and g++ 13 from conda-forge
  (into `~/.local/share/una/build-env`) and builds the version `uv.lock` pins, for
  the Orin's sm_87, without cuDNN, the way CTranslate2's own CUDA wheels are built.
  At runtime it uses cuBLAS 12 from the `cuda12` extra.
- **Everything else from the lock.** PyPI's aarch64 torch is a CUDA 13 build and runs
  on the Orin (it warns that sm_87 isn't in its list; its sm_80 kernels run there).
  The sync leaves out ctranslate2, or it would put the CPU-only wheel back, and
  `nvidia-cudnn-cu12`, which unpacks over torch's own cuDNN and breaks training.
  Don't run a plain `uv sync` in `server/`; run the script.
- **Ollama 0.34.0** from its generic arm64 release (its `cuda_v13` backend is the one
  that takes the Orin), listening on `127.0.0.1:11434`. Not newer: 0.34.1 dropped
  `ADAPTER` from `ollama create`, which is how a trained style model is made.
- **una.toml** from [`server/deploy/jetson/una.toml`](../server/deploy/jetson/una.toml),
  once: Whisper large-v3-turbo in int8_float16, a 3B cleanup model kept wholly on the
  GPU (`cleanup.num_gpu = 99`; left to itself Ollama keeps part of it on the CPU, a
  second copy in the same memory), and `training.pause_serving`, so a training run has
  the memory to itself (dictation is off until it finishes).
- **Services** `una.service` and `ollama.service` in `~/.config/systemd/user`. They
  keep running after you log out only with lingering on
  (`sudo loginctl enable-linger $USER`, once); the script tells you if it's off.

Logs: `journalctl --user -u una -f`. The dashboard is `http://<jetson>:8100`.

## Memory and speed

On an Orin Nano Super (25 W mode) with the Ubuntu desktop running:

| | |
|---|---|
| free before una | ~4.1 GB of 7.3 GB |
| una server, Whisper loaded (int8_float16) | ~1.3 GB |
| llama3.2:3b in Ollama, 2048-token context | ~2.7 GB |
| left over | ~0.4 GB |
| a dictation, release → text | ~3.5–4 s (Whisper ~2.5–2.8 s, cleanup ~0.7–1.1 s) |
| Whisper LoRA run, batch 2 | lowest free ~1.0 GB, training through eval* |
| style run (QLoRA + DPO) | lowest free ~0.6 GB while training* |

\* Measured with the server stopped; with `pause_serving` it stays up without its
models. Both trainings run in a process of their own, so what torch held is back
before the eval loads Whisper or Ollama models.

That is tight, and a Jetson ships with no swap, so a big desktop app can get una
or Ollama killed (systemd restarts them). Give it a swapfile, once:

```sh
sudo fallocate -l 8G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile
sudo swapon /swapfile && echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

What doesn't fit, or doesn't help: Whisper in float16 is ~0.5 s faster but needs
0.7 GB more; qwen3:4b took 13 s a cleanup; llama3.2:1b is fast but answers or
rewrites what you said instead of cleaning it. MAXN SUPER power mode
(`sudo nvpmodel -m 2`) raises the GPU clock from 918 to 1020 MHz.

## Pointing clients at it

Add the Jetson in each client's Settings → Server: its LAN address, for example
`http://<jetson>.local:8100`, and a mesh or VPN name for everywhere else. Replace the
old server's addresses rather than adding to them. Clients race every address and keep
the fastest, so two live servers would split your dictations between them.

## Moving from another server

Your history, recordings and trained models are in the old server's `server/data/`.
Don't copy that folder over the new server's: it replaces the new server's database,
so every dictation made there is lost. Merge it instead. Ids are ULIDs, so rows from
two servers never collide.

```sh
# on the old server: a consistent copy, safe while una runs
cd ~/una/server && .venv/bin/python -m una_server.tools.merge_data snapshot data ~/una-snapshot
# bring ~/una-snapshot to the new server (rsync -a), then from its ~/una/server:
.venv/bin/python -m una_server.tools.merge_data merge ~/una-snapshot data --dry-run
.venv/bin/python -m una_server.tools.merge_data merge ~/una-snapshot data
```

`snapshot` takes una.db with SQLite's online backup, along with the recordings, the
registered fine-tuned models and the training-run folders. A rejected run's model isn't
registered, so it stays behind. `merge` adds the rows the new server lacks and never
deletes or replaces its own. It rewrites recording and model paths to the new server's
layout (Docker's `/data/...` or bare metal's `data/...`) and copies the files across.
It backs up the new una.db into `data/backups/` first. Where both servers have the
same setting, the new server's value is kept, and the report lists the conflict.
Running it again with a newer snapshot merges only what changed since, so take a last
snapshot just before you switch the clients over. If the new server has no rows yet
and runs in Docker, pass `--stored-root /data`.

**A style model** lives in the old server's Ollama, and the `cleanup.model` setting
names it. Until the new server's Ollama has it, cleanup falls back to the raw
transcript. Recreate it from its run with
`ollama create una-style-xxxx -f data/runs/<run>/Modelfile` (fix the `ADAPTER` path
in that file first), or set Server settings → cleanup model back to the base model
and train a new one.

Then restart una on the new server (`systemctl --user restart una` here).
