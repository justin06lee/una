<script lang="ts">
  import { onMount } from 'svelte';

  import { api, type Health } from './api';
  import Logo from './lib/Logo.svelte';
  import ThemeToggle from './lib/ThemeToggle.svelte';
  import { router } from './router.svelte';
  import Dictionary from './pages/Dictionary.svelte';
  import Home from './pages/Home.svelte';
  import Insights from './pages/Insights.svelte';
  import Review from './pages/Review.svelte';
  import Settings from './pages/Settings.svelte';
  import Training from './pages/Training.svelte';

  const nav = [
    { path: '/home', label: 'Home', icon: ['m3 10.5 9-7 9 7', 'M5.5 9.5V20h13V9.5'] },
    {
      path: '/review',
      label: 'Review',
      icon: ['M4 5.5h16', 'M4 12h10', 'M4 18.5h7', 'm16 17 2 2 4-4'],
    },
    {
      path: '/dictionary',
      label: 'Dictionary',
      icon: ['M4 19.5A2.5 2.5 0 0 1 6.5 17H20', 'M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z'],
    },
    { path: '/training', label: 'Training', icon: ['M13 2 3 14h9l-1 8 10-12h-9l1-8Z'] },
    { path: '/insights', label: 'Insights', icon: ['M18 20V9', 'M12 20V4', 'M6 20v-6'] },
  ];

  let health = $state<Health | null>(null);
  let unreachable = $state(false);

  async function pollHealth(): Promise<void> {
    try {
      health = await api.health();
      unreachable = false;
    } catch {
      unreachable = true;
    }
  }

  onMount(() => {
    void pollHealth();
    const t = setInterval(() => void pollHealth(), 10_000);
    return () => clearInterval(t);
  });

  const dotColor = $derived(
    unreachable || !health
      ? 'var(--c-danger)'
      : health.status === 'ok'
        ? 'var(--c-accent)'
        : 'var(--c-warn)',
  );
  const statusLabel = $derived(
    unreachable ? 'Offline' : health === null ? 'Connecting…' : health.status === 'ok' ? 'Ready' : 'Degraded',
  );
  const active = $derived((path: string) => router.path === path);
</script>

<div class="flex h-screen overflow-hidden bg-canvas text-text">
  <aside class="flex w-60 flex-none flex-col border-r border-border bg-surface">
    <div class="flex h-16 items-center gap-2.5 px-5">
      <Logo size={26} />
      <div class="leading-none">
        <div class="text-[15px] font-semibold tracking-tight">una</div>
        <div class="mt-1 text-[11px] text-faint">your voice, your model</div>
      </div>
    </div>

    <nav class="flex-1 space-y-0.5 overflow-y-auto px-3 py-2">
      {#each nav as item (item.path)}
        <a href="#{item.path}" class="nav-item" class:active={active(item.path)}>
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
            class="flex-none"
          >
            {#each item.icon as d, i (i)}<path {d} />{/each}
          </svg>
          {item.label}
        </a>
      {/each}
    </nav>

    <div class="space-y-2 px-3 pt-2 pb-3">
      <a href="#/settings" class="nav-item" class:active={active('/settings')}>
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.7"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
          class="flex-none"
        >
          <circle cx="12" cy="12" r="3" />
          <path
            d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6 1.65 1.65 0 0 0 10 3.09V3a2 2 0 0 1 4 0v.09A1.65 1.65 0 0 0 15 4.6a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9v.09a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"
          />
        </svg>
        Settings
      </a>

      <div class="card-quiet space-y-2 p-3">
        <div class="flex items-center gap-2">
          <span
            class="h-1.5 w-1.5 flex-none rounded-full"
            style="background: {dotColor}"
            class:animate-pulse={unreachable}
          ></span>
          <span class="text-[12px] font-semibold">{statusLabel}</span>
          {#if health?.training_active}
            <span class="chip chip-accent ml-auto !h-[18px] !px-1.5 !text-[10px]">
              <span class="chip-dot animate-pulse"></span>training
            </span>
          {/if}
        </div>
        {#if health}
          <div class="space-y-1 text-[11px] text-faint">
            <div class="truncate" title={health.asr_model ?? undefined}>
              {health.asr_model ?? 'no model'}{health.asr_model_loaded ? '' : ' · unloaded'}
            </div>
            <div>
              ollama
              <span style="color: {health.ollama === 'ok' ? 'var(--c-muted)' : 'var(--c-warn)'}">
                {health.ollama}
              </span>
            </div>
            {#if health.gpu}
              <div class="truncate tabular-nums" title={health.gpu.name}>
                {health.gpu.name} · {(health.gpu.vram_free_mb / 1024).toFixed(1)} GB free
              </div>
            {/if}
          </div>
        {/if}
        <ThemeToggle />
      </div>
    </div>
  </aside>

  <main class="min-w-0 flex-1 overflow-y-auto">
    {#if router.path === '/review'}
      <Review />
    {:else if router.path === '/dictionary'}
      <Dictionary />
    {:else if router.path === '/training'}
      <Training />
    {:else if router.path === '/insights'}
      <Insights />
    {:else if router.path === '/settings'}
      <Settings />
    {:else}
      <Home />
    {/if}
  </main>
</div>
