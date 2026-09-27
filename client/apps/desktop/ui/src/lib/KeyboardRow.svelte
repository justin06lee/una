<script lang="ts">
  import Keycap from "./Keycap.svelte";
  import { BOTTOM_ROW } from "./keycaps";

  let {
    selected,
    onpick,
  }: {
    /** Keycode of the bound key, if it is one of these. */
    selected: number | null;
    onpick: (keycode: number, name: string) => void;
  } = $props();
</script>

<div class="kb">
  <div class="kb-row" role="radiogroup" aria-label="Hotkey">
    {#each BOTTOM_ROW as key, i (i)}
      {#if "space" in key}
        <span class="kb-space" style:flex-grow={key.grow} aria-hidden="true">space</span>
      {:else}
        <button
          class="kb-key"
          style:flex-grow={key.grow}
          role="radio"
          aria-checked={selected === key.keycode}
          aria-label={key.name}
          title={key.name}
          onclick={() => onpick(key.keycode, key.name)}
        >
          <Keycap cap={key.cap} fill selected={selected === key.keycode} />
        </button>
      {/if}
    {/each}
  </div>
</div>

<style>
  .kb {
    container-type: inline-size;
  }

  .kb-row {
    display: flex;
    gap: 6px;
  }

  .kb-key,
  .kb-space {
    flex-basis: 0;
    min-width: 0;
  }

  .kb-key {
    appearance: none;
    display: flex;
    padding: 0;
    border: 0;
    background: none;
    border-radius: 8px;
    cursor: pointer;
    transition: transform 80ms ease;
  }

  .kb-key:hover :global(.keycap:not(.selected)) {
    background: var(--hover);
    border-color: var(--muted);
  }

  .kb-key:active {
    transform: translateY(1px);
  }

  .kb-key:focus-visible {
    outline: 2px solid var(--fg);
    outline-offset: 2px;
  }

  /* The space bar is only there to put the right-hand keys on the right. */
  .kb-space {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px dashed var(--line-strong);
    border-radius: 8px;
    color: var(--faint);
    font-size: 12px;
  }

  /* Too narrow for "command" under ⌘: keep just the symbols, centred. */
  @container (max-width: 430px) {
    .kb-key :global(.keycap.mod:not(.fn) .word) {
      display: none;
    }

    .kb-key :global(.keycap.mod:not(.fn) .symbol) {
      position: static;
      font-size: 15px;
    }
  }
</style>
