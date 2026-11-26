# Una — Dashboard review queue

One layer of Una's stack, owning `server/dashboard/src/pages/Review.svelte, server/dashboard/src/lib/AudioPlayer.svelte, server/dashboard/src/lib/autosize.ts`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer dashboard-review-queue` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer dashboard-review-queue add|set|drop <section> …`. Where it and the code disagree, the code is right.

This layer is the dictation review queue: listen to audio, edit transcript and polish text, compare raw vs final word diffs, and approve/reject/skip. Review.svelte owns state and API actions; AudioPlayer, DiffText/wordDiff, and autosize are its supporting UI primitives. Newcomers must grasp the session cursor model: everything at or before an id is handled or skipped.

## Where to change what

- **Review queue page:** server/dashboard/src/pages/Review.svelte
- **Transcript / polish editors:** server/dashboard/src/pages/Review.svelte, server/dashboard/src/lib/autosize.ts
- **Audio playback:** server/dashboard/src/lib/AudioPlayer.svelte, server/dashboard/src/pages/Review.svelte
- **Word diff view:** server/dashboard/src/pages/Review.svelte
- **Textarea autosize action:** server/dashboard/src/lib/autosize.ts

## How it works

- **Review a dictation:** Review.svelte loads next item, backlog, and eligibility via API client → AudioPlayer.svelte plays src with toggle/replay/setRate → User edits transcript editor and optional polish editor (autosize grows textareas) → DiffText.svelte renders wordDiff(a,b) one side at a time (del vs ins) → Review.svelte sends review action (approve/reject/skip) and advances cursor

## Key files

- server/dashboard/src/pages/Review.svelte — review queue state, editors, actions, cursor
- server/dashboard/src/lib/AudioPlayer.svelte — audio element with toggle/replay/setRate, seek bar
- server/dashboard/src/lib/DiffText.svelte — renders one side of wordDiff with del/ins marks
- server/dashboard/src/lib/diff.ts — LCS wordDiff(a,b) with pathological-input guard
- server/dashboard/src/lib/autosize.ts — Svelte action growing textarea to content with min height

## Rules and traps

- Do not reimplement diffing: DiffText.svelte must go through wordDiff from diff.ts.
- Respect the diff.ts O(n*m) guard (250k): pathological inputs fall back to whole-text del/ins.
- autosize takes {value, min}; pass the bound value so outside replacements re-measure.
- AudioPlayer resets position on new src but keeps chosen rate; use untrack in the src effect.

## What it talks to

- Dashboard API client and UI kit — Review.svelte fetches DictationDetail/Eligibility and uses EmptyState, Skeleton, Kbd, format helpers
- Dashboard dictionary and training — borrows ProgressBar for review progress display
- Server API — review actions and dictation detail/audio served from /v1 routes
