# una desktop on macOS

Building, installing, and granting permissions for the una desktop client on
macOS (Apple Silicon).

## Prerequisites

- Xcode Command Line Tools: `xcode-select --install`
- [Rust](https://rustup.rs) (stable)
- [bun](https://bun.sh) for the UI build
- Optionally `tauri-cli` for producing a `.app`/`.dmg` bundle:
  `cargo install tauri-cli --locked`

## Build from source

```sh
cd client

# 1. Build the UI (both pages: HUD + settings)
cd apps/desktop/ui
bun install
bun run build
cd ../../..

# 2a. Dev run (no bundle, fastest loop)
cargo run -p una-desktop

# 2b. Release bundle (.app + .dmg)
cargo tauri build
# bundles land in target/release/bundle/
```

There is also a `Makefile` in `client/` — a bare `make` builds the UI and the
app bundle, resets stale permission grants, installs `Una.app` into
`/Applications` plus the `una` CLI into `/usr/local/bin`, and launches the
app.

The `una` CLI helper builds with the workspace:

```sh
cargo build --release -p una-cli
# → target/release/una  (toggle / start / stop / cancel)
```

## Gatekeeper

The app is not notarized, so the first launch of a downloaded or copied
bundle is blocked with "Una is damaged or can't be opened":

- Right-click (Control-click) `Una.app` → **Open** → **Open**. This only
  needs to be done once.
- If that option doesn't appear, clear the quarantine attribute instead:

  ```sh
  xattr -cr /Applications/Una.app
  ```

Bundles you built yourself with `cargo tauri build` on the same machine are
not quarantined and open normally.

## Permissions walkthrough

una needs two TCC grants. It lives in the menu bar (no dock icon) — look for
the small star-badge icon.

### 1. Microphone

The first time you hold the dictation hotkey (default `Ctrl+Alt+Space`),
macOS shows the microphone permission dialog (the usage description explains
audio goes only to your own server). Click **Allow**.

If you dismissed it, or dictations record silence:
**System Settings → Privacy & Security → Microphone** → enable **Una**.

### 2. Accessibility

Pasting the transcript into the focused app synthesizes a Cmd+V keystroke,
which requires Accessibility. Open the una settings (tray → Settings… →
Insertion tab) and click **Grant…**, or add it manually:
**System Settings → Privacy & Security → Accessibility** → enable **Una**.

Without this grant una still works, but ends each dictation with the text on
the clipboard instead of pasting it (the same fallback used while a password
field has secure input active).

## Rebuilds and stale TCC grants

macOS ties permission grants to the app's code signature. Ad-hoc-signed dev
builds get a new identity on every rebuild, which **silently invalidates the
grant while System Settings keeps showing it as enabled** — and the new
binary often won't re-prompt until the stale entry is removed.

`make` / `make update` in `client/` handle this automatically. What they run,
if you need it by hand:

```sh
osascript -e 'quit app "System Settings"'   # it caches the TCC table
tccutil reset Accessibility sh.tenet.una
tccutil reset Microphone sh.tenet.una
rm -rf /Applications/Una.app                # remove the stale binary
# then install + launch the fresh build; it re-prompts cleanly
```

Never run bare `tccutil reset Accessibility` (without the bundle id) — that
resets the grants of every app on the system.

## First run checklist

1. Launch Una; the star badge appears in the menu bar.
2. Tray → **Settings… → Server**: enter your server URL, or click
   **Discover** to find `_una._tcp` servers on the LAN. The dot next to the
   URL turns green when `/v1/health` answers.
3. Settings → **Audio** → **Test**: speak; the meter should move.
4. Hold `Ctrl+Alt+Space`, speak, release. The HUD shows live level bars,
   then a shimmer while transcribing, then the text is pasted where your
   cursor is.
