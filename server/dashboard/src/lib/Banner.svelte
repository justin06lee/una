<script lang="ts">
  /** Inline error notice with an optional retry. */
  let { message, onretry }: { message: string; onretry?: () => void } = $props();
</script>

<div
  class="mb-5 flex items-center gap-3 rounded-lg border px-3.5 py-2.5 text-[13px] fade-in"
  style="border-color: color-mix(in oklab, var(--danger) 30%, var(--line)); background: var(--danger-soft)"
  role="alert"
>
  <span class="dot dot-danger"></span>
  <span class="min-w-0 flex-1 text-fg">{message}</span>
  {#if onretry}<button class="btn btn-sm" onclick={onretry}>Retry</button>{/if}
</div>
