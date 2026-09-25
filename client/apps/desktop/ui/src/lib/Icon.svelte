<script lang="ts" module>
  /**
   * Outline icons for the desktop windows, in the same Lucide idiom (ISC) as
   * the dashboard's icon set: 24px grid, one stroke weight.
   */
  type Shape = string | { circle: [number, number, number] } | { rect: [number, number, number, number, number] };

  const ICONS = {
    sliders: ['M21 4h-7', 'M10 4H3', 'M21 12h-9', 'M8 12H3', 'M21 20h-5', 'M12 20H3', 'M14 2v4', 'M8 10v4', 'M16 18v4'],
    keyboard: [
      { rect: [2, 5, 20, 14, 2] },
      'M6 9h.01',
      'M10 9h.01',
      'M14 9h.01',
      'M18 9h.01',
      'M8 13h.01',
      'M12 13h.01',
      'M16 13h.01',
      'M8 16.5h8',
    ],
    server: [{ rect: [2, 3, 20, 8, 2] }, { rect: [2, 13, 20, 8, 2] }, 'M6 7h.01', 'M6 17h.01'],
    mic: ['M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z', 'M19 10v2a7 7 0 0 1-14 0v-2', 'M12 19v3'],
    clipboard: [
      { rect: [8, 2, 8, 4, 1] },
      'M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2',
    ],
    flask: [
      'M14 2v6a2 2 0 0 0 .25.96l5.5 10.08A2 2 0 0 1 18 22H6a2 2 0 0 1-1.75-2.96l5.5-10.08A2 2 0 0 0 10 8V2',
      'M6.45 15h11.1',
      'M8.5 2h7',
    ],
    info: [{ circle: [12, 12, 9] }, 'M12 16v-4', 'M12 8h.01'],
    check: ['M20 6 9 17l-5-5'],
    x: ['M18 6 6 18', 'm6 6 12 12'],
    plus: ['M5 12h14', 'M12 5v14'],
    wifi: ['M12 20h.01', 'M2 8.82a15 15 0 0 1 20 0', 'M5 12.86a10 10 0 0 1 14 0', 'M8.5 16.43a5 5 0 0 1 7 0'],
    activity: ['M22 12h-4l-3 9L9 3l-3 9H2'],
  } satisfies Record<string, Shape[]>;

  export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
  let { name, size = 16, stroke = 1.75 }: { name: IconName; size?: number; stroke?: number } = $props();

  const shapes = $derived(ICONS[name] as Shape[]);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  style="flex: none"
>
  {#each shapes as s, i (i)}
    {#if typeof s === "string"}
      <path d={s} />
    {:else if "circle" in s}
      <circle cx={s.circle[0]} cy={s.circle[1]} r={s.circle[2]} />
    {:else}
      <rect x={s.rect[0]} y={s.rect[1]} width={s.rect[2]} height={s.rect[3]} rx={s.rect[4]} />
    {/if}
  {/each}
</svg>
