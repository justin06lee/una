/** Simple LCS word diff between two strings. */

export type DiffOp = { type: 'same' | 'del' | 'ins'; text: string };

export function wordDiff(a: string, b: string): DiffOp[] {
  const aw = a.split(/\s+/).filter(Boolean);
  const bw = b.split(/\s+/).filter(Boolean);
  const n = aw.length;
  const m = bw.length;

  // Guard the O(n*m) table for pathological inputs.
  if (n * m > 250_000) {
    if (a === b) return [{ type: 'same', text: a }];
    return [
      { type: 'del', text: a },
      { type: 'ins', text: b },
    ];
  }

  const cols = m + 1;
  const dp = new Uint32Array((n + 1) * cols);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i * cols + j] =
        aw[i] === bw[j]
          ? dp[(i + 1) * cols + j + 1]! + 1
          : Math.max(dp[(i + 1) * cols + j]!, dp[i * cols + j + 1]!);
    }
  }

  const ops: DiffOp[] = [];
  const push = (type: DiffOp['type'], word: string) => {
    const last = ops[ops.length - 1];
    if (last && last.type === type) last.text += ` ${word}`;
    else ops.push({ type, text: word });
  };

  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (aw[i] === bw[j]) {
      push('same', aw[i]!);
      i++;
      j++;
    } else if (dp[(i + 1) * cols + j]! >= dp[i * cols + j + 1]!) {
      push('del', aw[i]!);
      i++;
    } else {
      push('ins', bw[j]!);
      j++;
    }
  }
  while (i < n) push('del', aw[i++]!);
  while (j < m) push('ins', bw[j++]!);
  return ops;
}
