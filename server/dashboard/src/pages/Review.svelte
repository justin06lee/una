<script lang="ts">
  import { onMount, tick } from 'svelte';

  import { api, errMsg, type DictationDetail, type Eligibility, type ReviewAction } from '../api';
  import { autosize } from '../lib/autosize';
  import AudioPlayer from '../lib/AudioPlayer.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Kbd from '../lib/Kbd.svelte';
  import ProgressBar from '../lib/ProgressBar.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import { fmtDate, fmtDur } from '../lib/format';

  let item = $state<DictationDetail | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let editor = $state('');
  let backlog = $state<number | null>(null);
  let eligibility = $state<Eligibility | null>(null);
  let flash = $state<{ ok: boolean; text: string } | null>(null);
  let acting = $state(false);

  let player = $state<AudioPlayer>();
  let textareaEl = $state<HTMLTextAreaElement>();

  /** Optional "final text" editor — the user's preferred rendering (style-LLM pair). */
  let polishEditor = $state('');
  let polishEl = $state<HTMLTextAreaElement>();

  /** Session cursor: everything at or before this id has been handled (or skipped). */
  let after: string | null = null;
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  const dirty = $derived(item !== null && editor.trim() !== item.raw_text.trim());
  const isMac = navigator.platform.toLowerCase().includes('mac');

  async function refreshBacklog(): Promise<void> {
    try {
      backlog = (await api.stats()).review.backlog;
    } catch {
      /* non-critical */
    }
  }

  async function loadNext(): Promise<void> {
    loading = true;
    error = null;
    try {
      item = await api.reviewNext(after ?? undefined);
      editor = item?.raw_text ?? '';
      polishEditor = item ? (item.polished_text ?? item.cleaned_text ?? item.raw_text) : '';
      if (item === null) eligibility = await api.trainingEligibility();
    } catch (e) {
      error = errMsg(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadNext();
    void refreshBacklog();
    return () => clearTimeout(flashTimer);
  });

  const ACTION_LABEL: Record<ReviewAction, string> = {
    accepted: 'Accepted',
    edited: 'Saved edit',
    skipped: 'Skipped',
    excluded: 'Excluded',
  };

  /** The polished text to send: non-empty and different from (cleaned ?? raw), else null. */
  function polishedPayload(): string | null {
    if (!item) return null;
    const value = polishEditor.trim();
    const base = (item.cleaned_text ?? item.raw_text).trim();
    return value !== '' && value !== base ? value : null;
  }

  async function act(action: ReviewAction, corrected?: string): Promise<void> {
    if (!item || acting) return;
    acting = true;
    error = null;
    try {
      const body: {
        action: ReviewAction;
        corrected_text?: string | null;
        polished_text?: string | null;
      } = { action, corrected_text: corrected ?? null };
      if (action === 'accepted' || action === 'edited') body.polished_text = polishedPayload();
      const res = await api.putCorrection(item.id, body);
      const label = ACTION_LABEL[action];
      const style = res.polished_text != null ? ' · style pair saved' : '';
      flash = res.training_eligible
        ? { ok: true, text: `${label} — eligible${style}` }
        : { ok: false, text: `${label} — ${res.eligibility_reason ?? 'not eligible'}${style}` };
      clearTimeout(flashTimer);
      flashTimer = setTimeout(() => (flash = null), 3200);
      after = item.id;
      await loadNext();
      void refreshBacklog();
    } catch (e) {
      error = errMsg(e);
    } finally {
      acting = false;
    }
  }

  function accept(): void {
    void act('accepted');
  }
  function saveEdit(): void {
    if (!item) return;
    if (dirty) void act('edited', editor.trim());
    else void act('accepted');
  }
  function skip(): void {
    void act('skipped');
  }
  function exclude(): void {
    void act('excluded');
  }

  function onRawKeydown(e: KeyboardEvent): void {
    if (e.key === 'Tab' && !e.shiftKey) {
      e.preventDefault();
      polishEl?.focus();
    }
  }

  async function focusEditor(): Promise<void> {
    await tick();
    const el = textareaEl;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }

  function onKeydown(e: KeyboardEvent): void {
    const t = e.target as HTMLElement | null;
    const typing = !!t && (t.tagName === 'TEXTAREA' || t.tagName === 'INPUT');

    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      saveEdit();
      return;
    }
    if (typing) {
      if (e.key === 'Escape') t.blur();
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (!item) return;

    switch (e.key) {
      case ' ':
        e.preventDefault();
        player?.toggle();
        break;
      case 'r':
      case 'R':
        e.preventDefault();
        player?.replay();
        break;
      case '1':
        e.preventDefault();
        player?.setRate(1);
        break;
      case '2':
        e.preventDefault();
        player?.setRate(1.5);
        break;
      case '3':
        e.preventDefault();
        player?.setRate(2);
        break;
      case 'Enter':
        e.preventDefault();
        accept();
        break;
      case 'e':
      case 'E':
        e.preventDefault();
        void focusEditor();
        break;
      case 's':
      case 'S':
        e.preventDefault();
        skip();
        break;
      case 'x':
      case 'X':
        e.preventDefault();
        exclude();
        break;
    }
  }

  const SHORTCUTS = [
    { keys: ['Space'], label: 'play' },
    { keys: ['R'], label: 'replay' },
    { keys: ['1', '2', '3'], label: 'speed' },
    { keys: ['↵'], label: 'accept' },
    { keys: ['E'], label: 'edit' },
    { keys: ['S'], label: 'skip' },
    { keys: ['X'], label: 'exclude' },
  ];
</script>

<svelte:window onkeydown={onKeydown} />

<div class="mx-auto flex min-h-full max-w-3xl flex-col px-8 py-10">
  <header class="mb-7 flex items-start justify-between gap-4">
    <div>
      <h1 class="text-[20px] font-semibold tracking-tight">Review</h1>
      <p class="mt-1 text-[13px] text-muted">
        Listen, fix what was misheard, and una learns your voice.
      </p>
    </div>
    <div class="flex flex-none items-center gap-2">
      {#if flash}
        <span class="chip {flash.ok ? 'chip-ok' : 'chip-warn'} max-w-72 fade-in">
          <span class="chip-dot"></span><span class="truncate">{flash.text}</span>
        </span>
      {/if}
      {#if backlog !== null}
        <span class="chip tabular-nums" title="Dictations still waiting">{backlog} waiting</span>
      {/if}
    </div>
  </header>

  {#if error}
    <div
      class="mb-4 flex items-center justify-between gap-3 rounded-xl border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--c-danger) 28%, transparent); background: var(--c-danger-soft); color: var(--c-danger)"
    >
      <span>{error}</span>
      <button class="btn btn-sm" onclick={() => void loadNext()}>Retry</button>
    </div>
  {/if}

  {#if loading}
    <div class="card overflow-hidden">
      <div class="flex items-center gap-2 border-b border-border px-6 py-4">
        <Skeleton class="h-5 w-20" />
        <Skeleton class="h-5 w-28" />
      </div>
      <div class="border-b border-border px-6 py-5"><Skeleton class="h-8 w-full" /></div>
      <div class="space-y-2 px-6 py-6">
        <Skeleton class="h-6 w-full" />
        <Skeleton class="h-6 w-11/12" />
        <Skeleton class="h-6 w-3/4" />
      </div>
    </div>
  {:else if item}
    <div class="card overflow-hidden rise-in">
      <div class="flex flex-wrap items-center gap-2.5 border-b border-border px-6 py-3.5 text-[11px] text-muted">
        {#if item.app_name}<span class="chip">{item.app_name}</span>{/if}
        <span>{fmtDate(item.created_at)}</span>
        <span class="text-faint">·</span>
        <span class="tabular-nums">{fmtDur(item.duration_ms)}</span>
        {#if item.language}<span class="text-faint uppercase">{item.language}</span>{/if}
        {#if item.eval_holdout}
          <span
            class="chip chip-warn"
            title="Held out of training — this dictation measures accuracy instead"
          >
            holdout
          </span>
        {/if}
      </div>

      <div class="border-b border-border px-6 py-4">
        <AudioPlayer bind:this={player} src={api.audioUrl(item.id)} autoplay />
      </div>

      <div class="px-6 py-5">
        <div class="mb-2.5 flex items-baseline justify-between gap-4">
          <span class="label">Raw transcript</span>
          <span class="text-[11px] text-faint italic">
            Fix what was said, not what you wish you'd said.
          </span>
        </div>
        <textarea
          bind:this={textareaEl}
          bind:value={editor}
          rows="1"
          spellcheck="false"
          use:autosize={{ value: editor, min: 76 }}
          class="w-full resize-none overflow-hidden rounded-lg border border-transparent bg-transparent px-2 py-1.5 text-[17px] leading-relaxed transition-colors duration-150 focus:border-edge focus:bg-raised/50 focus:outline-none"
          aria-label="Raw transcript editor"
          onkeydown={onRawKeydown}
        ></textarea>

        {#if item.cleaned_text}
          <div class="mt-4 border-t border-border pt-3.5">
            <div class="label mb-1.5">What una sent to your app</div>
            <p class="text-[13px] leading-relaxed text-muted">{item.cleaned_text}</p>
          </div>
        {/if}

        <div class="mt-4 border-t border-border pt-3.5">
          <div class="mb-2 flex items-baseline justify-between gap-4">
            <span class="label">Final text</span>
            <span class="text-[11px] text-faint italic">
              Optional — how you'd have written it. Trains your style.
            </span>
          </div>
          <textarea
            bind:this={polishEl}
            bind:value={polishEditor}
            rows="1"
            spellcheck="false"
            use:autosize={{ value: polishEditor, min: 56 }}
            class="w-full resize-none overflow-hidden rounded-lg border border-border bg-transparent px-2 py-1.5 text-[13px] leading-relaxed text-muted transition-colors duration-150 focus:border-edge focus:bg-raised/50 focus:text-text focus:outline-none"
            aria-label="Final text editor"
          ></textarea>
        </div>
      </div>

      <div class="flex items-center gap-2 border-t border-border bg-raised/40 px-6 py-3.5">
        {#if dirty}
          <button class="btn btn-primary" onclick={saveEdit} disabled={acting}>
            Save edit <Kbd>{isMac ? '⌘↵' : 'Ctrl ↵'}</Kbd>
          </button>
          <button class="btn btn-ghost" onclick={() => (editor = item?.raw_text ?? '')} disabled={acting}>
            Revert
          </button>
        {:else}
          <button class="btn btn-primary" onclick={accept} disabled={acting}>
            Sounds right <Kbd>↵</Kbd>
          </button>
        {/if}
        <div class="ml-auto flex items-center gap-2">
          <button class="btn" onclick={skip} disabled={acting}>Skip <Kbd>S</Kbd></button>
          <button class="btn btn-danger" onclick={exclude} disabled={acting}>
            Exclude <Kbd>X</Kbd>
          </button>
        </div>
      </div>
    </div>
  {:else}
    <div class="card">
      <EmptyState
        title="All caught up"
        sub="Every dictation has been reviewed. New recordings land here automatically."
      >
        {#if eligibility}
          <div class="text-left">
            <div class="mb-2 flex items-baseline justify-between text-[12px]">
              <span class="text-muted">Eligible training data</span>
              <span class="tabular-nums">
                {eligibility.eligible_minutes.toFixed(1)} / {eligibility.threshold_minutes.toFixed(0)} min
              </span>
            </div>
            <ProgressBar
              value={eligibility.eligible_minutes / Math.max(1e-9, eligibility.threshold_minutes)}
            />
            <div class="mt-2 flex items-center justify-between text-[11px] text-faint">
              <span class="tabular-nums">
                {eligibility.eligible_pairs} pairs · {eligibility.style_pairs} style pairs
              </span>
              {#if eligibility.ready}
                <a href="#/training" class="font-semibold" style="color: var(--c-accent)">
                  Ready to train →
                </a>
              {/if}
            </div>
          </div>
        {/if}
      </EmptyState>
    </div>
  {/if}

  <div class="mt-auto pt-10">
    <div class="flex flex-wrap items-center justify-center gap-x-5 gap-y-2 border-t border-border pt-4 text-[11px] text-faint">
      {#each SHORTCUTS as s (s.label)}
        <span class="flex items-center gap-1.5">
          {#each s.keys as k (k)}<Kbd>{k}</Kbd>{/each}
          {s.label}
        </span>
      {/each}
      <span class="flex items-center gap-1.5">
        <Kbd>{isMac ? '⌘↵' : 'Ctrl ↵'}</Kbd> save edit
      </span>
    </div>
  </div>
</div>
