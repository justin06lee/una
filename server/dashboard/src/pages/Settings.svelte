<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type Settings } from '../api';
  import Skeleton from '../lib/Skeleton.svelte';
  import ThemeToggle from '../lib/ThemeToggle.svelte';
  import Toggle from '../lib/Toggle.svelte';

  let stored = $state<Settings | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let saved = $state(false);
  let savedTimer: ReturnType<typeof setTimeout> | undefined;

  // form state
  let cleanupEnabled = $state(false);
  let cleanupModel = $state('');
  let cleanupTimeout = $state(3);
  let thresholdMinutes = $state(30);
  let trainingAuto = $state(false);
  let autoIdleMinutes = $state(30);
  let maxEditDistance = $state(0.3);

  function fill(s: Settings): void {
    stored = s;
    cleanupEnabled = s['cleanup.enabled'];
    cleanupModel = s['cleanup.model'];
    cleanupTimeout = s['cleanup.timeout_s'];
    thresholdMinutes = s['training.threshold_minutes'];
    trainingAuto = s['training.auto'];
    autoIdleMinutes = s['training.auto_idle_minutes'];
    maxEditDistance = s['training.max_edit_distance'];
  }

  onMount(() => {
    void (async () => {
      try {
        fill(await api.getSettings());
      } catch (e) {
        error = errMsg(e);
      } finally {
        loading = false;
      }
    })();
    return () => clearTimeout(savedTimer);
  });

  const dirty = $derived(
    stored !== null &&
      (cleanupEnabled !== stored['cleanup.enabled'] ||
        cleanupModel !== stored['cleanup.model'] ||
        Number(cleanupTimeout) !== stored['cleanup.timeout_s'] ||
        Number(thresholdMinutes) !== stored['training.threshold_minutes'] ||
        trainingAuto !== stored['training.auto'] ||
        Number(autoIdleMinutes) !== stored['training.auto_idle_minutes'] ||
        Number(maxEditDistance) !== stored['training.max_edit_distance']),
  );

  async function save(): Promise<void> {
    if (saving) return;
    saving = true;
    error = null;
    try {
      fill(
        await api.putSettings({
          'cleanup.enabled': cleanupEnabled,
          'cleanup.model': cleanupModel,
          'cleanup.timeout_s': Number(cleanupTimeout),
          'training.threshold_minutes': Number(thresholdMinutes),
          'training.auto': trainingAuto,
          'training.auto_idle_minutes': Number(autoIdleMinutes),
          'training.max_edit_distance': Number(maxEditDistance),
        }),
      );
      saved = true;
      clearTimeout(savedTimer);
      savedTimer = setTimeout(() => (saved = false), 2500);
    } catch (e) {
      error = errMsg(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="mx-auto max-w-2xl px-8 py-10 pb-24">
  <header class="mb-7">
    <h1 class="text-[20px] font-semibold tracking-tight">Settings</h1>
    <p class="mt-1 text-[13px] text-muted">
      Applied the moment you save, and kept across restarts.
    </p>
  </header>

  {#if error}
    <div
      class="mb-4 rounded-xl border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--c-danger) 28%, transparent); background: var(--c-danger-soft); color: var(--c-danger)"
    >
      {error}
    </div>
  {/if}

  <!-- Appearance ---------------------------------------------------------- -->
  <section class="card mb-4">
    <div class="px-5 pt-4 pb-1"><h2 class="text-[14px] font-semibold">Appearance</h2></div>
    <div class="flex items-center justify-between gap-6 px-5 py-4">
      <div>
        <div class="text-[13.5px]">Theme</div>
        <div class="mt-0.5 text-[12px] text-muted">
          Auto follows your operating system.
        </div>
      </div>
      <div class="w-44 flex-none"><ThemeToggle /></div>
    </div>
  </section>

  {#if loading}
    <div class="space-y-4">
      <Skeleton class="h-44 w-full" />
      <Skeleton class="h-56 w-full" />
    </div>
  {:else if stored}
    <!-- Cleanup ----------------------------------------------------------- -->
    <section class="card mb-4 divide-y divide-border">
      <div class="px-5 pt-4 pb-3">
        <h2 class="text-[14px] font-semibold">Cleanup</h2>
        <p class="mt-0.5 text-[12px] text-muted">
          The local LLM that strips filler words and punctuates before the text reaches your app.
        </p>
      </div>
      <div class="flex items-center justify-between gap-6 px-5 py-4">
        <div>
          <div class="text-[13.5px]">Clean up transcripts</div>
          <div class="mt-0.5 text-[12px] text-muted">
            With this off, you get the raw transcript exactly as heard.
          </div>
        </div>
        <Toggle checked={cleanupEnabled} onchange={(v) => (cleanupEnabled = v)} label="Enable cleanup" />
      </div>
      <div class="flex items-center justify-between gap-6 px-5 py-4">
        <div>
          <div class="text-[13.5px]">Model</div>
          <div class="mt-0.5 text-[12px] text-muted">
            Any Ollama model — a 3–4B instruct model is plenty on a small GPU.
          </div>
        </div>
        <input class="input max-w-48" bind:value={cleanupModel} aria-label="Cleanup model" />
      </div>
      <div class="flex items-center justify-between gap-6 px-5 py-4">
        <div>
          <div class="text-[13.5px]">Timeout</div>
          <div class="mt-0.5 text-[12px] text-muted">
            Seconds to wait before giving up and using the raw transcript.
          </div>
        </div>
        <input
          class="input max-w-24 text-right tabular-nums"
          type="number"
          min="0.5"
          step="0.5"
          bind:value={cleanupTimeout}
          aria-label="Cleanup timeout in seconds"
        />
      </div>
    </section>

    <!-- Training ---------------------------------------------------------- -->
    <section class="card mb-6 divide-y divide-border">
      <div class="px-5 pt-4 pb-3">
        <h2 class="text-[14px] font-semibold">Training</h2>
        <p class="mt-0.5 text-[12px] text-muted">
          When and how una fine-tunes itself on the dictations you've reviewed.
        </p>
      </div>
      <div class="flex items-center justify-between gap-6 px-5 py-4">
        <div>
          <div class="text-[13.5px]">Train after</div>
          <div class="mt-0.5 text-[12px] text-muted">
            Minutes of reviewed, eligible audio before a run is worth starting.
          </div>
        </div>
        <div class="flex flex-none items-center gap-2">
          <input
            class="input w-24 text-right tabular-nums"
            type="number"
            min="1"
            step="1"
            bind:value={thresholdMinutes}
            aria-label="Training threshold in minutes"
          />
          <span class="text-[12px] text-faint">min</span>
        </div>
      </div>
      <div class="flex items-center justify-between gap-6 px-5 py-4">
        <div>
          <div class="text-[13.5px]">Train automatically</div>
          <div class="mt-0.5 text-[12px] text-muted">
            Start a run on its own once there's new reviewed data and the machine is idle.
          </div>
        </div>
        <Toggle checked={trainingAuto} onchange={(v) => (trainingAuto = v)} label="Auto-train" />
      </div>
      <div class="flex items-center justify-between gap-6 px-5 py-4" class:opacity-55={!trainingAuto}>
        <div>
          <div class="text-[13.5px]">Idle before auto-training</div>
          <div class="mt-0.5 text-[12px] text-muted">
            How long dictation must be quiet first — training takes the GPU.
          </div>
        </div>
        <div class="flex flex-none items-center gap-2">
          <input
            class="input w-24 text-right tabular-nums"
            type="number"
            min="1"
            step="5"
            bind:value={autoIdleMinutes}
            disabled={!trainingAuto}
            aria-label="Auto-train idle minutes"
          />
          <span class="text-[12px] text-faint">min</span>
        </div>
      </div>
      <div class="flex items-center justify-between gap-6 px-5 py-4">
        <div>
          <div class="text-[13.5px]">Correction strictness</div>
          <div class="mt-0.5 text-[12px] text-muted">
            Corrections that stray further than this from the raw transcript are treated as
            rewrites and left out of training (0–1).
          </div>
        </div>
        <input
          class="input max-w-24 text-right tabular-nums"
          type="number"
          min="0"
          max="1"
          step="0.05"
          bind:value={maxEditDistance}
          aria-label="Maximum normalized edit distance"
        />
      </div>
    </section>

    <!-- Save bar ---------------------------------------------------------- -->
    <div
      class="sticky bottom-4 flex items-center gap-3 rounded-xl border border-border bg-surface px-4 py-3 shadow-[var(--shadow-raised)]"
    >
      <button class="btn btn-primary" onclick={() => void save()} disabled={saving || !dirty}>
        {saving ? 'Saving…' : 'Save changes'}
      </button>
      {#if saved}
        <span class="chip chip-ok fade-in"><span class="chip-dot"></span>Saved</span>
      {:else if dirty}
        <span class="text-[12px] text-faint">You have unsaved changes</span>
      {:else}
        <span class="text-[12px] text-faint">Everything is up to date</span>
      {/if}
    </div>
  {/if}
</div>
