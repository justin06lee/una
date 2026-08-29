<script lang="ts">
  import type { StatsDay } from '../api';
  import { fmtNum } from './format';

  let { days, weeks = 26 }: { days: StatsDay[]; weeks?: number } = $props();

  const DAY_MS = 86_400_000;

  function key(d: Date): string {
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(
      d.getDate(),
    ).padStart(2, '0')}`;
  }

  const byDay = $derived(new Map(days.map((d) => [d.day, d])));
  const max = $derived(Math.max(1, ...days.map((d) => d.words)));

  /** Columns of 7 cells, oldest week first, ending on the current week. */
  const grid = $derived.by(() => {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    // Walk back to the most recent Sunday, then back `weeks` weeks.
    const end = new Date(today.getTime() + (6 - today.getDay()) * DAY_MS);
    const start = new Date(end.getTime() - (weeks * 7 - 1) * DAY_MS);
    const columns: { date: Date; k: string; future: boolean }[][] = [];
    for (let w = 0; w < weeks; w++) {
      const column: { date: Date; k: string; future: boolean }[] = [];
      for (let d = 0; d < 7; d++) {
        const date = new Date(start.getTime() + (w * 7 + d) * DAY_MS);
        column.push({ date, k: key(date), future: date.getTime() > today.getTime() });
      }
      columns.push(column);
    }
    return columns;
  });

  /** Month labels, placed on the column where a new month starts. */
  const months = $derived.by(() =>
    grid.map((column, i) => {
      const first = column.at(0)?.date;
      if (!first) return '';
      const previous = i > 0 ? grid[i - 1]?.at(0)?.date : undefined;
      const isNew = !previous || previous.getMonth() !== first.getMonth();
      return isNew && i < grid.length - 1
        ? first.toLocaleDateString(undefined, { month: 'short' })
        : '';
    }),
  );

  function level(words: number): number {
    if (words <= 0) return 0;
    const share = words / max;
    if (share > 0.66) return 4;
    if (share > 0.36) return 3;
    if (share > 0.12) return 2;
    return 1;
  }

  const FILL = [
    'var(--c-raised)',
    'color-mix(in oklab, var(--c-accent) 22%, var(--c-raised))',
    'color-mix(in oklab, var(--c-accent) 45%, var(--c-raised))',
    'color-mix(in oklab, var(--c-accent) 70%, var(--c-raised))',
    'var(--c-accent)',
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

<div class="space-y-1.5">
  <div class="flex gap-[3px] overflow-x-auto pb-0.5">
    {#each grid as column, i (i)}
      <div class="flex flex-none flex-col gap-[3px]">
        <div class="h-3 text-[10px] leading-3 text-faint">{months[i]}</div>
        {#each column as cell (cell.k)}
          <div
            class="h-[11px] w-[11px] rounded-[3px]"
            style="background: {cell.future ? 'transparent' : FILL[level(byDay.get(cell.k)?.words ?? 0)]}"
            title={tip(cell.k, cell.future)}
          ></div>
        {/each}
      </div>
    {/each}
  </div>
  <div class="flex items-center justify-end gap-1.5 text-[11px] text-faint">
    <span>Less</span>
    {#each FILL as fill, i (i)}
      <span class="h-[11px] w-[11px] rounded-[3px]" style="background: {fill}"></span>
    {/each}
    <span>More</span>
  </div>
</div>
