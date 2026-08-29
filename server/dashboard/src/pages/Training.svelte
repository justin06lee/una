<script lang="ts">
  import { onMount } from 'svelte';

  import {
    ACTIVE_RUN_STATUSES,
    api,
    errMsg,
    UnaError,
    type Eligibility,
    type ModelInfo,
    type RunKind,
    type TrainingRun,
  } from '../api';
  import EmptyState from '../lib/EmptyState.svelte';
  import ProgressBar from '../lib/ProgressBar.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import StatusChip from '../lib/StatusChip.svelte';
  import { fmtDate, fmtWer, fmtWerDelta, shortId } from '../lib/format';

  let runs = $state<TrainingRun[]>([]);
  let models = $state<ModelInfo[]>([]);
  let elig = $state<Eligibility | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let startError = $state<string | null>(null);
  let starting = $state(false);
  let cancelling = $state(false);

  let logOpen = $state(false);
  let log = $state('');
  let logEl = $state<HTMLPreElement>();

  let confirmActivate = $state<string | null>(null);
  let activating = $state(false);

  const activeRun = $derived(runs.find((r) => ACTIVE_RUN_STATUSES.includes(r.status)) ?? null);
  const hasActive = $derived(activeRun !== null);

  let wasActive = false;

  onMount(() => void refreshAll());

  async function refreshAll(): Promise<void> {
    error = null;
    try {
      const [r, m, e] = await Promise.all([
        api.listTrainingRuns(),
        api.listModels(),
        api.trainingEligibility(),
      ]);
      runs = r;
      models = m;
      elig = e;
      wasActive = r.some((run) => ACTIVE_RUN_STATUSES.includes(run.status));
    } catch (e) {
      error = errMsg(e);
    } finally {
      loading = false;
    }
  }

  // Poll every 3s while a run is active.
  $effect(() => {
    if (!hasActive) return;
    const t = setInterval(() => void pollActive(), 3000);
    return () => clearInterval(t);
  });

  async function pollActive(): Promise<void> {
    try {
      const r = await api.listTrainingRuns();
      runs = r;
      const active = r.find((run) => ACTIVE_RUN_STATUSES.includes(run.status)) ?? null;
      if (active && logOpen) log = await api.trainingRunLog(active.id, 200);
      if (wasActive && !active) {
        const [m, e] = await Promise.all([api.listModels(), api.trainingEligibility()]);
        models = m;
        elig = e;
      }
      wasActive = active !== null;
    } catch {
      /* transient poll failure; next tick retries */
    }
  }

  // Keep the log pinned to the bottom as it grows.
  $effect(() => {
    void log;
    const el = logEl;
    if (el) el.scrollTop = el.scrollHeight;
  });

  async function start(kind: RunKind = 'asr'): Promise<void> {
    if (starting) return;
    starting = true;
    startError = null;
    try {
      await api.startTrainingRun(kind);
      await refreshAll();
    } catch (e) {
      if (e instanceof UnaError && e.code === 'RUN_ACTIVE') {
        startError = 'A training run is already going.';
        await refreshAll();
      } else {
        startError = errMsg(e);
      }
    } finally {
      starting = false;
    }
  }

  async function cancel(id: string): Promise<void> {
    if (cancelling) return;
    cancelling = true;
    try {
      await api.cancelTrainingRun(id);
      await refreshAll();
    } catch (e) {
      error = errMsg(e);
    } finally {
      cancelling = false;
    }
  }

  async function toggleLog(): Promise<void> {
    logOpen = !logOpen;
    if (logOpen && activeRun) {
      try {
        log = await api.trainingRunLog(activeRun.id, 200);
      } catch {
        log = '';
      }
    }
  }

  async function activate(id: string): Promise<void> {
    if (activating) return;
    activating = true;
    error = null;
    try {
      await api.activateModel(id);
      models = await api.listModels();
      confirmActivate = null;
    } catch (e) {
      error = errMsg(e);
    } finally {
      activating = false;
    }
  }
</script>

<div class="mx-auto max-w-5xl px-8 py-10">
  <header class="mb-7">
    <h1 class="text-[20px] font-semibold tracking-tight">Training</h1>
    <p class="mt-1 text-[13px] text-muted">
      Fine-tune una on your own voice and your own writing style. A candidate only replaces the
      model you're using if it measurably beats it.
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

  <!-- Readiness ---------------------------------------------------------- -->
  <div class="card mb-4 grid gap-6 p-6 md:grid-cols-2">
    {#if loading}
      <Skeleton class="h-24 w-full" />
      <Skeleton class="h-24 w-full" />
    {:else if elig}
      <!-- Voice -->
      <div>
        <div class="mb-2 flex items-baseline justify-between gap-3">
          <div>
            <h2 class="text-[14px] font-semibold">Your voice</h2>
            <p class="mt-0.5 text-[12px] text-muted">Whisper, fine-tuned on your corrections.</p>
          </div>
          <span class="text-[13px] tabular-nums whitespace-nowrap">
            <span class="font-semibold">{elig.eligible_minutes.toFixed(1)}</span>
            <span class="text-faint">/ {elig.threshold_minutes.toFixed(0)} min</span>
          </span>
        </div>
        <ProgressBar value={elig.eligible_minutes / Math.max(1e-9, elig.threshold_minutes)} />
        <div class="mt-2 flex items-center justify-between gap-3">
          <span class="text-[11px] text-faint tabular-nums">{elig.eligible_pairs} pairs ready</span>
          <button
            class="btn btn-sm {elig.ready ? 'btn-primary' : ''}"
            onclick={() => void start('asr')}
            disabled={starting || hasActive}
            title={hasActive ? 'A run is already going' : undefined}
          >
            {starting ? 'Starting…' : 'Train on my voice'}
          </button>
        </div>
      </div>

      <!-- Style -->
      <div class="md:border-l md:border-border md:pl-6">
        <div class="mb-2 flex items-baseline justify-between gap-3">
          <div>
            <h2 class="text-[14px] font-semibold">Your style</h2>
            <p class="mt-0.5 text-[12px] text-muted">
              The cleanup model, tuned on your <em>Final text</em>.
            </p>
          </div>
          <span class="text-[13px] tabular-nums whitespace-nowrap">
            <span class="font-semibold">{elig.style_pairs}</span>
            <span class="text-faint">/ {elig.style_threshold_pairs}</span>
          </span>
        </div>
        <ProgressBar value={elig.style_pairs / Math.max(1, elig.style_threshold_pairs)} />
        <div class="mt-2 flex items-center justify-between gap-3">
          <span class="text-[11px] text-faint">
            {elig.style_pairs === 0 ? 'Add Final text while reviewing' : 'polished pairs collected'}
          </span>
          <button
            class="btn btn-sm {elig.style_ready ? 'btn-primary' : ''}"
            onclick={() => void start('style')}
            disabled={starting || hasActive || elig.style_pairs === 0}
            title={hasActive
              ? 'A run is already going'
              : elig.style_pairs === 0
                ? 'Add Final text in Review to collect style pairs'
                : 'Fine-tune the cleanup model on your polished texts'}
          >
            Train my style
          </button>
        </div>
      </div>

      {#if startError}
        <p class="text-[12px] md:col-span-2" style="color: var(--c-warn)">{startError}</p>
      {/if}
    {/if}
  </div>

  <!-- Active run ---------------------------------------------------------- -->
  {#if activeRun}
    <div
      class="card mb-4 overflow-hidden rise-in"
      style="border-color: color-mix(in oklab, var(--c-accent) 40%, var(--c-border))"
    >
      <div class="flex flex-wrap items-center gap-3 px-5 py-4">
        <StatusChip status={activeRun.status} />
        <span class="text-[13.5px] font-semibold">run {shortId(activeRun.id)}</span>
        {#if activeRun.kind === 'style'}<span class="chip chip-accent">style</span>{/if}
        <span class="text-[11px] text-faint">started {fmtDate(activeRun.started_at)}</span>
        <div class="ml-auto flex items-center gap-2">
          <button class="btn btn-sm" onclick={() => void toggleLog()} aria-expanded={logOpen}>
            {logOpen ? 'Hide log' : 'Live log'}
          </button>
          <button
            class="btn btn-sm btn-danger"
            onclick={() => void cancel(activeRun.id)}
            disabled={cancelling}
          >
            {cancelling ? 'Cancelling…' : 'Cancel'}
          </button>
        </div>
      </div>
      <div class="flex items-center gap-3 px-5 pb-4">
        <div class="flex-1"><ProgressBar value={activeRun.progress} /></div>
        <span class="w-10 text-right text-[12px] text-muted tabular-nums">
          {Math.round(activeRun.progress * 100)}%
        </span>
      </div>
      {#if logOpen}
        <pre
          bind:this={logEl}
          class="max-h-64 overflow-y-auto border-t border-border bg-canvas px-5 py-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-muted">{log ||
            'Waiting for log output…'}</pre>
      {/if}
    </div>
  {/if}

  <!-- Runs ---------------------------------------------------------------- -->
  <section class="mb-6">
    <h2 class="mb-2 px-1 text-[12px] font-semibold text-muted">Runs</h2>
    <div class="card overflow-x-auto">
      {#if loading}
        <div class="space-y-2 p-5"><Skeleton class="h-5 w-full" /><Skeleton class="h-5 w-full" /></div>
      {:else if runs.length === 0}
        <EmptyState
          title="No training runs yet"
          sub="Review dictations until you cross the threshold, then start a run."
        />
      {:else}
        <table class="w-full min-w-[680px] text-[13px]">
          <thead>
            <tr class="border-b border-border">
              <th class="th">Run</th>
              <th class="th">Status</th>
              <th class="th">Result</th>
              <th class="th !text-right">Train / eval</th>
              <th class="th">Finished</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border">
            {#each runs as run (run.id)}
              <tr class="transition-colors duration-150 hover:bg-raised/60">
                <td class="px-3 py-3">
                  <span class="font-mono text-[12px]">{shortId(run.id)}</span>
                  {#if run.kind === 'style'}<span class="chip chip-accent ml-1.5">style</span>{/if}
                  <span class="ml-2 text-[11px] text-faint">{fmtDate(run.started_at)}</span>
                </td>
                <td class="px-3 py-3">
                  <StatusChip status={run.status} />
                  {#if run.error}
                    <div
                      class="mt-1 max-w-64 truncate text-[11px]"
                      style="color: var(--c-danger)"
                      title={run.error}
                    >
                      {run.error}
                    </div>
                  {/if}
                </td>
                <td class="px-3 py-3 tabular-nums">
                  {#if run.wer_baseline !== null && run.wer_candidate !== null}
                    {#if run.kind === 'style'}
                      <!-- style metric: mean edit distance to the polished target -->
                      <span class="text-muted">{run.wer_baseline.toFixed(3)}</span>
                      <span class="text-faint">→</span>
                      <span>{run.wer_candidate.toFixed(3)}</span>
                      <span
                        class="ml-1 text-[11px]"
                        style="color: var(--c-{run.wer_candidate <= run.wer_baseline
                          ? 'accent'
                          : 'danger'})"
                        title="mean edit distance to your polished targets (lower is better)"
                      >
                        dist
                      </span>
                    {:else}
                      <span class="text-muted">{fmtWer(run.wer_baseline)}</span>
                      <span class="text-faint">→</span>
                      <span>{fmtWer(run.wer_candidate)}</span>
                      <span
                        class="ml-1 text-[11px]"
                        style="color: var(--c-{run.wer_candidate <= run.wer_baseline
                          ? 'accent'
                          : 'danger'})"
                      >
                        {fmtWerDelta(run.wer_baseline, run.wer_candidate)}
                      </span>
                    {/if}
                  {:else}
                    <span class="text-faint">—</span>
                  {/if}
                </td>
                <td class="px-3 py-3 text-right text-muted tabular-nums">
                  {run.n_train ?? '—'} / {run.n_eval ?? '—'}
                </td>
                <td class="px-3 py-3 text-[11px] text-muted">
                  {run.finished_at ? fmtDate(run.finished_at) : '—'}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </section>

  <!-- Models --------------------------------------------------------------- -->
  <section>
    <h2 class="mb-2 px-1 text-[12px] font-semibold text-muted">Models</h2>
    <div class="card overflow-x-auto">
      {#if loading}
        <div class="space-y-2 p-5"><Skeleton class="h-5 w-full" /><Skeleton class="h-5 w-full" /></div>
      {:else if models.length === 0}
        <EmptyState title="No models registered" sub="The base model appears after first launch." />
      {:else}
        <table class="w-full min-w-[680px] text-[13px]">
          <thead>
            <tr class="border-b border-border">
              <th class="th">Model</th>
              <th class="th">Kind</th>
              <th class="th">Accuracy</th>
              <th class="th">Created</th>
              <th class="th w-48 !text-right"><span class="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border">
            {#each models as model (model.id)}
              <tr
                class="transition-colors duration-150 hover:bg-raised/60"
                class:bg-accent-soft={model.is_active}
              >
                <td class="max-w-64 truncate px-3 py-3 font-mono text-[12px]" title={model.id}>
                  {model.id}
                </td>
                <td class="px-3 py-3"><span class="chip">{model.kind}</span></td>
                <td class="px-3 py-3 tabular-nums" title="Word error rate on held-out audio">
                  {fmtWer(model.eval_wer)}
                </td>
                <td class="px-3 py-3 text-[11px] text-muted">{fmtDate(model.created_at)}</td>
                <td class="px-3 py-3 text-right whitespace-nowrap">
                  {#if model.is_active}
                    <StatusChip status="active" label="in use" />
                  {:else if confirmActivate === model.id}
                    <span class="mr-1.5 text-[11px] text-muted">Switch to this model?</span>
                    <button
                      class="btn btn-sm btn-primary"
                      onclick={() => void activate(model.id)}
                      disabled={activating}
                    >
                      {activating ? 'Switching…' : 'Switch'}
                    </button>
                    <button class="btn btn-sm btn-ghost" onclick={() => (confirmActivate = null)}>
                      Cancel
                    </button>
                  {:else}
                    <button class="btn btn-sm" onclick={() => (confirmActivate = model.id)}>
                      Use this one
                    </button>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </section>
</div>
