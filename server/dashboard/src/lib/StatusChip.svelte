<script lang="ts">
  import { ACTIVE_RUN_STATUSES } from '../api';

  let { status, label, title }: { status: string; label?: string; title?: string } = $props();

  /** Colour only where it carries meaning; everything else stays neutral. */
  const tone = $derived.by(() => {
    if (ACTIVE_RUN_STATUSES.includes(status)) return status === 'queued' ? '' : 'dot-fg';
    switch (status) {
      case 'promoted':
      case 'accepted':
      case 'eligible':
      case 'active':
        return 'dot-ok';
      case 'rejected':
      case 'ineligible':
        return 'dot-warn';
      case 'failed':
      case 'excluded':
        return 'dot-danger';
      default:
        return '';
    }
  });

  const live = $derived(ACTIVE_RUN_STATUSES.includes(status) && status !== 'queued');
  const text = $derived(label ?? status.charAt(0).toUpperCase() + status.slice(1));
</script>

<span class="chip" {title}>
  <span class="dot {tone}" class:dot-live={live}></span>{text}
</span>
