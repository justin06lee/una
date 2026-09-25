<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    sub,
    children,
  }: { title: string; sub?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center justify-center px-6 py-14 text-center">
  <div class="text-[14px] font-medium">{title}</div>
  {#if sub}<div class="mt-1 max-w-sm text-[13px] text-muted">{sub}</div>{/if}
  {#if children}<div class="mt-6 w-full max-w-sm">{@render children()}</div>{/if}
</div>
