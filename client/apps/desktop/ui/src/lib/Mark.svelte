<script lang="ts">
  /**
   * The una mark: Una's hair bow drawn as a waveform — the app icon's bow at
   * four bars a side, in the text colour so it flips with the theme.
   * `size` is the width; the bow is wider than it is tall. From 48px up it
   * switches to the app icon's finer five-bars-a-side bow.
   */
  let { size = 22 }: { size?: number } = $props();
</script>

{#if size >= 48}
  <svg
    width={size}
    height={size * 0.6226}
    viewBox="6.78 10.07 18.45 11.48"
    fill="currentColor"
    aria-hidden="true"
    style="flex: none"
  >
    <rect x="14.6" y="14.97" width="2.79" height="4.51" rx="1.4"/>
    <rect x="13.1" y="15.66" width="1.07" height="2.96" rx="0.54"/>
    <rect x="11.59" y="14.04" width="1.07" height="5.4" rx="0.54"/>
    <rect x="10.09" y="12.73" width="1.07" height="7.35" rx="0.54"/>
    <rect x="8.58" y="11.52" width="1.07" height="9.16" rx="0.54"/>
    <rect x="7.08" y="10.37" width="1.07" height="10.88" rx="0.54"/>
    <rect x="17.83" y="15.66" width="1.07" height="2.96" rx="0.54"/>
    <rect x="19.33" y="14.04" width="1.07" height="5.4" rx="0.54"/>
    <rect x="20.84" y="12.73" width="1.07" height="7.35" rx="0.54"/>
    <rect x="22.34" y="11.52" width="1.07" height="9.16" rx="0.54"/>
    <rect x="23.85" y="10.37" width="1.07" height="10.88" rx="0.54"/>
  </svg>
{:else}
  <svg
    width={size}
    height={(size * 19.5) / 29}
    viewBox="1.5 5.8 29 19.5"
    fill="currentColor"
    aria-hidden="true"
    style="flex: none"
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
{/if}
