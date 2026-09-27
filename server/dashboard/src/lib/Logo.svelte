<script lang="ts">
  /**
   * The una mark: Una's hair bow drawn as seven waveform bars, the same
   * drawing as the app icon, in the text colour so it flips with the theme.
   * `size` is the width; the bow is wider than it is tall.
   */
  let { size = 22 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size * 0.5952}
  viewBox="0 0 604.8 360.0"
  fill="currentColor"
  aria-hidden="true"
  class="flex-none"
>
  <rect x="259.2" y="156.6" width="86.4" height="136.8" rx="43.2"/>
  <rect x="172.8" y="158.4" width="59.4" height="126" rx="29.7"/>
  <rect x="86.4" y="79.2" width="59.4" height="243" rx="29.7"/>
  <rect x="0" y="0" width="59.4" height="360" rx="29.7"/>
  <rect x="372.6" y="158.4" width="59.4" height="126" rx="29.7"/>
  <rect x="459" y="79.2" width="59.4" height="243" rx="29.7"/>
  <rect x="545.4" y="0" width="59.4" height="360" rx="29.7"/>
</svg>
