<script lang="ts">
  import { untrack } from 'svelte';

  import { api, errMsg, type DictationDetail, type DictationSummary } from '../api';
  import AudioPlayer from '../lib/AudioPlayer.svelte';
  import DiffText from '../lib/DiffText.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import StatusChip from '../lib/StatusChip.svelte';
  import { fmtDate, fmtDur } from '../lib/format';

  let items = $state<DictationSummary[]>([]);
  let nextCursor = $state<string | null>(null);
  let loading = $state(false);
  let initial = $state(true);
  let error = $state<string | null>(null);

  let q = $state('');
  let qDebounced = $state('');
  let reviewed = $state<'all' | 'yes' | 'no'>('all');
  let appFilter = $state<string | null>(null);

  let expanded = $state<string | null>(null);
  let detail = $state<DictationDetail | null>(null);
  let detailLoading = $state(false);
  let confirmDelete = $state(false);

  $effect(() => {
    const v = q;
    if (v === untrack(() => qDebounced)) return;
    const t = setTimeout(() => (qDebounced = v), 300);
    return () => clearTimeout(t);
  });

  $effect(() => {
    void qDebounced;
    void reviewed;
    void appFilter;
    untrack(() => void reload());
  });

  async function reload(): Promise<void> {
    initial = true;
    expanded = null;
    detail = null;
    await load(null);
    initial = false;
  }

  async function load(cursor: string | null): Promise<void> {
    loading = true;
    error = null;
    try {
      const res = await api.listDictations({
        limit: 50,
        cursor: cursor ?? undefined,
        q: qDebounced || undefined,
        app: appFilter ?? undefined,
        reviewed: reviewed === 'all' ? undefined : reviewed === 'yes',
      });
      items = cursor ? [...items, ...res.items] : res.items;
      nextCursor = res.next_cursor;
    } catch (e) {
      error = errMsg(e);
    } finally {
      loading = false;
    }
  }

  async function toggleRow(id: string): Promise<void> {
    confirmDelete = false;
    if (expanded === id) {
      expanded = null;
      detail = null;
      return;
    }
    expanded = id;
    detail = null;
    detailLoading = true;
    try {
      const res = await api.getDictation(id);
      if (expanded === id) detail = res;
    } catch (e) {
      error = errMsg(e);
    } finally {
      detailLoading = false;
    }
  }

  async function remove(id: string): Promise<void> {
    try {
      await api.deleteDictation(id);
      items = items.filter((i) => i.id !== id);
      expanded = null;
      detail = null;
      confirmDelete = false;
    } catch (e) {
      error = errMsg(e);
    }
  }

  const filters: { key: 'all' | 'yes' | 'no'; label: string }[] = [
    { key: 'all', label: 'All' },
    { key: 'yes', label: 'Reviewed' },
    { key: 'no', label: 'Unreviewed' },
  ];
</script>

<div class="mx-auto max-w-5xl px-6 py-8">
  <header class="mb-6">
    <h1 class="text-base font-semibold tracking-tight">History</h1>
    <p class="mt-0.5 text-[13px] text-muted">Every dictation, newest first.</p>
  </header>

  <div class="mb-4 flex flex-wrap items-center gap-3">
    <div class="relative w-full max-w-xs">
      <svg
        class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-faint"
        width="13"
        height="13"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="7" />
        <path d="m21 21-4.3-4.3" />
      </svg>
      <input
        type="search"
        class="input pl-8"
        placeholder="Search transcripts…"
        bind:value={q}
        aria-label="Search transcripts"
      />
    </div>
    <div class="inline-flex rounded-md border border-edge bg-raised p-0.5">
      {#each filters as f (f.key)}
        <button
          class="rounded px-2.5 py-1 text-xs transition-colors duration-150
            {reviewed === f.key ? 'bg-hover text-text' : 'text-muted hover:text-text'}"
          onclick={() => (reviewed = f.key)}
          aria-pressed={reviewed === f.key}
        >
          {f.label}
        </button>
      {/each}
    </div>
    {#if appFilter}
      <button
        class="chip chip-accent"
        onclick={() => (appFilter = null)}
        title="Clear app filter"
      >
        {appFilter} ✕
      </button>
    {/if}
  </div>

  {#if error}
    <div
      class="mb-4 rounded-lg border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--color-danger) 30%, transparent); background: color-mix(in oklab, var(--color-danger) 7%, transparent); color: var(--color-danger)"
    >
      {error}
    </div>
  {/if}

  {#if initial && loading}
    <div class="card divide-y divide-border">
      {#each Array(6) as _, i (i)}
        <div class="flex items-center gap-3 px-4 py-3.5">
          <Skeleton class="h-4 w-28" />
          <Skeleton class="h-4 flex-1" />
          <Skeleton class="h-4 w-16" />
        </div>
      {/each}
    </div>
  {:else if items.length === 0 && !loading}
    <div class="card">
      <EmptyState
        title={qDebounced || reviewed !== 'all' || appFilter ? 'No matches' : 'No dictations yet'}
        sub={qDebounced || reviewed !== 'all' || appFilter
          ? 'Try a different search or filter.'
          : 'Dictations from your devices will appear here.'}
      />
    </div>
  {:else}
    <div class="card divide-y divide-border overflow-hidden">
      {#each items as it (it.id)}
        <div>
          <button
            class="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors duration-150 hover:bg-hover
              {expanded === it.id ? 'bg-raised/50' : ''}"
            onclick={() => void toggleRow(it.id)}
            aria-expanded={expanded === it.id}
          >
            <span class="w-28 flex-none text-xs text-muted tabular-nums">
              {fmtDate(it.created_at)}
            </span>
            <span class="min-w-0 flex-1 truncate text-sm">
              {#if it.text}{it.text}{:else}<em class="text-faint">(empty)</em>{/if}
            </span>
            {#if it.app_name}
              <span
                class="chip hidden flex-none cursor-pointer hover:text-text sm:inline-flex"
                role="button"
                tabindex="-1"
                title="Filter by {it.app_name}"
                onclick={(e) => {
                  e.stopPropagation();
                  appFilter = it.app_name;
                }}
                onkeydown={(e) => e.key === 'Enter' && (appFilter = it.app_name)}
              >
                {it.app_name}
              </span>
            {/if}
            <span class="w-10 flex-none text-right text-xs text-faint tabular-nums">
              {fmtDur(it.duration_ms)}
            </span>
            <span class="w-20 flex-none text-right">
              {#if it.review_action}
                <StatusChip status={it.review_action} />
              {:else}
                <span class="chip">pending</span>
              {/if}
            </span>
          </button>

          {#if expanded === it.id}
            <div class="border-t border-border bg-raised/30 px-4 py-4">
              {#if detailLoading}
                <div class="space-y-2">
                  <Skeleton class="h-8 w-full" />
                  <Skeleton class="h-16 w-full" />
                </div>
              {:else if detail}
                <AudioPlayer src={api.audioUrl(detail.id)} />

                <div
                  class="mt-4 grid gap-4 {detail.polished_text
                    ? 'md:grid-cols-2 xl:grid-cols-4'
                    : 'md:grid-cols-3'}"
                >
                  <div>
                    <div class="mb-1.5 text-[11px] font-medium tracking-wide text-faint uppercase">
                      Raw
                    </div>
                    <div class="text-sm text-text">
                      {#if detail.corrected_text}
                        <DiffText a={detail.raw_text} b={detail.corrected_text} show="a" />
                      {:else}
                        <p class="leading-relaxed whitespace-pre-wrap">{detail.raw_text}</p>
                      {/if}
                    </div>
                  </div>
                  <div>
                    <div class="mb-1.5 text-[11px] font-medium tracking-wide text-faint uppercase">
                      Cleaned
                    </div>
                    {#if detail.cleaned_text}
                      <p class="text-sm leading-relaxed whitespace-pre-wrap text-muted">
                        {detail.cleaned_text}
                      </p>
                    {:else}
                      <p class="text-sm text-faint">—</p>
                    {/if}
                  </div>
                  <div>
                    <div class="mb-1.5 text-[11px] font-medium tracking-wide text-faint uppercase">
                      Corrected
                    </div>
                    {#if detail.corrected_text}
                      <div class="text-sm text-text">
                        <DiffText a={detail.raw_text} b={detail.corrected_text} show="b" />
                      </div>
                    {:else}
                      <p class="text-sm text-faint">—</p>
                    {/if}
                  </div>
                  {#if detail.polished_text}
                    <div>
                      <div
                        class="mb-1.5 text-[11px] font-medium tracking-wide text-faint uppercase"
                      >
                        Polished
                      </div>
                      <div class="text-sm text-text">
                        <DiffText
                          a={detail.cleaned_text ?? detail.raw_text}
                          b={detail.polished_text}
                          show="b"
                        />
                      </div>
                    </div>
                  {/if}
                </div>

                <div class="mt-4 flex flex-wrap items-center gap-2">
                  {#if detail.review_action}
                    <StatusChip status={detail.review_action} />
                  {:else}
                    <span class="chip">pending review</span>
                  {/if}
                  {#if detail.training_eligible !== null}
                    <StatusChip
                      status={detail.training_eligible ? 'eligible' : 'ineligible'}
                      label={detail.training_eligible ? 'eligible' : 'not eligible'}
                      title={detail.eligibility_reason ?? undefined}
                    />
                  {/if}
                  {#if detail.eval_holdout}
                    <span class="chip chip-warn" title="Held out of training; used for WER eval">
                      holdout
                    </span>
                  {/if}
                  {#if detail.asr_model}
                    <span class="chip" title="ASR model">{detail.asr_model}</span>
                  {/if}

                  <div class="ml-auto flex items-center gap-2">
                    {#if confirmDelete}
                      <span class="text-xs text-muted">Delete this dictation and its audio?</span>
                      <button class="btn btn-sm btn-danger" onclick={() => void remove(it.id)}>
                        Delete
                      </button>
                      <button class="btn btn-sm" onclick={() => (confirmDelete = false)}>
                        Cancel
                      </button>
                    {:else}
                      <button class="btn btn-sm btn-danger" onclick={() => (confirmDelete = true)}>
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

    {#if nextCursor}
      <button
        class="btn mt-3 w-full"
        onclick={() => void load(nextCursor)}
        disabled={loading}
      >
        {loading ? 'Loading…' : 'Load more'}
      </button>
    {/if}
  {/if}
</div>
