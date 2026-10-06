<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import type { LevelFrame, Snapshot } from "../lib/types";
  import { LevelNormalizer } from "./level";

  // The waveform: a fixed row of bars, drawn from the centre out. The pill
  // springs open around it and clips it, so the bars appear from the middle.
  const BAR_COUNT = 13;
  const BAR_W = 3;
  const BAR_GAP = 3;
  const BARS_W = BAR_COUNT * BAR_W + (BAR_COUNT - 1) * BAR_GAP;
  const BARS_H = 20;
  const MID = (BAR_COUNT - 1) / 2;

  let snapshot = $state<Snapshot>({ state: "idle" });
  let canvas = $state<HTMLCanvasElement | null>(null);

  // 30Hz level data, interpolated at 60fps in the rAF loop below.
  const normalizer = new LevelNormalizer();
  let targetLevel = 0;
  const bars = new Float32Array(BAR_COUNT);
  // A bell over the row: tall in the middle, dots at the ends.
  const envelope = new Float32Array(BAR_COUNT);
  const speeds = new Float32Array(BAR_COUNT);
  const phases = new Float32Array(BAR_COUNT);
  for (let i = 0; i < BAR_COUNT; i++) {
    envelope[i] = 0.22 + 0.78 * (0.5 + 0.5 * Math.cos((Math.PI * (i - MID)) / (MID + 1)));
    speeds[i] = 5 + ((i * 7) % 5);
    phases[i] = Math.random() * Math.PI * 2;
  }
  let raf = 0;

  const phase = $derived.by(() => {
    switch (snapshot.state) {
      case "recording":
        return "recording";
      case "transcribing":
      case "inserting":
        return "busy";
      case "done":
        return "done";
      case "error":
        return "error";
      default:
        return "idle";
    }
  });

  const errorText = $derived.by(() => {
    if (snapshot.state !== "error") return "";
    switch (snapshot.kind) {
      case "connect":
        return "Can’t reach server";
      case "timeout":
        return "Server timed out";
      case "server":
        return "Server error";
      case "decode":
        return "Bad server response";
      case "audio":
        return "Microphone error";
      case "inject":
        return "Couldn’t insert text";
      default:
        return snapshot.message || "Something went wrong";
    }
  });

  function draw() {
    raf = requestAnimationFrame(draw);
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== BARS_W * dpr || canvas.height !== BARS_H * dpr) {
      canvas.width = BARS_W * dpr;
      canvas.height = BARS_H * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, BARS_W, BARS_H);

    const t = performance.now() / 1000;
    for (let i = 0; i < BAR_COUNT; i++) {
      // Each bar sways a little on its own, so the row reads as a waveform
      // rather than a block; fast attack, slower release.
      const sway = 0.68 + 0.32 * (0.5 + 0.5 * Math.sin(t * speeds[i] + phases[i]));
      const target = targetLevel * envelope[i] * sway;
      bars[i] += (target - bars[i]) * (target > bars[i] ? 0.45 : 0.14);
      // At rest a bar is a dot; it brightens as it grows.
      const bh = BAR_W + bars[i] * (BARS_H - BAR_W);
      ctx.fillStyle = `rgba(255,255,255,${0.5 + 0.45 * Math.min(1, bars[i] * 3)})`;
      ctx.beginPath();
      ctx.roundRect(i * (BAR_W + BAR_GAP), (BARS_H - bh) / 2, BAR_W, bh, BAR_W / 2);
      ctx.fill();
    }
  }

  // Only run the rAF loop while the waveform is on screen.
  $effect(() => {
    if (phase === "recording") {
      normalizer.reset();
      targetLevel = 0;
      bars.fill(0);
      raf = requestAnimationFrame(draw);
      return () => {
        cancelAnimationFrame(raf);
        normalizer.save();
      };
    }
  });

  function retry() {
    invoke("retry_last").catch(() => {});
  }

  onMount(() => {
    const unlistenState = listen<Snapshot>("state-changed", (e) => {
      snapshot = e.payload;
    });
    const unlistenLevel = listen<LevelFrame>("audio-level", (e) => {
      targetLevel = normalizer.push(e.payload.rms);
    });
    return () => {
      unlistenState.then((f) => f());
      unlistenLevel.then((f) => f());
      cancelAnimationFrame(raf);
    };
  });
</script>

<div class="wrap">
  <div
    class="pill {phase}"
    title={snapshot.state === "error" ? snapshot.message : undefined}
  >
    {#if phase === "recording"}
      <canvas
        bind:this={canvas}
        class="bars"
        style:width="{BARS_W}px"
        style:height="{BARS_H}px"
      ></canvas>
    {:else if phase === "busy"}
      <div class="shimmer"></div>
    {:else if phase === "done"}
      <svg class="check" viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M4 12.5l5 5L20 6.5"
          fill="none"
          stroke="currentColor"
          stroke-width="3"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    {:else if phase === "error"}
      <svg class="warn" viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M12 3L2 21h20L12 3zm0 6v6m0 3v.5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
      <span class="msg">{errorText}</span>
      {#if snapshot.state === "error" && snapshot.retryable}
        <button class="retry" onclick={retry}>Retry</button>
      {/if}
    {/if}
  </div>
</div>

<style>
  .wrap {
    height: 100%;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding-bottom: 8px;
  }

  .pill {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    overflow: hidden;
    border-radius: 9999px;
    background: rgba(14, 14, 14, 0.82);
    -webkit-backdrop-filter: blur(20px) saturate(1.5);
    backdrop-filter: blur(20px) saturate(1.5);
    border: 0.5px solid rgba(255, 255, 255, 0.14);
    box-shadow:
      0 4px 16px rgba(0, 0, 0, 0.35),
      inset 0 0.5px 0 rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.94);
    /* Settle/shrink: quick, no overshoot. Grow states override with a
       spring below. */
    transition:
      width 0.22s cubic-bezier(0.25, 1, 0.35, 1),
      height 0.22s cubic-bezier(0.25, 1, 0.35, 1),
      background-color 0.2s ease,
      border-color 0.2s ease,
      opacity 0.25s ease;
  }

  /* Spring easing (perceptual ~250ms) for every growing transition. The
     cubic-bezier declaration is the fallback for webviews without linear()
     support (< Safari 17.2); the linear() spring wins where available. */
  .pill.recording,
  .pill.error {
    transition:
      width 250ms cubic-bezier(0.34, 1.56, 0.64, 1),
      height 250ms cubic-bezier(0.34, 1.56, 0.64, 1),
      background-color 0.2s ease,
      border-color 0.2s ease;
    transition:
      width 450ms
        linear(
          0, 0.1605, 0.4497, 0.7063, 0.8805, 0.9768, 1.0183, 1.0284, 1.0242,
          1.0161, 1.0087, 1.0036, 1.0008, 0.9995, 1
        ),
      height 450ms
        linear(
          0, 0.1605, 0.4497, 0.7063, 0.8805, 0.9768, 1.0183, 1.0284, 1.0242,
          1.0161, 1.0087, 1.0036, 1.0008, 0.9995, 1
        ),
      background-color 0.2s ease,
      border-color 0.2s ease;
  }

  /* -------------------------------------------------------------- Idle */
  /* At rest: a small, empty pill. */
  .pill.idle {
    width: 40px;
    height: 10px;
    opacity: 0.85;
  }

  /* --------------------------------------------------------- Recording */
  /* Just the waveform, centred. */
  .pill.recording {
    width: 96px;
    height: 32px;
  }

  .bars {
    flex: none;
  }

  /* -------------------------------------------- Transcribing/Inserting */
  .pill.busy {
    width: 96px;
    height: 32px;
    padding: 0 18px;
  }

  .shimmer {
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background:
      linear-gradient(
          90deg,
          rgba(255, 255, 255, 0) 0%,
          rgba(255, 255, 255, 0.55) 50%,
          rgba(255, 255, 255, 0) 100%
        )
        no-repeat,
      rgba(255, 255, 255, 0.14);
    background-size: 45% 100%;
    animation: sweep 1.1s ease-in-out infinite;
  }

  @keyframes sweep {
    0% {
      background-position:
        -60% 0,
        0 0;
    }
    100% {
      background-position:
        160% 0,
        0 0;
    }
  }

  /* -------------------------------------------------------------- Done */
  .pill.done {
    width: 64px;
    height: 32px;
    border-color: rgba(255, 255, 255, 0.4);
    animation: pulse 0.7s ease-out;
  }

  .check {
    width: 17px;
    height: 17px;
    flex: none;
    color: #ffffff;
  }

  .check path {
    stroke-dasharray: 26;
    stroke-dashoffset: 26;
    animation: drawcheck 0.3s ease-out 0.08s forwards;
  }

  @keyframes drawcheck {
    to {
      stroke-dashoffset: 0;
    }
  }

  @keyframes pulse {
    0% {
      box-shadow:
        0 4px 16px rgba(0, 0, 0, 0.35),
        0 0 0 0 rgba(255, 255, 255, 0.35);
    }
    100% {
      box-shadow:
        0 4px 16px rgba(0, 0, 0, 0.35),
        0 0 0 14px rgba(255, 255, 255, 0);
    }
  }

  /* ------------------------------------------------------------- Error */
  .pill.error {
    width: 356px;
    height: 44px;
    padding: 0 10px 0 16px;
    border-color: rgba(255, 255, 255, 0.4);
    animation: shake 0.35s ease;
  }

  .warn {
    width: 17px;
    height: 17px;
    flex: none;
    color: #ffffff;
  }

  .msg {
    font-size: 12.5px;
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    flex: 1;
  }

  .retry {
    flex: none;
    appearance: none;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
    font-family: inherit;
    font-size: 12px;
    font-weight: 600;
    padding: 5px 13px;
    border-radius: 9999px;
    cursor: pointer;
  }

  .retry:hover {
    background: rgba(255, 255, 255, 0.22);
  }

  @keyframes shake {
    0%,
    100% {
      transform: translateX(0);
    }
    25% {
      transform: translateX(-5px);
    }
    50% {
      transform: translateX(4px);
    }
    75% {
      transform: translateX(-2px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .pill,
    .pill.recording,
    .pill.error {
      transition: none;
      animation: none;
    }
    .shimmer,
    .check path {
      animation: none;
    }
    .check path {
      stroke-dashoffset: 0;
    }
  }
</style>
