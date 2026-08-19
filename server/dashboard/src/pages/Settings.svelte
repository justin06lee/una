<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type Settings } from '../api';
  import Skeleton from '../lib/Skeleton.svelte';
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

  onMount(async () => {
    try {
      fill(await api.getSettings());
    } catch (e) {
      error = errMsg(e);
    } finally {
      loading = false;
    }
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
      const res = await api.putSettings({
        'cleanup.enabled': cleanupEnabled,
        'cleanup.model': cleanupModel,
        'cleanup.timeout_s': Number(cleanupTimeout),
        'training.threshold_minutes': Number(thresholdMinutes),
        'training.auto': trainingAuto,
        'training.auto_idle_minutes': Number(autoIdleMinutes),
        'training.max_edit_distance': Number(maxEditDistance),
      });
      fill(res);
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

<div class="mx-auto max-w-2xl px-6 py-8">
  <header class="mb-6">
    <h1 class="text-base font-semibold tracking-tight">Settings</h1>
    <p class="mt-0.5 text-[13px] text-muted">
      Runtime settings — applied immediately, persisted across restarts.
    </p>
  </header>

  {#if error}
    <div
      class="mb-4 rounded-lg border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--color-danger) 30%, transparent); background: color-mix(in oklab, var(--color-danger) 7%, transparent); color: var(--color-danger)"
    >
      {error}
    </div>
  {/if}

  {#if loading}
    <div class="space-y-4">
      <Skeleton class="h-40 w-full" />
      <Skeleton class="h-40 w-full" />
    </div>
  {:else if stored}
    <div class="card mb-4 divide-y divide-border">
      <div class="px-5 py-3">
        <h2 class="text-[13px] font-semibold">Cleanup</h2>
      </div>
      <div class="flex items-center justify-between gap-4 px-5 py-3.5">
        <div>
          <div class="text-sm">Enable LLM cleanup</div>
          <div class="mt-0.5 text-xs text-muted">
            Polish raw transcripts with a local Ollama model after transcription.
          </div>
        </div>
        <Toggle
          checked={cleanupEnabled}
          onchange={(v) => (cleanupEnabled = v)}
          label="Enable LLM cleanup"
        />
      </div>
      <div class="flex items-center justify-between gap-4 px-5 py-3.5">
        <div>
          <div class="text-sm">Cleanup model</div>
          <div class="mt-0.5 text-xs text-muted">Ollama model name, e.g. qwen3:8b.</div>
        </div>
        <input class="input max-w-44" bind:value={cleanupModel} aria-label="Cleanup model" />
      </div>
      <div class="flex items-center justify-between gap-4 px-5 py-3.5">
        <div>
          <div class="text-sm">Cleanup timeout</div>
          <div class="mt-0.5 text-xs text-muted">
            Seconds before falling back to the raw transcript.
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
    </div>

    <div class="card mb-6 divide-y divide-border">
      <div class="px-5 py-3">
        <h2 class="text-[13px] font-semibold">Training</h2>
      </div>
      <div class="flex items-center justify-between gap-4 px-5 py-3.5">
        <div>
          <div class="text-sm">Threshold minutes</div>
          <div class="mt-0.5 text-xs text-muted">
            Eligible minutes of audio required before training is worthwhile.
          </div>
        </div>
        <input
          class="input max-w-24 text-right tabular-nums"
          type="number"
          min="1"
          step="1"
          bind:value={thresholdMinutes}
          aria-label="Training threshold in minutes"
        />
      </div>
      <div class="flex items-center justify-between gap-4 px-5 py-3.5">
        <div>
          <div class="text-sm">Auto-train</div>
          <div class="mt-0.5 text-xs text-muted">
            Start a run automatically when the threshold is reached and the server is idle.
          </div>
        </div>
        <Toggle checked={trainingAuto} onchange={(v) => (trainingAuto = v)} label="Auto-train" />
      </div>
      <div class="flex items-center justify-between gap-4 px-5 py-3.5">
        <div>
          <div class="text-sm">Auto-train idle minutes</div>
          <div class="mt-0.5 text-xs text-muted">
            How long dictation must be quiet before an automatic run may start.
          </div>
        </div>
        <input
          class="input max-w-24 text-right tabular-nums"
          type="number"
          min="1"
          step="5"
          bind:value={autoIdleMinutes}
          aria-label="Auto-train idle minutes"
        />
      </div>
      <div class="flex items-center justify-between gap-4 px-5 py-3.5">
        <div>
          <div class="text-sm">Max edit distance</div>
          <div class="mt-0.5 text-xs text-muted">
            Corrections that diverge further than this from the raw transcript are rejected as
            rewrites (0–1).
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
    </div>

    <div class="flex items-center gap-3">
      <button class="btn btn-primary" onclick={() => void save()} disabled={saving || !dirty}>
        {saving ? 'Saving…' : 'Save changes'}
      </button>
      {#if saved}
        <span class="text-xs" style="color: var(--color-ok)">Saved ✓</span>
      {:else if dirty}
        <span class="text-xs text-faint">Unsaved changes</span>
      {/if}
    </div>
  {/if}
</div>
