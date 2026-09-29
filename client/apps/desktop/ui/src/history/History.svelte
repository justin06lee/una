<script lang="ts">
  /**
   * The history window when no server answers. The history itself is the
   * server's dashboard, loaded straight into this window; this page is only
   * what shows instead when there is nothing to load it from.
   */
  import { invoke } from "@tauri-apps/api/core";
  import Mark from "../lib/Mark.svelte";

  const isMac = navigator.userAgent.includes("Mac");

  let trying = $state(false);

  async function retry() {
    trying = true;
    await invoke("open_history");
    // Still here a moment later: the server didn't answer this time either.
    setTimeout(() => (trying = false), 3000);
  }
</script>

<main class:mac={isMac}>
  <header class="titlebar" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region><Mark size={18} /> Dictation history</span>
  </header>

  <section class="empty">
    <p class="big">Can't reach your una server</p>
    <p class="muted">
      Your history lives on the server. una tried every address in Settings → Server and none
      answered. Check that the server is running and this computer is on its network.
    </p>
    <div class="actions">
      <button class="btn btn-primary" onclick={() => void retry()} disabled={trying}>
        {trying ? "Trying…" : "Try again"}
      </button>
      <button class="btn btn-ghost" onclick={() => void invoke("open_settings")}>
        Server settings
      </button>
    </div>
  </section>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .titlebar {
    height: 38px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    border-bottom: 1px solid var(--line);
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
  }
  .title {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 0 32px 38px;
    text-align: center;
  }
  .big {
    font-size: 15px;
    font-weight: 500;
    color: var(--fg);
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
