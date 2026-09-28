<script lang="ts">
  /**
   * One line of player for a dictation's recording: play/pause, elapsed,
   * a seek track, length and speed. Same look as the review window's.
   * `src` is a blob URL, or null while it loads; `unavailable` says there
   * is nothing to load. Parents drive it through `toggle()` and `stop()`.
   */
  let {
    src,
    unavailable = false,
    rates = [0.75, 1, 1.5],
    shortcut,
  }: {
    src: string | null;
    unavailable?: boolean;
    rates?: number[];
    /** Keyboard shortcut to show on the play button, e.g. "⌘P". */
    shortcut?: string;
  } = $props();

  let audio: HTMLAudioElement | undefined = $state();
  let playing = $state(false);
  let current = $state(0);
  let duration = $state(0);
  let rate = $state(1);

  export function toggle() {
    if (!audio) return;
    if (audio.paused) void audio.play();
    else audio.pause();
  }

  export function stop() {
    if (!audio) return;
    audio.pause();
    audio.currentTime = 0;
    current = 0;
  }

  function setRate(value: number) {
    rate = value;
    if (audio) audio.playbackRate = value;
  }

  function seek(event: MouseEvent) {
    if (!audio || !duration) return;
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    audio.currentTime = ((event.clientX - rect.left) / rect.width) * duration;
  }

  // A new recording starts from the top.
  $effect(() => {
    void src;
    current = 0;
    duration = 0;
    playing = false;
  });

  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
</script>

<div class="player" class:unavailable>
  <button
    class="play"
    onclick={toggle}
    disabled={!src}
    aria-label={playing ? "Pause" : "Play what you said"}
    title={shortcut ? `${playing ? "Pause" : "Play"} (${shortcut})` : undefined}
  >
    {#if playing}
      <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"
        ><rect x="6" y="4" width="4" height="16" rx="1" /><rect x="14" y="4" width="4" height="16" rx="1" /></svg
      >
    {:else}
      <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"
        ><path d="M7 4.5v15a1 1 0 0 0 1.52.85l12-7.5a1 1 0 0 0 0-1.7l-12-7.5A1 1 0 0 0 7 4.5Z" /></svg
      >
    {/if}
  </button>
  {#if unavailable}
    <span class="note">The recording isn't available.</span>
  {:else}
    <span class="time">{fmt(current)}</span>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="track" role="slider" aria-label="Seek" aria-valuenow={current} tabindex="-1" onclick={seek}>
      <div class="fill" style="width: {duration ? (current / duration) * 100 : 0}%"></div>
    </div>
    <span class="time">{fmt(duration)}</span>
    <div class="seg" role="group" aria-label="Speed">
      {#each rates as r (r)}
        <button class:on={rate === r} onclick={() => setRate(r)} tabindex="-1">{r}×</button>
      {/each}
    </div>
    {#if shortcut}<span class="kbd">{shortcut}</span>{/if}
  {/if}
  {#if src}
    <audio
      bind:this={audio}
      {src}
      onplay={() => (playing = true)}
      onpause={() => (playing = false)}
      onended={() => (playing = false)}
      ontimeupdate={() => (current = audio?.currentTime ?? 0)}
      onloadedmetadata={() => {
        duration = audio?.duration ?? 0;
        if (audio) audio.playbackRate = rate;
      }}
    ></audio>
  {/if}
</div>

<style>
  .player {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px 8px 8px;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .play {
    width: 28px;
    height: 28px;
    flex: none;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--inverse);
    color: var(--on-inverse);
    cursor: pointer;
  }

  .play:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .time {
    width: 30px;
    flex: none;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }

  .track {
    flex: 1;
    height: 4px;
    border-radius: 999px;
    background: var(--line-strong);
    cursor: pointer;
    position: relative;
  }

  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 999px;
    background: var(--fg);
  }

  .seg {
    display: flex;
    flex: none;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .seg button {
    border: 0;
    background: transparent;
    color: var(--faint);
    font: inherit;
    font-size: 11px;
    padding: 3px 7px;
    cursor: pointer;
  }

  .seg button.on {
    background: var(--subtle);
    color: var(--fg);
  }

  .note {
    font-size: 12px;
    color: var(--faint);
  }

  .kbd {
    flex: none;
  }
</style>
