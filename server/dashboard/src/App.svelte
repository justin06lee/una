<script lang="ts">
  import { onMount } from 'svelte';

  import { api, type Health } from './api';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import Logo from './lib/Logo.svelte';
  import { theme } from './lib/theme.svelte';
  import { router } from './router.svelte';
  import Dictionary from './pages/Dictionary.svelte';
  import Home from './pages/Home.svelte';
  import Insights from './pages/Insights.svelte';
  import Review from './pages/Review.svelte';
  import Settings from './pages/Settings.svelte';
  import Training from './pages/Training.svelte';

  const nav: { path: string; label: string; icon: IconName }[] = [
    { path: '/home', label: 'Home', icon: 'home' },
    { path: '/review', label: 'Review', icon: 'inbox' },
    { path: '/dictionary', label: 'Dictionary', icon: 'book' },
    { path: '/training', label: 'Training', icon: 'flask' },
    { path: '/insights', label: 'Insights', icon: 'chart' },
  ];

  let health = $state<Health | null>(null);
  let unreachable = $state(false);
  let backlog = $state<number | null>(null);

  async function pollHealth(): Promise<void> {
    try {
      health = await api.health();
      unreachable = false;
    } catch {
      unreachable = true;
    }
  }

  async function refreshBacklog(): Promise<void> {
    try {
      backlog = (await api.stats()).review.backlog;
    } catch {
      /* the count is a nicety; the nav works without it */
    }
  }

  onMount(() => {
    void pollHealth();
    const t = setInterval(() => void pollHealth(), 10_000);
    const b = setInterval(() => void refreshBacklog(), 60_000);
    return () => {
      clearInterval(t);
      clearInterval(b);
    };
  });

  // Re-count on every navigation, so the Review badge drops as you work.
  $effect(() => {
    void router.path;
    void refreshBacklog();
  });

  const tone = $derived(
    unreachable ? 'dot-danger' : !health ? '' : health.status === 'ok' ? 'dot-ok' : 'dot-warn',
  );
  const statusLabel = $derived(
    unreachable
      ? 'Server offline'
      : health === null
        ? 'Connecting…'
        : health.status === 'ok'
          ? 'Connected'
          : 'Degraded',
  );
  const statusDetail = $derived.by(() => {
    if (!health) return '';
    const parts = [health.asr_model ?? 'no model'];
    if (!health.asr_model_loaded) parts.push('unloaded');
    if (health.ollama !== 'ok') parts.push(`ollama ${health.ollama}`);
    if (health.gpu) parts.push(`${(health.gpu.vram_free_mb / 1024).toFixed(1)} GB free`);
    return parts.join(' · ');
  });
  const themeIcon = $derived<IconName>(
    theme.choice === 'light' ? 'sun' : theme.choice === 'dark' ? 'moon' : 'monitor',
  );
  const active = $derived((path: string) => router.path === path);
</script>

<div class="flex h-screen overflow-hidden bg-canvas text-fg">
  <aside class="flex w-56 flex-none flex-col border-r border-line bg-sidebar">
    <div class="flex h-14 items-center gap-2.5 px-4">
      <Logo size={22} />
      <span class="text-[15px] font-semibold tracking-[-0.02em]">una</span>
    </div>

    <nav class="flex-1 space-y-0.5 overflow-y-auto px-2.5 pt-2">
      {#each nav as item (item.path)}
        <a href="#{item.path}" class="nav-item" class:active={active(item.path)}>
          <Icon name={item.icon} size={16} />
          <span class="flex-1">{item.label}</span>
          {#if item.path === '/review' && backlog}
            <span class="text-[11.5px] text-faint tabular-nums">{backlog}</span>
          {/if}
        </a>
      {/each}
    </nav>

    <div class="px-2.5 pb-3">
      <a href="#/settings" class="nav-item" class:active={active('/settings')}>
        <Icon name="settings" size={16} />
        Settings
      </a>

      <div class="mt-2 flex items-start gap-2 border-t border-line px-2.5 pt-3">
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2 text-[12.5px] font-medium">
            <span class="dot {tone}" class:dot-live={unreachable}></span>
            {statusLabel}
            {#if health?.training_active}
              <span class="text-[11.5px] font-normal text-faint">· training</span>
            {/if}
          </div>
          {#if statusDetail}
            <div class="mt-1 truncate pl-3.5 text-[11.5px] text-faint" title={statusDetail}>
              {statusDetail}
            </div>
          {/if}
        </div>
        <button
          type="button"
          class="btn btn-ghost btn-sm btn-icon -mt-1 -mr-1"
          onclick={() => theme.cycle()}
          title="Theme: {theme.choice}"
          aria-label="Switch theme (now {theme.choice})"
        >
          <Icon name={themeIcon} size={14} />
        </button>
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
