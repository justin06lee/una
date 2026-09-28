<script lang="ts">
  /**
   * "Fix up": what was said, rewritten as clean text by the model picked in
   * settings (through yagami on this machine), put into the written field.
   *
   * It sits between the two texts of the fix and review windows, since it
   * turns the one above into the one below. The previous wording stays one
   * click away until the field is edited again.
   */
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";

  let {
    from,
    to = $bindable(),
    busy = $bindable(false),
    shortcut,
  }: { from: string; to: string; busy?: boolean; shortcut: string } = $props();

  let error = $state<string | null>(null);
  let done = $state<{ model: string; before: string; after: string } | null>(null);

  const canRun = $derived(!busy && !!from.trim());

  export async function run() {
    if (!canRun) return;
    busy = true;
    error = null;
    const before = to;
    try {
      const res = await invoke<{ text: string; model: string }>("fixup_text", { text: from });
      to = res.text;
      done = { model: res.model, before, after: res.text };
    } catch (e) {
      error = String(e);
      done = null;
    } finally {
      busy = false;
    }
  }

  /** Forget the last run, for when the window moves on to another dictation. */
  export function reset() {
    error = null;
    done = null;
  }
</script>

<span class="fixup">
  {#if busy}
    <span class="status">Fixing up…</span>
  {:else if error}
    <span class="status bad" title={error}>{error}</span>
  {:else if done && to === done.after}
    <span class="status">Fixed up by {done.model}</span>
    <button class="link" onclick={() => done && (to = done.before)}>Undo</button>
  {/if}
  <button
    class="btn btn-sm"
    onclick={() => void run()}
    disabled={!canRun}
    title="Rewrite what you said as clean text"
  >
    {#if busy}<span class="spinner"></span>{:else}<Icon name="wand" size={13} />{/if}
    Fix up <span class="kbd">{shortcut}</span>
  </button>
</span>

<style>
  .fixup {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    margin-left: auto;
  }
  .status {
    font-size: 11.5px;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .status.bad {
    color: var(--warn);
    max-width: 320px;
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
  .btn {
    flex: none;
  }
  .spinner {
    width: 11px;
    height: 11px;
    border-radius: 999px;
    border: 1.5px solid var(--line-strong);
    border-top-color: var(--fg);
    animation: spin 700ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
