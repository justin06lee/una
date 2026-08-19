<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    sub,
    children,
  }: { title: string; sub?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center justify-center py-16 text-center">
  <div
    class="mb-3 flex h-10 w-10 items-center justify-center rounded-full border border-border bg-raised text-faint"
  >
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M20 6 9 17l-5-5" />
    </svg>
  </div>
  <div class="text-sm font-medium">{title}</div>
  {#if sub}<div class="mt-1 max-w-sm text-xs text-muted">{sub}</div>{/if}
  {#if children}<div class="mt-5 w-full max-w-sm">{@render children()}</div>{/if}
</div>
