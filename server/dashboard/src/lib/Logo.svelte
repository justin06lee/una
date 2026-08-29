<script lang="ts">
  /** The una mark: a mic on a rounded accent tile. */
  let { size = 26 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 32 32"
  fill="none"
  aria-hidden="true"
  class="flex-none"
>
  <rect width="32" height="32" rx="9" fill="var(--c-accent)" />
  <rect x="13.25" y="7" width="5.5" height="12" rx="2.75" fill="var(--c-on-accent)" />
  <path
    d="M9.5 15.5a6.5 6.5 0 0 0 13 0"
    stroke="var(--c-on-accent)"
    stroke-width="2.4"
    stroke-linecap="round"
  />
  <path d="M16 22v3.5" stroke="var(--c-on-accent)" stroke-width="2.4" stroke-linecap="round" />
</svg>
