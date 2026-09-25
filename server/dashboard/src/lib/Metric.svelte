<script lang="ts">
  import type { Snippet } from 'svelte';

  /** A labelled number: the building block of every stat row. */
  let {
    label,
    value,
    unit,
    hint,
    children,
  }: { label: string; value: string; unit?: string; hint?: string; children?: Snippet } = $props();
</script>

<div class="min-w-0">
  <div class="label">{label}</div>
  <div class="mt-2 flex items-baseline gap-1.5">
    <span class="text-[26px] leading-none font-semibold tracking-[-0.03em] tabular-nums">{value}</span>
    {#if unit}<span class="text-[13px] text-muted">{unit}</span>{/if}
  </div>
  {#if hint}<div class="mt-1.5 truncate text-[12px] text-faint">{hint}</div>{/if}
  {#if children}{@render children()}{/if}
</div>
