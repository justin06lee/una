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

.PHONY: all build install update ui tauri-cli reset-permissions stop launch clean icons

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

# Rasterise the icon SVGs into every PNG/ICNS the bundle and tray load. The
# outputs are committed, so this only needs running after editing an SVG.
# Needs rsvg-convert (brew install librsvg) and, for the .icns, macOS iconutil.
ICON_DIR := apps/desktop/src-tauri/icons
icons:
	@command -v rsvg-convert >/dev/null || { echo "icons: needs rsvg-convert (brew install librsvg)" >&2; exit 1; }
	for s in 32 64 128 256; do rsvg-convert -w $$s -h $$s $(ICON_DIR)/icon.svg -o $(ICON_DIR)/$${s}x$${s}.png; done
	cp $(ICON_DIR)/256x256.png $(ICON_DIR)/128x128@2x.png
	rsvg-convert -w 512 -h 512 $(ICON_DIR)/icon.svg -o $(ICON_DIR)/icon.png
	rsvg-convert -w 44 -h 44 $(ICON_DIR)/tray.svg -o $(ICON_DIR)/tray.png
	rm -rf $(ICON_DIR)/una.iconset && mkdir $(ICON_DIR)/una.iconset
	for s in 16 32 128 256 512; do \
		rsvg-convert -w $$s -h $$s $(ICON_DIR)/icon.svg -o $(ICON_DIR)/una.iconset/icon_$${s}x$${s}.png; \
		rsvg-convert -w $$((s*2)) -h $$((s*2)) $(ICON_DIR)/icon.svg -o $(ICON_DIR)/una.iconset/icon_$${s}x$${s}@2x.png; \
	done
	iconutil -c icns $(ICON_DIR)/una.iconset -o $(ICON_DIR)/icon.icns
	rm -rf $(ICON_DIR)/una.iconset

clean:
	cargo clean
	rm -rf $(UI_DIR)/dist
