<script lang="ts">
  /**
   * The Una window: everything in one place.
   *
   * The sidebar holds the app's pages — Home, Review, Dictionary, Training,
   * Insights — and Settings at the bottom, which swaps the sidebar for the
   * settings tabs with a way back. Review and the settings tabs are this
   * app's own; Home, Dictionary, Training, Insights and the cleanup and
   * training settings are the server's pages, framed in (see Dashboard).
   * The settings are local on purpose: with the server down they still open,
   * and they're where you'd fix that.
   *
   * Una is a Dock app and this is its window: it opens on launch and from the
   * Dock, and the tray's items open it at the matching page.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Icon, { type IconName } from "../lib/Icon.svelte";
  import Mark from "../lib/Mark.svelte";
  import type { Config } from "../lib/types";
  import Review from "../review/Review.svelte";
  import About from "../settings/tabs/About.svelte";
  import Audio from "../settings/tabs/Audio.svelte";
  import FixUp from "../settings/tabs/FixUp.svelte";
  import General from "../settings/tabs/General.svelte";
  import Hotkey from "../settings/tabs/Hotkey.svelte";
  import Learning from "../settings/tabs/Learning.svelte";
  import Pasting from "../settings/tabs/Pasting.svelte";
  import Server from "../settings/tabs/Server.svelte";
  import Dashboard from "./Dashboard.svelte";

  type Item = { id: string; label: string; icon: IconName; path?: string };

  /** `path`: a page of the server's, shown framed. */
  const PAGES: Item[] = [
    { id: "home", label: "Home", icon: "home", path: "/home" },
    { id: "review", label: "Review", icon: "inbox" },
    { id: "dictionary", label: "Dictionary", icon: "book", path: "/dictionary" },
    { id: "training", label: "Training", icon: "flask", path: "/training" },
    { id: "insights", label: "Insights", icon: "chart", path: "/insights" },
  ];
  const SETTINGS: Item[] = [
    { id: "general", label: "General", icon: "sliders" },
    { id: "hotkey", label: "Hotkey", icon: "keyboard" },
    { id: "server", label: "Server", icon: "server" },
    { id: "audio", label: "Audio", icon: "mic" },
    { id: "pasting", label: "Pasting", icon: "clipboard" },
    { id: "learning", label: "Learning", icon: "activity" },
    { id: "fixup", label: "Fix up", icon: "wand" },
    { id: "cleanup", label: "Cleanup & training", icon: "cpu", path: "/settings" },
    { id: "about", label: "About", icon: "info" },
  ];
  const ALL = [...PAGES, ...SETTINGS];

  /** macOS draws the traffic lights over the top of the sidebar. */
  const isMac = navigator.userAgent.includes("Mac");

  let view = $state("home");
  /** Where Settings' back button returns to. */
  let lastPage = $state("home");
  /** The settings tab last open, for coming back to Settings. */
  let lastSetting = $state("general");

  const current = $derived(ALL.find((i) => i.id === view) ?? PAGES[0]);
  const inSettings = $derived(SETTINGS.some((i) => i.id === view));
  /** The server page on screen, or the last one (kept loaded while hidden). */
  let framedPath = $state("/home");

  function go(id: string) {
    if (id === "settings") id = lastSetting;
    const item = ALL.find((i) => i.id === id);
    if (!item) return;
    view = id;
    if (SETTINGS.includes(item)) lastSetting = id;
    else lastPage = id;
    if (item.path) framedPath = item.path;
    if (id === "review" || item.path) void refreshBacklog();
  }

  /** A server page moved itself (a link inside it). */
  function onroute(path: string) {
    const item = ALL.find((i) => i.path === path);
    // Only while it's the page on screen; a hidden frame settling isn't news.
    if (item && current.path && item.id !== view) go(item.id);
  }

  /** A server page's link to a page the window serves itself. */
  function onnavigate(path: string) {
    go(path === "/review" ? "review" : path === "/settings" ? "settings" : "home");
  }

  // -- config (the settings tabs) --------------------------------------------

  let config = $state<Config | null>(null);
  let saveError = $state("");
  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  /** Persist the whole config a beat after the last change. */
  function save() {
    if (!config) return;
    clearTimeout(saveTimer);
    const snapshot = JSON.parse(JSON.stringify(config));
    saveTimer = setTimeout(async () => {
      try {
        await invoke("set_config", { config: snapshot });
        saveError = "";
      } catch (e) {
        saveError = String(e);
      }
    }, 350);
  }

  // -- server status and the review count -----------------------------------

  type Health = { status?: string; asr_model?: string; training_active?: boolean };
  let health = $state<Health | null>(null);
  let online = $state<boolean | null>(null);
  let backlog = $state<number | null>(null);

  async function pollHealth() {
    try {
      health = await invoke<Health>("health_check", { url: null });
      online = true;
    } catch {
      online = false;
    }
  }

  async function refreshBacklog() {
    try {
      backlog = (await invoke<{ pending: number }>("review_queue", { limit: 1 })).pending;
    } catch {
      /* the count is a nicety */
    }
  }

  const statusLabel = $derived(
    online === false ? "Server offline" : online === null ? "Connecting…" : health?.status === "ok" ? "Connected" : "Degraded",
  );

  onMount(() => {
    invoke<Config>("get_config").then((c) => (config = c));
    void pollHealth();
    void refreshBacklog();
    const h = setInterval(() => void pollHealth(), 15_000);
    const b = setInterval(() => void refreshBacklog(), 60_000);
    // The tray and the Dock open the window at a page.
    const nav = listen<string>("navigate", (e) => go(e.payload));
    return () => {
      clearInterval(h);
      clearInterval(b);
      clearTimeout(saveTimer);
      void nav.then((un) => un());
    };
  });

  function onKeydown(event: KeyboardEvent) {
    // ⌘, is Settings in every Mac app.
    if (event.key === "," && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      go("settings");
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="layout" class:mac={isMac}>
  <nav class="sidebar">
    <div class="drag" data-tauri-drag-region></div>

    {#if inSettings}
      <button class="back" onclick={() => go(lastPage)}>
        <Icon name="chevronLeft" size={15} />
        Settings
      </button>
      <div class="items">
        {#each SETTINGS as t (t.id)}
          <button class="item" class:active={view === t.id} onclick={() => go(t.id)}>
            <Icon name={t.icon} size={15} />
            {t.label}
          </button>
        {/each}
      </div>
    {:else}
      <div class="brand" data-tauri-drag-region="deep">
        <Mark size={22} />
        <span>una</span>
      </div>
      <div class="items">
        {#each PAGES as p (p.id)}
          <button class="item" class:active={view === p.id} onclick={() => go(p.id)}>
            <Icon name={p.icon} size={15} />
            <span class="grow">{p.label}</span>
            {#if p.id === "review" && backlog}
              <span class="count">{backlog}</span>
            {/if}
          </button>
        {/each}
      </div>
      <div class="bottom">
        <button class="item" onclick={() => go("settings")}>
          <Icon name="settings" size={15} />
          <span class="grow">Settings</span>
          <span class="kbd">{isMac ? "⌘," : "Ctrl+,"}</span>
        </button>
      </div>
    {/if}

    <div class="status" title={health?.asr_model ?? ""}>
      <span class="dot" class:ok={online === true && health?.status === "ok"} class:down={online === false}
      ></span>
      <div class="status-copy">
        <div class="status-label">
          {statusLabel}{#if health?.training_active}<span class="faint"> · training</span>{/if}
        </div>
        {#if online && health?.asr_model}
          <div class="status-detail">{health.asr_model}</div>
        {/if}
      </div>
    </div>
  </nav>

  <main class="content">
    <!-- The server's pages: one frame, kept while other pages are up. -->
    <Dashboard
      path={framedPath}
      visible={!!current.path}
      {online}
      {onroute}
      {onnavigate}
      onserver={() => go("server")}
    />

    {#if view === "review"}
      <Review />
    {:else if !current.path}
      <div class="settings">
        <div class="drag" data-tauri-drag-region></div>
        <div class="inner">
          {#if !config}
            <p class="faint">Loading…</p>
          {:else if view === "general"}
            <General bind:config {save} />
          {:else if view === "hotkey"}
            <Hotkey bind:config {save} />
          {:else if view === "server"}
            <Server bind:config {save} />
          {:else if view === "audio"}
            <Audio bind:config {save} bind:error={saveError} />
          {:else if view === "pasting"}
            <Pasting bind:config {save} />
          {:else if view === "learning"}
            <Learning bind:config {save} />
          {:else if view === "fixup"}
            <FixUp bind:config {save} />
          {:else if view === "about"}
            <About />
          {/if}

          {#if saveError}
            <div class="note error"><span class="dot down"></span>{saveError}</div>
          {/if}
        </div>
      </div>
    {/if}
  </main>
</div>

<style>
  .layout {
    display: flex;
    height: 100%;
  }

  .sidebar {
    width: 208px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--sidebar);
    border-right: 1px solid var(--line);
  }

  /* Room for the traffic lights, and a handle to drag the window by. */
  .drag {
    height: 12px;
    flex: none;
  }
  .mac .drag {
    height: 40px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 17px 14px;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }

  .back {
    appearance: none;
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 8px 10px;
    height: 30px;
    padding: 0 8px 0 5px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--fg);
    font-size: 13px;
    font-weight: 600;
    text-align: left;
  }
  .back:hover {
    background: var(--hover);
  }

  .items {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 0 8px;
    flex: 1;
    overflow-y: auto;
  }

  .item {
    appearance: none;
    display: flex;
    align-items: center;
    gap: 9px;
    height: 30px;
    padding: 0 9px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--muted);
    font-size: 13px;
    font-weight: 500;
    text-align: left;
    transition:
      background-color 140ms ease,
      color 140ms ease;
  }
  .item:hover,
  .item.active {
    background: var(--hover);
    color: var(--fg);
  }
  .grow {
    flex: 1;
  }
  .count {
    font-size: 11.5px;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }
  .item .kbd {
    opacity: 0;
    transition: opacity 140ms ease;
  }
  .item:hover .kbd {
    opacity: 1;
  }

  .bottom {
    padding: 0 8px 6px;
  }

  .status {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 0 8px;
    padding: 11px 9px 14px;
    border-top: 1px solid var(--line);
  }
  .status .dot {
    margin-top: 5px;
  }
  .status-copy {
    min-width: 0;
  }
  .status-label {
    font-size: 12.5px;
    font-weight: 500;
  }
  .status-detail {
    margin-top: 2px;
    font-size: 11.5px;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--canvas);
  }

  .settings {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .settings .drag {
    position: sticky;
    top: 0;
    background: var(--canvas);
    z-index: 1;
  }
  .inner {
    padding: 4px 32px 36px;
    max-width: 640px;
  }
  .mac .inner {
    padding-top: 0;
  }
</style>
