<script lang="ts">
  import type { WerPoint } from '../api';
  import { fmtShortDate, fmtWer, fmtWerDelta, niceStep, shortId } from './format';

  /** WER over training runs: baseline vs candidate per run. */
  let { series }: { series: WerPoint[] } = $props();

  let width = $state(0);
  const H = 210;
  const PT = 10;
  const PB = 26;
  const PL = 42;
  const PR = 16;

  const points = $derived(series.filter((p) => p.wer_candidate !== null));
  const n = $derived(points.length);

  const maxPct = $derived(
    Math.max(
      1,
      ...points.flatMap((p) =>
        [p.wer_baseline, p.wer_candidate]
          .filter((v): v is number => v !== null)
          .map((v) => v * 100),
      ),
    ),
  );
  const step = $derived(niceStep(maxPct / 3));
  const topPct = $derived(Math.max(step, Math.ceil(maxPct / step) * step));
  const yTicks = $derived.by(() => {
    const t: number[] = [];
    for (let v = 0; v <= topPct; v += step) t.push(v);
    return t;
  });

  const plotW = $derived(Math.max(0, width - PL - PR));
  const plotH = H - PT - PB;

  function x(i: number): number {
    if (n <= 1) return PL + plotW / 2;
    return PL + (i * plotW) / (n - 1);
  }
  function y(wer: number): number {
    return PT + plotH * (1 - (wer * 100) / topPct);
  }

  function line(key: 'wer_baseline' | 'wer_candidate'): string {
    return points
      .map((p, i) => (p[key] === null ? null : `${x(i)},${y(p[key])}`))
      .filter((s): s is string => s !== null)
      .join(' ');
  }

  let hover = $state<number | null>(null);

  function onMove(e: MouseEvent): void {
    if (n === 0) return;
    const rect = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
    const px = e.clientX - rect.left;
    let best = 0;
    let bestDist = Infinity;
    for (let i = 0; i < n; i++) {
      const d = Math.abs(px - x(i));
      if (d < bestDist) {
        bestDist = d;
        best = i;
      }
    }
    hover = best;
  }

  const hovered = $derived(hover === null ? null : (points[hover] ?? null));
  const tipLeft = $derived.by(() => {
    if (hover === null || width === 0) return 0;
    return Math.min(Math.max(x(hover), 78), width - 86);
  });

  const xLabelIdx = $derived.by(() => {
    if (n === 0) return [] as number[];
    const stride = Math.max(1, Math.ceil(n / 6));
    const idx = points.map((_, i) => i).filter((i) => i % stride === 0);
    if (!idx.includes(n - 1)) idx.push(n - 1);
    return idx;
  });
</script>

<div class="relative" bind:clientWidth={width}>
  <div class="mb-2 flex items-center justify-end gap-4 text-xs text-muted">
    <span class="inline-flex items-center gap-1.5">
      <span class="h-2 w-2 rounded-full" style="background: var(--color-chart-1)"></span>
      baseline
    </span>
    <span class="inline-flex items-center gap-1.5">
      <span class="h-2 w-2 rounded-full" style="background: var(--color-chart-2)"></span>
      candidate
    </span>
  </div>

  {#if width > 0}
    <svg
      {width}
      height={H}
      onmousemove={onMove}
      onmouseleave={() => (hover = null)}
      role="img"
      aria-label="Word error rate per training run, baseline versus candidate"
    >
      {#each yTicks as t (t)}
        <line
          x1={PL}
          x2={width - PR}
          y1={y(t / 100)}
          y2={y(t / 100)}
          stroke="var(--color-border)"
          stroke-width="1"
        />
        <text
          x={PL - 6}
          y={y(t / 100) + 3}
          text-anchor="end"
          font-size="10"
          fill="var(--color-faint)"
          style="font-variant-numeric: tabular-nums"
        >
          {t}%
        </text>
      {/each}

      {#if hover !== null}
        <line
          x1={x(hover)}
          x2={x(hover)}
          y1={PT}
          y2={PT + plotH}
          stroke="var(--color-edge)"
          stroke-width="1"
        />
      {/if}

      {#if n >= 2}
        <polyline
          points={line('wer_baseline')}
          fill="none"
          stroke="var(--color-chart-1)"
          stroke-width="2"
          stroke-linejoin="round"
          stroke-linecap="round"
        />
        <polyline
          points={line('wer_candidate')}
          fill="none"
          stroke="var(--color-chart-2)"
          stroke-width="2"
          stroke-linejoin="round"
          stroke-linecap="round"
        />
      {/if}

      {#each points as p, i (p.id)}
        {#if p.wer_baseline !== null}
          <circle
            cx={x(i)}
            cy={y(p.wer_baseline)}
            r="4.5"
            fill="var(--color-chart-1)"
            stroke="var(--color-surface)"
            stroke-width="2"
          />
        {/if}
        {#if p.wer_candidate !== null}
          <circle
            cx={x(i)}
            cy={y(p.wer_candidate)}
            r="4.5"
            fill="var(--color-chart-2)"
            stroke="var(--color-surface)"
            stroke-width="2"
          />
        {/if}
      {/each}

      {#each xLabelIdx as i (i)}
        <text
          x={Math.min(Math.max(x(i), PL + 14), width - 20)}
          y={H - 8}
          text-anchor="middle"
          font-size="10"
          fill="var(--color-faint)"
        >
          {points[i]?.finished_at ? fmtShortDate(points[i]!.finished_at) : shortId(points[i]!.id)}
        </text>
      {/each}
    </svg>

    {#if hovered}
      <div
        class="pointer-events-none absolute z-10 -translate-x-1/2 rounded-md border border-edge bg-raised px-2.5 py-1.5 text-xs"
        style="left: {tipLeft}px; top: 18px"
      >
        <div class="font-medium">
          run {shortId(hovered.id)} · {fmtShortDate(hovered.finished_at)}
        </div>
        <div class="mt-0.5 space-y-0.5 text-muted tabular-nums">
          <div>baseline {fmtWer(hovered.wer_baseline)}</div>
          <div>candidate {fmtWer(hovered.wer_candidate)}</div>
          {#if hovered.wer_baseline !== null && hovered.wer_candidate !== null}
            <div
              style="color: var(--color-{hovered.wer_candidate <= hovered.wer_baseline
                ? 'ok'
                : 'danger'})"
            >
              {fmtWerDelta(hovered.wer_baseline, hovered.wer_candidate)}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>
