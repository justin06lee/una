# Una — Dashboard shell and settings

One layer of Una's stack, owning `server/dashboard/index.html, server/dashboard/src/App.svelte, server/dashboard/src/main.ts, server/dashboard/src/router.svelte.ts, server/dashboard/src/app.css, server/dashboard/src/lib/theme.svelte.ts, server/dashboard/src/lib/Logo.svelte, server/dashboard/src/lib/ThemeToggle.svelte, server/dashboard/src/pages/Settings.svelte, server/dashboard/svelte.config.js, server/dashboard/vite.config.ts, server/dashboard/src/api.ts`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer dashboard-shell-settings` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer dashboard-shell-settings add|set|drop <section> …`. Where it and the code disagree, the code is right.

Dashboard shell and settings is the Svelte 5 app frame around all dashboard pages: sidebar nav, hash routing, theming, and global styles with a Vite build. It was fully redesigned to a clean minimal monochrome system on Geist type with hairline borders and no purple accent. A newcomer must grasp that routing is hash-based with no dependency and theming flips via `data-theme` CSS variables resolved before first paint.

## Where to change what

- **App frame and sidebar nav:** server/dashboard/src/App.svelte, server/dashboard/src/lib/Icon.svelte
- **Hash routing:** server/dashboard/src/router.svelte.ts
- **Theme mode (light/dark/system):** server/dashboard/src/lib/theme.svelte.ts, server/dashboard/index.html, server/dashboard/src/lib/ThemeToggle.svelte
- **Settings form:** server/dashboard/src/pages/Settings.svelte
- **Server API client:** server/dashboard/src/api.ts
- **Global theme and styles:** server/dashboard/src/app.css
- **Vite build and entry:** server/dashboard/vite.config.ts, server/dashboard/svelte.config.js, server/dashboard/index.html
- **App logo mark:** server/dashboard/src/lib/Logo.svelte
- **App mount:** server/dashboard/src/main.ts
- **App icon set:** server/dashboard/src/lib/Icon.svelte

## How it works

- **Page navigation:** router.svelte.ts parses location.hash on load, defaulting to #/home and mapping legacy aliases → App.svelte reads router.path to render the matching page component → Nav click calls router.go() which sets location.hash → hashchange event updates router.path and swaps the page
- **Theme resolution:** index.html inline script reads una:theme from localStorage before first paint and sets data-theme → main.ts mounts App.svelte which imports theme.svelte.ts store → Theme store reconciles stored choice with prefers-color-scheme and OS change events → ThemeToggle.svelte sets choice; store updates data-theme, localStorage, and resolved value
- **Settings edit:** Settings.svelte loads current values via API client on mount → Form state fills from stored Settings → User edits fields and saves back through the API client → Saved indicator shows on success, error state on failure

## Key files

- server/dashboard/src/App.svelte — sidebar frame, nav list, health chip, page switch
- server/dashboard/src/router.svelte.ts — tiny hash router with defaults and legacy aliases
- server/dashboard/src/lib/theme.svelte.ts — light/dark/system store syncing data-theme and localStorage
- server/dashboard/src/pages/Settings.svelte — settings form loading and saving server settings
- server/dashboard/src/api.ts — typed fetch client for the server API
- server/dashboard/src/app.css — minimal monochrome Geist design system, hairline borders, no accent color
- server/dashboard/src/lib/Icon.svelte — shared app icon set for nav and UI
- server/dashboard/index.html — entry HTML with pre-paint theme script and favicon
- server/dashboard/vite.config.ts — Vite/Tailwind/Svelte build emitting into server web dir with dev proxy
- server/dashboard/src/main.ts — mounts App into #app
- server/dashboard/src/lib/ThemeToggle.svelte — light/dark/auto segmented control

## Rules and traps

- Build output is server/src/una_server/web with emptyOutDir; the keep-gitkeep plugin must restore .gitkeep after every build.
- Never change the localStorage key una:theme or the data-theme attribute contract; index.html and theme.svelte.ts must stay in sync or first paint flashes.
- Routing is hash-only (#/home); do not introduce a router dependency or history routing, and preserve /history and /stats aliases.
- Fonts must stay self-hosted for offline LAN use; do not add CDN font or CSS links.
- Colors must go through --c-* custom properties mapped with @theme inline so dark mode flips with data-theme.

## What it talks to

- Dashboard API client and UI kit — Settings.svelte and App.svelte fetch Health/Settings through src/api.ts and reuse Skeleton/Toggle primitives
- Dashboard home and insights — Home and Insights pages render inside App.svelte frame via router paths
- Dashboard review queue — Review page renders inside App.svelte frame via router path
- Dashboard dictionary and training — Dictionary and Training pages render inside App.svelte frame via router path
- Server API — Vite dev server proxies /v1 to the FastAPI server; production build is served from una_server/web
