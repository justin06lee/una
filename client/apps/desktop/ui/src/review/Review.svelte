<script lang="ts">
  /**
   * The review window: recent dictations, one at a time, with both of their
   * right answers already drafted — what was literally said (trains Whisper)
   * and how it should read (trains the cleanup model).
   *
   * The drafts are what una heard and what it pasted (or your own earlier fix).
   * Most of the time the review is: listen, press ⌘↵. Fix up (F, ⌘J) redoes the
   * written version from the transcript once that is right.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import FixUp from "../lib/FixUp.svelte";
  import Icon from "../lib/Icon.svelte";
  import Mark from "../lib/Mark.svelte";

  type Item = {
    id: string;
    created_at: string;
    app_name: string | null;
    duration_ms: number;
    raw_text: string;
    cleaned_text: string | null;
    cleanup_diverged?: boolean;
    polished_text: string | null;
    correction_source: string | null;
  };
  type Queue = { items: Item[]; pending: number };
  type Saved = { training_eligible?: boolean; eligibility_reason?: string | null };

  const isMac = navigator.userAgent.includes("Mac");
  const RATES = [1, 1.5, 2];

  let items = $state<Item[]>([]);
  let pending = $state(0);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let flash = $state<{ ok: boolean; text: string } | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;

  /** Skipped this session: not shown again until the window is reopened. */
  const skipped = new Set<string>();

  let literal = $state("");
  let polished = $state("");
  let literalEl: HTMLTextAreaElement | undefined = $state();
  let polishedEl: HTMLTextAreaElement | undefined = $state();
  let fixer: FixUp | undefined = $state();
  let fixing = $state(false);

  const item = $derived(items[0] ?? null);
  const literalDraft = $derived(item ? item.raw_text : "");
  const pasted = $derived(item ? (item.cleaned_text ?? item.raw_text) : "");

  /** What una pasted, unless that was a reply rather than a cleanup. */
  function usableCleanup(i: Item): string | null {
    return i.cleanup_diverged ? null : i.cleaned_text;
  }

  // -- audio ------------------------------------------------------------------
  let audio: HTMLAudioElement | undefined = $state();
  let audioUrl = $state<string | null>(null);
  let playing = $state(false);
  let current = $state(0);
  let duration = $state(0);
  let rate = $state(1);

  async function loadAudio(id: string) {
    if (audioUrl) URL.revokeObjectURL(audioUrl);
    audioUrl = null;
    current = 0;
    try {
      const bytes = await invoke<ArrayBuffer>("review_audio", { id });
      if (items[0]?.id !== id) return; // moved on while it loaded
      audioUrl = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
      await tick();
      if (audio) {
        audio.playbackRate = rate;
        audio.play().catch(() => {});
      }
    } catch (e) {
      console.warn("audio unavailable", e);
    }
  }

  function toggle() {
    if (!audio) return;
    if (audio.paused) void audio.play();
    else audio.pause();
  }

  function replay() {
    if (!audio) return;
    audio.currentTime = 0;
    void audio.play();
  }

  function setRate(value: number) {
    rate = value;
    if (audio) audio.playbackRate = value;
  }

  function seek(event: MouseEvent) {
    if (!audio || !duration) return;
    const bar = event.currentTarget as HTMLElement;
    const rect = bar.getBoundingClientRect();
    audio.currentTime = ((event.clientX - rect.left) / rect.width) * duration;
  }

  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

  // -- queue --------------------------------------------------------------------

  function prime(next: Item | null) {
    literal = next ? next.raw_text : "";
    polished = next ? (next.polished_text ?? usableCleanup(next) ?? next.raw_text) : "";
    fixer?.reset();
    if (next) void loadAudio(next.id);
  }

  async function load() {
    loading = true;
    error = null;
    try {
      const queue = await invoke<Queue>("review_queue", { limit: 40 });
      items = queue.items.filter((i) => !skipped.has(i.id));
      pending = queue.pending;
      prime(items[0] ?? null);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function advance() {
    items = items.slice(1);
    if (items.length === 0) await load();
    else prime(items[0]);
  }

  function say(ok: boolean, text: string) {
    flash = { ok, text };
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), 3200);
  }

  async function submit(action: "confirm" | "exclude") {
    if (!item || busy || fixing) return;
    busy = true;
    const said = literal.trim();
    const wrote = polished.trim();
    let args: Record<string, unknown>;
    if (action === "exclude") {
      args = { id: item.id, action: "excluded", correctedText: null, polishedText: null };
    } else {
      const edited = said !== item.raw_text.trim();
      args = {
        id: item.id,
        action: edited ? "edited" : "accepted",
        correctedText: edited ? said : null,
        polishedText: wrote || null,
      };
    }
    try {
      const saved = await invoke<Saved>("review_submit", args);
      pending = Math.max(0, pending - 1);
      if (action === "exclude") say(true, "Excluded from training");
      else if (saved.training_eligible) say(true, "Saved · trains your voice and your style");
      else say(false, `Saved for style · ${saved.eligibility_reason ?? "not used for Whisper"}`);
      audio?.pause();
      await advance();
    } catch (e) {
      say(false, String(e));
    } finally {
      busy = false;
    }
  }

  function skip() {
    if (!item) return;
    skipped.add(item.id);
    audio?.pause();
    void advance();
  }

  async function focusField(el: HTMLTextAreaElement | undefined) {
    await tick();
    el?.focus();
    el?.setSelectionRange(el.value.length, el.value.length);
  }

  function onKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    const typing = target?.tagName === "TEXTAREA";
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      void submit("confirm");
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "j") {
      event.preventDefault();
      void fixer?.run();
      return;
    }
    if (typing) {
      if (event.key === "Escape") {
        event.preventDefault();
        target?.blur();
      } else if (event.key === "Tab" && !event.shiftKey && target === literalEl) {
        event.preventDefault();
        void focusField(polishedEl);
      }
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    // a focused button handles its own Enter/Space
    if (target?.tagName === "BUTTON" && (event.key === "Enter" || event.key === " ")) return;
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        void invoke("review_close");
        return;
      case " ":
        event.preventDefault();
        toggle();
        return;
      case "Enter":
        event.preventDefault();
        void submit("confirm");
        return;
    }
    if (!item) return;
    const key = event.key.toLowerCase();
    if (key === "r") replay();
    else if (key === "1" || key === "2" || key === "3") setRate(RATES[Number(key) - 1]);
    else if (key === "e") void focusField(literalEl);
    else if (key === "w") void focusField(polishedEl);
    else if (key === "s") skip();
    else if (key === "x") void submit("exclude");
    else if (key === "f") void fixer?.run();
    else return;
    event.preventDefault();
  }

  // The window is created hidden at launch and lives for the whole session:
  // it loads (and starts playing) only when opened, and falls silent when closed.
  onMount(() => {
    const opened = listen("review-opened", () => {
      skipped.clear();
      void load();
    });
    const closed = listen("review-closed", () => {
      audio?.pause();
    });
    return () => {
      void opened.then((un) => un());
      void closed.then((un) => un());
      clearTimeout(flashTimer);
      if (audioUrl) URL.revokeObjectURL(audioUrl);
    };
  });

  function when(iso: string) {
    const d = new Date(iso);
    return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class:mac={isMac}>
  <header class="titlebar" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region><Mark size={18} /> Review dictations</span>
    {#if !loading && !error}
      <span class="count" data-tauri-drag-region>
        {pending} left
      </span>
    {/if}
  </header>

  <section class="body">
    {#if error}
      <div class="empty">
        <p class="big">Can't load dictations</p>
        <p class="muted">{error}</p>
        <button class="btn" onclick={() => void load()}>Try again</button>
      </div>
    {:else if loading && !item}
      <div class="empty"><p class="muted">Loading…</p></div>
    {:else if !item}
      <div class="empty">
        <Icon name="check" size={20} />
        <p class="big">All caught up</p>
        <p class="muted">New dictations show up here as you dictate.</p>
      </div>
    {:else}
      <div class="meta">
        {#if item.app_name}<span class="app">{item.app_name}</span><span>·</span>{/if}
        <span>{when(item.created_at)}</span><span>·</span>
        <span class="num">{fmt(item.duration_ms / 1000)}</span>
        <span class="chips">
          {#if item.correction_source === "auto" || item.correction_source === "popup"}
            <span class="chip" title="Pre-filled with the fix you made after it was pasted">
              Your edit
            </span>
          {/if}
        </span>
      </div>

      <div class="player">
        <button class="play" onclick={toggle} aria-label={playing ? "Pause" : "Play"} disabled={!audioUrl}>
          {#if playing}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16" rx="1" /><rect x="14" y="4" width="4" height="16" rx="1" /></svg>
          {:else}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15a1 1 0 0 0 1.52.85l12-7.5a1 1 0 0 0 0-1.7l-12-7.5A1 1 0 0 0 7 4.5Z" /></svg>
          {/if}
        </button>
        <span class="num time">{fmt(current)}</span>
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="track" role="slider" aria-label="Seek" aria-valuenow={current} tabindex="-1" onclick={seek}>
          <div class="fill" style="width: {duration ? (current / duration) * 100 : 0}%"></div>
        </div>
        <span class="num time">{fmt(duration)}</span>
        <div class="seg">
          {#each RATES as r (r)}
            <button class:on={rate === r} onclick={() => setRate(r)}>{r}×</button>
          {/each}
        </div>
        {#if audioUrl}
          <audio
            bind:this={audio}
            src={audioUrl}
            onplay={() => (playing = true)}
            onpause={() => (playing = false)}
            onended={() => (playing = false)}
            ontimeupdate={() => (current = audio?.currentTime ?? 0)}
            onloadedmetadata={() => (duration = audio?.duration ?? 0)}
          ></audio>
        {/if}
      </div>

      <div class="field">
        <div class="label-row">
          <span class="label">What you said</span>
          <span class="hint">Word for word — ums and all. Trains your voice.</span>
        </div>
        <textarea
          class="literal"
          bind:this={literalEl}
          bind:value={literal}
          rows="3"
          spellcheck="false"
          autocapitalize="off"
          {...{ autocorrect: "off" }}
          aria-label="What you said"
        ></textarea>
        <div class="under">
          {#if literal !== literalDraft}
            <span class="hint">Edited.</span>
            <button class="link" onclick={() => (literal = literalDraft)}>Undo</button>
          {/if}
          <FixUp
            from={literal}
            bind:to={polished}
            bind:busy={fixing}
            bind:this={fixer}
            shortcut={isMac ? "⌘J" : "Ctrl+J"}
          />
        </div>
      </div>

      <div class="field">
        <div class="label-row">
          <span class="label">How you'd have written it</span>
          <span class="hint">Your style. Trains the cleanup model.</span>
        </div>
        <textarea
          class="polished"
          bind:this={polishedEl}
          bind:value={polished}
          readonly={fixing}
          class:fixing
          rows="3"
          spellcheck="false"
          autocapitalize="off"
          {...{ autocorrect: "off" }}
          aria-label="How you'd have written it"
        ></textarea>
        {#if polished.trim() !== pasted.trim()}
          <p class="pasted"><span class="hint">una pasted:</span> {pasted}</p>
        {/if}
      </div>
    {/if}
  </section>

  {#if item && !error}
    <footer>
      <button class="btn btn-primary" onclick={() => void submit("confirm")} disabled={busy || fixing}>
        <Icon name="check" size={14} /> Confirm both <span class="kbd">{isMac ? "⌘↵" : "Ctrl ↵"}</span>
      </button>
      {#if flash}
        <span class="flash" class:bad={!flash.ok}>{flash.text}</span>
      {/if}
      <div class="right">
        <button class="btn btn-ghost" onclick={skip} disabled={busy}>Skip <span class="kbd">S</span></button>
        <button class="btn btn-ghost danger" onclick={() => void submit("exclude")} disabled={busy}>
          Exclude <span class="kbd">X</span>
        </button>
      </div>
    </footer>
    <p class="keys">
      <span><span class="kbd">Space</span> play</span>
      <span><span class="kbd">R</span> replay</span>
      <span><span class="kbd">1</span><span class="kbd">2</span><span class="kbd">3</span> speed</span>
      <span><span class="kbd">E</span> <span class="kbd">W</span> edit</span>
      <span><span class="kbd">F</span> fix up</span>
      <span><span class="kbd">↵</span> confirm</span>
      <span><span class="kbd">esc</span> close</span>
    </p>
  {/if}
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .titlebar {
    height: 38px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    border-bottom: 1px solid var(--line);
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
  }
  .title {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .count {
    position: absolute;
    right: 16px;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 20px 8px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--faint);
  }
  .meta .app {
    color: var(--muted);
    font-weight: 500;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .chips {
    margin-left: auto;
    display: flex;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 20px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 999px;
    font-size: 11px;
    color: var(--muted);
    background: var(--panel);
  }

  .player {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }
  .play {
    width: 30px;
    height: 30px;
    flex: none;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 999px;
    background: var(--inverse);
    color: var(--on-inverse);
  }
  .play:disabled {
    opacity: 0.4;
  }
  .time {
    font-size: 11.5px;
    color: var(--muted);
    width: 30px;
  }
  .track {
    flex: 1;
    height: 4px;
    border-radius: 999px;
    background: var(--line-strong);
    cursor: pointer;
    position: relative;
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 999px;
    background: var(--fg);
  }
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .seg button {
    border: 0;
    background: transparent;
    color: var(--faint);
    font-size: 11px;
    padding: 3px 7px;
  }
  .seg button.on {
    background: var(--subtle);
    color: var(--fg);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .label-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .label {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg);
  }
  .hint {
    font-size: 11.5px;
    color: var(--faint);
  }

  textarea {
    resize: none;
    padding: 10px 12px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--fg);
    font: inherit;
    line-height: 1.55;
    user-select: text;
    -webkit-user-select: text;
    transition:
      border-color 140ms ease,
      box-shadow 140ms ease;
  }
  textarea.literal {
    font-size: 15px;
    min-height: 84px;
  }
  textarea.polished {
    font-size: 13.5px;
    min-height: 72px;
  }
  textarea.fixing {
    opacity: 0.55;
  }
  textarea:focus {
    outline: none;
    border-color: var(--muted);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--fg) 8%, transparent);
  }

  .under {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 16px;
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    font-size: 11.5px;
    color: var(--muted);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .link:hover {
    color: var(--fg);
  }
  .pasted {
    font-size: 12px;
    line-height: 1.5;
    color: var(--muted);
  }

  footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
    background: var(--subtle);
  }
  .right {
    margin-left: auto;
    display: flex;
    gap: 4px;
  }
  .danger {
    color: var(--danger);
  }
  .flash {
    font-size: 12px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .flash.bad {
    color: var(--warn);
  }

  .keys {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 4px 14px;
    padding: 8px 20px 12px;
    font-size: 11px;
    color: var(--faint);
    background: var(--subtle);
  }
  .keys > span {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    text-align: center;
    color: var(--muted);
  }
  .big {
    font-size: 15px;
    font-weight: 500;
    color: var(--fg);
  }
  .muted {
    font-size: 12.5px;
    color: var(--faint);
    max-width: 380px;
  }
</style>
