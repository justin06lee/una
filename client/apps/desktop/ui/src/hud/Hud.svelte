<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import type { LevelFrame, Snapshot } from "../lib/types";

  const BAR_COUNT = 24;

  let snapshot = $state<Snapshot>({ state: "idle" });
  let canvas = $state<HTMLCanvasElement | null>(null);

  // 30Hz data, interpolated at 60fps.
  let targetLevel = 0;
  let bars = new Array(BAR_COUNT).fill(0.05);
  let phases = Array.from({ length: BAR_COUNT }, () => Math.random() * Math.PI * 2);
  let raf = 0;
  let shimmerT = 0;

  const isRecording = $derived(snapshot.state === "recording");
  const isBusy = $derived(
    snapshot.state === "transcribing" || snapshot.state === "inserting",
  );
  const isDone = $derived(snapshot.state === "done");
  const isError = $derived(snapshot.state === "error");

  const label = $derived.by(() => {
    switch (snapshot.state) {
      case "recording":
        return snapshot.latched ? "Listening — press again to finish" : "Listening…";
      case "transcribing":
        return "Transcribing…";
      case "inserting":
        return "Inserting…";
      case "done":
        return "Inserted";
      case "error":
        return snapshot.message || "Something went wrong";
      default:
        return "";
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

    const gap = 3;
    const barW = (w - gap * (BAR_COUNT - 1)) / BAR_COUNT;
    const now = performance.now() / 1000;

    if (isRecording) {
      for (let i = 0; i < BAR_COUNT; i++) {
        // Center-weighted target with per-bar noise, exponentially smoothed.
        const centerBias = 1 - Math.abs(i - (BAR_COUNT - 1) / 2) / (BAR_COUNT / 2);
        const noise = 0.10 * Math.sin(now * (5 + (i % 5)) + phases[i]);
        const target = Math.max(
          0.06,
          Math.min(1, targetLevel * (0.55 + 0.65 * centerBias) + noise * targetLevel),
        );
        bars[i] += (target - bars[i]) * 0.25;
      }
      ctx.fillStyle = "rgba(255,255,255,0.92)";
      for (let i = 0; i < BAR_COUNT; i++) {
        const bh = Math.max(2, bars[i] * h);
        const x = i * (barW + gap);
        const y = (h - bh) / 2;
        roundRect(ctx, x, y, barW, bh, barW / 2);
      }
    } else if (isBusy) {
      // Indeterminate shimmer pill.
      shimmerT = (shimmerT + 0.012) % 1.4;
      const grad = ctx.createLinearGradient(
        (shimmerT - 0.4) * w,
        0,
        shimmerT * w,
        0,
      );
      grad.addColorStop(0, "rgba(255,255,255,0.10)");
      grad.addColorStop(0.5, "rgba(255,255,255,0.55)");
      grad.addColorStop(1, "rgba(255,255,255,0.10)");
      ctx.fillStyle = "rgba(255,255,255,0.14)";
      roundRect(ctx, 0, h / 2 - 3, w, 6, 3);
      ctx.fillStyle = grad;
      roundRect(ctx, 0, h / 2 - 3, w, 6, 3);
      // decay bars for next recording
      for (let i = 0; i < BAR_COUNT; i++) bars[i] *= 0.9;
    }
  }

  function roundRect(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
    r: number,
  ) {
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, r);
    ctx.fill();
  }

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
    raf = requestAnimationFrame(draw);
    return () => {
      unlistenState.then((f) => f());
      unlistenLevel.then((f) => f());
      cancelAnimationFrame(raf);
    };
  });
</script>

<div class="capsule" class:error={isError} class:done={isDone}>
  {#if isRecording || isBusy}
    <canvas bind:this={canvas} class="bars"></canvas>
  {:else if isDone}
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
  {:else if isError}
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
  {/if}

  {#if label}
    <span class="label" title={label}>{label}</span>
  {/if}

  {#if isError && snapshot.state === "error" && snapshot.retryable}
    <button class="retry" onclick={retry}>Retry</button>
  {/if}
</div>

<style>
  .capsule {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 56px;
    max-width: 320px;
    min-width: 180px;
    padding: 0 18px;
    border-radius: 9999px;
    background: rgba(18, 18, 22, 0.72);
    -webkit-backdrop-filter: blur(18px) saturate(1.4);
    backdrop-filter: blur(18px) saturate(1.4);
    border: 1px solid rgba(255, 255, 255, 0.14);
    box-shadow:
      0 8px 24px rgba(0, 0, 0, 0.35),
      inset 0 0.5px 0 rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 0.92);
  }

  .capsule.error {
    animation: shake 0.35s ease;
    border-color: rgba(255, 105, 97, 0.5);
  }

  .capsule.done .check {
    animation: pop 0.25s ease;
  }

  .bars {
    width: 130px;
    height: 34px;
    flex: none;
  }

  .check {
    width: 22px;
    height: 22px;
    flex: none;
    color: #7ee787;
  }

  .warn {
    width: 20px;
    height: 20px;
    flex: none;
    color: #ff6961;
  }

  .label {
    font-size: 13px;
    font-weight: 500;
    letter-spacing: 0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .retry {
    flex: none;
    appearance: none;
    border: 1px solid rgba(255, 255, 255, 0.25);
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    padding: 4px 12px;
    border-radius: 9999px;
    cursor: pointer;
  }

  .retry:hover {
    background: rgba(255, 255, 255, 0.22);
  }

  @keyframes shake {
    0%, 100% { transform: translateX(0); }
    25% { transform: translateX(-5px); }
    50% { transform: translateX(4px); }
    75% { transform: translateX(-2px); }
  }

  @keyframes pop {
    0% { transform: scale(0.4); opacity: 0; }
    100% { transform: scale(1); opacity: 1; }
  }
</style>
