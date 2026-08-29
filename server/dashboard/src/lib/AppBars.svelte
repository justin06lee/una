<script lang="ts">
  import type { StatsApp } from '../api';
  import { fmtNum, fmtSpan } from './format';

  let { rows }: { rows: StatsApp[] } = $props();

  const total = $derived(rows.reduce((sum, r) => sum + r.words, 0));
  const max = $derived(Math.max(1, ...rows.map((r) => r.words)));

  /** Light / Moderate / Power, matching how much of your dictation lands here. */
  function intensity(words: number): string {
    const share = total > 0 ? words / total : 0;
    if (share >= 0.35) return 'Power';
    if (share >= 0.12) return 'Moderate';
    return 'Light';
  }
</script>

<div class="space-y-2.5">
  {#each rows as row (row.app)}
    {@const share = total > 0 ? row.words / total : 0}
    <div
      class="group grid grid-cols-[9rem_1fr_auto] items-center gap-3"
      title="{row.app} — {fmtNum(row.words)} words over {row.n} dictation{row.n === 1
        ? ''
        : 's'} ({fmtSpan(row.ms)}) · {intensity(row.words)}"
    >
      <span class="truncate text-[13px] font-medium">{row.app}</span>
      <div class="h-2 overflow-hidden rounded-full bg-raised">
        <div
          class="h-full rounded-full transition-[width] duration-500"
          style="width: {(row.words / max) * 100}%; background: var(--c-accent); opacity: {0.45 +
            0.55 * (row.words / max)}"
        ></div>
      </div>
      <span class="w-24 text-right text-[12px] text-muted tabular-nums">
        {fmtNum(row.words)}
        <span class="text-faint">· {(share * 100).toFixed(0)}%</span>
      </span>
    </div>
  {/each}
</div>
