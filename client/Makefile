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
# Linux: where the launcher and its icons go, so Una shows up in the app grid.
DATA_HOME := $(or $(XDG_DATA_HOME),$(HOME)/.local/share)
DESKTOP_ID := $(BUNDLE_ID)

.PHONY: all build install update ui tauri-cli reset-permissions stop launch clean icons linux-desktop-entry

all: build
ifeq ($(UNAME_S),Darwin)
	$(MAKE) reset-permissions
else
	# A running Una would keep the old build: `git pull && make` restarts it.
	$(MAKE) stop
endif
	$(MAKE) install
	$(MAKE) launch

# `--bundles app` stops at Una.app: that is all `install` needs, and skipping
# the dmg keeps the golden path from re-running hdiutil on every rebuild.
# Linking is fat-LTO'd, so the last cargo step sits at 100% for a while — that
# is the linker working, not a hang.
# On Linux, cargo builds the binary alone. It needs tauri's custom-protocol
# feature, which `tauri build` adds on macOS: without it the windows look for
# the Vite dev server instead of the built pages. (The tauri CLI itself isn't
# used here: it sets up a file watcher and fails once a busy machine is out of
# inotify instances.)
build: ui
ifeq ($(UNAME_S),Darwin)
	$(MAKE) tauri-cli
	cargo tauri build --bundles app
else
	cargo build --release -p una-desktop --features tauri/custom-protocol
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
	$(MAKE) linux-desktop-entry
endif
	install -m 755 $(CLI_BIN) $(PREFIX)/una
	@echo "installed $(PREFIX)/una"

# The app-grid launcher and its icons, under ~/.local/share (no root needed).
ICON_SRC := apps/desktop/src-tauri/icons
linux-desktop-entry:
	@mkdir -p $(DATA_HOME)/applications
	sed 's|@BIN@|$(PREFIX)/una-desktop|' apps/desktop/linux/una.desktop \
		> $(DATA_HOME)/applications/$(DESKTOP_ID).desktop
	for s in 32 64 128 256; do \
		install -Dm644 $(ICON_SRC)/$${s}x$${s}.png $(DATA_HOME)/icons/hicolor/$${s}x$${s}/apps/$(DESKTOP_ID).png; \
	done
	install -Dm644 $(ICON_SRC)/icon.png $(DATA_HOME)/icons/hicolor/512x512/apps/$(DESKTOP_ID).png
	-update-desktop-database -q $(DATA_HOME)/applications 2>/dev/null
	-gtk-update-icon-cache -q -t $(DATA_HOME)/icons/hicolor 2>/dev/null
	@echo "installed $(DATA_HOME)/applications/$(DESKTOP_ID).desktop"

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
	@if [ -n "$$DISPLAY$$WAYLAND_DISPLAY" ]; then \
		setsid -f $(PREFIX)/una-desktop >/dev/null 2>&1 </dev/null; \
		echo "launched $(PREFIX)/una-desktop"; \
	else \
		echo "no display here: open Una from the app grid"; \
	fi
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
	# Linux panels draw the tray icon untinted, on a dark bar: a white copy.
	sed 's/#000000/#ffffff/' $(ICON_DIR)/tray.svg | rsvg-convert -w 44 -h 44 -o $(ICON_DIR)/tray-light.png
	# Opaque art comes out RGB; tauri's bundle icons must be RGBA.
	python3 scripts/png_rgba.py $(ICON_DIR)/32x32.png $(ICON_DIR)/64x64.png \
		$(ICON_DIR)/128x128.png $(ICON_DIR)/128x128@2x.png $(ICON_DIR)/256x256.png \
		$(ICON_DIR)/icon.png
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
