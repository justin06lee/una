<script lang="ts">
  /**
   * The una mark: Una's hair bow drawn as five waveform bars, the same
   * drawing as the app icon, in the text colour so it flips with the theme.
   * `size` is the width; the bow is wider than it is tall.
   */
  let { size = 22 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size * 0.6169}
  viewBox="0 0 616 380"
  fill="currentColor"
  aria-hidden="true"
  class="flex-none"
>
  <rect x="0" y="0" width="84" height="380" rx="42"/>
  <rect x="126" y="88" width="84" height="262" rx="42"/>
  <rect x="252" y="170" width="112" height="150" rx="56"/>
  <rect x="406" y="88" width="84" height="262" rx="42"/>
  <rect x="532" y="0" width="84" height="380" rx="42"/>
</svg>
