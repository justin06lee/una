<script lang="ts">
  import { onMount, tick } from 'svelte';

  import { api, errMsg, type DictationDetail, type Eligibility, type ReviewAction } from '../api';
  import { autosize } from '../lib/autosize';
  import AudioPlayer from '../lib/AudioPlayer.svelte';
  import Banner from '../lib/Banner.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Icon from '../lib/Icon.svelte';
  import Kbd from '../lib/Kbd.svelte';
  import PageHeader from '../lib/PageHeader.svelte';
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
  /** The transcript editor starts from the teacher's reading rather than what una heard. */
  const guessed = $derived(
    !!item?.teacher?.literal_guess && item.teacher.literal_guess.trim() !== item.raw_text.trim(),
  );
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
      // The teacher's guesses, when there are any, are the drafts to confirm.
      editor = item?.teacher?.literal_guess ?? item?.raw_text ?? '';
      polishEditor = item
        ? (item.polished_text ??
          item.teacher?.polished_guess ??
          (item.cleanup_diverged ? null : item.cleaned_text) ??
          item.raw_text)
        : '';
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
    edited: 'Edit saved',
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
        ? { ok: true, text: `${label} · training pair${style}` }
        : { ok: false, text: `${label} · ${res.eligibility_reason ?? 'not used for training'}${style}` };
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
    { keys: ['Space'], label: 'Play' },
    { keys: ['R'], label: 'Replay' },
    { keys: ['1', '2', '3'], label: 'Speed' },
    { keys: ['E'], label: 'Edit' },
  ];
</script>

<svelte:window onkeydown={onKeydown} />

<div class="mx-auto flex min-h-full max-w-[46rem] flex-col px-10 py-12">
  <PageHeader title="Review" sub="Listen back and fix what was misheard. Every reviewed dictation teaches una your voice.">
    {#snippet actions()}
      {#if flash}
        <span class="chip max-w-72 fade-in">
          <span class="dot {flash.ok ? 'dot-ok' : 'dot-warn'}"></span>
          <span class="truncate">{flash.text}</span>
        </span>
      {/if}
      {#if backlog !== null}
        <span class="text-[13px] text-muted tabular-nums">{backlog} left</span>
      {/if}
    {/snippet}
  </PageHeader>

  {#if error}
    <Banner message={error} onretry={() => void loadNext()} />
  {/if}

  {#if loading}
    <div class="panel overflow-hidden">
      <div class="border-b border-line px-6 py-4"><Skeleton class="h-8 w-full" /></div>
      <div class="space-y-2.5 px-6 py-6">
        <Skeleton class="h-6 w-full" />
        <Skeleton class="h-6 w-11/12" />
        <Skeleton class="h-6 w-3/4" />
      </div>
    </div>
  {:else if item}
    <div class="panel overflow-hidden shadow-[var(--shadow-sm)] rise-in">
      <div class="border-b border-line px-6 pt-4 pb-4">
        <div class="mb-3.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-faint">
          {#if item.app_name}<span class="font-medium text-muted">{item.app_name}</span><span>·</span>{/if}
          <span>{fmtDate(item.created_at)}</span>
          <span>·</span>
          <span class="tabular-nums">{fmtDur(item.duration_ms)}</span>
          {#if item.language}<span>·</span><span class="uppercase">{item.language}</span>{/if}
          <span class="ml-auto flex items-center gap-1.5">
            {#if item.correction_source === 'auto'}
              <span class="chip" title="The client captured this correction from your edits">
                Captured edit
              </span>
            {:else if item.correction_source === 'popup'}
              <span class="chip" title="Corrected in the client's correction window">
                Corrected in app
              </span>
            {/if}
            {#if item.eval_holdout}
              <span class="chip" title="Held out of training — this dictation measures accuracy instead">
                Held out
              </span>
            {/if}
          </span>
        </div>
        <AudioPlayer bind:this={player} src={api.audioUrl(item.id)} autoplay />
      </div>

      <div class="px-6 pt-5 pb-6">
        <div class="mb-2 flex items-baseline justify-between gap-4">
          <span class="label">What una heard</span>
          <span class="text-[12px] text-faint">Fix what was said, not what you wish you'd said</span>
        </div>
        {#if item.teacher && item.teacher.disagreements.length > 0}
          <div class="mb-2 flex flex-wrap items-center gap-1.5 text-[12px]">
            <span class="text-faint">The second listen heard</span>
            {#each item.teacher.disagreements as span, i (i)}
              <button
                type="button"
                class="chip dispute cursor-pointer gap-1"
                title="Play this moment"
                onclick={() => span.t0 != null && player?.playRange(span.t0, span.t1 ?? span.t0 + 1)}
              >
                <span class="text-faint line-through">{item.raw_text.slice(span.start, span.end) || '—'}</span>
                <span class="text-faint">→</span>
                <span>{span.alt || '—'}</span>
                <Icon name="play" size={9} />
              </button>
            {/each}
          </div>
        {/if}
        <textarea
          bind:this={textareaEl}
          bind:value={editor}
          rows="1"
          spellcheck="false"
          use:autosize={{ value: editor, min: 64 }}
          class="-mx-2.5 w-[calc(100%+1.25rem)] resize-none overflow-hidden rounded-lg border border-transparent bg-transparent px-2.5 py-2 text-[18px] leading-relaxed tracking-[-0.01em] transition-colors duration-150 hover:bg-subtle focus:border-line-strong focus:bg-panel focus:outline-none"
          aria-label="Transcript editor"
          onkeydown={onRawKeydown}
        ></textarea>

        {#if guessed && editor.trim() === item.teacher?.literal_guess?.trim()}
          <p class="mt-1 text-[12px] text-faint">
            Pre-filled with Claude's reading of both listens ·
            <button type="button" class="underline underline-offset-2 hover:text-fg" onclick={() => (editor = item?.raw_text ?? '')}>
              use what una heard
            </button>
          </p>
        {/if}

        {#if item.cleaned_text}
          <div class="mt-5">
            <div class="label mb-1.5">What una pasted</div>
            <p class="text-[13.5px] leading-relaxed text-muted">{item.cleaned_text}</p>
          </div>
        {/if}

        <div class="mt-5">
          <div class="mb-2 flex items-baseline justify-between gap-4">
            <span class="label">How you'd have written it <span class="text-faint">· optional</span></span>
            <span class="text-[12px] text-faint">Trains your writing style</span>
          </div>
          <textarea
            bind:this={polishEl}
            bind:value={polishEditor}
            rows="1"
            spellcheck="false"
            use:autosize={{ value: polishEditor, min: 60 }}
            class="textarea resize-none overflow-hidden text-[13.5px] leading-relaxed"
            aria-label="Final text editor"
          ></textarea>
        </div>
      </div>

      <div class="flex items-center gap-2 border-t border-line bg-subtle px-6 py-3.5">
        {#if dirty}
          <button class="btn btn-primary" onclick={saveEdit} disabled={acting}>
            Save edit <Kbd>{isMac ? '⌘↵' : 'Ctrl ↵'}</Kbd>
          </button>
          <button class="btn btn-ghost" onclick={() => (editor = item?.raw_text ?? '')} disabled={acting}>
            <Icon name="undo" size={14} /> Revert
          </button>
        {:else}
          <button class="btn btn-primary" onclick={accept} disabled={acting}>
            <Icon name="check" size={14} /> Sounds right <Kbd>↵</Kbd>
          </button>
        {/if}
        <div class="ml-auto flex items-center gap-1.5">
          <button class="btn btn-ghost" onclick={skip} disabled={acting}>
            Skip <Kbd>S</Kbd>
          </button>
          <button class="btn btn-danger" onclick={exclude} disabled={acting}>
            Exclude <Kbd>X</Kbd>
          </button>
        </div>
      </div>
    </div>
  {:else}
    <div class="panel">
      <EmptyState
        title="All caught up"
        sub="Every dictation has been reviewed. New ones show up here as you dictate."
      >
        {#if eligibility}
          <div class="text-left">
            <div class="mb-2.5 flex items-baseline justify-between text-[12.5px]">
              <span class="text-muted">Training data</span>
              <span class="tabular-nums">
                {eligibility.eligible_minutes.toFixed(1)}
                <span class="text-faint">/ {eligibility.threshold_minutes.toFixed(0)} min</span>
              </span>
            </div>
            <ProgressBar
              value={eligibility.eligible_minutes / Math.max(1e-9, eligibility.threshold_minutes)}
            />
            <div class="mt-2.5 flex items-center justify-between text-[12px] text-faint">
              <span class="tabular-nums">
                {eligibility.eligible_pairs} voice pairs · {eligibility.style_pairs} style pairs
              </span>
              {#if eligibility.ready}
                <a href="#/training" class="inline-flex items-center gap-1 font-medium text-fg">
                  Ready to train <Icon name="arrowRight" size={12} />
                </a>
              {/if}
            </div>
          </div>
        {/if}
      </EmptyState>
    </div>
  {/if}

  <div class="mt-auto pt-10">
    <div class="flex flex-wrap items-center justify-center gap-x-5 gap-y-2 text-[12px] text-faint">
      {#each SHORTCUTS as s (s.label)}
        <span class="flex items-center gap-1.5">
          {#each s.keys as k (k)}<Kbd>{k}</Kbd>{/each}
          {s.label}
        </span>
      {/each}
      <span class="flex items-center gap-1.5"><Kbd>Tab</Kbd> Next field</span>
    </div>
  </div>
</div>

<style>
  /* Disputed words need attention: amber, the palette's colour for that. */
  .dispute {
    border-color: color-mix(in oklab, var(--warn) 45%, var(--line));
    background: color-mix(in oklab, var(--warn) 14%, transparent);
  }
</style>
