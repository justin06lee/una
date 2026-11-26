# Una — Dashboard home and insights

One layer of Una's stack, owning `server/dashboard/src/pages/Home.svelte, server/dashboard/src/pages/Insights.svelte, server/dashboard/src/lib/AppBars.svelte, server/dashboard/src/lib/DayBars.svelte, server/dashboard/src/lib/Heatmap.svelte`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer dashboard-home-insights` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer dashboard-home-insights add|set|drop <section> …`. Where it and the code disagree, the code is right.

This layer is the dashboard's read-mostly home and analytics surface: Home.svelte is a paginated dictation feed with search, filters, expandable detail, copy and delete; Insights.svelte is a stats overview built from a single Stats payload. Charts and tiles are small presentational Svelte 5 components fed derived arrays from the pages. A newcomer must grasp that pages own all fetching and derived math while lib components only render the props they receive.

## Where to change what

- **Home dictation feed:** server/dashboard/src/pages/Home.svelte
- **Feed search and app/reviewed filters:** server/dashboard/src/pages/Home.svelte
- **Expandable dictation detail with what una pasted, copy and delete:** server/dashboard/src/pages/Home.svelte
- **Time-saved vs typing estimate:** server/dashboard/src/pages/Home.svelte, server/dashboard/src/pages/Insights.svelte
- **Insights stats overview:** server/dashboard/src/pages/Insights.svelte
- **Month-over-month words delta:** server/dashboard/src/pages/Insights.svelte
- **26-week activity heatmap:** server/dashboard/src/lib/Heatmap.svelte
- **Daily words/dictations bar chart:** server/dashboard/src/lib/DayBars.svelte
- **Per-app usage bars with intensity label:** server/dashboard/src/lib/AppBars.svelte

## How it works

- **Home feed load and expand:** Home.svelte fetches paginated summaries plus Stats on mount → User searches or filters by app and reviewed state → User expands a row to fetch its DictationDetail → The detail always shows what una pasted, with a reason when cleanup did not run → User copies text or confirms delete, then list and stats refresh
- **Insights load and render:** Insights.svelte fetches Stats once on mount → Page derives month delta and totals from Stats → StatTile renders headline numbers → Heatmap, DayBars, and AppBars render day and app breakdowns from the same payload

## Key files

- server/dashboard/src/pages/Home.svelte — paginated dictation feed with search, filters, detail expansion (including what una pasted), copy, and delete
- server/dashboard/src/pages/Insights.svelte — usage-insights page fetching Stats and composing tiles and charts
- server/dashboard/src/lib/StatTile.svelte — label/value/hint tile with optional accent tone and child slot
- server/dashboard/src/lib/Heatmap.svelte — week-column activity heatmap ending on the current week
- server/dashboard/src/lib/DayBars.svelte — last-N-days bar chart for words or dictation count
- server/dashboard/src/lib/AppBars.svelte — per-app word-share bars with Light/Moderate/Power intensity

## Rules and traps

- Typing-saved estimates assume a sustained TYPING_WPM of 40 in both pages; change it in both places.
- DayBars expects server days newest-first and reverses a sliced copy for oldest-first display; do not mutate the prop.
- Heatmap normalizes against max words with a floor of 1 and pads weeks out to the end of the current week; future cells are placeholders.
- AppBars intensity thresholds are fixed shares of total words: >=0.35 Power, >=0.12 Moderate, else Light.
- The 'What una pasted' card must always render; when cleanup did not run it shows a fallback reason, because hiding it made raw-paste dictations look like they had no data.

## What it talks to

- Dashboard API client and UI kit — pages fetch Stats, summaries, and details via api in server/dashboard/src/api.ts and render EmptyState, Skeleton, Kbd, and formatting helpers
- Dashboard dictionary and training — Insights.svelte reuses WerChart from that layer for the WER chart
- Dashboard review queue — Home.svelte duplicates lightweight audio playback and detail display instead of reusing AudioPlayer review flow; keep feed edits out of the review queue
- Dashboard shell and settings — pages are mounted by hash routing and assume the app frame and theme; they only set local layout classes
