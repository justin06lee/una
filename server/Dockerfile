# Stage 1: build the dashboard to static files
FROM oven/bun:1-slim AS dashboard
WORKDIR /build/server/dashboard
COPY dashboard/package.json dashboard/bun.lock ./
RUN bun install --frozen-lockfile
COPY dashboard/ ./
# vite outDir is ../src/una_server/web relative to dashboard/
RUN mkdir -p ../src/una_server/web && bun run build

# Stage 2: CUDA runtime + uv-managed Python (serving AND training live in one image;
# training runs as a subprocess of the server)
FROM nvidia/cuda:12.4.1-cudnn-runtime-ubuntu22.04
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /usr/local/bin/

RUN apt-get update && apt-get install -y --no-install-recommends \
    libsndfile1 libgomp1 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
ENV UV_PYTHON_INSTALL_DIR=/opt/uv-python
RUN uv python install 3.12

COPY pyproject.toml uv.lock* ./
RUN uv sync --extra training --no-dev --no-install-project

COPY migrations/ migrations/
COPY src/ src/
COPY --from=dashboard /build/server/src/una_server/web/ src/una_server/web/
RUN uv sync --extra training --no-dev

ENV UNA__SERVER__DATA_DIR=/data
ENV HF_HOME=/hf-cache
VOLUME ["/data", "/hf-cache"]

EXPOSE 8100
CMD ["uv", "run", "--no-sync", "uvicorn", "una_server.main:app", "--host", "0.0.0.0", "--port", "8100"]
