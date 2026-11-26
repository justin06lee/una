# Una — CLI

One layer of Una's stack, owning `client/crates/una-cli/`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer cli` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer cli add|set|drop <section> …`. Where it and the code disagree, the code is right.

The CLI layer is the `una` binary: a thin Clap wrapper that drives the running desktop app over its unix socket. It takes one subcommand (toggle/start/stop/cancel), maps it to a wire name, and sends it via una-core socket helpers. There is no audio, state, or server logic here — just command delivery for compositor keybinds and scripts.

## Where to change what

- **Dictation subcommands (toggle/start/stop/cancel):** client/crates/una-cli/src/main.rs
- **Socket command delivery:** client/crates/una-cli/src/main.rs
- **CLI help text / keybind usage:** client/crates/una-cli/src/main.rs

## How it works

- **Send dictation command:** Parse subcommand with Clap (Toggle/Start/Stop/Cancel) → Map variant to wire name via wire_name() → Send over desktop unix socket via core helper → Exit 0 on delivery; non-zero with stderr message on failure

## Key files

- client/crates/una-cli/src/main.rs — the entire `una` binary: Clap CLI, wire-name mapping, and socket send

## Rules and traps

- Never add recording/transcription logic here — it only signals the running desktop app.
- Wire names (toggle/start/stop/cancel) must match what the desktop shell expects; renaming breaks keybinds.
- Commands are fire-and-forget no-ops when inapplicable (e.g. stop while idle); do not add blocking waits.
- Failure mode is stderr message plus non-zero exit so scripts/keybinds can detect it.

## What it talks to

- Desktop shell — receives the socket command and runs the actual start/stop/cancel flow
- Client core — provides the unix-socket send helper the CLI calls
- Platform inject — indirectly triggered downstream when dictation produces text (not touched here)
