# Component library

Every piece of this project's interface ruri has on file: the user's own
name for it, its handle, and its files. When the user names something
here, this is what they mean — go straight to its files rather than
searching for their words. Before building interface, look here first
and reuse what exists: `ruri search <words>` finds one, `ruri show <slug>`
reads it with its code and its pictures, `ruri add <slug>` copies it into
place, `ruri help` says the rest.

ruri maintains this file. Don't edit it by hand; it is rewritten whenever
the library changes.

## dashboard-sidebar — the sidebar
Dashboard shell: logo, nav with live Review backlog count, Settings, and the server status line with a theme-cycle button at the bottom.
Reaches into: server/dashboard/src/app.css, server/dashboard/src/lib/theme.svelte.ts, server/dashboard/src/router.svelte.ts, server/dashboard/src/App.svelte, server/dashboard/src/lib/Logo.svelte, server/dashboard/src/lib/Icon.svelte
Tags: nav, dashboard, shell

## home-feed — the home feed
Dashboard Home: big words-dictated number, hairline stat row, search + filter, and dictations grouped by day with expandable rows: audio, what una heard vs pasted, and under each your fix (what you actually said / how you'd have written it), chips for which model each trains (voice/style), delete.
Reaches into: server/dashboard/src/app.css, server/dashboard/src/api.ts, server/dashboard/src/pages/Home.svelte, server/dashboard/src/lib/Metric.svelte, server/dashboard/src/lib/SearchInput.svelte, server/dashboard/src/lib/AudioPlayer.svelte
Tags: page, dashboard, feed, stats

## review-card — the review card
Keyboard-driven review queue: audio player, the teacher's disputed words as amber click-to-play chips, the transcript pre-filled with its literal guess, what una pasted, and 'How you'd have written it' for style training; Sounds right / Skip / Exclude.
Reaches into: server/dashboard/src/app.css, server/dashboard/src/api.ts, server/dashboard/src/pages/Review.svelte, server/dashboard/src/lib/AudioPlayer.svelte, server/dashboard/src/lib/autosize.ts
Tags: page, dashboard, review, keyboard

## insights-charts — the insights charts
Insights page: headline metrics, grey activity heatmap that fills the panel width (steps days by calendar, not 24h, for DST), words-per-day bars, per-app bars, and a WER chart with a data-fitted axis.
Reaches into: server/dashboard/src/app.css, server/dashboard/src/lib/format.ts, server/dashboard/src/pages/Insights.svelte, server/dashboard/src/lib/Heatmap.svelte, server/dashboard/src/lib/DayBars.svelte, server/dashboard/src/lib/AppBars.svelte, server/dashboard/src/lib/WerChart.svelte
Tags: page, dashboard, charts, heatmap

## training-page — the training page
Voice and style readiness side by side, the live run with its log, and the runs and models tables (switch models from here).
Reaches into: server/dashboard/src/app.css, server/dashboard/src/api.ts, server/dashboard/src/pages/Training.svelte, server/dashboard/src/lib/StatusChip.svelte, server/dashboard/src/lib/ProgressBar.svelte
Tags: page, dashboard, training, table

## dictionary-page — the dictionary page
One-row add form (word, heard as, note), search and sort, and a word list with hit counts, on/off switches and hover edit/delete.
Reaches into: server/dashboard/src/app.css, server/dashboard/src/api.ts, server/dashboard/src/pages/Dictionary.svelte, server/dashboard/src/lib/Toggle.svelte
Tags: page, dashboard, form, list

## dashboard-settings — dashboard settings
Server-side settings in grouped rows (theme, cleanup, training) with a floating save bar that only appears when something changed.
Reaches into: server/dashboard/src/app.css, server/dashboard/src/api.ts, server/dashboard/src/pages/Settings.svelte, server/dashboard/src/lib/ThemeToggle.svelte
Tags: page, dashboard, settings, form

## desktop-settings-window — the settings window
The app's settings, in their own smaller window (780x560): sidebar of eight tabs (General, Hotkey, Server, Audio, Pasting, Learning, Fix up, About) under macOS overlay traffic lights; autosaves config via set_config. Opened from the history window's Settings item or ⌘, (a una://settings link history.rs stops in on_navigation), or tray → Settings…; hides on close.
Its files: client/apps/desktop/ui/src/settings/Settings.svelte, client/apps/desktop/ui/src/settings/settings.css
Reaches into: client/apps/desktop/ui/src/lib/ui.css, client/apps/desktop/ui/src/lib/tokens.css, client/apps/desktop/src-tauri/src/windows.rs, client/apps/desktop/ui/src/settings/tabs/General.svelte, client/apps/desktop/ui/src/settings/tabs/Hotkey.svelte, client/apps/desktop/ui/src/settings/tabs/Server.svelte, client/apps/desktop/ui/src/settings/tabs/Audio.svelte, client/apps/desktop/ui/src/settings/tabs/Pasting.svelte, client/apps/desktop/ui/src/settings/tabs/Learning.svelte, client/apps/desktop/ui/src/settings/tabs/About.svelte
Tags: window, desktop, settings, tauri

## hud-pill — the HUD pill
The floating pill at the bottom of the screen: idle shows the bow mark, recording shows a breathing white dot + live waveform + timer, then shimmer, a white check, or an error with a white warning and Retry — no colour, like the rest of the app. All states are CSS in a fixed 400x72 window.
Its files: client/apps/desktop/ui/src/hud/Hud.svelte, client/apps/desktop/ui/src/hud/hud.css
Reaches into: client/apps/desktop/src-tauri/src/windows.rs
Tags: hud, desktop, animation, overlay

## fix-dictation-window — the fix-dictation window
Focus-taking editor that pops up when you start fixing pasted text in apps AX can't read (terminals, canvas), or from the tray's Fix Last Dictation. Plays the recording (⌘P, never autoplays) and has 'What you said' (raw transcript, trains Whisper) and 'How you'd have written it' (pasted text, trains cleanup; goes back into the app), with the Fix up control (⌘J) between them. Submit files both with transcript_shown; Esc walks away and returns focus to the app.
Its files: client/apps/desktop/ui/src/correction/Correction.svelte, client/apps/desktop/ui/src/correction/correction.css
Reaches into: client/apps/desktop/ui/src/lib/ui.css, client/apps/desktop/src-tauri/src/windows.rs
Tags: window, desktop, dialog, correction

## app-icon — the app icon
Una Mauri's black hair bow as five bold waveform bars (knot + two a side, wings tipping up), flat pure black on an opaque full-bleed white square: macOS 26 masks it to its own squircle and draws it flat (a transparent-margin Big Sur-style icon gets its glassy legacy treatment). The same drawing is the tray icon, logos, favicon and idle HUD glyph. SVGs are the source; 'make icons' in client/ regenerates every PNG and the .icns.
Its files: client/apps/desktop/src-tauri/icons/icon.svg
Reaches into: client/Makefile, client/apps/desktop/src-tauri/icons/tray.svg
Tags: icon, brand, desktop

## review-window — the review window
Desktop app's review queue window (tray → Review Dictations…): audio, the words the two listens disagreed on as grey click-to-play chips, 'What you said' pre-filled with Claude's literal guess and 'How you'd have written it' with its polished guess; Fix up (F / ⌘J) under the transcript. ⌘↵ confirms both, S skip, X exclude, Esc closes.
Its files: client/apps/desktop/ui/src/review/Review.svelte, client/apps/desktop/ui/src/review/review.css, client/apps/desktop/ui/review.html
Reaches into: client/apps/desktop/ui/src/lib/ui.css, client/apps/desktop/ui/src/lib/Icon.svelte, client/apps/desktop/ui/src/lib/Mark.svelte, client/apps/desktop/src-tauri/src/windows.rs, client/apps/desktop/src-tauri/src/commands.rs, client/apps/desktop/ui/src/review/main.ts
Tags: window, desktop, review, keyboard, training

## keycaps — the keycaps
A hotkey drawn as the keys printed on the keyboard: on a Mac fn with its 🌐, modifiers with symbol top-right and name bottom-left; elsewhere (IS_MAC false) a PC's plain ctrl/alt/super/shift labels with X11 keycodes. A left/right side word, wide space/return; fill stretches a key to its slot and selected inks it. capsFor(binding) turns a stored binding into caps; spoken() gives the in-sentence form ("fn", "right ⌥", "right ctrl", "ctrl+alt+space"). Keycode 179 (Globe) is Fn on macOS.
Its files: client/apps/desktop/ui/src/lib/Keycap.svelte, client/apps/desktop/ui/src/lib/keycaps.ts
Reaches into: client/apps/desktop/ui/src/lib/Icon.svelte, client/apps/desktop/ui/src/lib/tokens.css, client/apps/desktop/ui/src/settings/tabs/Hotkey.svelte
Tags: keyboard, hotkey, desktop, control

## key-row — the key row
The keyboard's bottom row as a one-click hotkey picker in the Hotkey tab — on a Mac fn ⌃ ⌥ ⌘ space ⌘ ⌥, on Linux/X11 ctrl super alt space alt ctrl — the bound key inked. Keys flex in keyboard proportions (BOTTOM_ROW in keycaps.ts, picked by platform) and drop the words under the symbols below 430px via a container query. Class names are kb-* because settings.css already owns a global .row.
Its files: client/apps/desktop/ui/src/lib/KeyboardRow.svelte
Reaches into: client/apps/desktop/ui/src/lib/Keycap.svelte, client/apps/desktop/ui/src/lib/keycaps.ts, client/apps/desktop/ui/src/settings/tabs/Hotkey.svelte
Tags: keyboard, hotkey, picker, desktop

## recording-player — the recording player
One-line player for a dictation's WAV: round play/pause, elapsed, seek track, length, 0.75/1/1.5× speed and an optional shortcut hint; 'unavailable' shows a note instead. Parents drive it via bind:this toggle()/stop(). Same look as the review window's inline player, which doesn't use it yet.
Its files: client/apps/desktop/ui/src/lib/AudioPlayer.svelte
Reaches into: client/apps/desktop/ui/src/correction/Correction.svelte, client/apps/desktop/ui/src/lib/ui.css
Tags: audio, player, desktop, control

## fix-up-button — the Fix up button
Wand button + status that rewrites 'What you said' into 'How you'd have written it' via invoke('fixup_text') (yagami, model from settings); shows Fixing up… / 'Fixed up by <model>' with Undo / the error in amber. Parent binds `to` and `busy`, calls run() for its shortcut and reset() on a new dictation; margin-left:auto so it sits right in a flex row.
Its files: client/apps/desktop/ui/src/lib/FixUp.svelte
Reaches into: client/apps/desktop/ui/src/lib/ui.css, client/apps/desktop/ui/src/lib/Icon.svelte, client/apps/desktop/ui/src/correction/Correction.svelte, client/apps/desktop/ui/src/review/Review.svelte
Tags: button, llm, yagami, correction

## fix-up-settings-tab — the Fix up settings
Settings tab picking the Fix up model (select grouped by CLI, from yagami_inventory) and effort (segmented Auto + the model's own levels; cleared for models with none), a Try it row that runs a canned ramble with the on-screen choice, and the CLIs found on this computer with model counts. Switching models keeps the effort if the new one takes it, else falls to low.
Its files: client/apps/desktop/ui/src/settings/tabs/FixUp.svelte
Reaches into: client/apps/desktop/ui/src/settings/settings.css, client/apps/desktop/ui/src/settings/Settings.svelte
Tags: settings, llm, yagami, model-picker

## history-window — the history window
Una's main window (launch and the Dock icon open it): the live server's dashboard loaded into an app window (overlay title bar, 1120x760, hides on close). window.__UNA_APP__ puts the dashboard in app mode: traffic-light room, drag strips, and a sidebar Settings item + ⌘, that follow una://settings, which history.rs stops in on_navigation to open the settings window; the server's own page becomes Server settings. History.svelte is only the local 'Can't reach your una server' page.
Its files: client/apps/desktop/ui/src/history/History.svelte, client/apps/desktop/ui/history.html, client/apps/desktop/src-tauri/src/history.rs
Reaches into: server/dashboard/src/App.svelte, client/apps/desktop/src-tauri/Info.plist, client/apps/desktop/src-tauri/src/windows.rs, server/dashboard/src/lib/app.ts
Tags: window, dashboard, history, main

## ios-record-screen — iPhone record screen
Una iOS Record tab (behind first-run onboarding): record, upload, result card, copied to clipboard; DEBUG-only WAV feed
Its files: client/apps/ios/Una/RecordView.swift, client/apps/ios/Una/Recorder.swift

## una-keyboard — Una keyboard
Typing keyboard with live mic key (2b); shows Full Access notice when off
Its files: client/apps/ios/UnaKeyboard/KeyboardView.swift

## fix-screen — the fix screen
Opens one dictation from History: raw and cleaned text editors, live word diff, audio playback, filed-status row, Save correction.
Its files: client/apps/ios/Una/FixView.swift
