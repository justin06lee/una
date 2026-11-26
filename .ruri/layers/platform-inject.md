# Una — Platform inject

One layer of Una's stack, owning `client/crates/una-platform/`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer platform-inject` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer platform-inject add|set|drop <section> …`. Where it and the code disagree, the code is right.

OS text injection, focused-app access and single-key hotkeys behind the traits in lib.rs so core and the shell stay platform-agnostic. macOS injects via NSPasteboard save/restore plus a CGEvent Cmd+V chord, reads/edits fields via AX AXValue, and captures hotkeys on an active CGEventTap. Linux selects X11 (XTest, with one long-lived arboard clipboard) vs Wayland (ydotool, else wtype, else clipboard-only) by env vars, and on X11 hears single-key hotkeys through XInput2 raw key events (keytap.rs); una_platform::hotkeys names whichever tap the platform has.

## Where to change what

- **Platform traits and inject contract:** client/crates/una-platform/src/lib.rs
- **macOS paste injection:** client/crates/una-platform/src/macos/mod.rs, client/crates/una-platform/src/lib.rs
- **macOS hotkey tap and capture:** client/crates/una-platform/src/macos/eventtap.rs, client/crates/una-platform/src/macos/keys.rs
- **macOS focused-field read/write:** client/crates/una-platform/src/macos/axtext.rs
- **macOS keycodes and combo names:** client/crates/una-platform/src/macos/keys.rs, client/crates/una-platform/src/macos/eventtap.rs
- **macOS permissions and frontmost app:** client/crates/una-platform/src/macos/mod.rs
- **Linux session detection:** client/crates/una-platform/src/linux/detect.rs, client/crates/una-platform/src/linux/mod.rs
- **Linux X11 paste:** client/crates/una-platform/src/linux/x11.rs, client/crates/una-platform/src/linux/mod.rs
- **Linux Wayland paste:** client/crates/una-platform/src/linux/wayland.rs, client/crates/una-platform/src/linux/mod.rs
- **Linux single-key hotkeys and key recorder (XInput2 raw keys):** client/crates/una-platform/src/linux/keytap.rs, client/crates/una-platform/src/lib.rs

## How it works

- **macOS inject text:** Caller invokes TextInjector with InjectOptions via lib.rs trait → mod.rs saves pasteboard (skipped past 10MB) and sets transient marker → mod.rs places dictation text on NSPasteboard → mod.rs synthesizes Cmd+V CGEvent chord (layout-resolved V keycode, fallback 9) → axtext.rs optionally snapshots focused AXValue for later edit detection → mod.rs restores saved pasteboard, returns Injected or ClipboardOnly
- **macOS hotkey match:** EventTap runs CFRunLoop with active HID tap on keyDown/keyUp/flagsChanged → set_binding registers target keycode from keys.rs tables → Matching keyDown/keyUp is consumed (Drop) and reported via on_hotkey press/release → Bare modifiers match on flagsChanged keycode and pass through unconsumed → capture_next resolves next press (or bare-modifier release) to CapturedKey
- **Linux inject text:** detect.rs picks Wayland vs X11 vs Unknown from WAYLAND_DISPLAY/DISPLAY → mod.rs sets arboard clipboard text and parses paste chord (Ctrl+V vs Ctrl+Shift+V) → X11: x11.rs resolves keysyms via server keymap and sends XTest chord → Wayland: wayland.rs uses ydotool socket if present, else wtype if in PATH → No backend available returns ClipboardOnly/Unavailable

## Key files

- client/crates/una-platform/src/lib.rs — platform-agnostic traits: TextInjector, InjectOptions/Outcome/Probe, Permissions, FrontmostApp
- client/crates/una-platform/src/macos/mod.rs — macOS injector: pasteboard save/restore, Cmd+V chord, frontmost app, TCC checks
- client/crates/una-platform/src/macos/eventtap.rs — active CGEventTap hotkey matching and one-shot key capture
- client/crates/una-platform/src/macos/axtext.rs — AX focused-field value read/write with messaging timeout
- client/crates/una-platform/src/macos/keys.rs — modifier tables, key names via UCKeyTranslate, combo tokens
- client/crates/una-platform/src/linux/mod.rs — Linux injector dispatch, arboard clipboard, PasteChord parsing
- client/crates/una-platform/src/linux/detect.rs — Wayland/X11/Unknown detection from env vars
- client/crates/una-platform/src/linux/wayland.rs — ydotool socket discovery and wtype fallback
- client/crates/una-platform/src/linux/x11.rs — XTest paste with keysym-to-keycode scan
- client/crates/una-platform/src/linux/keytap.rs — X11 single-key hotkeys: XInput2 raw keys on the root (bare modifiers pass through, other keys passively grabbed), shortcut-abort window, one-shot capture for the settings recorder; lib.rs re-exports it with the Mac event tap as una_platform::hotkeys

## Rules and traps

- Never run AX calls on the UI thread or inside the event-tap callback; they are blocking IPC with a per-element messaging timeout.
- Every axtext entry returns Option: None (terminals like Ghostty/Alacritty/kitty, canvas editors, password fields) must fall back to the correction window, never unwrap.
- flagsChanged events cannot be consumed; bare-modifier hotkeys must pass through.
- Active tap requires Accessibility permission already held; do not switch to listen-only semantics needing Input Monitoring.
- Only una-platform compiles for Linux from the macOS dev machine ('cargo check --target aarch64-unknown-linux-gnu -p una-platform': pure Rust); build the rest on Linux (the Jetson) after changing Linux code.
- Do not save/restore pasteboards larger than 10MB; always set the TransientType marker so clipboard managers skip dictation text.

## What it talks to

- Client core — implements its TextInjector/Permissions traits; core never calls AppKit/CG/X11 directly
- Desktop shell — consumes hotkey press/release and capture results, inject outcomes, frontmost-app and permission states
- Focused applications — paste target via clipboard plus synthesized chord; AX value read/write where the control publishes AXValue
