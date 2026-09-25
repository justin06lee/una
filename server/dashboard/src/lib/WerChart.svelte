<script lang="ts">
  import type { WerPoint } from '../api';
  import { fmtShortDate, fmtWer, fmtWerDelta, niceStep, shortId } from './format';

  /**
   * WER across training runs: baseline vs candidate per run.
   *
   * WER arrives already in percent (the server stores jiwer x 100), so values
   * are plotted as-is — no further scaling.
   */
  let { series }: { series: WerPoint[] } = $props();

  let width = $state(0);
  const H = 200;
  const PT = 12;
  const PB = 26;
  const PL = 44;
  const PR = 16;

  const points = $derived(series.filter((p) => p.wer_candidate !== null));
  const n = $derived(points.length);

  /**
   * Fit the axis to the data rather than pinning it at 0%: runs differ by
   * fractions of a point, which a zero-based axis flattens into one line.
   */
  const values = $derived(
    points.flatMap((p) => [p.wer_baseline, p.wer_candidate].filter((v): v is number => v !== null)),
  );
  const lo = $derived(values.length ? Math.min(...values) : 0);
  const hi = $derived(values.length ? Math.max(...values) : 1);
  const step = $derived(niceStep(Math.max(hi - lo, 0.5) / 3));
  const bottom = $derived(Math.max(0, Math.floor((lo - step / 2) / step) * step));
  const top = $derived(Math.max(bottom + step, Math.ceil((hi + step / 2) / step) * step));
  const yTicks = $derived.by(() => {
    const ticks: number[] = [];
    for (let v = bottom; v <= top + 1e-9; v += step) ticks.push(v);
    return ticks;
  });

  const plotW = $derived(Math.max(0, width - PL - PR));
  const plotH = H - PT - PB;

  function x(i: number): number {
    return n <= 1 ? PL + plotW / 2 : PL + (i * plotW) / (n - 1);
  }
  function y(wer: number): number {
    return PT + plotH * (1 - (wer - bottom) / (top - bottom));
  }
  function line(key: 'wer_baseline' | 'wer_candidate'): string {
    return points
      .map((p, i) => (p[key] === null ? null : `${x(i)},${y(p[key] as number)}`))
      .filter((s): s is string => s !== null)
      .join(' ');
  }

  let hover = $state<number | null>(null);
  const hovered = $derived(hover !== null ? (points[hover] ?? null) : null);

  function onMove(e: MouseEvent): void {
    if (n === 0) return;
    const rect = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
    const px = e.clientX - rect.left;
    let best = 0;
    let bestD = Infinity;
    for (let i = 0; i < n; i++) {
      const d = Math.abs(x(i) - px);
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    }
    hover = best;
  }
</script>

<div class="relative" bind:clientWidth={width}>
  {#if n === 0}
    <div class="py-12 text-center text-[13px] text-muted">
      No evaluated runs yet — the chart fills in once a fine-tune is scored.
    </div>
  {:else}
    <svg
      {width}
      height={H}
      role="img"
      aria-label="Word error rate by training run"
      onmousemove={onMove}
      onmouseleave={() => (hover = null)}
    >
      {#each yTicks as t (t)}
        <line
          x1={PL}
          x2={width - PR}
          y1={y(t)}
          y2={y(t)}
          stroke="var(--line)"
          stroke-width="1"
        />
        <text x={PL - 8} y={y(t) + 3.5} text-anchor="end" font-size="10" fill="var(--faint)">
          {t.toFixed(step < 1 ? 1 : 0)}%
        </text>
      {/each}

      <polyline
        points={line('wer_baseline')}
        fill="none"
        stroke="var(--faint)"
        stroke-width="1.75"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-dasharray="4 3"
      />
      <polyline
        points={line('wer_candidate')}
        fill="none"
        stroke="var(--fg)"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />

      {#each points as p, i (p.id)}
        {#if p.wer_baseline !== null}
          <circle cx={x(i)} cy={y(p.wer_baseline)} r={hover === i ? 4 : 2.5} fill="var(--panel)" stroke="var(--faint)" stroke-width="1.5" />
        {/if}
        {#if p.wer_candidate !== null}
          <circle cx={x(i)} cy={y(p.wer_candidate)} r={hover === i ? 4.5 : 3} fill="var(--fg)" />
        {/if}
      {/each}

      {#if hover !== null}
        <line
          x1={x(hover)}
          x2={x(hover)}
          y1={PT}
          y2={PT + plotH}
          stroke="var(--line-strong)"
          stroke-width="1"
        />
      {/if}
    </svg>

    <div class="mt-2 flex items-center gap-5 text-[12px] text-muted">
      <span class="flex items-center gap-1.5">
        <span class="w-4 border-t-[1.5px] border-dashed border-faint"></span>current model
      </span>
      <span class="flex items-center gap-1.5">
        <span class="h-[2px] w-4 rounded bg-fg"></span>fine-tuned
      </span>
      <span class="ml-auto text-faint">lower is better</span>
    </div>

    {#if hovered}
      <div
        class="pointer-events-none absolute top-1 right-1 rounded-lg border border-line bg-panel px-3 py-2 text-[12px] shadow-[var(--shadow-pop)]"
      >
        <div class="font-medium">Run {shortId(hovered.id, 4)}</div>
        <div class="text-faint">{fmtShortDate(hovered.finished_at)} · {hovered.status}</div>
        <div class="mt-1 tabular-nums">current {fmtWer(hovered.wer_baseline)}</div>
        <div class="tabular-nums">fine-tuned {fmtWer(hovered.wer_candidate)}</div>
        {#if hovered.wer_baseline !== null && hovered.wer_candidate !== null}
          <div
            class="mt-0.5 font-semibold tabular-nums"
            style="color: {hovered.wer_candidate < hovered.wer_baseline
              ? 'var(--ok)'
              : 'var(--warn)'}"
          >
            {fmtWerDelta(hovered.wer_baseline, hovered.wer_candidate)}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>
