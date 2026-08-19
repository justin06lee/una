<script lang="ts">
  let { class: klass = '' }: { class?: string } = $props();
</script>

<div class="skeleton {klass}"></div>
