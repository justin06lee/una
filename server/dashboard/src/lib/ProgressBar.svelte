<script lang="ts">
  let { value, tone = 'accent' }: { value: number; tone?: 'accent' | 'ok' } = $props();

  const pct = $derived(Math.max(0, Math.min(1, value)) * 100);
</script>

<div
  class="h-1.5 w-full overflow-hidden rounded-full"
  style="background: color-mix(in oklab, var(--color-{tone}) 16%, transparent)"
>
  <div
    class="h-full rounded-full transition-[width] duration-300"
    style="width: {pct}%; background: var(--color-{tone})"
  ></div>
</div>
