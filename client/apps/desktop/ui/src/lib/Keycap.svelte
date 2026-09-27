<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { Cap } from "./keycaps";

  let { cap }: { cap: Cap } = $props();
</script>

<span class="cap-wrap">
  {#if cap.side}<span class="side">{cap.side}</span>{/if}
  {#if cap.symbol}
    <kbd class="keycap mod" class:fn={cap.globe}>
      <span class="symbol">{cap.symbol}</span>
      <span class="word">
        {#if cap.globe}<Icon name="globe" size={13} stroke={1.6} />{:else}{cap.word}{/if}
      </span>
    </kbd>
  {:else}
    <kbd class="keycap" class:wide={cap.wide}>{cap.label}</kbd>
  {/if}
</span>

<style>
  .cap-wrap {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }

  .side {
    font-size: 12px;
    color: var(--muted);
  }

  /* A Mac key, flattened: hairline edge, a slightly heavier lower lip. */
  .keycap {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 44px;
    height: 44px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    background: var(--subtle);
    color: var(--fg);
    font-family: var(--font-sans);
    font-size: 15px;
    font-weight: 500;
    line-height: 1;
  }

  .keycap.wide {
    min-width: 88px;
  }

  /* Two legends, as printed: symbol top right, name bottom left. */
  .keycap.mod {
    width: 66px;
    padding: 0;
  }

  .keycap.mod.fn {
    width: 46px;
  }

  .symbol {
    position: absolute;
    top: 6px;
    right: 7px;
    font-size: 13px;
  }

  .word {
    position: absolute;
    bottom: 5px;
    left: 7px;
    display: inline-flex;
    font-size: 10.5px;
    font-weight: 450;
    color: var(--muted);
  }

  .fn .symbol {
    font-size: 12.5px;
    font-weight: 500;
  }
</style>
