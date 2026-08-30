# una desktop client — golden path: `make` builds, resets stale macOS
# permission grants, installs, and launches. See docs/macos-install.md and
# docs/linux.md for details.

BUNDLE_ID := sh.tenet.una
APP       := Una
APP_PATH  := target/release/bundle/macos/$(APP).app
CLI_BIN   := target/release/una
# /usr/local/bin is root-owned on a stock macOS; fall back to ~/.local/bin
# rather than failing the whole install on a permission error.
PREFIX    := $(shell if [ -w /usr/local/bin ]; then echo /usr/local/bin; else echo $$HOME/.local/bin; fi)
UI_DIR    := apps/desktop/ui
UNAME_S   := $(shell uname -s)

.PHONY: all build install update ui tauri-cli reset-permissions stop launch clean

all: build
ifeq ($(UNAME_S),Darwin)
	$(MAKE) reset-permissions
endif
	$(MAKE) install
	$(MAKE) launch

# `--bundles app` stops at Una.app: that is all `install` needs, and skipping
# the dmg keeps the golden path from re-running hdiutil on every rebuild.
# Linking is fat-LTO'd, so the last cargo step sits at 100% for a while — that
# is the linker working, not a hang.
build: ui
ifeq ($(UNAME_S),Darwin)
	$(MAKE) tauri-cli
	cargo tauri build --bundles app
else
	cargo build --release -p una-desktop
endif
	cargo build --release -p una-cli

ui:
	cd $(UI_DIR) && bun install && bun run build

tauri-cli:
	@command -v cargo-tauri >/dev/null 2>&1 || cargo install tauri-cli --locked

install:
	@mkdir -p $(PREFIX)
ifeq ($(UNAME_S),Darwin)
	@if [ -d "$(APP_PATH)" ]; then \
		rm -rf "/Applications/$(APP).app"; \
		cp -R "$(APP_PATH)" /Applications/; \
		echo "installed /Applications/$(APP).app"; \
	else \
		echo "no $(APP_PATH) — the bundle step did not run; nothing installed" >&2; \
		exit 1; \
	fi
else
	install -m 755 target/release/una-desktop $(PREFIX)/una-desktop
endif
	install -m 755 $(CLI_BIN) $(PREFIX)/una
	@echo "installed $(PREFIX)/una"

update: stop
ifeq ($(UNAME_S),Darwin)
	rm -rf "/Applications/$(APP).app"
endif
	$(MAKE) build
ifeq ($(UNAME_S),Darwin)
	$(MAKE) reset-permissions
endif
	$(MAKE) install
	$(MAKE) launch

# macOS ties TCC grants to the binary's signature; every ad-hoc rebuild
# invalidates them while System Settings keeps showing stale entries.
# Reset only una's own grants, never the global tables.
reset-permissions:
	-osascript -e 'quit app "System Settings"'
	-tccutil reset Accessibility $(BUNDLE_ID)
	-tccutil reset Microphone $(BUNDLE_ID)
	-rm -rf "/Applications/$(APP).app"

stop:
	-pkill -x una-desktop 2>/dev/null || true
	-pkill -x $(APP) 2>/dev/null || true

launch:
ifeq ($(UNAME_S),Darwin)
	@if [ -d "/Applications/$(APP).app" ]; then open "/Applications/$(APP).app"; \
	else ./target/release/una-desktop & fi
else
	@echo "run: una-desktop (or set up an autostart entry)"
endif

clean:
	cargo clean
	rm -rf $(UI_DIR)/dist
