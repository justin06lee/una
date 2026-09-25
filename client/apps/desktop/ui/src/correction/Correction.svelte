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
  import Mark from "../lib/Mark.svelte";

  type Pending = {
    text: string;
    app_name: string | null;
    can_write_back: boolean;
  };

  let pending = $state<Pending | null>(null);
  let text = $state("");
  let busy = $state(false);
  let editor: HTMLTextAreaElement | undefined = $state();

  /** macOS draws the traffic lights over the top of the window (overlay title bar). */
  const isMac = navigator.userAgent.includes("Mac");

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

<svelte:window onkeydown={onKeydown} />

<main class:mac={isMac}>
  {#if isMac}
    <div class="titlebar" data-tauri-drag-region>
      <Mark size={14} />
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

  <textarea
    bind:this={editor}
    bind:value={text}
    spellcheck="false"
    autocapitalize="off"
    {...{ autocorrect: "off" }}
    placeholder="What you actually said…"
    aria-label="Corrected text"
  ></textarea>

  <footer>
    <span class="hint">
      {#if pending && !pending.can_write_back}
        Saves the correction only — the cursor moved, so the text in the app stays as it is.
      {:else if dirty}
        Replaces the text in the app.
      {:else}
        Unchanged — submitting confirms it was right.
      {/if}
    </span>
    <div class="actions">
      <button class="btn btn-ghost" onclick={dismiss} disabled={busy}>
        Cancel <span class="kbd">esc</span>
      </button>
      <button class="btn btn-primary" onclick={submit} disabled={busy || !text.trim()}>
        Submit <span class="kbd">⌘↵</span>
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

  textarea {
    flex: 1;
    min-height: 0;
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
