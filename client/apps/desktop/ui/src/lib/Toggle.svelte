<script lang="ts">
  let {
    checked,
    onchange,
    disabled = false,
    label,
  }: {
    checked: boolean;
    onchange: (value: boolean) => void;
    disabled?: boolean;
    label: string;
  } = $props();
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  class="toggle"
  class:on={checked}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>
