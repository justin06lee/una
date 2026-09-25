<script lang="ts">
  import { onMount } from 'svelte';

  import {
    api,
    errMsg,
    type DictationDetail,
    type DictationSummary,
    type Stats,
  } from '../api';
  import AudioPlayer from '../lib/AudioPlayer.svelte';
  import Banner from '../lib/Banner.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Icon from '../lib/Icon.svelte';
  import Metric from '../lib/Metric.svelte';
  import SearchInput from '../lib/SearchInput.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import { fmtClock, fmtDayHeading, fmtDur, fmtNum, fmtSpan, localDay } from '../lib/format';

  const PAGE = 40;
  /** Sustained typing speed used for the "saved vs typing" estimate. */
  const TYPING_WPM = 40;

  let items = $state<DictationSummary[]>([]);
  let stats = $state<Stats | null>(null);
  let nextCursor = $state<string | null>(null);
  let loading = $state(true);
  let loadingMore = $state(false);
  let error = $state<string | null>(null);

  let query = $state('');
  let appFilter = $state<string | null>(null);
  let reviewedFilter = $state<'all' | 'pending' | 'reviewed'>('all');

  let expanded = $state<string | null>(null);
  let detail = $state<DictationDetail | null>(null);
  let detailLoading = $state(false);
  let confirmDelete = $state<string | null>(null);
  let copied = $state<string | null>(null);

  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  function params(cursor?: string) {
    return {
      limit: PAGE,
      cursor,
      q: query.trim() || undefined,
      app: appFilter ?? undefined,
      reviewed:
        reviewedFilter === 'all' ? undefined : reviewedFilter === 'reviewed' ? true : false,
    };
  }

  async function load(cursor?: string): Promise<void> {
    if (cursor) loadingMore = true;
    else loading = true;
    error = null;
    try {
      const page = await api.listDictations(params(cursor));
      items = cursor ? [...items, ...page.items] : page.items;
      nextCursor = page.next_cursor;
    } catch (e) {
      error = errMsg(e);
    } finally {
      loading = false;
      loadingMore = false;
    }
  }

  async function loadStats(): Promise<void> {
    try {
      stats = await api.stats();
    } catch {
      /* the feed is the important part; tiles just stay empty */
    }
  }

  onMount(() => {
    void load();
    void loadStats();
    return () => {
      clearTimeout(searchTimer);
      clearTimeout(copyTimer);
    };
  });

  /** Debounced re-query whenever a filter changes. */
  function refilter(): void {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      expanded = null;
      void load();
    }, 220);
  }

  async function toggleRow(id: string): Promise<void> {
    if (expanded === id) {
      expanded = null;
      return;
    }
    expanded = id;
    detail = null;
    detailLoading = true;
    confirmDelete = null;
    try {
      detail = await api.getDictation(id);
    } catch (e) {
      error = errMsg(e);
    } finally {
      detailLoading = false;
    }
  }

  async function copy(text: string, id: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      copied = id;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = null), 1600);
    } catch {
      error = 'Could not copy to the clipboard';
    }
  }

  async function remove(id: string): Promise<void> {
    try {
      await api.deleteDictation(id);
      items = items.filter((i) => i.id !== id);
      expanded = null;
      confirmDelete = null;
      void loadStats();
    } catch (e) {
      error = errMsg(e);
    }
  }

  /** Feed grouped into Today / Yesterday / date sections, preserving order. */
  const groups = $derived.by(() => {
    const out: { day: string; rows: DictationSummary[] }[] = [];
    for (const item of items) {
      const day = localDay(item.created_at);
      const last = out[out.length - 1];
      if (last && last.day === day) last.rows.push(item);
      else out.push({ day, rows: [item] });
    }
    return out;
  });

  const savedMinutes = $derived.by(() => {
    if (!stats) return 0;
    const typing = stats.totals.words / TYPING_WPM;
    return Math.max(0, typing - stats.totals.ms / 60_000);
  });

  const filtering = $derived(
    query.trim() !== '' || appFilter !== null || reviewedFilter !== 'all',
  );
</script>

<div class="mx-auto max-w-[52rem] px-10 py-12">
  <!-- Hero -------------------------------------------------------------- -->
  <header class="mb-9 rise-in">
    {#if stats}
      <div class="flex flex-wrap items-end justify-between gap-x-8 gap-y-3">
        <h1 class="flex items-baseline gap-2.5 leading-none">
          <span class="text-[48px] font-semibold tracking-[-0.045em] tabular-nums">
            {fmtNum(stats.totals.words)}
          </span>
          <span class="text-[17px] text-muted">words dictated</span>
        </h1>
        <p class="flex items-center gap-1.5 pb-1 text-[12.5px] text-faint">
          <Icon name="mic" size={13} />
          Hold your hotkey anywhere to dictate
        </p>
      </div>
      <p class="mt-3 text-[13.5px] text-muted">
        {fmtNum(stats.totals.dictations)} dictations · {fmtSpan(stats.totals.ms)} of audio
        {#if savedMinutes > 1}
          · about <span class="text-fg">{fmtSpan(savedMinutes * 60_000)}</span> saved against typing
        {/if}
      </p>
    {:else}
      <Skeleton class="h-12 w-72" />
      <Skeleton class="mt-3 h-4 w-96" />
    {/if}
  </header>

  <!-- Stats ------------------------------------------------------------- -->
  <div class="mb-12 grid grid-cols-2 gap-px overflow-hidden rounded-xl border border-line bg-line sm:grid-cols-4">
    {#if stats}
      <div class="bg-panel p-5">
        <Metric
          label="Streak"
          value={`${stats.streak.current}`}
          unit={stats.streak.current === 1 ? 'day' : 'days'}
          hint={`Longest ${stats.streak.longest}`}
        />
      </div>
      <div class="bg-panel p-5">
        <Metric
          label="Speed"
          value={`${Math.round(stats.totals.avg_wpm)}`}
          unit="wpm"
          hint={`${(stats.totals.avg_wpm / TYPING_WPM).toFixed(1)}× your typing`}
        />
      </div>
      <div class="bg-panel p-5">
        <Metric
          label="Cleaned up"
          value={fmtNum(stats.cleanup.words_removed)}
          unit="words"
          hint="Filler removed"
        />
      </div>
      <div class="bg-panel p-5">
        <Metric label="To review" value={`${stats.review.backlog}`}>
          {#if stats.review.backlog > 0}
            <a href="#/review" class="mt-1.5 inline-flex items-center gap-1 text-[12px] text-muted hover:text-fg">
              Start reviewing <Icon name="arrowRight" size={12} />
            </a>
          {:else}
            <div class="mt-1.5 text-[12px] text-faint">All caught up</div>
          {/if}
        </Metric>
      </div>
    {:else}
      {#each [0, 1, 2, 3] as i (i)}
        <div class="bg-panel p-5"><Skeleton class="h-16 w-full" /></div>
      {/each}
    {/if}
  </div>

  <!-- Search + filters --------------------------------------------------- -->
  <div class="mb-6 flex flex-wrap items-center gap-2">
    <SearchInput
      bind:value={query}
      oninput={refilter}
      placeholder="Search your dictations…"
      label="Search transcripts"
    />

    {#if appFilter}
      <button
        type="button"
        class="btn btn-sm"
        onclick={() => {
          appFilter = null;
          refilter();
        }}
        title="Clear app filter"
      >
        {appFilter}
        <Icon name="x" size={12} class="text-muted" />
      </button>
    {/if}

    <div class="seg">
      {#each [{ k: 'all', l: 'All' }, { k: 'pending', l: 'Unreviewed' }, { k: 'reviewed', l: 'Reviewed' }] as f (f.k)}
        <button
          type="button"
          class="seg-item"
          class:active={reviewedFilter === f.k}
          onclick={() => {
            reviewedFilter = f.k as typeof reviewedFilter;
            refilter();
          }}
        >
          {f.l}
        </button>
      {/each}
    </div>
  </div>

  {#if error}
    <Banner message={error} onretry={() => void load()} />
  {/if}

  <!-- Feed --------------------------------------------------------------- -->
  {#if loading}
    <div class="space-y-2">
      {#each [0, 1, 2, 3, 4] as i (i)}
        <Skeleton class="h-16 w-full" />
      {/each}
    </div>
  {:else if items.length === 0}
    <div class="panel">
      <EmptyState
        title={filtering ? 'Nothing matches' : 'No dictations yet'}
        sub={filtering
          ? 'Try a different search, or clear the filters.'
          : 'Hold your hotkey anywhere and start talking. Everything you dictate shows up here.'}
      />
    </div>
  {:else}
    <div class="space-y-8">
      {#each groups as group (group.day)}
        <section>
          <h2 class="mb-2.5 flex items-baseline justify-between px-0.5 text-[12.5px]">
            <span class="font-medium">{fmtDayHeading(group.day)}</span>
            <span class="text-faint tabular-nums">
              {group.rows.length} dictation{group.rows.length === 1 ? '' : 's'}
            </span>
          </h2>
          <div class="panel divide-y divide-line overflow-hidden">
            {#each group.rows as item (item.id)}
              <div>
                <div
                  class="feed-row px-4 py-3.5"
                  role="button"
                  tabindex="0"
                  aria-expanded={expanded === item.id}
                  onclick={() => void toggleRow(item.id)}
                  onkeydown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      void toggleRow(item.id);
                    }
                  }}
                >
                  <div class="flex items-start gap-4">
                    <p class="min-w-0 flex-1 text-[14px] leading-relaxed" class:text-faint={!item.text}>
                      {item.text || 'Nothing was transcribed'}
                    </p>
                    <div class="flex flex-none items-center gap-1">
                      <div class="row-actions">
                        <button
                          class="btn btn-ghost btn-sm btn-icon"
                          title={copied === item.id ? 'Copied' : 'Copy text'}
                          aria-label="Copy text"
                          onclick={(e) => {
                            e.stopPropagation();
                            void copy(item.text, item.id);
                          }}
                        >
                          {#if copied === item.id}
                            <Icon name="check" size={14} class="text-ok" />
                          {:else}
                            <Icon name="copy" size={14} />
                          {/if}
                        </button>
                      </div>
                      <span class="w-10 text-right text-[12px] text-faint tabular-nums">
                        {fmtClock(item.created_at)}
                      </span>
                    </div>
                  </div>

                  <div class="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-faint">
                    {#if item.app_name}
                      <button
                        type="button"
                        class="hover:text-fg"
                        title="Show only {item.app_name}"
                        onclick={(e) => {
                          e.stopPropagation();
                          appFilter = item.app_name;
                          refilter();
                        }}
                      >
                        {item.app_name}
                      </button>
                      <span aria-hidden="true">·</span>
                    {/if}
                    <span class="tabular-nums">{fmtDur(item.duration_ms)}</span>
                    {#if !item.reviewed}
                      <span aria-hidden="true">·</span>
                      <span>Unreviewed</span>
                    {/if}
                  </div>
                </div>

                {#if expanded === item.id}
                  <div class="border-t border-line bg-subtle px-4 py-4 fade-in">
                    {#if detailLoading}
                      <Skeleton class="h-20 w-full" />
                    {:else if detail}
                      <AudioPlayer src={api.audioUrl(detail.id)} />

                      <div class="mt-5 grid gap-5 sm:grid-cols-2">
                        <div>
                          <div class="label mb-1.5">What una heard</div>
                          <p class="text-[13px] leading-relaxed">{detail.raw_text || '—'}</p>
                        </div>
                        {#if detail.cleaned_text}
                          <div>
                            <div class="label mb-1.5">What una pasted</div>
                            <p class="text-[13px] leading-relaxed">{detail.cleaned_text}</p>
                          </div>
                        {/if}
                      </div>

                      {#if detail.corrected_text}
                        <div class="mt-5">
                          <div class="label mb-1.5">Your correction</div>
                          <p class="text-[13px] leading-relaxed">{detail.corrected_text}</p>
                        </div>
                      {/if}

                      <div class="mt-5 flex flex-wrap items-center gap-1.5 border-t border-line pt-3.5">
                        {#if detail.training_eligible}
                          <span class="chip"><span class="dot dot-ok"></span>Training pair</span>
                        {:else if detail.eligibility_reason}
                          <span class="chip" title={detail.eligibility_reason}>
                            <span class="dot"></span>Not used for training
                          </span>
                        {/if}
                        {#if detail.eval_holdout}
                          <span class="chip" title="Held out of training to measure accuracy">
                            Held out
                          </span>
                        {/if}
                        {#if detail.asr_model}
                          <span class="chip font-mono !text-[10.5px]" title="Speech model">
                            {detail.asr_model}
                          </span>
                        {/if}
                        {#if detail.llm_model}
                          <span class="chip font-mono !text-[10.5px]" title="Cleanup model">
                            {detail.llm_model}
                          </span>
                        {/if}

                        <div class="ml-auto flex items-center gap-1.5">
                          {#if !item.reviewed}
                            <a href="#/review" class="btn btn-sm">Review</a>
                          {/if}
                          {#if confirmDelete === item.id}
                            <button class="btn btn-sm btn-ghost" onclick={() => (confirmDelete = null)}>
                              Cancel
                            </button>
                            <button
                              class="btn btn-sm !border-transparent !bg-[var(--danger)] !text-white"
                              onclick={() => void remove(item.id)}
                            >
                              Delete for good
                            </button>
                          {:else}
                            <button
                              class="btn btn-sm btn-danger btn-icon"
                              title="Delete"
                              aria-label="Delete dictation"
                              onclick={() => (confirmDelete = item.id)}
                            >
                              <Icon name="trash" size={14} />
                            </button>
                          {/if}
                        </div>
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        </section>
      {/each}
    </div>

    {#if nextCursor}
      <div class="mt-8 flex justify-center">
        <button class="btn" disabled={loadingMore} onclick={() => void load(nextCursor ?? undefined)}>
          {loadingMore ? 'Loading…' : 'Load older'}
        </button>
      </div>
    {/if}
  {/if}
</div>
