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

/** Milliseconds of audio as a human span: "4.3 min", "2.1 h". */
export function fmtSpan(ms: number): string {
  const minutes = ms / 60_000;
  if (minutes < 1) return `${Math.round(ms / 1000)} s`;
  if (minutes < 90) return `${minutes.toFixed(1)} min`;
  return `${(minutes / 60).toFixed(1)} h`;
}

/** Thousands-separated integer. */
export function fmtNum(n: number): string {
  return Math.round(n).toLocaleString();
}

/** Compact count for tight spaces: 1234 -> "1.2k". */
export function fmtCompact(n: number): string {
  if (n < 1000) return String(Math.round(n));
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
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

/** "15:12" in local time — the feed already groups by day. */
export function fmtClock(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', hour12: false });
}

/** "Aug 18" in local time. */
export function fmtShortDate(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/** Local YYYY-MM-DD for an ISO timestamp. */
export function localDay(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(
    d.getDate(),
  ).padStart(2, '0')}`;
}

/** "Today" / "Yesterday" / "Monday, Aug 18" for a YYYY-MM-DD key. */
export function fmtDayHeading(day: string): string {
  const [y, m, d] = day.split('-').map(Number);
  if (!y || !m || !d) return day;
  const date = new Date(y, m - 1, d);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const diff = Math.round((today.getTime() - date.getTime()) / 86_400_000);
  if (diff === 0) return 'Today';
  if (diff === 1) return 'Yesterday';
  const sameYear = date.getFullYear() === new Date().getFullYear();
  return date.toLocaleDateString(undefined, {
    weekday: 'long',
    month: 'short',
    day: 'numeric',
    ...(sameYear ? {} : { year: 'numeric' }),
  });
}

/**
 * WER as displayed. The server stores WER already in percent (jiwer x 100),
 * so this formats the number as-is rather than scaling it again.
 */
export function fmtWer(wer: number | null | undefined): string {
  if (wer === null || wer === undefined) return '—';
  return `${wer.toFixed(2)}%`;
}

/** Signed percentage-point delta between two stored-percent WERs, e.g. "-1.4 pp". */
export function fmtWerDelta(baseline: number, candidate: number): string {
  const pp = candidate - baseline;
  return `${pp > 0 ? '+' : ''}${pp.toFixed(2)} pp`;
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
