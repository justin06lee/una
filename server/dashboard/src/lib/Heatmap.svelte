<script lang="ts">
  import type { StatsDay } from '../api';
  import { fmtNum } from './format';

  let { days }: { days: StatsDay[] } = $props();

  /** Cell edge and gap in px; the grid shows as many weeks as fit the width. */
  const CELL = 12;
  const GAP = 3;

  let width = $state(0);
  const weeks = $derived(Math.max(12, Math.min(53, Math.floor((width + GAP) / (CELL + GAP)))));

  function key(d: Date): string {
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(
      d.getDate(),
    ).padStart(2, '0')}`;
  }

  const byDay = $derived(new Map(days.map((d) => [d.day, d])));
  const max = $derived(Math.max(1, ...days.map((d) => d.words)));

  /**
   * Columns of 7 cells, oldest week first, ending on the current week.
   *
   * Days are stepped with calendar arithmetic, not by adding 24h of
   * milliseconds: across a DST change that lands two cells on one date.
   */
  const grid = $derived.by(() => {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    // The Saturday ending this week, then back to the Sunday `weeks` weeks earlier.
    const endOffset = 6 - today.getDay();
    const startOffset = endOffset - (weeks * 7 - 1);
    const columns: { date: Date; k: string; future: boolean }[][] = [];
    for (let w = 0; w < weeks; w++) {
      const column: { date: Date; k: string; future: boolean }[] = [];
      for (let d = 0; d < 7; d++) {
        const date = new Date(
          today.getFullYear(),
          today.getMonth(),
          today.getDate() + startOffset + w * 7 + d,
        );
        column.push({ date, k: key(date), future: date.getTime() > today.getTime() });
      }
      columns.push(column);
    }
    return columns;
  });

  /**
   * Month labels, placed on the column where a new month starts. A label
   * needs about three columns of room, so one that would crowd the previous
   * label (the partial first month, usually) is dropped.
   */
  const months = $derived.by(() => {
    let lastLabelled = -Infinity;
    return grid.map((column, i) => {
      const first = column.at(0)?.date;
      if (!first) return '';
      const previous = i > 0 ? grid[i - 1]?.at(0)?.date : undefined;
      const isNew = previous !== undefined && previous.getMonth() !== first.getMonth();
      if (!isNew || i - lastLabelled < 3 || i > grid.length - 2) return '';
      lastLabelled = i;
      return first.toLocaleDateString(undefined, { month: 'short' });
    });
  });

  function level(words: number): number {
    if (words <= 0) return 0;
    const share = words / max;
    if (share > 0.66) return 4;
    if (share > 0.36) return 3;
    if (share > 0.12) return 2;
    return 1;
  }

  /** A grey ramp from the empty cell up to full foreground. */
  const FILL = [
    'var(--hover)',
    'color-mix(in oklab, var(--fg) 20%, var(--hover))',
    'color-mix(in oklab, var(--fg) 42%, var(--hover))',
    'color-mix(in oklab, var(--fg) 68%, var(--hover))',
    'var(--fg)',
  ];

  function tip(k: string, future: boolean): string {
    if (future) return '';
    const row = byDay.get(k);
    const when = new Date(`${k}T00:00:00`).toLocaleDateString(undefined, {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
    });
    if (!row) return `${when} — nothing dictated`;
    return `${when} — ${fmtNum(row.words)} words · ${row.n} dictation${row.n === 1 ? '' : 's'}`;
  }
</script>

<div class="space-y-2" bind:clientWidth={width}>
  <div class="flex" style="gap: {GAP}px">
    {#each grid as column, i (column[0]?.k ?? i)}
      <div class="flex flex-none flex-col" style="gap: {GAP}px; width: {CELL}px">
        <!-- Labels overflow their column instead of widening it. -->
        <div class="h-4 overflow-visible text-[11px] leading-3 whitespace-nowrap text-faint">{months[i]}</div>
        {#each column as cell (cell.k)}
          <div
            class="rounded-[3px]"
            style="width: {CELL}px; height: {CELL}px; background: {cell.future
              ? 'transparent'
              : FILL[level(byDay.get(cell.k)?.words ?? 0)]}"
            title={tip(cell.k, cell.future)}
          ></div>
        {/each}
      </div>
    {/each}
  </div>
  <div class="flex items-center justify-end gap-1.5 text-[11px] text-faint">
    <span class="mr-0.5">Less</span>
    {#each FILL as fill, i (i)}
      <span class="h-[10px] w-[10px] rounded-[2.5px]" style="background: {fill}"></span>
    {/each}
    <span class="ml-0.5">More</span>
  </div>
</div>
