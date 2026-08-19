<script lang="ts">
  import { wordDiff } from './diff';

  /** Renders one side of a word diff: `show: 'a'` = base with deletions marked,
      `show: 'b'` = other side with insertions marked. */
  let { a, b, show }: { a: string; b: string; show: 'a' | 'b' } = $props();

  const visible = $derived(
    wordDiff(a, b).filter(
      (op) => op.type === 'same' || op.type === (show === 'a' ? 'del' : 'ins'),
    ),
  );
</script>

<p class="leading-relaxed whitespace-pre-wrap">
  {#each visible as op, k (k)}{#if k > 0}{' '}{/if}{#if op.type === 'same'}{op.text}{:else if op.type === 'del'}<span
      class="diff-del">{op.text}</span
    >{:else}<span class="diff-ins">{op.text}</span>{/if}{/each}
</p>
