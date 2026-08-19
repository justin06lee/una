<script lang="ts">
  import { onMount } from 'svelte';

  import { api, type Health } from './api';
  import { router } from './router.svelte';
  import Dictionary from './pages/Dictionary.svelte';
  import History from './pages/History.svelte';
  import Review from './pages/Review.svelte';
  import Settings from './pages/Settings.svelte';
  import Stats from './pages/Stats.svelte';
  import Training from './pages/Training.svelte';

  const nav = [
    {
      path: '/review',
      label: 'Review',
      icon: ['M12 22a10 10 0 1 1 0-20 10 10 0 0 1 0 20Z', 'm10 8 6 4-6 4V8Z'],
    },
    {
      path: '/history',
      label: 'History',
      icon: ['M12 22a10 10 0 1 1 0-20 10 10 0 0 1 0 20Z', 'M12 7v5l3.5 2'],
    },
    {
      path: '/dictionary',
      label: 'Dictionary',
      icon: [
        'M4 19.5A2.5 2.5 0 0 1 6.5 17H20',
        'M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z',
      ],
    },
    { path: '/training', label: 'Training', icon: ['M13 2 3 14h9l-1 8 10-12h-9l1-8Z'] },
    { path: '/stats', label: 'Stats', icon: ['M18 20V10', 'M12 20V4', 'M6 20v-6'] },
    {
      path: '/settings',
      label: 'Settings',
      icon: [
        'M21 5h-7M10 5H3',
        'M21 12h-9M8 12H3',
        'M21 19h-5M12 19H3',
        'M12 3v4',
        'M10 10v4',
        'M14 17v4',
      ],
    },
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
      ? 'var(--color-danger)'
      : health.status === 'ok'
        ? 'var(--color-ok)'
        : 'var(--color-warn)',
  );
  const statusLabel = $derived(
    unreachable ? 'offline' : health === null ? 'connecting…' : health.status,
  );
</script>

<div class="flex h-screen overflow-hidden bg-bg text-text">
  <aside class="flex w-52 flex-none flex-col border-r border-border bg-surface">
    <div class="flex h-14 items-center gap-2.5 px-4">
      <div
        class="flex h-6 w-6 items-center justify-center rounded-md bg-accent text-[13px] font-bold text-[#0b0b10]"
      >
        u
      </div>
      <div class="leading-none">
        <div class="text-sm font-semibold tracking-tight">una</div>
        <div class="mt-0.5 text-[10px] text-faint">dictation server</div>
      </div>
    </div>

    <nav class="flex-1 space-y-0.5 px-2 py-2">
      {#each nav as item (item.path)}
        <a
          href="#{item.path}"
          class="nav-item"
          class:active={router.path === item.path || router.path.startsWith(item.path + '/')}
        >
          <svg
            width="15"
            height="15"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
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

    <div class="border-t border-border p-3">
      <div class="space-y-1.5 rounded-lg border border-border bg-raised/50 p-2.5 text-[11px]">
        <div class="flex items-center gap-2">
          <span
            class="h-2 w-2 flex-none rounded-full"
            style="background: {dotColor}"
            class:animate-pulse={unreachable}
          ></span>
          <span class="font-medium capitalize">{statusLabel}</span>
          {#if health?.training_active}
            <span class="chip chip-accent ml-auto !h-[18px] !px-1.5 !text-[10px]">
              <span class="chip-dot animate-pulse"></span>training
            </span>
          {/if}
        </div>
        {#if health}
          <div class="truncate text-faint" title={health.asr_model ?? undefined}>
            {health.asr_model ?? 'no model'}{health.asr_model_loaded ? '' : ' (unloaded)'}
          </div>
          <div class="text-faint">
            ollama ·
            <span style="color: {health.ollama === 'ok' ? 'var(--color-muted)' : 'var(--color-warn)'}">
              {health.ollama}
            </span>
          </div>
          {#if health.gpu}
            <div class="truncate text-faint tabular-nums" title={health.gpu.name}>
              {health.gpu.name} · {(health.gpu.vram_free_mb / 1024).toFixed(1)} GB free
            </div>
          {/if}
        {/if}
      </div>
    </div>
  </aside>

  <main class="min-w-0 flex-1 overflow-y-auto">
    {#if router.path === '/history'}
      <History />
    {:else if router.path === '/dictionary'}
      <Dictionary />
    {:else if router.path === '/training'}
      <Training />
    {:else if router.path === '/stats'}
      <Stats />
    {:else if router.path === '/settings'}
      <Settings />
    {:else}
      <Review />
    {/if}
  </main>
</div>
