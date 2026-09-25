<script lang="ts">
  import { untrack } from 'svelte';

  import { fmtTime } from './format';
  import Icon from './Icon.svelte';

  let { src, autoplay = false }: { src: string; autoplay?: boolean } = $props();

  let el: HTMLAudioElement | undefined = $state();
  let track: HTMLDivElement | undefined = $state();
  let playing = $state(false);
  let current = $state(0);
  let duration = $state(0);
  let rate = $state(1);
  let seeking = false;

  // New source: reset position, keep the chosen playback rate, optionally autoplay.
  $effect(() => {
    void src;
    const a = untrack(() => el);
    if (!a) return;
    current = 0;
    duration = 0;
    a.playbackRate = untrack(() => rate);
    if (autoplay) a.play().catch(() => {});
  });

  export function toggle(): void {
    if (!el) return;
    stopAt = null;
    if (el.paused) el.play().catch(() => {});
    else el.pause();
  }

  export function replay(): void {
    if (!el) return;
    el.currentTime = 0;
    el.play().catch(() => {});
  }

  /** Where to stop when playing one stretch of the recording. */
  let stopAt: number | null = null;

  /** Play just [t0, t1] seconds, with a little room either side. */
  export function playRange(t0: number, t1: number): void {
    if (!el) return;
    el.currentTime = Math.max(0, t0 - 0.35);
    stopAt = t1 + 0.35;
    el.play().catch(() => {});
  }

  export function setRate(r: number): void {
    rate = r;
    if (el) el.playbackRate = r;
  }

  function seekTo(clientX: number): void {
    if (!el || !track || !duration) return;
    const r = track.getBoundingClientRect();
    const frac = Math.min(1, Math.max(0, (clientX - r.left) / r.width));
    el.currentTime = frac * duration;
    current = frac * duration;
  }

  function onPointerDown(e: PointerEvent): void {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    seeking = true;
    seekTo(e.clientX);
  }

  function onPointerMove(e: PointerEvent): void {
    if (seeking) seekTo(e.clientX);
  }

  function onPointerUp(): void {
    seeking = false;
  }

  function onTrackKeydown(e: KeyboardEvent): void {
    if (!el || !duration) return;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      const d = e.key === 'ArrowLeft' ? -3 : 3;
      el.currentTime = Math.min(duration, Math.max(0, el.currentTime + d));
    }
  }

  const frac = $derived(duration > 0 ? current / duration : 0);
</script>

<audio
  bind:this={el}
  {src}
  preload="auto"
  onplay={() => (playing = true)}
  onpause={() => (playing = false)}
  onended={() => (playing = false)}
  ontimeupdate={() => {
    if (!seeking && el) current = el.currentTime;
    if (el && stopAt !== null && el.currentTime >= stopAt) {
      el.pause();
      stopAt = null;
    }
  }}
  ondurationchange={() => {
    if (el && Number.isFinite(el.duration)) duration = el.duration;
  }}
></audio>

<div class="flex items-center gap-3">
  <button
    type="button"
    class="flex h-8 w-8 flex-none items-center justify-center rounded-full bg-inverse text-on-inverse transition-opacity duration-150 hover:opacity-85"
    onclick={toggle}
    aria-label={playing ? 'Pause' : 'Play'}
    tabindex="-1"
  >
    <Icon name={playing ? 'pause' : 'play'} size={13} class={playing ? '' : 'translate-x-px'} />
  </button>

  <span class="w-8 text-right text-[12px] text-muted tabular-nums">{fmtTime(current)}</span>

  <div
    bind:this={track}
    class="group flex h-6 flex-1 cursor-pointer items-center"
    role="slider"
    aria-label="Seek"
    aria-valuemin={0}
    aria-valuemax={Math.round(duration)}
    aria-valuenow={Math.round(current)}
    aria-valuetext={fmtTime(current)}
    tabindex="0"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onkeydown={onTrackKeydown}
  >
    <div class="relative h-1 w-full rounded-full bg-line-strong">
      <div class="absolute inset-y-0 left-0 rounded-full bg-fg" style="width: {frac * 100}%"></div>
      <div
        class="absolute top-1/2 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-fg opacity-0 transition-opacity duration-150 group-hover:opacity-100"
        style="left: {frac * 100}%"
      ></div>
    </div>
  </div>

  <span class="w-8 text-[12px] text-faint tabular-nums">{fmtTime(duration)}</span>

  <div class="seg !p-0.5">
    {#each [1, 1.5, 2] as r (r)}
      <button
        type="button"
        class="seg-item !h-5 !px-1.5 !text-[11px] tabular-nums"
        class:active={rate === r}
        onclick={() => setRate(r)}
        tabindex="-1"
        aria-pressed={rate === r}
      >
        {r}×
      </button>
    {/each}
  </div>
</div>
