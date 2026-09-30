<script lang="ts">
  /**
   * The correction window: what una pasted, editable, in the apps whose text
   * the accessibility API cannot read.
   *
   * Like the review window it has the recording and two texts: what was said,
   * word for word (the raw transcript, which trains the voice model), and how
   * it should read (the pasted text, which trains the cleanup model and is
   * what goes back into the app). The raw field is left out when neither
   * memory nor the server has the transcript.
   *
   * It opens because the user has already started fixing the pasted text, so
   * that field is focused with the caret at the end on every open. The
   * recording plays on ⌘P and never by itself: the window opens mid-edit,
   * and sound out of nowhere would be jarring. Submit files both and rewrites
   * the text in the app; Escape walks away and records nothing. Fix up (⌘J)
   * rewrites what was said as clean text, once the transcript is right.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import AudioPlayer from "../lib/AudioPlayer.svelte";
  import FixUp from "../lib/FixUp.svelte";
  import Mark from "../lib/Mark.svelte";

  type Pending = {
    text: string;
    app_name: string | null;
    can_write_back: boolean;
    raw_text: string | null;
  };

  let pending = $state<Pending | null>(null);
  let text = $state("");
  let said = $state("");
  let busy = $state(false);
  let editor: HTMLTextAreaElement | undefined = $state();
  let player: AudioPlayer | undefined = $state();
  let fixer: FixUp | undefined = $state();
  let fixing = $state(false);
  let audioUrl = $state<string | null>(null);
  let audioMissing = $state(false);

  /** macOS draws the traffic lights over the top of the window (overlay title bar). */
  const isMac = navigator.userAgent.includes("Mac");
  const fixKey = isMac ? "⌘J" : "Ctrl+J";

  const showSaid = $derived(pending?.raw_text != null);
  /** Whether the user changed how it reads (and so what goes back in the app). */
  const dirty = $derived(pending !== null && text !== pending.text);
  /** Whether the user changed the transcript of what was said. */
  const saidDirty = $derived(showSaid && said !== pending?.raw_text);
  const canSubmit = $derived(!!text.trim() && (!showSaid || !!said.trim()));

  async function loadAudio() {
    if (audioUrl) URL.revokeObjectURL(audioUrl);
    audioUrl = null;
    audioMissing = false;
    try {
      const bytes = await invoke<ArrayBuffer>("correction_audio");
      audioUrl = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
    } catch (e) {
      console.warn("recording unavailable", e);
      audioMissing = true;
    }
  }

  async function load() {
    pending = await invoke<Pending | null>("correction_pending");
    text = pending?.text ?? "";
    said = pending?.raw_text ?? "";
    fixer?.reset();
    if (pending) void loadAudio();
    // The window is shown before this resolves; wait a frame so the textarea
    // exists, then put the caret where a person would expect it.
    requestAnimationFrame(() => {
      editor?.focus();
      editor?.setSelectionRange(text.length, text.length);
    });
  }

  onMount(() => {
    void load();
    const opened = listen("correction-opened", () => void load());
    // Hidden, not closed: stop the recording with the window.
    const closed = listen("correction-closed", () => player?.stop());
    return () => {
      void opened.then((un) => un());
      void closed.then((un) => un());
      if (audioUrl) URL.revokeObjectURL(audioUrl);
    };
  });

  async function submit() {
    if (busy || fixing || !canSubmit) return;
    busy = true;
    try {
      await invoke("correction_submit", { text, said: showSaid ? said : null });
    } catch (e) {
      console.error("correction submit failed", e);
    } finally {
      busy = false;
    }
  }

  async function dismiss() {
    if (busy) return;
    await invoke("correction_dismiss");
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      void dismiss();
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void submit();
    } else if (event.key.toLowerCase() === "p" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      player?.toggle();
    } else if (event.key.toLowerCase() === "j" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void fixer?.run();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<main class:mac={isMac}>
  {#if isMac}
    <div class="titlebar" data-tauri-drag-region>
      <Mark size={18} />
      <span data-tauri-drag-region>Fix dictation</span>
    </div>
  {/if}

  <p class="sub">
    {#if pending?.app_name}
      Correct what una pasted into <strong>{pending.app_name}</strong>.
    {:else}
      Correct what una just pasted.
    {/if}
    Your fix teaches it how you'd have written it.
  </p>

  <AudioPlayer
    bind:this={player}
    src={audioUrl}
    unavailable={audioMissing}
    shortcut={isMac ? "⌘P" : "Ctrl+P"}
  />

  {#if showSaid}
    <div class="field">
      <div class="label-row">
        <label class="label" for="said">What you said</label>
        <span class="hint">Word for word — ums and all. Trains your voice.</span>
      </div>
      <textarea
        id="said"
        bind:value={said}
        spellcheck="false"
        autocapitalize="off"
        {...{ autocorrect: "off" }}
        placeholder="What you said, word for word…"
      ></textarea>
      <div class="under">
        <FixUp from={said} bind:to={text} bind:busy={fixing} bind:this={fixer} shortcut={fixKey} />
      </div>
    </div>
  {:else}
    <div class="under">
      <FixUp from={text} bind:to={text} bind:busy={fixing} bind:this={fixer} shortcut={fixKey} />
    </div>
  {/if}

  <div class="field">
    <div class="label-row">
      <label class="label" for="written">How you'd have written it</label>
      <span class="hint">Your style. Trains the cleanup model.</span>
    </div>
    <textarea
      id="written"
      bind:this={editor}
      bind:value={text}
      readonly={fixing}
      class:fixing
      spellcheck="false"
      autocapitalize="off"
      {...{ autocorrect: "off" }}
      placeholder="How it should read…"
    ></textarea>
  </div>

  <footer>
    <span class="hint">
      {#if dirty && pending && !pending.can_write_back}
        Saves your fix only — the cursor moved, so the text in the app stays as it is.
      {:else if dirty}
        Replaces the text in {pending?.app_name ?? "the app"}.
      {:else if saidDirty}
        Leaves the text in the app as it is.
      {:else if showSaid}
        Unchanged — submitting confirms both were right.
      {:else}
        Unchanged — submitting confirms it was right.
      {/if}
    </span>
    <div class="actions">
      <button class="btn btn-ghost" onclick={dismiss} disabled={busy}>
        Cancel <span class="kbd">esc</span>
      </button>
      <button class="btn btn-primary" onclick={submit} disabled={busy || fixing || !canSubmit}>
        Submit <span class="kbd">{isMac ? "⌘↵" : "Ctrl ↵"}</span>
      </button>
    </div>
  </footer>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
    padding: 14px 16px 14px;
  }

  main.mac {
    padding-top: 0;
  }

  /* Sits level with the traffic lights; the whole strip drags the window. */
  .titlebar {
    height: 34px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
    margin: 0 -16px;
    border-bottom: 1px solid var(--line);
    margin-bottom: 4px;
  }

  .sub {
    color: var(--muted);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .sub strong {
    color: var(--fg);
    font-weight: 500;
  }

  /* The two texts share the height the window has. */
  .field {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .label-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 0 2px;
  }

  .label {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg);
  }

  .label-row .hint {
    font-size: 11.5px;
  }

  textarea {
    flex: 1;
    min-height: 44px;
    resize: none;
    padding: 10px 12px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--fg);
    font: inherit;
    font-size: 14px;
    line-height: 1.55;
    user-select: text;
    -webkit-user-select: text;
    transition:
      border-color 140ms ease,
      box-shadow 140ms ease;
  }

  textarea:focus {
    outline: none;
    border-color: var(--muted);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--fg) 8%, transparent);
  }

  textarea.fixing {
    opacity: 0.55;
  }

  /* Fix up, right under what was said: it turns that into the text below. */
  .under {
    display: flex;
    align-items: center;
    min-height: 26px;
  }

  textarea::placeholder {
    color: var(--faint);
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .hint {
    color: var(--faint);
    font-size: 12px;
    line-height: 1.4;
  }

  .actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
</style>
