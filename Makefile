# una — golden paths

.PHONY: help dmg server-dev server-test dashboard-dev dashboard-build client-check client-test ui-build client-dev docker

help:
	@grep -E '^[a-z-]+:' Makefile | sed 's/:.*//' | column

# ---- the one command: build the macOS app + dmg (drag it into /Applications) ----
DMG_DIR := client/target/release/bundle/dmg

dmg:
	cd client/apps/desktop/ui && bun install
	cd client/apps/desktop && ./ui/node_modules/.bin/tauri build --bundles dmg
	@echo "\n✅ dmg ready:" && ls $(DMG_DIR)/*.dmg && open -R $(DMG_DIR)/*.dmg

# ---- server (run on the GPU box; server-dev works CPU-only for hacking with UNA__ASR__DEVICE=cpu) ----
server-dev:
	cd server && uv run uvicorn una_server.main:app --reload --host 0.0.0.0 --port 8100

server-test:
	cd server && uv run pytest -q

# ---- dashboard ----
dashboard-dev:
	cd server/dashboard && pnpm dev

dashboard-build:
	cd server/dashboard && pnpm install && pnpm build

# ---- client ----
client-check:
	cd client && cargo check --workspace --all-targets

client-test:
	cd client && cargo test -p una-core

ui-build:
	cd client/apps/desktop/ui && bun install && bun run build

# needs tauri-cli: cargo install tauri-cli --version '^2'
client-dev: ui-build
	cd client/apps/desktop/src-tauri && cargo tauri dev

# ---- deployment (GPU box) ----
docker:
	docker compose up -d --build
