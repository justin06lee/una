<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type Stats } from '../api';
  import AppBars from '../lib/AppBars.svelte';
  import DayBars from '../lib/DayBars.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Heatmap from '../lib/Heatmap.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import StatTile from '../lib/StatTile.svelte';
  import WerChart from '../lib/WerChart.svelte';
  import { fmtCompact, fmtNum, fmtSpan } from '../lib/format';

  const TYPING_WPM = 40;

  let stats = $state<Stats | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  onMount(() => {
    void (async () => {
      try {
        stats = await api.stats();
      } catch (e) {
        error = errMsg(e);
      } finally {
        loading = false;
      }
    })();
  });

  /** Words this calendar month vs the same stretch of last month. */
  const monthDelta = $derived.by(() => {
    if (!stats) return null;
    const now = new Date();
    const thisMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;
    const prev = new Date(now.getFullYear(), now.getMonth() - 1, 1);
    const lastMonth = `${prev.getFullYear()}-${String(prev.getMonth() + 1).padStart(2, '0')}`;

    let current = 0;
    let previous = 0;
    for (const day of stats.per_day) {
      if (day.day.startsWith(thisMonth)) current += day.words;
      else if (day.day.startsWith(lastMonth)) previous += day.words;
    }
    if (previous === 0) return current > 0 ? null : null;
    return Math.round(((current - previous) / previous) * 100);
  });

  const reviewShare = $derived.by(() => {
    if (!stats) return 0;
    const total = stats.review.backlog + stats.review.reviewed;
    return total > 0 ? stats.review.reviewed / total : 0;
  });
</script>

<div class="mx-auto max-w-4xl px-8 py-10">
  <header class="mb-7">
    <h1 class="text-[20px] font-semibold tracking-tight">Insights</h1>
    <p class="mt-1 text-[13px] text-muted">How much you've said, where it went, and how una is doing.</p>
  </header>

  {#if error}
    <div
      class="mb-4 rounded-xl border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--c-danger) 28%, transparent); background: var(--c-danger-soft); color: var(--c-danger)"
    >
      {error}
    </div>
  {/if}

  {#if loading}
    <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
      {#each [0, 1, 2, 3] as i (i)}<Skeleton class="h-28 w-full" />{/each}
    </div>
  {:else if stats}
    <!-- Tiles ------------------------------------------------------------ -->
    <div class="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
      <StatTile
        label="Speaking speed"
        value={`${Math.round(stats.totals.avg_wpm)}`}
        hint={stats.totals.avg_wpm > 0
          ? `about ${(stats.totals.avg_wpm / TYPING_WPM).toFixed(1)}× typing`
          : 'no audio yet'}
        tone="accent"
      />
      <StatTile
        label="Words dictated"
        value={fmtCompact(stats.totals.words)}
        hint={monthDelta === null
          ? `over ${fmtSpan(stats.totals.ms)}`
          : `${monthDelta >= 0 ? '+' : ''}${monthDelta}% this month`}
      />
      <StatTile
        label="Cleaned up by una"
        value={fmtCompact(stats.cleanup.words_removed)}
        hint={`filler removed · ${fmtNum(stats.cleanup.dictionary_hits)} dictionary hits`}
      />
      <StatTile
        label="Streak"
        value={`${stats.streak.current}`}
        hint={`longest ${stats.streak.longest} · ${stats.totals.days_active} active days`}
      />
    </div>

    <!-- Where the words go ----------------------------------------------- -->
    <div class="card mb-4 p-6">
      <h2 class="mb-1 text-[14px] font-semibold">Where your words go</h2>
      <p class="mb-5 text-[12px] text-muted">
        The apps you dictate into. una adapts its tone to each one.
      </p>
      {#if stats.by_app.length === 0}
        <EmptyState title="Nothing yet" sub="Your apps show up here once you start dictating." />
      {:else}
        <AppBars rows={stats.by_app} />
      {/if}
    </div>

    <!-- Activity --------------------------------------------------------- -->
    <div class="card mb-4 p-6">
      <div class="mb-5 flex items-baseline justify-between gap-4">
        <div>
          <h2 class="text-[14px] font-semibold">Activity</h2>
          <p class="mt-1 text-[12px] text-muted">Shaded by words dictated each day.</p>
        </div>
        {#if stats.streak.current > 0}
          <span class="chip chip-accent">
            <span class="chip-dot"></span>
            {stats.streak.current} day streak
          </span>
        {/if}
      </div>
      <Heatmap days={stats.per_day} />
    </div>

    <!-- Per-day ---------------------------------------------------------- -->
    <div class="card mb-4 p-6">
      <h2 class="mb-5 text-[14px] font-semibold">Words per day</h2>
      <DayBars days={stats.per_day} />
    </div>

    <!-- Review progress -------------------------------------------------- -->
    <div class="card mb-4 p-6">
      <div class="mb-4 flex items-baseline justify-between gap-4">
        <div>
          <h2 class="text-[14px] font-semibold">Review progress</h2>
          <p class="mt-1 text-[12px] text-muted">
            Reviewed dictations are what the fine-tune actually learns from.
          </p>
        </div>
        <span class="text-[13px] tabular-nums">
          <span class="font-semibold">{fmtNum(stats.review.reviewed)}</span>
          <span class="text-faint">/ {fmtNum(stats.review.reviewed + stats.review.backlog)}</span>
        </span>
      </div>
      <div class="h-2 overflow-hidden rounded-full bg-raised">
        <div
          class="h-full rounded-full transition-[width] duration-500"
          style="width: {reviewShare * 100}%; background: var(--c-accent)"
        ></div>
      </div>
      <div class="mt-2 flex items-center justify-between text-[11px] text-faint">
        <span>{fmtNum(stats.review.eligible)} eligible training pairs</span>
        {#if stats.review.backlog > 0}
          <a href="#/review" class="font-semibold" style="color: var(--c-accent)">
            Review {stats.review.backlog} waiting →
          </a>
        {/if}
      </div>
    </div>

    <!-- Accuracy --------------------------------------------------------- -->
    <div class="card p-6">
      <h2 class="mb-1 text-[14px] font-semibold">Word error rate by training run</h2>
      <p class="mb-4 text-[12px] text-muted">
        Each fine-tune is scored against the model it would replace, on held-out audio.
      </p>
      <WerChart series={stats.wer_series} />
    </div>
  {/if}
</div>
