<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    value = $bindable(''),
    placeholder = 'Search…',
    label = 'Search',
    oninput,
  }: { value?: string; placeholder?: string; label?: string; oninput?: () => void } = $props();
</script>

<div class="relative min-w-52 flex-1">
  <Icon name="search" size={14} class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-faint" />
  <input class="input pl-8" {placeholder} bind:value {oninput} aria-label={label} />
</div>
