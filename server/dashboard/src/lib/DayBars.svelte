<script lang="ts">
  import type { StatsDay } from '../api';
  import { fmtNum, fmtSpan } from './format';

  let {
    days,
    limit = 30,
    metric = 'words',
  }: { days: StatsDay[]; limit?: number; metric?: 'words' | 'n' } = $props();

  /** Server sends newest-first; charts read left-to-right oldest-first. */
  const series = $derived([...days].slice(0, limit).reverse());
  const max = $derived(Math.max(1, ...series.map((d) => (metric === 'words' ? d.words : d.n))));
  const first = $derived(series.at(0));
  const last = $derived(series.at(-1));

  let hover = $state<number | null>(null);
  const hovered = $derived(hover !== null ? (series[hover] ?? null) : null);

  function label(day: string): string {
    const [y = 1970, m = 1, d = 1] = day.split('-').map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }
</script>

{#if series.length === 0}
  <div class="py-10 text-center text-[13px] text-muted">No dictations yet.</div>
{:else}
  <div class="flex h-36 items-end gap-[3px]" role="img" aria-label="Words dictated per day">
    {#each series as day, i (day.day)}
      {@const value = metric === 'words' ? day.words : day.n}
      <div
        class="flex h-full flex-1 items-end"
        role="presentation"
        onmouseenter={() => (hover = i)}
        onmouseleave={() => (hover = null)}
      >
        <div
          class="w-full rounded-[3px] transition-[height,background-color] duration-300"
          style="height: {Math.max(2, (value / max) * 100)}%; background: {value === 0
            ? 'var(--hover)'
            : hover === i
              ? 'var(--fg)'
              : hover === null
                ? 'color-mix(in oklab, var(--fg) 70%, var(--panel))'
                : 'color-mix(in oklab, var(--fg) 25%, var(--panel))'}"
        ></div>
      </div>
    {/each}
  </div>
  <div class="mt-2.5 flex justify-between gap-4 text-[12px] text-faint">
    <span>{first ? label(first.day) : ''}</span>
    {#if hovered}
      <span class="text-fg tabular-nums">
        {label(hovered.day)} · {fmtNum(hovered.words)} words · {hovered.n} dictation{hovered.n === 1
          ? ''
          : 's'} · {fmtSpan(hovered.ms)}
      </span>
    {/if}
    <span>{last ? label(last.day) : ''}</span>
  </div>
{/if}
