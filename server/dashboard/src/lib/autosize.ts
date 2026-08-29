/**
 * Svelte action: grow a textarea to fit its content.
 *
 * Transcripts are usually one or two lines but occasionally a paragraph, so a
 * fixed row count is either mostly empty or too small. `min` keeps a stable
 * resting height so the layout doesn't jump between dictations.
 *
 * Pass the bound value so the action re-measures when the text is replaced
 * from outside (loading the next dictation), not just on keystrokes.
 */
export function autosize(node: HTMLTextAreaElement, options: { value: string; min?: number }) {
  let min = options.min ?? 72;

  function resize(): void {
    node.style.height = 'auto';
    node.style.height = `${Math.max(min, node.scrollHeight)}px`;
  }

  resize();
  node.addEventListener('input', resize);

  return {
    update(next: { value: string; min?: number }) {
      min = next.min ?? min;
      resize();
    },
    destroy() {
      node.removeEventListener('input', resize);
    },
  };
}
