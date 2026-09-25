<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';
  import { theme, type ThemeChoice } from './theme.svelte';

  const options: { value: ThemeChoice; label: string; icon: IconName }[] = [
    { value: 'light', label: 'Light', icon: 'sun' },
    { value: 'dark', label: 'Dark', icon: 'moon' },
    { value: 'system', label: 'System', icon: 'monitor' },
  ];
</script>

<div class="seg" role="radiogroup" aria-label="Colour theme">
  {#each options as opt (opt.value)}
    <button
      type="button"
      role="radio"
      aria-checked={theme.choice === opt.value}
      class="seg-item"
      class:active={theme.choice === opt.value}
      title={opt.label}
      onclick={() => theme.set(opt.value)}
    >
      <Icon name={opt.icon} size={13} />
      {opt.label}
    </button>
  {/each}
</div>
