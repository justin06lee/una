<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon, { type IconName } from "../lib/Icon.svelte";
  import Mark from "../lib/Mark.svelte";
  import type { Config } from "../lib/types";
  import About from "./tabs/About.svelte";
  import Audio from "./tabs/Audio.svelte";
  import General from "./tabs/General.svelte";
  import Hotkey from "./tabs/Hotkey.svelte";
  import Learning from "./tabs/Learning.svelte";
  import Pasting from "./tabs/Pasting.svelte";
  import Server from "./tabs/Server.svelte";

  const TABS: { id: string; label: string; icon: IconName }[] = [
    { id: "general", label: "General", icon: "sliders" },
    { id: "hotkey", label: "Hotkey", icon: "keyboard" },
    { id: "server", label: "Server", icon: "server" },
    { id: "audio", label: "Audio", icon: "mic" },
    { id: "pasting", label: "Pasting", icon: "clipboard" },
    { id: "learning", label: "Learning", icon: "flask" },
    { id: "about", label: "About", icon: "info" },
  ];

  /** macOS draws the traffic lights over the sidebar (overlay title bar). */
  const isMac = navigator.userAgent.includes("Mac");

  let tab = $state("general");
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

  onMount(() => {
    invoke<Config>("get_config").then((c) => (config = c));
    return () => clearTimeout(saveTimer);
  });
</script>

<div class="layout" class:mac={isMac}>
  <nav class="sidebar">
    <div class="drag" data-tauri-drag-region></div>
    <div class="tabs">
      {#each TABS as t (t.id)}
        <button class="tab" class:active={tab === t.id} onclick={() => (tab = t.id)}>
          <Icon name={t.icon} size={15} />
          {t.label}
        </button>
      {/each}
    </div>
    <div class="brand">
      <Mark size={18} />
      <span>una</span>
    </div>
  </nav>

  <main class="content">
    <div class="drag" data-tauri-drag-region></div>
    <div class="inner">
      {#if !config}
        <p class="faint">Loading…</p>
      {:else if tab === "general"}
        <General bind:config {save} />
      {:else if tab === "hotkey"}
        <Hotkey bind:config {save} />
      {:else if tab === "server"}
        <Server bind:config {save} />
      {:else if tab === "audio"}
        <Audio bind:config {save} bind:error={saveError} />
      {:else if tab === "pasting"}
        <Pasting bind:config {save} />
      {:else if tab === "learning"}
        <Learning bind:config {save} />
      {:else if tab === "about"}
        <About />
      {/if}

      {#if saveError}
        <div class="note error"><span class="dot down"></span>{saveError}</div>
      {/if}
    </div>
  </main>
</div>

<style>
  .layout {
    display: flex;
    height: 100%;
  }

  .sidebar {
    width: 184px;
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
    height: 44px;
  }

  .tabs {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 0 8px;
    flex: 1;
  }

  .tab {
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

  .tab:hover,
  .tab.active {
    background: var(--hover);
    color: var(--fg);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 17px;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: -0.02em;
  }

  .content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .content .drag {
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
