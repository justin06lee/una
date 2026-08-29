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

  function label(day: string): string {
    const [y = 1970, m = 1, d = 1] = day.split('-').map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }
</script>

{#if series.length === 0}
  <div class="py-10 text-center text-[13px] text-faint">No dictations yet.</div>
{:else}
  <div class="flex h-36 items-end gap-[3px]">
    {#each series as day (day.day)}
      {@const value = metric === 'words' ? day.words : day.n}
      <div
        class="group flex h-full flex-1 items-end"
        title="{label(day.day)} — {fmtNum(day.words)} words · {day.n} dictation{day.n === 1
          ? ''
          : 's'} · {fmtSpan(day.ms)}"
      >
        <div
          class="w-full rounded-t-[3px] transition-[height,opacity] duration-500 group-hover:opacity-100"
          style="height: {Math.max(2, (value / max) * 100)}%; background: var(--c-accent); opacity: {value >
          0
            ? 0.85
            : 0.25}"
        ></div>
      </div>
    {/each}
  </div>
  <div class="mt-2 flex justify-between text-[11px] text-faint">
    <span>{first ? label(first.day) : ''}</span>
    <span>{last ? label(last.day) : ''}</span>
  </div>
{/if}
