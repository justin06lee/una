<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type Settings } from '../api';
  import Banner from '../lib/Banner.svelte';
  import PageHeader from '../lib/PageHeader.svelte';
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

{#snippet heading(title: string, sub: string)}
  <div class="mb-3">
    <h2 class="text-[14px] font-medium">{title}</h2>
    <p class="mt-1 text-[13px] text-muted">{sub}</p>
  </div>
{/snippet}

{#snippet copy(title: string, sub: string)}
  <div class="min-w-0">
    <div class="text-[13.5px]">{title}</div>
    <div class="mt-0.5 text-[12.5px] leading-relaxed text-muted">{sub}</div>
  </div>
{/snippet}

<div class="mx-auto max-w-[42rem] px-10 py-12 pb-28">
  <PageHeader title="Settings" sub="Saved on the server, applied the moment you save, and kept across restarts." />

  {#if error}
    <Banner message={error} />
  {/if}

  <!-- Appearance ---------------------------------------------------------- -->
  <section class="mb-10">
    {@render heading('Appearance', 'How this dashboard looks. The desktop app follows your system.')}
    <div class="panel flex items-center justify-between gap-6 px-5 py-4">
      {@render copy('Theme', 'System follows your operating system.')}
      <ThemeToggle />
    </div>
  </section>

  {#if loading}
    <div class="space-y-4">
      <Skeleton class="h-44 w-full" />
      <Skeleton class="h-56 w-full" />
    </div>
  {:else if stored}
    <!-- Cleanup ----------------------------------------------------------- -->
    <section class="mb-10">
      {@render heading(
        'Cleanup',
        'The local language model that strips filler words and punctuates before text reaches your app.',
      )}
      <div class="panel divide-y divide-line">
        <div class="flex items-center justify-between gap-6 px-5 py-4">
          {@render copy('Clean up transcripts', 'With this off, you get exactly what was heard.')}
          <Toggle checked={cleanupEnabled} onchange={(v) => (cleanupEnabled = v)} label="Clean up transcripts" />
        </div>
        <div class="flex items-center justify-between gap-6 px-5 py-4" class:opacity-50={!cleanupEnabled}>
          {@render copy('Model', 'Any Ollama model. A 3–4B instruct model is plenty on a small GPU.')}
          <input
            class="input w-48 flex-none font-mono !text-[12.5px]"
            bind:value={cleanupModel}
            disabled={!cleanupEnabled}
            aria-label="Cleanup model"
          />
        </div>
        <div class="flex items-center justify-between gap-6 px-5 py-4" class:opacity-50={!cleanupEnabled}>
          {@render copy('Timeout', 'How long to wait before falling back to the raw transcript.')}
          <div class="flex flex-none items-center gap-2">
            <input
              class="input w-20 text-right tabular-nums"
              type="number"
              min="0.5"
              step="0.5"
              bind:value={cleanupTimeout}
              disabled={!cleanupEnabled}
              aria-label="Cleanup timeout in seconds"
            />
            <span class="w-6 text-[12.5px] text-faint">sec</span>
          </div>
        </div>
      </div>
    </section>

    <!-- Training ---------------------------------------------------------- -->
    <section class="mb-10">
      {@render heading('Training', 'When and how una fine-tunes itself on the dictations you review.')}
      <div class="panel divide-y divide-line">
        <div class="flex items-center justify-between gap-6 px-5 py-4">
          {@render copy('Train after', 'Minutes of reviewed audio before a voice run is worth starting.')}
          <div class="flex flex-none items-center gap-2">
            <input
              class="input w-20 text-right tabular-nums"
              type="number"
              min="1"
              step="1"
              bind:value={thresholdMinutes}
              aria-label="Training threshold in minutes"
            />
            <span class="w-6 text-[12.5px] text-faint">min</span>
          </div>
        </div>
        <div class="flex items-center justify-between gap-6 px-5 py-4">
          {@render copy(
            'Train automatically',
            "Start a run by itself once there's new reviewed data and the machine is idle.",
          )}
          <Toggle checked={trainingAuto} onchange={(v) => (trainingAuto = v)} label="Train automatically" />
        </div>
        <div class="flex items-center justify-between gap-6 px-5 py-4" class:opacity-50={!trainingAuto}>
          {@render copy('Idle time first', 'How long dictation has to be quiet, since training takes the GPU.')}
          <div class="flex flex-none items-center gap-2">
            <input
              class="input w-20 text-right tabular-nums"
              type="number"
              min="1"
              step="5"
              bind:value={autoIdleMinutes}
              disabled={!trainingAuto}
              aria-label="Idle minutes before auto-training"
            />
            <span class="w-6 text-[12.5px] text-faint">min</span>
          </div>
        </div>
        <div class="flex items-center justify-between gap-6 px-5 py-4">
          {@render copy(
            'Correction strictness',
            'Corrections that stray further than this from what was heard count as rewrites and are left out of voice training (0–1).',
          )}
          <div class="flex flex-none items-center gap-2">
            <input
              class="input w-20 text-right tabular-nums"
              type="number"
              min="0"
              max="1"
              step="0.05"
              bind:value={maxEditDistance}
              aria-label="Maximum normalized edit distance"
            />
            <span class="w-6"></span>
          </div>
        </div>
      </div>
    </section>

    <!-- Save bar: only there when there's something to save ---------------- -->
    {#if dirty || saved}
      <div
        class="sticky bottom-5 flex items-center gap-3 rounded-xl border border-line bg-panel py-2.5 pr-2.5 pl-4 shadow-[var(--shadow-pop)] rise-in"
      >
        <span class="flex-1 text-[13px]">
          {#if dirty}
            You have unsaved changes
          {:else}
            <span class="inline-flex items-center gap-2"><span class="dot dot-ok"></span>Saved</span>
          {/if}
        </span>
        {#if dirty}
          <button class="btn btn-ghost" onclick={() => stored && fill(stored)} disabled={saving}>
            Discard
          </button>
          <button class="btn btn-primary" onclick={() => void save()} disabled={saving}>
            {saving ? 'Saving…' : 'Save changes'}
          </button>
        {/if}
      </div>
    {/if}
  {/if}
</div>
