<script lang="ts">
  import { onMount } from 'svelte';

  import {
    ACTIVE_RUN_STATUSES,
    api,
    errMsg,
    UnaError,
    type Eligibility,
    type ModelInfo,
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
      if (active && logOpen) {
        log = await api.trainingRunLog(active.id, 200);
      }
      if (wasActive && !active) {
        // Run just finished: refresh the registry and eligibility.
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

  async function start(): Promise<void> {
    if (starting) return;
    starting = true;
    startError = null;
    try {
      await api.startTrainingRun();
      await refreshAll();
    } catch (e) {
      if (e instanceof UnaError && e.code === 'RUN_ACTIVE') {
        startError = 'A training run is already active.';
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

<div class="mx-auto max-w-5xl px-6 py-8">
  <header class="mb-6">
    <h1 class="text-base font-semibold tracking-tight">Training</h1>
    <p class="mt-0.5 text-[13px] text-muted">
      Fine-tune the ASR model on your corrected dictations.
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

  <!-- Eligibility -->
  <div class="card mb-6 p-5">
    {#if loading}
      <Skeleton class="h-16 w-full" />
    {:else if elig}
      <div class="flex flex-wrap items-end justify-between gap-4">
        <div class="min-w-56 flex-1">
          <div class="mb-1.5 flex items-baseline justify-between text-xs">
            <span class="text-muted">Eligible training data</span>
            <span class="tabular-nums">
              <span class="font-medium text-text">{elig.eligible_minutes.toFixed(1)}</span>
              <span class="text-faint">/ {elig.threshold_minutes.toFixed(0)} min</span>
            </span>
          </div>
          <ProgressBar
            value={elig.eligible_minutes / Math.max(1e-9, elig.threshold_minutes)}
            tone={elig.ready ? 'ok' : 'accent'}
          />
          <div class="mt-1.5 text-[11px] text-faint tabular-nums">
            {elig.eligible_pairs} pairs{typeof elig.style_pairs === 'number'
              ? ` · ${elig.style_pairs} style pairs collected`
              : ''}{elig.ready ? ' · ready to train' : ''}
          </div>
        </div>
        <div class="flex flex-col items-end gap-1.5">
          <button
            class="btn {elig.ready ? 'btn-primary' : ''}"
            onclick={() => void start()}
            disabled={starting || hasActive}
            title={hasActive ? 'A run is already active' : undefined}
          >
            {starting ? 'Starting…' : 'Start training run'}
          </button>
          {#if startError}
            <span class="text-xs" style="color: var(--color-warn)">{startError}</span>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  <!-- Active run -->
  {#if activeRun}
    <div
      class="card mb-6 overflow-hidden"
      style="border-color: color-mix(in oklab, var(--color-accent) 35%, var(--color-border))"
    >
      <div class="flex items-center gap-3 px-5 py-4">
        <StatusChip status={activeRun.status} />
        <span class="text-sm font-medium">run {shortId(activeRun.id)}</span>
        <span class="text-xs text-faint">started {fmtDate(activeRun.started_at)}</span>
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
        <div class="flex-1">
          <ProgressBar value={activeRun.progress} />
        </div>
        <span class="w-10 text-right text-xs text-muted tabular-nums">
          {Math.round(activeRun.progress * 100)}%
        </span>
      </div>
      {#if logOpen}
        <pre
          bind:this={logEl}
          class="max-h-64 overflow-y-auto border-t border-border bg-bg px-5 py-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-muted">{log ||
            'Waiting for log output…'}</pre>
      {/if}
    </div>
  {/if}

  <!-- Runs -->
  <section class="mb-8">
    <h2 class="mb-2 text-[13px] font-semibold text-muted">Runs</h2>
    <div class="card overflow-x-auto">
      {#if loading}
        <div class="space-y-2 p-4">
          <Skeleton class="h-5 w-full" />
          <Skeleton class="h-5 w-full" />
        </div>
      {:else if runs.length === 0}
        <EmptyState
          title="No training runs yet"
          sub="Review dictations until you cross the threshold, then start a run."
        />
      {:else}
        <table class="w-full min-w-[680px] text-sm">
          <thead>
            <tr class="border-b border-border">
              <th class="th">Run</th>
              <th class="th">Status</th>
              <th class="th">WER</th>
              <th class="th !text-right">Train / eval</th>
              <th class="th">Finished</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border">
            {#each runs as run (run.id)}
              <tr class="transition-colors duration-150 hover:bg-hover/50">
                <td class="px-3 py-2.5">
                  <span class="font-mono text-xs">{shortId(run.id)}</span>
                  <span class="ml-2 text-xs text-faint">{fmtDate(run.started_at)}</span>
                </td>
                <td class="px-3 py-2.5">
                  <StatusChip status={run.status} />
                  {#if run.error}
                    <div
                      class="mt-1 max-w-64 truncate text-[11px]"
                      style="color: var(--color-danger)"
                      title={run.error}
                    >
                      {run.error}
                    </div>
                  {/if}
                </td>
                <td class="px-3 py-2.5 tabular-nums">
                  {#if run.wer_baseline !== null && run.wer_candidate !== null}
                    <span class="text-muted">{fmtWer(run.wer_baseline)}</span>
                    <span class="text-faint">→</span>
                    <span>{fmtWer(run.wer_candidate)}</span>
                    <span
                      class="ml-1 text-xs"
                      style="color: var(--color-{run.wer_candidate <= run.wer_baseline
                        ? 'ok'
                        : 'danger'})"
                    >
                      {fmtWerDelta(run.wer_baseline, run.wer_candidate)}
                    </span>
                  {:else}
                    <span class="text-faint">—</span>
                  {/if}
                </td>
                <td class="px-3 py-2.5 text-right text-muted tabular-nums">
                  {run.n_train ?? '—'} / {run.n_eval ?? '—'}
                </td>
                <td class="px-3 py-2.5 text-xs text-muted">
                  {run.finished_at ? fmtDate(run.finished_at) : '—'}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  </section>

  <!-- Models -->
  <section>
    <h2 class="mb-2 text-[13px] font-semibold text-muted">Model registry</h2>
    <div class="card overflow-x-auto">
      {#if loading}
        <div class="space-y-2 p-4">
          <Skeleton class="h-5 w-full" />
          <Skeleton class="h-5 w-full" />
        </div>
      {:else if models.length === 0}
        <EmptyState title="No models registered" sub="The base model appears after first launch." />
      {:else}
        <table class="w-full min-w-[680px] text-sm">
          <thead>
            <tr class="border-b border-border">
              <th class="th">Model</th>
              <th class="th">Kind</th>
              <th class="th">Eval WER</th>
              <th class="th">Created</th>
              <th class="th">Notes</th>
              <th class="th w-44 !text-right"><span class="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border">
            {#each models as model (model.id)}
              <tr
                class="transition-colors duration-150 hover:bg-hover/50"
                class:bg-raised={model.is_active}
              >
                <td class="max-w-64 truncate px-3 py-2.5 font-mono text-xs" title={model.id}>
                  {model.id}
                </td>
                <td class="px-3 py-2.5"><span class="chip">{model.kind}</span></td>
                <td class="px-3 py-2.5 tabular-nums">{fmtWer(model.eval_wer)}</td>
                <td class="px-3 py-2.5 text-xs text-muted">{fmtDate(model.created_at)}</td>
                <td class="max-w-48 truncate px-3 py-2.5 text-xs text-muted" title={model.notes ?? undefined}>
                  {model.notes ?? '—'}
                </td>
                <td class="px-3 py-2.5 text-right whitespace-nowrap">
                  {#if model.is_active}
                    <StatusChip status="active" label="serving" />
                  {:else if confirmActivate === model.id}
                    <span class="mr-1 text-xs text-muted">Swap serving model now?</span>
                    <button
                      class="btn btn-sm btn-primary"
                      onclick={() => void activate(model.id)}
                      disabled={activating}
                    >
                      {activating ? 'Swapping…' : 'Swap'}
                    </button>
                    <button class="btn btn-sm btn-ghost" onclick={() => (confirmActivate = null)}>
                      Cancel
                    </button>
                  {:else}
                    <button class="btn btn-sm" onclick={() => (confirmActivate = model.id)}>
                      Activate
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
