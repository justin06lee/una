<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label,
    value,
    hint,
    tone = 'default',
    children,
  }: {
    label: string;
    value: string;
    hint?: string;
    tone?: 'default' | 'accent';
    children?: Snippet;
  } = $props();
</script>

<div class="tile">
  <div class="label">{label}</div>
  <div class="tile-value" style={tone === 'accent' ? 'color: var(--c-accent)' : undefined}>
    {value}
  </div>
  {#if hint}<div class="text-[12px] text-muted">{hint}</div>{/if}
  {#if children}{@render children()}{/if}
</div>
