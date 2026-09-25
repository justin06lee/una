<script lang="ts">
  import type { StatsApp } from '../api';
  import { fmtNum, fmtSpan } from './format';

  let { rows }: { rows: StatsApp[] } = $props();

  const total = $derived(rows.reduce((sum, r) => sum + r.words, 0));
  const max = $derived(Math.max(1, ...rows.map((r) => r.words)));
</script>

<div class="space-y-3">
  {#each rows as row (row.app)}
    {@const share = total > 0 ? row.words / total : 0}
    <div
      class="grid grid-cols-[8.5rem_1fr_5.5rem] items-center gap-4"
      title="{row.app} — {fmtNum(row.words)} words over {row.n} dictation{row.n === 1
        ? ''
        : 's'} ({fmtSpan(row.ms)})"
    >
      <span class="truncate text-[13px]">{row.app}</span>
      <div class="h-1.5 overflow-hidden rounded-full bg-hover">
        <div
          class="h-full rounded-full bg-fg transition-[width] duration-500"
          style="width: {(row.words / max) * 100}%"
        ></div>
      </div>
      <span class="text-right text-[12px] text-muted tabular-nums">
        {fmtNum(row.words)}
        <span class="ml-1 inline-block w-8 text-faint">{(share * 100).toFixed(0)}%</span>
      </span>
    </div>
  {/each}
</div>
