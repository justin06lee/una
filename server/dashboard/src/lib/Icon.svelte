<script lang="ts" module>
  /**
   * The dashboard's icon set: 24px-grid outline glyphs in the Lucide idiom
   * (ISC), drawn with a single stroke weight so every icon reads the same.
   * Filled shapes are marked `fill` and ignore the stroke.
   */
  type Shape =
    | string
    | { circle: [number, number, number]; fill?: boolean }
    | { rect: [number, number, number, number, number]; fill?: boolean }
    | { path: string; fill: true };

  const ICONS = {
    home: [
      'M3 10.2 12 3l9 7.2V20a1 1 0 0 1-1 1h-5v-6h-6v6H4a1 1 0 0 1-1-1Z',
    ],
    inbox: [
      'M22 12h-6l-2 3h-4l-2-3H2',
      'M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11Z',
    ],
    book: [
      'M12 7v14',
      'M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3Z',
    ],
    flask: [
      'M14 2v6a2 2 0 0 0 .25.96l5.5 10.08A2 2 0 0 1 18 22H6a2 2 0 0 1-1.75-2.96l5.5-10.08A2 2 0 0 0 10 8V2',
      'M6.45 15h11.1',
      'M8.5 2h7',
    ],
    chart: ['M3 3v16a2 2 0 0 0 2 2h16', 'M18 17V9', 'M13 17V5', 'M8 17v-3'],
    settings: [
      'M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2Z',
      { circle: [12, 12, 3] },
    ],
    search: [{ circle: [11, 11, 7] }, 'm20 20-3.5-3.5'],
    copy: [
      { rect: [8, 8, 13, 13, 2] },
      'M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2',
    ],
    check: ['M20 6 9 17l-5-5'],
    x: ['M18 6 6 18', 'm6 6 12 12'],
    trash: ['M3 6h18', 'M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6', 'M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2'],
    pencil: [
      'M21.17 6.81a1 1 0 0 0-3.99-3.99L3.84 16.17a2 2 0 0 0-.5.83l-1.32 4.35a.5.5 0 0 0 .62.62l4.35-1.32a2 2 0 0 0 .83-.5Z',
    ],
    play: [{ path: 'M7 4.8v14.4a1 1 0 0 0 1.52.85l11.5-7.2a1 1 0 0 0 0-1.7L8.52 3.95A1 1 0 0 0 7 4.8Z', fill: true }],
    pause: [
      { rect: [6, 4, 4, 16, 1], fill: true },
      { rect: [14, 4, 4, 16, 1], fill: true },
    ],
    replay: ['M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8', 'M3 3v5h5'],
    undo: ['M9 14 4 9l5-5', 'M4 9h10.5a5.5 5.5 0 0 1 0 11H11'],
    skip: ['m5 4 10 8-10 8Z', 'M19 5v14'],
    ban: [{ circle: [12, 12, 9] }, 'm5.7 5.7 12.6 12.6'],
    sun: [
      { circle: [12, 12, 4] },
      'M12 2v2',
      'M12 20v2',
      'm4.93 4.93 1.41 1.41',
      'm17.66 17.66 1.41 1.41',
      'M2 12h2',
      'M20 12h2',
      'm6.34 17.66-1.41 1.41',
      'm19.07 4.93-1.41 1.41',
    ],
    moon: ['M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z'],
    monitor: [{ rect: [2, 3, 20, 14, 2] }, 'M8 21h8', 'M12 17v4'],
    plus: ['M5 12h14', 'M12 5v14'],
    arrowRight: ['M5 12h14', 'm12 5 7 7-7 7'],
    chevronDown: ['m6 9 6 6 6-6'],
    terminal: ['m4 17 6-6-6-6', 'M12 19h8'],
    square: [{ rect: [5, 5, 14, 14, 2] }],
    mic: [
      'M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z',
      'M19 10v2a7 7 0 0 1-14 0v-2',
      'M12 19v3',
    ],
  } satisfies Record<string, Shape[]>;

  export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
  let {
    name,
    size = 16,
    stroke = 1.75,
    class: klass = '',
  }: { name: IconName; size?: number; stroke?: number; class?: string } = $props();

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
  class="flex-none {klass}"
>
  {#each shapes as s, i (i)}
    {#if typeof s === 'string'}
      <path d={s} />
    {:else if 'circle' in s}
      <circle
        cx={s.circle[0]}
        cy={s.circle[1]}
        r={s.circle[2]}
        fill={s.fill ? 'currentColor' : 'none'}
      />
    {:else if 'rect' in s}
      <rect
        x={s.rect[0]}
        y={s.rect[1]}
        width={s.rect[2]}
        height={s.rect[3]}
        rx={s.rect[4]}
        fill={s.fill ? 'currentColor' : 'none'}
        stroke={s.fill ? 'none' : 'currentColor'}
      />
    {:else}
      <path d={s.path} fill="currentColor" stroke="none" />
    {/if}
  {/each}
</svg>
