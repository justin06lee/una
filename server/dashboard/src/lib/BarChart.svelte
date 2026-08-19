<script lang="ts">
  import type { StatsDay } from '../api';
  import { niceStep } from './format';

  /** Dictations-per-day bars over the trailing 60 days. */
  let { data }: { data: StatsDay[] } = $props();

  let width = $state(0);
  const H = 190;
  const PT = 8;
  const PB = 24;
  const PL = 34;
  const PR = 4;
  const N_DAYS = 60;

  type Day = { key: string; label: string; n: number; ms: number };

  const days = $derived.by<Day[]>(() => {
    const map = new Map(data.map((d) => [d.day, d]));
    const out: Day[] = [];
    const today = new Date();
    for (let i = N_DAYS - 1; i >= 0; i--) {
      const dt = new Date(today.getFullYear(), today.getMonth(), today.getDate() - i);
      const key = `${dt.getFullYear()}-${String(dt.getMonth() + 1).padStart(2, '0')}-${String(dt.getDate()).padStart(2, '0')}`;
      const src = map.get(key);
      out.push({
        key,
        label: dt.toLocaleDateString(undefined, { month: 'short', day: 'numeric' }),
        n: src?.n ?? 0,
        ms: src?.ms ?? 0,
      });
    }
    return out;
  });

  const maxN = $derived(Math.max(1, ...days.map((d) => d.n)));
  const step = $derived(niceStep(maxN / 3));
  const yTop = $derived(Math.max(step, Math.ceil(maxN / step) * step));
  const yTicks = $derived.by(() => {
    const t: number[] = [];
    for (let v = 0; v <= yTop; v += step) t.push(v);
    return t;
  });

  const plotW = $derived(Math.max(0, width - PL - PR));
  const plotH = H - PT - PB;
  const slot = $derived(plotW / N_DAYS);
  const barW = $derived(Math.min(24, Math.max(1.5, slot - 2)));

  function x(i: number): number {
    return PL + i * slot + (slot - barW) / 2;
  }
  function y(v: number): number {
    return PT + plotH * (1 - v / yTop);
  }

  /** Rounded top, square baseline. */
  function barPath(i: number, n: number): string {
    const bx = x(i);
    const by = y(n);
    const h = PT + plotH - by;
    const r = Math.min(4, barW / 2, h);
    return `M${bx},${by + h} V${by + r} Q${bx},${by} ${bx + r},${by} H${bx + barW - r} Q${bx + barW},${by} ${bx + barW},${by + r} V${by + h} Z`;
  }

  const xLabelIdx = $derived(
    days.map((_, i) => i).filter((i) => (N_DAYS - 1 - i) % 14 === 0),
  );

  let hover = $state<number | null>(null);

  function onMove(e: MouseEvent): void {
    const rect = (e.currentTarget as SVGSVGElement).getBoundingClientRect();
    const px = e.clientX - rect.left - PL;
    if (px < 0 || px > plotW || plotW <= 0) {
      hover = null;
      return;
    }
    hover = Math.min(N_DAYS - 1, Math.max(0, Math.floor(px / slot)));
  }

  const hovered = $derived(hover === null ? null : (days[hover] ?? null));
  const tipLeft = $derived.by(() => {
    if (hover === null || width === 0) return 0;
    return Math.min(Math.max(x(hover) + barW / 2, 60), width - 70);
  });
</script>

<div class="relative" bind:clientWidth={width}>
  {#if width > 0}
    <svg
      {width}
      height={H}
      onmousemove={onMove}
      onmouseleave={() => (hover = null)}
      role="img"
      aria-label="Dictations per day, last 60 days"
    >
      {#each yTicks as t (t)}
        <line
          x1={PL}
          x2={width - PR}
          y1={y(t)}
          y2={y(t)}
          stroke="var(--color-border)"
          stroke-width="1"
        />
        <text
          x={PL - 6}
          y={y(t) + 3}
          text-anchor="end"
          font-size="10"
          fill="var(--color-faint)"
          style="font-variant-numeric: tabular-nums"
        >
          {t}
        </text>
      {/each}

      {#if hover !== null}
        <rect
          x={PL + hover * slot}
          y={PT}
          width={slot}
          height={plotH}
          fill="rgba(255,255,255,0.035)"
        />
      {/if}

      {#each days as d, i (d.key)}
        {#if d.n > 0}
          <path d={barPath(i, d.n)} fill="var(--color-chart-1)" />
        {/if}
      {/each}

      {#each xLabelIdx as i (i)}
        <text
          x={Math.min(Math.max(PL + i * slot + slot / 2, PL + 14), width - 20)}
          y={H - 7}
          text-anchor="middle"
          font-size="10"
          fill="var(--color-faint)"
        >
          {days[i]?.label}
        </text>
      {/each}
    </svg>

    {#if hovered}
      <div
        class="pointer-events-none absolute z-10 -translate-x-1/2 rounded-md border border-edge bg-raised px-2.5 py-1.5 text-xs shadow-none"
        style="left: {tipLeft}px; top: -4px"
      >
        <div class="font-medium">{hovered.label}</div>
        <div class="text-muted tabular-nums">
          {hovered.n} dictation{hovered.n === 1 ? '' : 's'} · {(hovered.ms / 60000).toFixed(1)} min
        </div>
      </div>
    {/if}
  {/if}
</div>
