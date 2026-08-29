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
  import EmptyState from '../lib/EmptyState.svelte';
  import Kbd from '../lib/Kbd.svelte';
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

<div class="mx-auto max-w-4xl px-8 py-10">
  <!-- Hero -------------------------------------------------------------- -->
  <header class="mb-8 rise-in">
    {#if stats}
      <div class="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 class="display text-[2.75rem] leading-none tracking-tight">
            {fmtNum(stats.totals.words)}
            <span class="text-[1.75rem] text-muted">words</span>
          </h1>
          <p class="mt-2 text-[13px] text-muted">
            across {fmtNum(stats.totals.dictations)} dictations · {fmtSpan(stats.totals.ms)} of audio
            {#if savedMinutes > 1}
              · roughly <span class="font-semibold text-text">{fmtSpan(savedMinutes * 60_000)}</span>
              saved against typing
            {/if}
          </p>
        </div>
        <div class="flex items-center gap-1.5 text-[12px] text-faint">
          <span>Hold</span>
          <Kbd>Ctrl</Kbd><Kbd>Alt</Kbd><Kbd>Space</Kbd>
          <span>anywhere to dictate</span>
        </div>
      </div>
    {:else}
      <Skeleton class="h-14 w-80" />
    {/if}
  </header>

  <!-- Stats card -------------------------------------------------------- -->
  <div class="card mb-8 grid grid-cols-2 divide-border sm:grid-cols-4 sm:divide-x">
    {#if stats}
      {@const cells = [
        {
          label: 'Streak',
          value: `${stats.streak.current}`,
          unit: stats.streak.current === 1 ? 'day' : 'days',
          hint: `longest ${stats.streak.longest}`,
        },
        {
          label: 'Speed',
          value: `${Math.round(stats.totals.avg_wpm)}`,
          unit: 'wpm',
          hint: `~${Math.round(stats.totals.avg_wpm / TYPING_WPM)}× typing`,
        },
        {
          label: 'Cleaned up',
          value: fmtNum(stats.cleanup.words_removed),
          unit: 'words',
          hint: 'filler removed',
        },
        {
          label: 'To review',
          value: `${stats.review.backlog}`,
          unit: '',
          hint: `${stats.review.eligible} training pairs`,
        },
      ]}
      {#each cells as cell (cell.label)}
        <div class="px-5 py-4">
          <div class="label">{cell.label}</div>
          <div class="mt-1.5 flex items-baseline gap-1.5">
            <span class="display text-[1.75rem] leading-none tabular-nums">{cell.value}</span>
            {#if cell.unit}<span class="text-[13px] text-muted">{cell.unit}</span>{/if}
          </div>
          <div class="mt-1 text-[11px] text-faint">{cell.hint}</div>
        </div>
      {/each}
    {:else}
      {#each [0, 1, 2, 3] as i (i)}
        <div class="px-5 py-4"><Skeleton class="h-14 w-full" /></div>
      {/each}
    {/if}
  </div>

  <!-- Search + filters --------------------------------------------------- -->
  <div class="mb-4 flex flex-wrap items-center gap-2">
    <div class="relative min-w-56 flex-1">
      <svg
        class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-faint"
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" />
      </svg>
      <input
        class="input pl-9"
        placeholder="Search your transcripts…"
        bind:value={query}
        oninput={refilter}
        aria-label="Search transcripts"
      />
    </div>

    <div class="seg">
      {#each [{ k: 'all', l: 'All' }, { k: 'pending', l: 'To review' }, { k: 'reviewed', l: 'Reviewed' }] as f (f.k)}
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

    {#if appFilter}
      <button
        type="button"
        class="chip chip-accent"
        onclick={() => {
          appFilter = null;
          refilter();
        }}
        title="Clear app filter"
      >
        {appFilter}
        <svg width="10" height="10" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3" aria-hidden="true">
          <path d="m6 6 12 12M18 6 6 18" />
        </svg>
      </button>
    {/if}
  </div>

  {#if error}
    <div
      class="mb-4 flex items-center justify-between gap-3 rounded-xl border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--c-danger) 28%, transparent); background: var(--c-danger-soft); color: var(--c-danger)"
    >
      <span>{error}</span>
      <button class="btn btn-sm" onclick={() => void load()}>Retry</button>
    </div>
  {/if}

  <!-- Feed --------------------------------------------------------------- -->
  {#if loading}
    <div class="space-y-2">
      {#each [0, 1, 2, 3, 4] as i (i)}
        <Skeleton class="h-16 w-full" />
      {/each}
    </div>
  {:else if items.length === 0}
    <div class="card">
      <EmptyState
        title={filtering ? 'Nothing matches' : 'No dictations yet'}
        sub={filtering
          ? 'Try a different search, or clear the filters.'
          : 'Hold your hotkey anywhere and start talking — everything you dictate shows up here.'}
      />
    </div>
  {:else}
    <div class="space-y-6">
      {#each groups as group (group.day)}
        <section>
          <h2 class="mb-1.5 px-1 text-[12px] font-semibold text-muted">
            {fmtDayHeading(group.day)}
          </h2>
          <div class="card divide-y divide-border overflow-hidden !rounded-xl">
            {#each group.rows as item (item.id)}
              <div>
                <div
                  class="feed-row !rounded-none"
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
                  <div class="flex items-start gap-3">
                    <p class="min-w-0 flex-1 text-[13.5px] leading-relaxed">
                      {item.text || '(nothing was transcribed)'}
                    </p>
                    <div class="row-actions flex flex-none items-center gap-0.5">
                      <button
                        class="btn btn-ghost btn-icon"
                        title={copied === item.id ? 'Copied' : 'Copy transcript'}
                        aria-label="Copy transcript"
                        onclick={(e) => {
                          e.stopPropagation();
                          void copy(item.text, item.id);
                        }}
                      >
                        {#if copied === item.id}
                          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--c-accent)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M20 6 9 17l-5-5" />
                          </svg>
                        {:else}
                          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <rect x="9" y="9" width="11" height="11" rx="2.5" />
                            <path d="M5 15V6.5A2.5 2.5 0 0 1 7.5 4H15" />
                          </svg>
                        {/if}
                      </button>
                    </div>
                  </div>

                  <div class="mt-1.5 flex flex-wrap items-center gap-2 text-[11px] text-faint">
                    <span class="tabular-nums">{fmtClock(item.created_at)}</span>
                    <span>·</span>
                    <span class="tabular-nums">{fmtDur(item.duration_ms)}</span>
                    {#if item.app_name}
                      <button
                        type="button"
                        class="chip cursor-pointer hover:text-text"
                        title="Filter by {item.app_name}"
                        onclick={(e) => {
                          e.stopPropagation();
                          appFilter = item.app_name;
                          refilter();
                        }}
                      >
                        {item.app_name}
                      </button>
                    {/if}
                    {#if !item.reviewed}
                      <span class="chip chip-warn">to review</span>
                    {/if}
                  </div>
                </div>

                {#if expanded === item.id}
                  <div class="border-t border-border bg-raised/40 px-4 py-4 fade-in">
                    {#if detailLoading}
                      <Skeleton class="h-20 w-full" />
                    {:else if detail}
                      <AudioPlayer src={api.audioUrl(detail.id)} />

                      <div class="mt-4 grid gap-4 sm:grid-cols-2">
                        <div>
                          <div class="label mb-1">Raw transcript</div>
                          <p class="text-[13px] leading-relaxed">{detail.raw_text || '—'}</p>
                        </div>
                        {#if detail.cleaned_text}
                          <div>
                            <div class="label mb-1">After cleanup</div>
                            <p class="text-[13px] leading-relaxed">{detail.cleaned_text}</p>
                          </div>
                        {/if}
                      </div>

                      {#if detail.corrected_text}
                        <div class="mt-4">
                          <div class="label mb-1">Your correction</div>
                          <p class="text-[13px] leading-relaxed">{detail.corrected_text}</p>
                        </div>
                      {/if}

                      <div class="mt-4 flex flex-wrap items-center gap-2 hairline-t pt-3">
                        {#if detail.eval_holdout}
                          <span class="chip chip-warn" title="Held out of training to measure WER">
                            holdout
                          </span>
                        {/if}
                        {#if detail.training_eligible}
                          <span class="chip chip-ok">training pair</span>
                        {:else if detail.eligibility_reason}
                          <span class="chip" title={detail.eligibility_reason}>
                            not eligible
                          </span>
                        {/if}
                        {#if detail.asr_model}
                          <span class="chip" title="ASR model">{detail.asr_model}</span>
                        {/if}
                        {#if detail.llm_model}
                          <span class="chip" title="Cleanup model">{detail.llm_model}</span>
                        {/if}

                        <div class="ml-auto flex items-center gap-2">
                          {#if !item.reviewed}
                            <a href="#/review" class="btn btn-sm">Review this</a>
                          {/if}
                          {#if confirmDelete === item.id}
                            <button class="btn btn-sm btn-danger" onclick={() => void remove(item.id)}>
                              Delete for good
                            </button>
                            <button class="btn btn-sm btn-ghost" onclick={() => (confirmDelete = null)}>
                              Cancel
                            </button>
                          {:else}
                            <button
                              class="btn btn-sm btn-danger"
                              onclick={() => (confirmDelete = item.id)}
                            >
                              Delete
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
      <div class="mt-6 flex justify-center">
        <button class="btn" disabled={loadingMore} onclick={() => void load(nextCursor ?? undefined)}>
          {loadingMore ? 'Loading…' : 'Load older'}
        </button>
      </div>
    {/if}
  {/if}
</div>
