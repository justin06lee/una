# una — golden paths

.PHONY: all build install update dmg server-dev server-test dashboard-dev dashboard-build client-check client-test ui-build client-dev docker import-writing

# ---- the golden path: build the client, reset stale TCC grants, install, launch ----
all:
	$(MAKE) -C client

build:
	$(MAKE) -C client build

install:
	$(MAKE) -C client install

# full refresh of an installed client: stop -> remove -> build -> install -> relaunch
update:
	$(MAKE) -C client update

# ---- macOS dmg bundle (drag it into /Applications yourself) ----
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

# ---- your writing -> cleanup-model training pairs (docs/personal-model.md) ----
# Reads your Claude Code / Codex prompts on this machine, back-translates them through a
# local yagami (started and stopped here if it isn't running), and uploads them.
# REFRESH=1 back-translates everything again and replaces what the server has.
UNA_SERVER ?= http://tenet.local:8100
import-writing:
	@started=""; if ! yagami status >/dev/null 2>&1; then yagami start --daemon >/dev/null && started=1; fi; \
	cd server && UNA_SERVER=$(UNA_SERVER) uv run python -m una_server.tools.writing --llm http://127.0.0.1:8787 $(if $(REFRESH),--refresh,); \
	status=$$?; if [ -n "$$started" ]; then yagami stop >/dev/null; fi; exit $$status

# ---- dashboard ----
dashboard-dev:
	cd server/dashboard && bun run dev

dashboard-build:
	cd server/dashboard && bun install && bun run build

# ---- client helpers ----
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
