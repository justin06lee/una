<script lang="ts">
  let { value, label }: { value: number; label?: string } = $props();

  const pct = $derived(Math.max(0, Math.min(1, value)) * 100);
</script>

<div
  class="h-1 w-full overflow-hidden rounded-full bg-hover"
  role="progressbar"
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={Math.round(pct)}
  aria-label={label}
>
  <div class="h-full rounded-full bg-fg transition-[width] duration-500" style="width: {pct}%"></div>
</div>
