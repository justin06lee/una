<script lang="ts">
  /**
   * The una mark: Una's hair bow drawn as a waveform — the app icon's bow at
   * four bars a side, in the text colour so it flips with the theme.
   * `size` is the width; the bow is wider than it is tall.
   */
  let { size = 22 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={(size * 19.5) / 29}
  viewBox="1.5 5.8 29 19.5"
  fill="currentColor"
  aria-hidden="true"
  class="flex-none"
>
  <rect x="13.9" y="15.2" width="4.2" height="6" rx="2.1"/>
  <rect x="10.95" y="15.4" width="2.2" height="5.3" rx="1.1"/>
  <rect x="8" y="12.4" width="2.2" height="9.67" rx="1.1"/>
  <rect x="5.05" y="9.4" width="2.2" height="14.03" rx="1.1"/>
  <rect x="2.1" y="6.4" width="2.2" height="18.4" rx="1.1"/>
  <rect x="18.85" y="15.4" width="2.2" height="5.3" rx="1.1"/>
  <rect x="21.8" y="12.4" width="2.2" height="9.67" rx="1.1"/>
  <rect x="24.75" y="9.4" width="2.2" height="14.03" rx="1.1"/>
  <rect x="27.7" y="6.4" width="2.2" height="18.4" rx="1.1"/>
</svg>
