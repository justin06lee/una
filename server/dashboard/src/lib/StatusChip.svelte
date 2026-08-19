<script lang="ts">
  import { ACTIVE_RUN_STATUSES } from '../api';

  let { status, label, title }: { status: string; label?: string; title?: string } = $props();

  const tone = $derived.by(() => {
    if (ACTIVE_RUN_STATUSES.includes(status)) return status === 'queued' ? 'muted' : 'accent';
    switch (status) {
      case 'promoted':
      case 'accepted':
      case 'eligible':
        return 'ok';
      case 'edited':
      case 'active':
        return 'accent';
      case 'rejected':
      case 'ineligible':
        return 'warn';
      case 'failed':
      case 'excluded':
        return 'danger';
      default:
        return 'muted';
    }
  });

  const pulsing = $derived(ACTIVE_RUN_STATUSES.includes(status) && status !== 'queued');
</script>

<span class="chip chip-{tone}" {title}>
  <span class="chip-dot" class:animate-pulse={pulsing}></span>{label ?? status}
</span>
