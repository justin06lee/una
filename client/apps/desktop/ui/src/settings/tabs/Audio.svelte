<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Toggle from "../../lib/Toggle.svelte";
  import type { Config, LevelFrame, TestRecordResult } from "../../lib/types";

  let {
    config = $bindable(),
    save,
    error = $bindable(),
  }: { config: Config; save: () => void; error: string } = $props();

  let devices = $state<string[]>([]);
  let testing = $state(false);
  let result = $state<TestRecordResult | null>(null);
  let level = $state(0);

  onMount(() => {
    invoke<string[]>("audio_devices")
      .then((d) => (devices = d))
      .catch(() => {});
    const unlisten = listen<LevelFrame>("audio-level", (e) => {
      level = Math.min(1, e.payload.rms * 3.2);
    });
    return () => {
      void unlisten.then((f) => f());
    };
  });

  async function testMic() {
    testing = true;
    result = null;
    try {
      result = await invoke<TestRecordResult>("test_record");
    } catch (e) {
      result = { ok: false, max_rms: 0, max_peak: 0 };
      error = String(e);
    } finally {
      testing = false;
    }
  }

  /** The live meter as discrete segments, lit up to the current level. */
  const SEGMENTS = 40;
  const lit = $derived(Math.round(level * SEGMENTS));
</script>

<h1 class="page-title">Audio</h1>
<p class="page-sub">Which microphone una listens to.</p>

<div class="section">
  <div class="group">
    <div class="row">
      <div class="row-copy">
        <div class="row-title">Input device</div>
        <div class="row-sub">Automatic follows your system's default input.</div>
      </div>
      <select class="input device" bind:value={config.audio.input_device} onchange={save}>
        <option value="auto">Automatic</option>
        {#each devices as d (d)}
          <option value={d}>{d}</option>
        {/each}
      </select>
    </div>
    <div class="row">
      <div class="row-copy">
        <div class="row-title">Prefer the built-in microphone</div>
        <div class="row-sub">Ignore headsets and AirPods in Automatic mode.</div>
      </div>
      <Toggle
        checked={config.audio.prefer_builtin}
        onchange={(v) => {
          config.audio.prefer_builtin = v;
          save();
        }}
        label="Prefer the built-in microphone"
      />
    </div>
  </div>
</div>

<div class="section">
  <div class="section-title">Test</div>
  <div class="group">
    <div class="row">
      <div class="meter" aria-hidden="true">
        {#each { length: SEGMENTS } as _, i (i)}
          <i class:on={i < lit}></i>
        {/each}
      </div>
      <button class="btn btn-sm" onclick={testMic} disabled={testing}>
        {testing ? "Listening for 2s…" : "Test microphone"}
      </button>
    </div>
  </div>
  {#if result}
    <p class="hint">
      {#if result.ok}
        <span class="dot ok"></span>&nbsp; The microphone works — peak level
        {(result.max_peak * 100).toFixed(0)}%.
      {:else}
        <span class="dot down"></span>&nbsp; No sound came through. Check the device above and
        una's microphone permission.
      {/if}
    </p>
  {/if}
</div>

<style>
  .device {
    width: 220px;
    flex: none;
  }

  .meter {
    flex: 1;
    display: flex;
    gap: 3px;
    height: 18px;
    align-items: stretch;
  }

  .meter i {
    flex: 1;
    max-width: 5px;
    border-radius: 1.5px;
    background: var(--hover);
    transition: background-color 60ms linear;
  }

  .meter i.on {
    background: var(--fg);
  }
</style>
