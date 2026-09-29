<script lang="ts">
  /**
   * The server's pages — Home, Dictionary, Training, Insights, and the
   * cleanup and training settings — inside the Una window.
   *
   * They are the dashboard the una server serves, framed with `?embed`: the
   * page drops its own sidebar and follows the system theme, tells the window
   * which page it's on, and hands its links to Review and Settings back here.
   * One frame lives for the whole session, so moving between these pages is a
   * route change rather than a reload; coming back to them from a page of the
   * window's own asks the page to fetch again, since a review may have
   * changed what it shows.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let {
    path,
    visible,
    online,
    onroute,
    onnavigate,
    onserver,
  }: {
    /** The dashboard route to show, e.g. "/home". */
    path: string;
    visible: boolean;
    /** From the window's health check: whether the server answers right now. */
    online: boolean | null;
    onroute: (path: string) => void;
    onnavigate: (path: string) => void;
    onserver: () => void;
  } = $props();

  let base = $state<string | null>(null);
  let error = $state<string | null>(null);
  let connecting = $state(false);
  let src = $state<string | null>(null);
  let loaded = $state(false);
  let frame: HTMLIFrameElement | undefined = $state();

  async function connect() {
    connecting = true;
    error = null;
    try {
      base = await invoke<string>("dashboard_base");
      // A fresh load at wherever the window is now.
      loaded = false;
      shown = { path, visible };
      src = `${base}/?embed=1#${path}`;
    } catch (e) {
      error = String(e);
    } finally {
      connecting = false;
    }
  }

  function onMessage(event: MessageEvent) {
    if (!frame || event.source !== frame.contentWindow) return;
    const message = event.data as { type?: string; path?: string } | null;
    if (!message?.path) return;
    if (message.type === "una:route") {
      // The page moved itself; nothing to send it.
      shown.path = message.path;
      onroute(message.path);
    }
    else if (message.type === "una:navigate") onnavigate(message.path);
  }

  onMount(() => {
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  });

  // Connect the first time one of these pages is shown.
  $effect(() => {
    if (visible && !src && !connecting && !error) void connect();
  });

  // The server went away and came back: load it again.
  let wasOnline: boolean | null = null;
  $effect(() => {
    if (online && wasOnline === false && src) void connect();
    wasOnline = online;
  });

  // Every later move goes to the page as a message: a route change, and a
  // fresh fetch of that page. Also sent on coming back from one of the
  // window's own pages, where a review may have changed what this one shows.
  let shown = { path: "", visible: false };
  $effect(() => {
    const now = { path, visible };
    if (loaded && now.visible && (now.path !== shown.path || !shown.visible)) {
      frame?.contentWindow?.postMessage({ type: "una:go", path: now.path }, "*");
    }
    shown = now;
  });

  const offline = $derived(!!error || (online === false && !!src));
</script>

<div class="dash" class:hidden={!visible}>
  <!-- Somewhere to drag the window by, over the pages' own top padding. -->
  <div class="drag" data-tauri-drag-region></div>

  {#if offline}
    <div class="empty">
      <p class="big">Can't reach your una server</p>
      <p class="muted">
        Your history, dictionary and training live on the server. una tried every address in
        Settings → Server and none answered.
      </p>
      <div class="actions">
        <button class="btn btn-primary" onclick={() => void connect()} disabled={connecting}>
          {connecting ? "Trying…" : "Try again"}
        </button>
        <button class="btn btn-ghost" onclick={onserver}>Server settings</button>
      </div>
    </div>
  {/if}

  {#if src}
    <iframe
      bind:this={frame}
      {src}
      title="una"
      class:ready={loaded && !offline}
      onload={() => (loaded = true)}
    ></iframe>
  {/if}
</div>

<style>
  .dash {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .dash.hidden {
    display: none;
  }

  .drag {
    position: absolute;
    inset: 0 0 auto 0;
    height: 28px;
    z-index: 2;
  }

  iframe {
    flex: 1;
    width: 100%;
    height: 100%;
    border: 0;
    background: var(--canvas);
    opacity: 0;
    transition: opacity 160ms ease;
  }
  iframe.ready {
    opacity: 1;
  }

  .empty {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 0 40px 40px;
    text-align: center;
    background: var(--canvas);
  }
  .big {
    font-size: 15px;
    font-weight: 500;
  }
  .muted {
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--faint);
    max-width: 400px;
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-top: 10px;
  }
</style>
