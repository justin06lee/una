<script lang="ts">
  /** The una mark: the app icon's three bars on a dark rounded square. */
  let { size = 22 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true" style="flex: none">
  <rect x="0.5" y="0.5" width="31" height="31" rx="8" fill="#141414" stroke="var(--line-strong)" />
  <g fill="#ffffff">
    <rect x="9.25" y="12.75" width="3" height="6.5" rx="1.5" />
    <rect x="14.5" y="9.25" width="3" height="13.5" rx="1.5" />
    <rect x="19.75" y="11.4" width="3" height="9.2" rx="1.5" />
  </g>
</svg>
