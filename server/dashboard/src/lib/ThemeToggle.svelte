<script lang="ts">
  import { theme, type ThemeChoice } from './theme.svelte';

  const options: { value: ThemeChoice; label: string; icon: string[] }[] = [
    { value: 'light', label: 'Light', icon: ['M12 4.5v-2', 'M12 21.5v-2', 'M4.5 12h-2', 'M21.5 12h-2', 'm6.3 6.3-1.4-1.4', 'm19.1 19.1-1.4-1.4', 'm6.3 17.7-1.4 1.4', 'm19.1 4.9-1.4 1.4'] },
    { value: 'dark', label: 'Dark', icon: ['M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5Z'] },
    { value: 'system', label: 'Auto', icon: ['M3 5.5h18v10H3z', 'M9 19.5h6', 'M12 15.5v4'] },
  ];
</script>

<div class="seg w-full" role="radiogroup" aria-label="Colour theme">
  {#each options as opt (opt.value)}
    <button
      type="button"
      role="radio"
      aria-checked={theme.choice === opt.value}
      class="seg-item flex-1 justify-center"
      class:active={theme.choice === opt.value}
      title={opt.label}
      onclick={() => theme.set(opt.value)}
    >
      <svg
        width="13"
        height="13"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        {#if opt.value === 'light'}<circle cx="12" cy="12" r="4" />{/if}
        {#each opt.icon as d, i (i)}<path {d} />{/each}
      </svg>
      <span class="sr-only">{opt.label}</span>
    </button>
  {/each}
</div>
