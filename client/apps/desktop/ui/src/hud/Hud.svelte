<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import type { LevelFrame, Snapshot } from "../lib/types";

  const BAR_COUNT = 34;

  let snapshot = $state<Snapshot>({ state: "idle" });
  let canvas = $state<HTMLCanvasElement | null>(null);
  let elapsed = $state("0:00");

  // 30Hz level data, interpolated at 60fps in the rAF loop below.
  let targetLevel = 0;
  const bars = new Float32Array(BAR_COUNT).fill(0.06);
  const phases = new Float32Array(BAR_COUNT);
  for (let i = 0; i < BAR_COUNT; i++) phases[i] = Math.random() * Math.PI * 2;
  let raf = 0;
  let recStart = 0;

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
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr;
      canvas.height = h * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    if (w <= 0 || h <= 0) return;

    const gap = 3;
    // The pill animates open from 64px, so for the first frames the canvas is
    // narrower than the gaps alone and the naive bar width goes negative —
    // roundRect throws on a negative radius and the frame is lost. Clamp.
    const barW = Math.max(0.5, (w - gap * (BAR_COUNT - 1)) / BAR_COUNT);
    const now = performance.now();
    const t = now / 1000;

    // mm:ss timer (state writes are cheap; the string changes once/second).
    const secs = Math.max(0, Math.floor((now - recStart) / 1000));
    const next = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
    if (next !== elapsed) elapsed = next;

    ctx.fillStyle = "rgba(255,255,255,0.92)";
    for (let i = 0; i < BAR_COUNT; i++) {
      // Center-weighted target with per-bar noise; fast attack, slow decay.
      const centerBias = 1 - Math.abs(i - (BAR_COUNT - 1) / 2) / (BAR_COUNT / 2);
      const noise = 0.12 * Math.sin(t * (5 + (i % 5)) + phases[i]);
      const target = Math.max(
        0.06,
        Math.min(1, targetLevel * (0.5 + 0.7 * centerBias) + noise * targetLevel),
      );
      const k = target > bars[i] ? 0.5 : 0.12;
      bars[i] += (target - bars[i]) * k;
      const bh = Math.max(2.5, bars[i] * h);
      const x = i * (barW + gap);
      const y = (h - bh) / 2;
      ctx.beginPath();
      ctx.roundRect(x, y, barW, bh, barW / 2);
      ctx.fill();
    }
  }

  // Only run the rAF loop while the waveform is on screen.
  $effect(() => {
    if (phase === "recording") {
      recStart = performance.now();
      elapsed = "0:00";
      bars.fill(0.06);
      raf = requestAnimationFrame(draw);
      return () => cancelAnimationFrame(raf);
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
      // Perceptual-ish scaling: mic RMS rarely exceeds ~0.3.
      targetLevel = Math.min(1, Math.pow(e.payload.rms * 3.2, 0.8));
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
      <span class="live" aria-hidden="true"></span>
      <canvas bind:this={canvas} class="bars"></canvas>
      <span class="timer">{elapsed}</span>
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
    {:else}
      <div class="mini"><i></i><i></i><i></i></div>
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
  /* The resting pill carries the app icon's three bars: short, tall, mid. */
  .pill.idle {
    width: 56px;
    height: 14px;
    opacity: 0.9;
  }

  .mini {
    display: flex;
    align-items: center;
    gap: 2.5px;
  }

  .mini i {
    width: 2px;
    border-radius: 1px;
    background: rgba(255, 255, 255, 0.5);
  }

  .mini i:nth-child(1) {
    height: 3.5px;
  }
  .mini i:nth-child(2) {
    height: 7px;
  }
  .mini i:nth-child(3) {
    height: 5px;
  }

  /* --------------------------------------------------------- Recording */
  .pill.recording {
    width: 300px;
    height: 40px;
    padding: 0 14px 0 15px;
  }

  /* The one spot of colour while listening: a steady red "on air" dot. */
  .live {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 50%;
    background: #ff453a;
    box-shadow: 0 0 8px rgba(255, 69, 58, 0.55);
    animation: breathe 1.6s ease-in-out infinite;
  }

  @keyframes breathe {
    50% {
      opacity: 0.55;
    }
  }

  .bars {
    width: 216px;
    height: 28px;
    flex: 1 1 auto;
    min-width: 0;
  }

  .timer {
    flex: none;
    font-size: 11.5px;
    font-weight: 550;
    font-variant-numeric: tabular-nums;
    color: rgba(255, 255, 255, 0.55);
  }

  /* -------------------------------------------- Transcribing/Inserting */
  .pill.busy {
    width: 240px;
    height: 34px;
    padding: 0 16px;
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
    width: 80px;
    height: 32px;
    border-color: rgba(74, 222, 128, 0.4);
    animation: pulse 0.7s ease-out;
  }

  .check {
    width: 17px;
    height: 17px;
    flex: none;
    color: #4ade80;
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
        0 0 0 0 rgba(74, 222, 128, 0.45);
    }
    100% {
      box-shadow:
        0 4px 16px rgba(0, 0, 0, 0.35),
        0 0 0 14px rgba(74, 222, 128, 0);
    }
  }

  /* ------------------------------------------------------------- Error */
  .pill.error {
    width: 356px;
    height: 44px;
    padding: 0 10px 0 16px;
    background: rgba(38, 14, 14, 0.88);
    border-color: rgba(248, 113, 113, 0.45);
    animation: shake 0.35s ease;
  }

  .warn {
    width: 17px;
    height: 17px;
    flex: none;
    color: #f87171;
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
    .live,
    .check path {
      animation: none;
    }
    .check path {
      stroke-dashoffset: 0;
    }
  }
</style>
