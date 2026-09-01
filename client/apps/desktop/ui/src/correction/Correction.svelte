<script lang="ts">
  /**
   * The correction window: what una pasted, editable, in the apps whose text
   * the accessibility API cannot read.
   *
   * It opens because the user has already started fixing the text by hand, so
   * it has to be immediately typeable — the textarea is focused and the caret
   * placed at the end on every open. Submit rewrites the text in the app it
   * came from and files the pair for training; Escape walks away and records
   * nothing.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  type Pending = {
    text: string;
    app_name: string | null;
    can_write_back: boolean;
  };

  let pending = $state<Pending | null>(null);
  let text = $state("");
  let busy = $state(false);
  let editor: HTMLTextAreaElement | undefined = $state();

  /** Whether the user actually changed anything. */
  const dirty = $derived(pending !== null && text !== pending.text);

  async function load() {
    pending = await invoke<Pending | null>("correction_pending");
    text = pending?.text ?? "";
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
    return () => {
      void opened.then((un) => un());
    };
  });

  async function submit() {
    if (busy || !text.trim()) return;
    busy = true;
    try {
      await invoke("correction_submit", { text });
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
    }
  }
</script>

<svelte:window on:keydown={onKeydown} />

<main>
  <header>
    <h1>Fix this dictation</h1>
    <p class="sub">
      {#if pending?.app_name}
        Correcting the text una pasted into <strong>{pending.app_name}</strong>.
      {:else}
        Correcting the text una just pasted.
      {/if}
      Your edit trains the model on this recording.
    </p>
  </header>

  <textarea
    bind:this={editor}
    bind:value={text}
    spellcheck="false"
    autocapitalize="off"
    autocorrect="off"
    placeholder="What you actually said…"
  ></textarea>

  <footer>
    <span class="hint">
      {#if pending && !pending.can_write_back}
        Records the correction only — the caret moved, so the text won't be replaced.
      {:else if dirty}
        Replaces the text in place.
      {:else}
        Unchanged — submitting confirms the transcription was right.
      {/if}
    </span>
    <div class="actions">
      <button class="ghost" onclick={dismiss} disabled={busy}>
        Cancel <kbd>esc</kbd>
      </button>
      <button class="primary" onclick={submit} disabled={busy || !text.trim()}>
        Submit <kbd>⌘↩</kbd>
      </button>
    </div>
  </footer>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    padding: 16px 18px 14px;
    box-sizing: border-box;
  }

  header {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  h1 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .sub {
    margin: 0;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.45;
  }

  .sub strong {
    color: var(--text);
    font-weight: 600;
  }

  textarea {
    flex: 1;
    min-height: 0;
    resize: none;
    padding: 10px 12px;
    border: 1px solid var(--edge);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    line-height: 1.5;
    user-select: text;
    -webkit-user-select: text;
  }

  textarea:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 55%, var(--edge));
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .hint {
    color: var(--faint);
    font-size: 11.5px;
    line-height: 1.35;
  }

  .actions {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }

  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--edge);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-weight: 500;
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    background: var(--hover);
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .primary {
    background: var(--accent);
    border-color: transparent;
    color: var(--on-accent);
  }

  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .ghost {
    background: transparent;
  }

  kbd {
    font: inherit;
    font-size: 11px;
    opacity: 0.7;
  }
</style>
