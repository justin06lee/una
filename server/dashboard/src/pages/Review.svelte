<script lang="ts">
  import { onMount, tick } from 'svelte';

  import {
    api,
    errMsg,
    type DictationDetail,
    type Eligibility,
    type ReviewAction,
  } from '../api';
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
      backlog = (await api.stats()).review_backlog;
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
        ? { ok: true, text: `${label} — eligible ✓${style}` }
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
</script>

<svelte:window onkeydown={onKeydown} />

<div class="mx-auto flex min-h-full max-w-3xl flex-col px-6 py-8">
  <header class="mb-6 flex items-start justify-between gap-4">
    <div>
      <h1 class="text-base font-semibold tracking-tight">Review</h1>
      <p class="mt-0.5 text-[13px] text-muted">Listen, correct, feed the model.</p>
    </div>
    <div class="flex items-center gap-2">
      {#if flash}
        <span class="chip {flash.ok ? 'chip-ok' : 'chip-warn'} max-w-72">
          <span class="chip-dot"></span><span class="truncate">{flash.text}</span>
        </span>
      {/if}
      {#if backlog !== null}
        <span class="chip tabular-nums" title="Unreviewed dictations">
          {backlog} in queue
        </span>
      {/if}
    </div>
  </header>

  {#if error}
    <div
      class="mb-4 flex items-center justify-between rounded-lg border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--color-danger) 30%, transparent); background: color-mix(in oklab, var(--color-danger) 7%, transparent); color: var(--color-danger)"
    >
      <span>{error}</span>
      <button class="btn btn-sm" onclick={() => void loadNext()}>Retry</button>
    </div>
  {/if}

  {#if loading}
    <div class="card overflow-hidden">
      <div class="flex items-center gap-2 border-b border-border px-5 py-3.5">
        <Skeleton class="h-5 w-20" />
        <Skeleton class="h-5 w-28" />
      </div>
      <div class="border-b border-border px-5 py-4">
        <Skeleton class="h-8 w-full" />
      </div>
      <div class="space-y-2 px-5 py-5">
        <Skeleton class="h-5 w-full" />
        <Skeleton class="h-5 w-11/12" />
        <Skeleton class="h-5 w-3/4" />
      </div>
    </div>
  {:else if item}
    <div class="card overflow-hidden">
      <div class="flex items-center gap-2.5 border-b border-border px-5 py-3 text-xs text-muted">
        {#if item.app_name}
          <span class="chip">{item.app_name}</span>
        {/if}
        <span>{fmtDate(item.created_at)}</span>
        <span class="text-faint">·</span>
        <span class="tabular-nums">{fmtDur(item.duration_ms)}</span>
        {#if item.language}
          <span class="text-faint uppercase">{item.language}</span>
        {/if}
        {#if item.eval_holdout}
          <span
            class="chip chip-warn"
            title="Held out of training — this dictation measures WER instead"
          >
            holdout
          </span>
        {/if}
      </div>

      <div class="border-b border-border px-5 py-4">
        <AudioPlayer bind:this={player} src={api.audioUrl(item.id)} autoplay />
      </div>

      <div class="px-5 py-4">
        <div class="mb-2 flex items-baseline justify-between gap-4">
          <span class="text-[11px] font-medium tracking-wide text-faint uppercase">
            Raw transcript
          </span>
          <span class="text-[11px] text-faint italic">
            Fix what was said, not what you wish you'd said.
          </span>
        </div>
        <textarea
          bind:this={textareaEl}
          bind:value={editor}
          rows="5"
          spellcheck="false"
          class="w-full resize-y rounded-md border border-transparent bg-transparent px-1 py-1 text-lg leading-relaxed transition-colors duration-150 focus:border-edge focus:bg-raised/40 focus:outline-none"
          aria-label="Raw transcript editor"
          onkeydown={onRawKeydown}
        ></textarea>

        {#if item.cleaned_text}
          <div class="mt-3 border-t border-border/60 pt-3">
            <div class="mb-1 text-[11px] font-medium tracking-wide text-faint uppercase">
              LLM-cleaned reference
            </div>
            <p class="text-sm leading-relaxed text-faint">{item.cleaned_text}</p>
          </div>
        {/if}

        <div class="mt-3 border-t border-border/60 pt-3">
          <div class="mb-1.5 flex items-baseline justify-between gap-4">
            <span class="text-[11px] font-medium tracking-wide text-faint uppercase">
              Final text
            </span>
            <span class="text-[11px] text-faint italic">
              Optional — how you want it written. Trains your personal style model.
            </span>
          </div>
          <textarea
            bind:this={polishEl}
            bind:value={polishEditor}
            rows="3"
            spellcheck="false"
            class="w-full resize-y rounded-md border border-border/50 bg-transparent px-1.5 py-1 text-sm leading-relaxed text-muted transition-colors duration-150 focus:border-edge focus:bg-raised/40 focus:text-text focus:outline-none"
            aria-label="Final text editor"
          ></textarea>
        </div>
      </div>

      <div class="flex items-center gap-2 border-t border-border bg-raised/40 px-5 py-3">
        {#if dirty}
          <button class="btn btn-primary" onclick={saveEdit} disabled={acting}>
            Save edit <Kbd>{isMac ? '⌘↵' : 'Ctrl ↵'}</Kbd>
          </button>
          <button
            class="btn btn-ghost"
            onclick={() => (editor = item?.raw_text ?? '')}
            disabled={acting}
          >
            Revert
          </button>
        {:else}
          <button class="btn btn-primary" onclick={accept} disabled={acting}>
            Accept as-is <Kbd>↵</Kbd>
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
            <div class="mb-1.5 flex items-baseline justify-between text-xs">
              <span class="text-muted">Eligible training data</span>
              <span class="tabular-nums">
                {eligibility.eligible_minutes.toFixed(1)} / {eligibility.threshold_minutes.toFixed(0)} min
              </span>
            </div>
            <ProgressBar
              value={eligibility.eligible_minutes / Math.max(1e-9, eligibility.threshold_minutes)}
              tone={eligibility.ready ? 'ok' : 'accent'}
            />
            <div class="mt-1.5 flex items-center justify-between text-[11px] text-faint">
              <span class="tabular-nums">
                {eligibility.eligible_pairs} pairs{typeof eligibility.style_pairs === 'number'
                  ? ` · ${eligibility.style_pairs} style pairs collected`
                  : ''}
              </span>
              {#if eligibility.ready}
                <a href="#/training" class="font-medium" style="color: var(--color-ok)">
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
    <div
      class="flex flex-wrap items-center justify-center gap-x-4 gap-y-2 border-t border-border pt-3 text-[11px] text-faint"
    >
      <span class="flex items-center gap-1.5"><Kbd>Space</Kbd> play / pause</span>
      <span class="flex items-center gap-1.5"><Kbd>R</Kbd> replay</span>
      <span class="flex items-center gap-1.5"><Kbd>1</Kbd><Kbd>2</Kbd><Kbd>3</Kbd> speed</span>
      <span class="flex items-center gap-1.5"><Kbd>↵</Kbd> accept</span>
      <span class="flex items-center gap-1.5"><Kbd>E</Kbd> edit</span>
      <span class="flex items-center gap-1.5">
        <Kbd>{isMac ? '⌘↵' : 'Ctrl ↵'}</Kbd> save edit
      </span>
      <span class="flex items-center gap-1.5"><Kbd>S</Kbd> skip</span>
      <span class="flex items-center gap-1.5"><Kbd>X</Kbd> exclude</span>
      <span class="flex items-center gap-1.5"><Kbd>Esc</Kbd> leave editor</span>
    </div>
  </div>
</div>
