<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type Stats } from '../api';
  import AppBars from '../lib/AppBars.svelte';
  import Banner from '../lib/Banner.svelte';
  import DayBars from '../lib/DayBars.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Heatmap from '../lib/Heatmap.svelte';
  import Icon from '../lib/Icon.svelte';
  import Metric from '../lib/Metric.svelte';
  import PageHeader from '../lib/PageHeader.svelte';
  import ProgressBar from '../lib/ProgressBar.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
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
    if (previous === 0) return null;
    return Math.round(((current - previous) / previous) * 100);
  });

  const reviewShare = $derived.by(() => {
    if (!stats) return 0;
    const total = stats.review.backlog + stats.review.reviewed;
    return total > 0 ? stats.review.reviewed / total : 0;
  });
</script>

<div class="mx-auto max-w-[58rem] px-10 py-12">
  <PageHeader title="Insights" sub="How much you've said, where it went, and how una is getting on." />

  {#if error}
    <Banner message={error} />
  {/if}

  {#if loading}
    <Skeleton class="mb-6 h-28 w-full" />
    <Skeleton class="h-48 w-full" />
  {:else if stats}
    <!-- Headline numbers ------------------------------------------------- -->
    <div class="mb-6 grid grid-cols-2 gap-px overflow-hidden rounded-xl border border-line bg-line lg:grid-cols-4">
      <div class="bg-panel p-5">
        <Metric
          label="Speaking speed"
          value={`${Math.round(stats.totals.avg_wpm)}`}
          unit="wpm"
          hint={stats.totals.avg_wpm > 0
            ? `About ${(stats.totals.avg_wpm / TYPING_WPM).toFixed(1)}× your typing`
            : 'No audio yet'}
        />
      </div>
      <div class="bg-panel p-5">
        <Metric
          label="Words dictated"
          value={fmtCompact(stats.totals.words)}
          hint={monthDelta === null
            ? `Over ${fmtSpan(stats.totals.ms)} of audio`
            : `${monthDelta >= 0 ? '+' : ''}${monthDelta}% on last month`}
        />
      </div>
      <div class="bg-panel p-5">
        <Metric
          label="Cleaned up"
          value={fmtCompact(stats.cleanup.words_removed)}
          unit="words"
          hint={`${fmtNum(stats.cleanup.dictionary_hits)} dictionary hits`}
        />
      </div>
      <div class="bg-panel p-5">
        <Metric
          label="Streak"
          value={`${stats.streak.current}`}
          unit={stats.streak.current === 1 ? 'day' : 'days'}
          hint={`Longest ${stats.streak.longest} · ${stats.totals.days_active} days active`}
        />
      </div>
    </div>

    <!-- Activity --------------------------------------------------------- -->
    <section class="panel mb-6 p-6">
      <div class="mb-5 flex items-baseline justify-between gap-4">
        <div>
          <h2 class="text-[14px] font-medium">Activity</h2>
          <p class="mt-1 text-[13px] text-muted">Each square is a day, shaded by words dictated.</p>
        </div>
      </div>
      <Heatmap days={stats.per_day} />
    </section>

    <div class="mb-6 grid gap-6 lg:grid-cols-2">
      <!-- Per-day -------------------------------------------------------- -->
      <section class="panel p-6">
        <h2 class="text-[14px] font-medium">Last 30 days</h2>
        <p class="mt-1 mb-5 text-[13px] text-muted">Words per day.</p>
        <DayBars days={stats.per_day} />
      </section>

      <!-- Where the words go --------------------------------------------- -->
      <section class="panel p-6">
        <h2 class="text-[14px] font-medium">Where your words go</h2>
        <p class="mt-1 mb-5 text-[13px] text-muted">una adapts its tone to each app.</p>
        {#if stats.by_app.length === 0}
          <EmptyState title="Nothing yet" sub="Your apps show up here once you start dictating." />
        {:else}
          <AppBars rows={stats.by_app} />
        {/if}
      </section>
    </div>

    <!-- Review progress -------------------------------------------------- -->
    <section class="panel mb-6 p-6">
      <div class="mb-4 flex items-start justify-between gap-4">
        <div>
          <h2 class="text-[14px] font-medium">Reviewed</h2>
          <p class="mt-1 text-[13px] text-muted">
            Reviewed dictations are what the fine-tune actually learns from.
          </p>
        </div>
        <span class="text-[13px] tabular-nums">
          {fmtNum(stats.review.reviewed)}
          <span class="text-faint">/ {fmtNum(stats.review.reviewed + stats.review.backlog)}</span>
        </span>
      </div>
      <ProgressBar value={reviewShare} label="Share of dictations reviewed" />
      <div class="mt-3 flex items-center justify-between text-[12px] text-faint">
        <span>{fmtNum(stats.review.eligible)} training pairs</span>
        {#if stats.review.backlog > 0}
          <a href="#/review" class="inline-flex items-center gap-1 text-muted hover:text-fg">
            Review {stats.review.backlog} more <Icon name="arrowRight" size={12} />
          </a>
        {/if}
      </div>
    </section>

    <!-- Accuracy --------------------------------------------------------- -->
    <section class="panel p-6">
      <h2 class="text-[14px] font-medium">Word error rate</h2>
      <p class="mt-1 mb-5 text-[13px] text-muted">
        Each fine-tune, scored against the model it would replace on audio it never trained on.
      </p>
      <WerChart series={stats.wer_series} />
    </section>
  {/if}
</div>
