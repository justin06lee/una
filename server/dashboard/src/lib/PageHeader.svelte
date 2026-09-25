<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    sub,
    actions,
  }: { title: string; sub?: string; actions?: Snippet } = $props();
</script>

<header class="mb-8 flex items-start justify-between gap-6">
  <div class="min-w-0">
    <h1 class="text-[22px] leading-tight font-semibold tracking-[-0.02em]">{title}</h1>
    {#if sub}<p class="mt-1.5 max-w-xl text-[13.5px] leading-relaxed text-muted">{sub}</p>{/if}
  </div>
  {#if actions}<div class="flex flex-none items-center gap-2 pt-0.5">{@render actions()}</div>{/if}
</header>
