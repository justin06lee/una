/** Formatting helpers shared across pages. */

/** "0:07" for 7s, "1:23" for 83s. */
export function fmtDur(ms: number): string {
  const s = Math.round(ms / 1000);
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, '0')}`;
}

/** Seconds as "m:ss" for the audio scrubber. */
export function fmtTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
  const s = Math.floor(seconds);
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, '0')}`;
}

/** "Aug 18, 15:12" in local time. */
export function fmtDate(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

/** "Aug 18" in local time. */
export function fmtShortDate(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/** WER fraction -> "12.3%". */
export function fmtWer(wer: number | null | undefined): string {
  if (wer === null || wer === undefined) return '—';
  return `${(wer * 100).toFixed(1)}%`;
}

/** Signed percentage-point delta, e.g. "-1.4 pp". */
export function fmtWerDelta(baseline: number, candidate: number): string {
  const pp = (candidate - baseline) * 100;
  return `${pp > 0 ? '+' : ''}${pp.toFixed(1)} pp`;
}

/** Last chars of a ULID for compact display. */
export function shortId(id: string, n = 6): string {
  return id.length > n ? id.slice(-n) : id;
}

/** Clean axis step: 1/2/5 x 10^k covering `raw`. */
export function niceStep(raw: number): number {
  if (raw <= 0) return 1;
  const mag = Math.pow(10, Math.floor(Math.log10(raw)));
  for (const m of [1, 2, 5, 10]) {
    if (raw <= m * mag) return m * mag;
  }
  return 10 * mag;
}
