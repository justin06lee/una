# Una — Desktop windows

One layer of Una's stack, owning `client/apps/desktop/ui/`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer desktop-windows` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer desktop-windows add|set|drop <section> …`. Where it and the code disagree, the code is right.

Tauri WebviewWindows for history (main home window), settings (small separate window), review, transient HUD pill and correction popup, built as Svelte 5 pages with Svelte runes state. Multi-page Vite build emits app/history, settings, review, hud, and correction entries mounted by per-window main.ts files; History.svelte is a sidebar shell with persistent dashboard iframe for server pages plus local Review, while Settings.svelte lives in its own window opened from the history sidebar Settings button. Strict monochrome design system in src/lib/tokens.css plus shared src/lib/ui.css controls (Geist type, hairline borders, no accent color, filled-vs-ring status dots); all windows talk to Rust only via Tauri invoke/listen and postMessage handshake with the embedded dashboard.

## Where to change what

- **History main window shell and sidebar:** client/apps/desktop/ui/src/history/History.svelte, client/apps/desktop/ui/src/history/main.ts, client/apps/desktop/ui/src/history/history.css
- **Settings window:** client/apps/desktop/ui/src/settings/Settings.svelte, client/apps/desktop/ui/src/settings/main.ts, client/apps/desktop/ui/src/settings/tabs/Hotkey.svelte
- **Recording HUD pill:** client/apps/desktop/ui/src/hud/Hud.svelte, client/apps/desktop/ui/src/hud/main.ts, client/apps/desktop/ui/src/lib/types.ts
- **Correction editor:** client/apps/desktop/ui/src/correction/Correction.svelte, client/apps/desktop/ui/src/correction/main.ts
- **Review window:** client/apps/desktop/ui/src/review/Review.svelte, client/apps/desktop/ui/src/review/main.ts, client/apps/desktop/ui/src/review/review.css
- **Shared design tokens and theming:** client/apps/desktop/ui/src/lib/tokens.css, client/apps/desktop/ui/src/lib/ui.css, client/apps/desktop/ui/src/lib/Icon.svelte
- **Window build entries:** client/apps/desktop/ui/vite.config.ts, client/apps/desktop/ui/history.html, client/apps/desktop/ui/settings.html, client/apps/desktop/ui/review.html
- **Hotkey keycaps and keyboard row (Mac vs PC keycaps, macOS vs X11 keycodes):** client/apps/desktop/ui/src/lib/keycaps.ts, client/apps/desktop/ui/src/lib/KeyboardRow.svelte, client/apps/desktop/ui/src/settings/tabs/Hotkey.svelte

## How it works

- **HUD live status render:** Snapshot and LevelFrame arrive via Tauri events → Hud.svelte maps snapshot.state to phase (recording/busy/done/error/idle) → rAF loop interpolates 30Hz levels into 9 centre-anchored canvas bars in a 76x26 pill (busy matches it so release doesn't grow it) → Pill shows shimmer, check/warn, retry affordance in monochrome
- **History window navigation and settings open:** History.svelte sidebar steers pages without reloading → Server pages (Home, Review, Dictionary, Training, Insights, Server settings) render in one persistent Dashboard.svelte iframe → Dashboard.svelte listens for go-to-page messages and refetches; offline server shows retry state → Settings button (and Cmd-,) at bottom of history sidebar opens the separate small settings window
- **Correction submit or dismiss:** Correction.svelte invokes correction_pending on open → Textarea prefilled, focused, caret at end → Submit rewrites text in source app and files pair for training; Escape records nothing

## Key files

- client/apps/desktop/ui/src/history/History.svelte — main home window shell: history sidebar with Settings button, local pages, embedded server pages
- client/apps/desktop/ui/src/history/main.ts — mounts history main window page
- client/apps/desktop/ui/src/history/history.css — history window shell styles
- client/apps/desktop/ui/src/app/App.svelte — Una window shell: sidebar, local pages, embedded server pages
- client/apps/desktop/ui/src/app/Dashboard.svelte — persistent dashboard iframe with offline state and page handshake
- client/apps/desktop/ui/src/settings/Settings.svelte — tabbed config UI in its own small window with server discovery, hotkey capture, and audio test
- client/apps/desktop/ui/src/settings/main.ts — mounts standalone settings window
- client/apps/desktop/ui/src/hud/Hud.svelte — floating recording pill with level canvas, timer, and error/retry states
- client/apps/desktop/ui/src/correction/Correction.svelte — editable pasted-text window with submit/Escape semantics
- client/apps/desktop/ui/src/review/Review.svelte — review page/window

## Rules and traps

- Keep windows independent: one Svelte root per html entry defined in vite.config.ts; do not merge HUD pill or correction popup into the Una window.
- Do not redefine backend shapes in UI: types.ts mirrors una-core serde output, so update it when Rust changes.
- HUD must stay transparent and theme-independent; only typeface and accent are shared.
- Correction window must be immediately typeable on open (focus textarea, caret at end) and Escape must record nothing.
- No color anywhere and no theme picker: status is filled dot (fine) vs ring (needs attention), windows follow the OS colour scheme; use tokens.css variables, never colored text, buttons, dots or per-window colors.
- Native hotkey bindings carry the platform's own keycodes (macOS virtual keycodes, X11 keycodes on Linux): keycaps.ts picks its keycode tables, bottom row and modifier list by IS_MAC; never compare a native keycode against the other platform's table.

## What it talks to

- Desktop shell — history main window, small settings window, review window, HUD pill, and correction popup are created as WebviewWindows and driven by its Tauri commands and events
- Client core — Snapshot, LevelFrame, Config shapes originate in una-core serde output
- Dashboard server pages — embedded via iframe in Dashboard.svelte with postMessage navigation, embed mode (no sidebar, follows system theme), and offline retry state
