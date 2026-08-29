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

  const maxPct = $derived(
    Math.max(
      0.5,
      ...points.flatMap((p) =>
        [p.wer_baseline, p.wer_candidate].filter((v): v is number => v !== null),
      ),
    ),
  );
  const step = $derived(niceStep(maxPct / 3));
  const top = $derived(Math.max(step, Math.ceil(maxPct / step) * step));
  const yTicks = $derived.by(() => {
    const ticks: number[] = [];
    for (let v = 0; v <= top; v += step) ticks.push(v);
    return ticks;
  });

  const plotW = $derived(Math.max(0, width - PL - PR));
  const plotH = H - PT - PB;

  function x(i: number): number {
    return n <= 1 ? PL + plotW / 2 : PL + (i * plotW) / (n - 1);
  }
  function y(wer: number): number {
    return PT + plotH * (1 - wer / top);
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
    <div class="py-12 text-center text-[13px] text-faint">
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
          stroke="var(--c-border)"
          stroke-width="1"
        />
        <text x={PL - 8} y={y(t) + 3.5} text-anchor="end" font-size="10" fill="var(--c-faint)">
          {t.toFixed(step < 1 ? 1 : 0)}%
        </text>
      {/each}

      <polyline
        points={line('wer_baseline')}
        fill="none"
        stroke="var(--c-chart-2)"
        stroke-width="1.75"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-dasharray="4 3"
      />
      <polyline
        points={line('wer_candidate')}
        fill="none"
        stroke="var(--c-chart-1)"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />

      {#each points as p, i (p.id)}
        {#if p.wer_baseline !== null}
          <circle cx={x(i)} cy={y(p.wer_baseline)} r={hover === i ? 4 : 2.5} fill="var(--c-chart-2)" />
        {/if}
        {#if p.wer_candidate !== null}
          <circle cx={x(i)} cy={y(p.wer_candidate)} r={hover === i ? 4.5 : 3} fill="var(--c-chart-1)" />
        {/if}
      {/each}

      {#if hover !== null}
        <line
          x1={x(hover)}
          x2={x(hover)}
          y1={PT}
          y2={PT + plotH}
          stroke="var(--c-edge)"
          stroke-width="1"
        />
      {/if}
    </svg>

    <div class="mt-1 flex items-center gap-4 text-[11px] text-muted">
      <span class="flex items-center gap-1.5">
        <span class="h-0.5 w-4 rounded" style="background: var(--c-chart-2)"></span>baseline
      </span>
      <span class="flex items-center gap-1.5">
        <span class="h-0.5 w-4 rounded" style="background: var(--c-chart-1)"></span>candidate
      </span>
      <span class="ml-auto text-faint">lower is better</span>
    </div>

    {#if hovered}
      <div
        class="pointer-events-none absolute top-2 right-2 card px-3 py-2 text-[11px] shadow-[var(--shadow-pop)]"
      >
        <div class="font-semibold">run {shortId(hovered.id, 4)}</div>
        <div class="text-faint">{fmtShortDate(hovered.finished_at)} · {hovered.status}</div>
        <div class="mt-1 tabular-nums">baseline {fmtWer(hovered.wer_baseline)}</div>
        <div class="tabular-nums">candidate {fmtWer(hovered.wer_candidate)}</div>
        {#if hovered.wer_baseline !== null && hovered.wer_candidate !== null}
          <div
            class="mt-0.5 font-semibold tabular-nums"
            style="color: {hovered.wer_candidate < hovered.wer_baseline
              ? 'var(--c-accent)'
              : 'var(--c-warn)'}"
          >
            {fmtWerDelta(hovered.wer_baseline, hovered.wer_candidate)}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>
