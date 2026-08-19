<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type Stats } from '../api';
  import BarChart from '../lib/BarChart.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import WerChart from '../lib/WerChart.svelte';

  let stats = $state<Stats | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      stats = await api.stats();
    } catch (e) {
      error = errMsg(e);
    } finally {
      loading = false;
    }
  });

  const totalDictations = $derived(
    stats ? stats.per_day.reduce((acc, d) => acc + d.n, 0) : 0,
  );
  const totalMinutes = $derived(
    stats ? stats.per_day.reduce((acc, d) => acc + (d.ms ?? 0), 0) / 60000 : 0,
  );
  const werPoints = $derived(stats?.wer_series ?? []);
</script>

<div class="mx-auto max-w-5xl px-6 py-8">
  <header class="mb-6">
    <h1 class="text-base font-semibold tracking-tight">Stats</h1>
    <p class="mt-0.5 text-[13px] text-muted">Usage and model quality over time.</p>
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
    <div class="mb-6 grid gap-4 sm:grid-cols-3">
      {#each Array(3) as _, i (i)}
        <Skeleton class="h-24 w-full" />
      {/each}
    </div>
    <Skeleton class="h-56 w-full" />
  {:else if stats}
    <div class="mb-6 grid gap-4 sm:grid-cols-3">
      <div class="card p-4">
        <div class="text-xs text-muted">Dictations · last 60 days</div>
        <div class="mt-1.5 text-2xl font-semibold">{totalDictations.toLocaleString()}</div>
      </div>
      <div class="card p-4">
        <div class="text-xs text-muted">Minutes dictated · last 60 days</div>
        <div class="mt-1.5 text-2xl font-semibold">
          {totalMinutes.toLocaleString(undefined, { maximumFractionDigits: 1 })}
        </div>
      </div>
      <div class="card p-4">
        <div class="text-xs text-muted">Review backlog</div>
        <div class="mt-1.5 flex items-baseline gap-2">
          <span class="text-2xl font-semibold">{stats.review_backlog.toLocaleString()}</span>
          {#if stats.review_backlog > 0}
            <a href="#/review" class="text-xs font-medium" style="color: var(--color-accent)">
              Review →
            </a>
          {/if}
        </div>
      </div>
    </div>

    <div class="card mb-6 p-5">
      <h2 class="mb-4 text-[13px] font-semibold">Dictations per day</h2>
      {#if totalDictations === 0}
        <EmptyState title="No dictations yet" sub="Charts appear once you start dictating." />
      {:else}
        <BarChart data={stats.per_day} />
      {/if}
    </div>

    <div class="card p-5">
      <h2 class="mb-1 text-[13px] font-semibold">Word error rate by training run</h2>
      <p class="mb-4 text-xs text-muted">
        Baseline is the serving model at run time; candidate is the freshly trained one. Lower is
        better.
      </p>
      {#if werPoints.length === 0}
        <EmptyState
          title="No evaluated runs yet"
          sub="Finish a training run to see baseline vs candidate WER."
        />
      {:else}
        <WerChart series={werPoints} />
      {/if}
    </div>
  {/if}
</div>
