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
  import Banner from '../lib/Banner.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Icon from '../lib/Icon.svelte';
  import PageHeader from '../lib/PageHeader.svelte';
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

<div class="mx-auto max-w-[58rem] px-10 py-12">
  <PageHeader
    title="Training"
    sub="Fine-tune una on your voice and your writing. A new model only replaces the one you're using if it measurably beats it."
  />

  {#if error}
    <Banner message={error} onretry={() => void refreshAll()} />
  {/if}

  <!-- Readiness ---------------------------------------------------------- -->
  <div class="mb-6 grid gap-px overflow-hidden rounded-xl border border-line bg-line md:grid-cols-2">
    {#if loading}
      <div class="bg-panel p-6"><Skeleton class="h-28 w-full" /></div>
      <div class="bg-panel p-6"><Skeleton class="h-28 w-full" /></div>
    {:else if elig}
      <!-- Voice -->
      <div class="flex flex-col bg-panel p-6">
        <div class="flex items-center gap-2">
          <Icon name="mic" size={15} class="text-muted" />
          <h2 class="text-[14px] font-medium">Your voice</h2>
          {#if elig.ready}<span class="chip ml-auto"><span class="dot dot-ok"></span>Ready</span>{/if}
        </div>
        <p class="mt-1 text-[13px] text-muted">Whisper, fine-tuned on the dictations you've reviewed.</p>
        <div class="mt-6 flex items-baseline gap-1.5">
          <span class="text-[28px] leading-none font-semibold tracking-[-0.03em] tabular-nums">
            {elig.eligible_minutes.toFixed(1)}
          </span>
          <span class="text-[13px] text-muted">of {elig.threshold_minutes.toFixed(0)} minutes</span>
        </div>
        <div class="mt-3">
          <ProgressBar
            value={elig.eligible_minutes / Math.max(1e-9, elig.threshold_minutes)}
            label="Voice training data"
          />
        </div>
        <div class="mt-auto flex items-center justify-between gap-3 pt-5">
          <span class="text-[12px] text-faint tabular-nums">{elig.eligible_pairs} pairs collected</span>
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
      <div class="flex flex-col bg-panel p-6">
        <div class="flex items-center gap-2">
          <Icon name="pencil" size={14} class="text-muted" />
          <h2 class="text-[14px] font-medium">Your style</h2>
          {#if elig.style_ready}<span class="chip ml-auto"><span class="dot dot-ok"></span>Ready</span>{/if}
        </div>
        <p class="mt-1 text-[13px] text-muted">The cleanup model, tuned on how you'd have written it.</p>
        <div class="mt-6 flex items-baseline gap-1.5">
          <span class="text-[28px] leading-none font-semibold tracking-[-0.03em] tabular-nums">
            {elig.style_pairs}
          </span>
          <span class="text-[13px] text-muted">of {elig.style_threshold_pairs} pairs</span>
        </div>
        <div class="mt-3">
          <ProgressBar
            value={elig.style_pairs / Math.max(1, elig.style_threshold_pairs)}
            label="Style training data"
          />
        </div>
        <div class="mt-auto flex items-center justify-between gap-3 pt-5">
          <span class="text-[12px] text-faint">
            {elig.style_pairs === 0 ? 'Fix pasted text, or rewrite in Review' : 'From your edits and Review'}
          </span>
          <button
            class="btn btn-sm {elig.style_ready ? 'btn-primary' : ''}"
            onclick={() => void start('style')}
            disabled={starting || hasActive || elig.style_pairs === 0}
            title={hasActive
              ? 'A run is already going'
              : elig.style_pairs === 0
                ? 'Fix a few pasted dictations, or rewrite them in Review, to collect style pairs'
                : 'Fine-tune the cleanup model on your edits and rewrites'}
          >
            Train my style
          </button>
        </div>
      </div>
    {/if}
  </div>

  {#if startError}
    <p class="-mt-3 mb-6 text-[12.5px] text-warn">{startError}</p>
  {/if}

  <!-- Active run ---------------------------------------------------------- -->
  {#if activeRun}
    <div class="panel mb-6 overflow-hidden shadow-[var(--shadow-sm)] rise-in">
      <div class="flex flex-wrap items-center gap-3 px-5 pt-4 pb-3.5">
        <StatusChip status={activeRun.status} />
        <span class="text-[14px] font-medium">
          {activeRun.kind === 'style' ? 'Style' : 'Voice'} run
          <span class="font-mono text-[12.5px] text-faint">{shortId(activeRun.id)}</span>
        </span>
        <span class="text-[12px] text-faint">started {fmtDate(activeRun.started_at)}</span>
        <div class="ml-auto flex items-center gap-1.5">
          <button class="btn btn-sm btn-ghost" onclick={() => void toggleLog()} aria-expanded={logOpen}>
            <Icon name="terminal" size={13} />
            {logOpen ? 'Hide log' : 'Log'}
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
        <div class="flex-1"><ProgressBar value={activeRun.progress} label="Run progress" /></div>
        <span class="w-10 text-right text-[12.5px] text-muted tabular-nums">
          {Math.round(activeRun.progress * 100)}%
        </span>
      </div>
      {#if logOpen}
        <pre
          bind:this={logEl}
          class="max-h-72 overflow-y-auto border-t border-line bg-subtle px-5 py-3.5 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-muted">{log ||
            'Waiting for log output…'}</pre>
      {/if}
    </div>
  {/if}

  <!-- Runs ---------------------------------------------------------------- -->
  <section class="mb-10">
    <h2 class="mb-3 text-[14px] font-medium">Runs</h2>
    <div class="panel overflow-x-auto">
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
            <tr class="border-b border-line">
              <th class="th">Run</th>
              <th class="th">Status</th>
              <th class="th">Result</th>
              <th class="th !text-right">Train / eval</th>
              <th class="th !text-right">Finished</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-line">
            {#each runs as run (run.id)}
              <tr class="transition-colors duration-150 hover:bg-subtle">
                <td class="td">
                  <div class="flex items-center gap-2">
                    <span class="text-[13px]">{run.kind === 'style' ? 'Style' : 'Voice'}</span>
                    <span class="font-mono text-[12px] text-faint">{shortId(run.id)}</span>
                  </div>
                </td>
                <td class="td">
                  <StatusChip status={run.status} />
                  {#if run.error}
                    <div class="mt-1 max-w-64 truncate text-[12px] text-danger" title={run.error}>
                      {run.error}
                    </div>
                  {/if}
                </td>
                <td class="td text-[13px] tabular-nums">
                  {#if run.wer_baseline !== null && run.wer_candidate !== null}
                    {@const better = run.wer_candidate <= run.wer_baseline}
                    {#if run.kind === 'style'}
                      <!-- style metric: mean edit distance to the polished target -->
                      <span
                        class="text-muted"
                        title="Mean edit distance to your rewrites (lower is better)"
                      >
                        {run.wer_baseline.toFixed(3)} → <span class="text-fg">{run.wer_candidate.toFixed(3)}</span>
                      </span>
                    {:else}
                      <span class="text-muted">
                        {fmtWer(run.wer_baseline)} → <span class="text-fg">{fmtWer(run.wer_candidate)}</span>
                      </span>
                      <!-- green only when the gain was real enough to promote -->
                      <span
                        class="ml-1.5 text-[12px] {!better
                          ? 'text-danger'
                          : run.status === 'promoted'
                            ? 'text-ok'
                            : 'text-faint'}"
                      >
                        {fmtWerDelta(run.wer_baseline, run.wer_candidate)}
                      </span>
                    {/if}
                  {:else}
                    <span class="text-faint">—</span>
                  {/if}
                </td>
                <td class="td text-right text-[13px] text-muted tabular-nums">
                  {run.n_train ?? '—'} / {run.n_eval ?? '—'}
                </td>
                <td class="td text-right text-[12.5px] text-muted">
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
    <h2 class="mb-3 text-[14px] font-medium">Models</h2>
    <div class="panel overflow-x-auto">
      {#if loading}
        <div class="space-y-2 p-5"><Skeleton class="h-5 w-full" /><Skeleton class="h-5 w-full" /></div>
      {:else if models.length === 0}
        <EmptyState title="No models yet" sub="The base model appears after the server's first launch." />
      {:else}
        <table class="w-full min-w-[680px] text-[13px]">
          <thead>
            <tr class="border-b border-line">
              <th class="th">Model</th>
              <th class="th">Kind</th>
              <th class="th">Error rate</th>
              <th class="th">Created</th>
              <th class="th w-52 !text-right"><span class="sr-only">Actions</span></th>
            </tr>
          </thead>
          <tbody class="divide-y divide-line">
            {#each models as model (model.id)}
              <tr class="transition-colors duration-150 hover:bg-subtle">
                <td class="td max-w-72 truncate font-mono text-[12px]" title={model.id}>
                  {model.id}
                </td>
                <td class="td text-muted">{model.kind}</td>
                <td class="td text-[13px] tabular-nums" title="Word error rate on held-out audio">
                  {fmtWer(model.eval_wer)}
                </td>
                <td class="td text-[12.5px] text-muted">{fmtDate(model.created_at)}</td>
                <td class="td text-right whitespace-nowrap">
                  {#if model.is_active}
                    <span class="chip"><span class="dot dot-ok"></span>In use</span>
                  {:else if confirmActivate === model.id}
                    <button class="btn btn-sm btn-ghost" onclick={() => (confirmActivate = null)}>
                      Cancel
                    </button>
                    <button
                      class="btn btn-sm btn-primary"
                      onclick={() => void activate(model.id)}
                      disabled={activating}
                    >
                      {activating ? 'Switching…' : 'Switch to this'}
                    </button>
                  {:else}
                    <button class="btn btn-sm" onclick={() => (confirmActivate = model.id)}>
                      Use this model
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
