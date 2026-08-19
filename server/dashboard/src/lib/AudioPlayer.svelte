<script lang="ts">
  import { untrack } from 'svelte';

  import { fmtTime } from './format';

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
    if (el.paused) el.play().catch(() => {});
    else el.pause();
  }

  export function replay(): void {
    if (!el) return;
    el.currentTime = 0;
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
  }}
  ondurationchange={() => {
    if (el && Number.isFinite(el.duration)) duration = el.duration;
  }}
></audio>

<div class="flex items-center gap-3">
  <button
    type="button"
    class="flex h-8 w-8 flex-none items-center justify-center rounded-full border border-edge bg-raised text-text transition-colors duration-150 hover:bg-hover"
    onclick={toggle}
    aria-label={playing ? 'Pause' : 'Play'}
    tabindex="-1"
  >
    {#if playing}
      <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
        <rect x="5" y="4" width="5" height="16" rx="1" />
        <rect x="14" y="4" width="5" height="16" rx="1" />
      </svg>
    {:else}
      <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
        <path d="M7 4.5v15a1 1 0 0 0 1.5.87l13-7.5a1 1 0 0 0 0-1.74l-13-7.5A1 1 0 0 0 7 4.5Z" />
      </svg>
    {/if}
  </button>

  <span class="w-9 text-right text-xs text-muted tabular-nums">{fmtTime(current)}</span>

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
    <div class="relative h-1.5 w-full overflow-hidden rounded-full bg-edge">
      <div
        class="absolute inset-y-0 left-0 rounded-full bg-accent"
        style="width: {frac * 100}%"
      ></div>
    </div>
  </div>

  <span class="w-9 text-xs text-faint tabular-nums">{fmtTime(duration)}</span>

  <div class="flex flex-none gap-0.5 rounded-md border border-edge bg-raised p-0.5">
    {#each [1, 1.5, 2] as r (r)}
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[11px] tabular-nums transition-colors duration-150
          {rate === r ? 'bg-hover text-text' : 'text-faint hover:text-muted'}"
        onclick={() => setRate(r)}
        tabindex="-1"
        aria-pressed={rate === r}
      >
        {r}×
      </button>
    {/each}
  </div>
</div>
