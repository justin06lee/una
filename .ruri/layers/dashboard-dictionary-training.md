# Una — Dashboard dictionary and training

One layer of Una's stack, owning `server/dashboard/src/pages/Dictionary.svelte, server/dashboard/src/pages/Training.svelte, server/dashboard/src/lib/WerChart.svelte, server/dashboard/src/lib/ProgressBar.svelte, server/dashboard/src/lib/StatusChip.svelte`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer dashboard-dictionary-training` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer dashboard-dictionary-training add|set|drop <section> …`. Where it and the code disagree, the code is right.

Dictionary page manages custom phrase entries (add, inline edit, enable toggle, delete) with client-side search and sort. Training page lists training runs and models, starts/cancels runs, activates models, and shows eligibility, logs, and WER progress. Shared WerChart, ProgressBar, and StatusChip render run status and WER history; WER values are already percent and plotted as-is.

## Where to change what

- **Dictionary entry list and search/sort:** server/dashboard/src/pages/Dictionary.svelte
- **Add dictionary entry form:** server/dashboard/src/pages/Dictionary.svelte
- **Inline edit dictionary entry:** server/dashboard/src/pages/Dictionary.svelte
- **Enable/disable and delete entry:** server/dashboard/src/pages/Dictionary.svelte
- **Training runs list and run detail:** server/dashboard/src/pages/Training.svelte
- **Start / cancel training run:** server/dashboard/src/pages/Training.svelte
- **Activate model and eligibility:** server/dashboard/src/pages/Training.svelte
- **Run log viewer:** server/dashboard/src/pages/Training.svelte
- **WER history chart:** server/dashboard/src/lib/WerChart.svelte, server/dashboard/src/pages/Training.svelte
- **Run progress bar:** server/dashboard/src/lib/ProgressBar.svelte, server/dashboard/src/pages/Training.svelte
- **Run status chip:** server/dashboard/src/lib/StatusChip.svelte, server/dashboard/src/pages/Training.svelte

## How it works

- **Edit dictionary entry:** Load entries via api.listDictionary on mount → Filter by query and sort (used/newest/alpha) client-side → Open inline edit, change phrase/sounds-like/notes → Save via API, reload or update list → Toggle enabled or confirm-delete when needed
- **Run and monitor training:** Load runs, models, and eligibility on mount → Start run (blocked while an ACTIVE_RUN_STATUS run exists) → Poll/refresh runs list while active → Show ProgressBar for active run and StatusChip per run → Plot completed runs in WerChart (baseline vs candidate) → Activate promoted model after confirm
- **Inspect a run:** Select run from list → Read StatusChip tone/pulse for state → Open log viewer for output → Compare WER baseline vs candidate with fmtWer/fmtWerDelta

## Key files

- server/dashboard/src/pages/Training.svelte — training runs, models, start/cancel/activate, log, eligibility, WER display
- server/dashboard/src/pages/Dictionary.svelte — dictionary CRUD, search/sort, toggle, inline edit, delete
- server/dashboard/src/lib/WerChart.svelte — baseline-vs-candidate WER chart over runs
- server/dashboard/src/lib/StatusChip.svelte — status tone and pulse dot from ACTIVE_RUN_STATUSES
- server/dashboard/src/lib/ProgressBar.svelte — clamped 0-1 progress bar with ARIA values

## Rules and traps

- WER is already percent (jiwer x 100) — never rescale in WerChart.
- WerChart drops points with null wer_candidate before plotting.
- Active run is runs.find ACTIVE_RUN_STATUSES — only one start at a time.
- StatusChip tone/pulse derives from ACTIVE_RUN_STATUSES; queued is muted, other active states pulse.
- ProgressBar clamps value to 0-1 before percent conversion.

## What it talks to

- Dashboard API client and UI kit — types, fetch calls, formatting, and EmptyState/Skeleton/Toggle primitives
- Dashboard shell and settings — hash routing to Dictionary and Training pages
- Server API — dictionary entries and training run/model endpoints
- Training loop — runs, WER eval, and promotion state this layer displays
