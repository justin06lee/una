/** Typed fetch client for the una /v1 API. Same-origin; dev server proxies /v1. */

export class UnaError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(code: string, message: string, status: number) {
    super(message);
    this.name = 'UnaError';
    this.code = code;
    this.status = status;
  }
}

export function errMsg(e: unknown): string {
  if (e instanceof UnaError) return e.message;
  if (e instanceof Error) return e.message;
  return 'Something went wrong';
}

// ---- contract types (mirror server/src/una_server/schemas.py) ----

export interface DictationSummary {
  id: string;
  created_at: string;
  app_name: string | null;
  duration_ms: number;
  text: string;
  reviewed: boolean;
  review_action: string | null;
}

export interface DictationDetail extends DictationSummary {
  raw_text: string;
  cleaned_text: string | null;
  cleanup_applied: boolean;
  asr_model: string | null;
  llm_model: string | null;
  language: string | null;
  corrected_text: string | null;
  /** Preferred final rendering (style-LLM training pair). Absent on older servers. */
  polished_text?: string | null;
  training_eligible: boolean | null;
  eligibility_reason: string | null;
  /** Where the correction came from: 'auto' | 'popup' | 'review'. Absent on older servers. */
  correction_source?: string | null;
  eval_holdout: boolean;
}

export interface DictationList {
  items: DictationSummary[];
  next_cursor: string | null;
}

export type ReviewAction = 'accepted' | 'edited' | 'skipped' | 'excluded';

export interface CorrectionResponse {
  dictation_id: string;
  action: string;
  norm_edit_distance: number | null;
  training_eligible: boolean;
  eligibility_reason: string | null;
  /** Stored style pair, if any. Absent on older servers. */
  polished_text?: string | null;
}

export interface DictionaryEntry {
  id: string;
  phrase: string;
  sounds_like: string | null;
  notes: string | null;
  active: boolean;
  hit_count: number;
}

export interface Eligibility {
  eligible_pairs: number;
  eligible_minutes: number;
  threshold_minutes: number;
  ready: boolean;
  /** Collected style pairs (polished-text corrections). */
  style_pairs: number;
  style_threshold_pairs: number;
  style_ready: boolean;
}

export type RunKind = 'asr' | 'style';

export interface TrainingRun {
  id: string;
  kind: RunKind;
  status: string;
  started_at: string | null;
  finished_at: string | null;
  base_model_id: string | null;
  produced_model_id: string | null;
  n_train: number | null;
  n_eval: number | null;
  wer_baseline: number | null;
  wer_candidate: number | null;
  error: string | null;
  progress: number;
}

export interface ModelInfo {
  id: string;
  kind: string;
  parent_model_id: string | null;
  training_run_id: string | null;
  eval_wer: number | null;
  is_active: boolean;
  created_at: string;
  notes: string | null;
}

export interface Health {
  status: string;
  asr_model: string | null;
  asr_model_loaded: boolean;
  ollama: string;
  training_active: boolean;
  gpu: { name: string; vram_free_mb: number } | null;
}

export interface StatsDay {
  day: string;
  n: number;
  ms: number;
  words: number;
}

export interface StatsApp {
  app: string;
  n: number;
  ms: number;
  words: number;
}

export interface WerPoint {
  id: string;
  finished_at: string | null;
  wer_baseline: number | null;
  wer_candidate: number | null;
  status: string;
}

export interface Stats {
  totals: {
    dictations: number;
    words: number;
    ms: number;
    /** Words spoken per minute of recorded audio. */
    avg_wpm: number;
    days_active: number;
  };
  streak: { current: number; longest: number };
  per_day: StatsDay[];
  by_app: StatsApp[];
  cleanup: { applied: number; words_removed: number; dictionary_hits: number };
  review: { backlog: number; reviewed: number; eligible: number };
  wer_series: WerPoint[];
}

export interface Settings {
  'cleanup.enabled': boolean;
  'cleanup.model': string;
  'cleanup.timeout_s': number;
  'training.threshold_minutes': number;
  'training.auto': boolean;
  'training.auto_idle_minutes': number;
  'training.max_edit_distance': number;
}

/** Training-run statuses that mean "still going". */
export const ACTIVE_RUN_STATUSES = ['queued', 'building', 'training', 'evaluating', 'converting'];

// ---- transport ----

const BASE = '/v1';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(BASE + path, init);
  } catch {
    throw new UnaError('NETWORK', 'Server unreachable', 0);
  }
  if (!res.ok) {
    let code = `HTTP_${res.status}`;
    let message = res.statusText || `HTTP ${res.status}`;
    try {
      const body = await res.json();
      if (body?.error) {
        code = body.error.code ?? code;
        message = body.error.message ?? message;
      } else if (body?.detail) {
        message = typeof body.detail === 'string' ? body.detail : JSON.stringify(body.detail);
      }
    } catch {
      /* non-JSON error body */
    }
    throw new UnaError(code, message, res.status);
  }
  if (res.status === 204) return undefined as T;
  const ct = res.headers.get('content-type') ?? '';
  if (!ct.includes('application/json')) return (await res.text()) as T;
  return (await res.json()) as T;
}

function json(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  };
}

function qs(params: Record<string, string | number | boolean | undefined>): string {
  const sp = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined) sp.set(k, String(v));
  }
  const s = sp.toString();
  return s ? `?${s}` : '';
}

// ---- endpoints ----

export const api = {
  // dictations
  listDictations(params: {
    limit?: number;
    cursor?: string;
    q?: string;
    app?: string;
    reviewed?: boolean;
  }): Promise<DictationList> {
    return request(`/dictations${qs(params)}`);
  },
  getDictation(id: string): Promise<DictationDetail> {
    return request(`/dictations/${id}`);
  },
  deleteDictation(id: string): Promise<void> {
    return request(`/dictations/${id}`, { method: 'DELETE' });
  },
  audioUrl(id: string): string {
    return `${BASE}/dictations/${id}/audio`;
  },
  putCorrection(
    id: string,
    body: { action: ReviewAction; corrected_text?: string | null; polished_text?: string | null },
  ): Promise<CorrectionResponse> {
    return request(`/dictations/${id}/correction`, json('PUT', body));
  },
  reviewNext(after?: string): Promise<DictationDetail | null> {
    return request(`/review/next${qs({ after })}`);
  },

  // dictionary
  listDictionary(): Promise<DictionaryEntry[]> {
    return request('/dictionary');
  },
  createDictionaryEntry(body: {
    phrase: string;
    sounds_like?: string | null;
    notes?: string | null;
  }): Promise<DictionaryEntry> {
    return request('/dictionary', json('POST', body));
  },
  patchDictionaryEntry(
    id: string,
    body: Partial<Pick<DictionaryEntry, 'phrase' | 'sounds_like' | 'notes' | 'active'>>,
  ): Promise<DictionaryEntry> {
    return request(`/dictionary/${id}`, json('PATCH', body));
  },
  deleteDictionaryEntry(id: string): Promise<void> {
    return request(`/dictionary/${id}`, { method: 'DELETE' });
  },

  // training
  trainingEligibility(): Promise<Eligibility> {
    return request('/training/eligibility');
  },
  listTrainingRuns(): Promise<TrainingRun[]> {
    return request('/training/runs');
  },
  startTrainingRun(kind: RunKind = 'asr'): Promise<TrainingRun> {
    return request('/training/runs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ kind }),
    });
  },
  getTrainingRun(id: string): Promise<TrainingRun> {
    return request(`/training/runs/${id}`);
  },
  trainingRunLog(id: string, tail = 200): Promise<string> {
    return request(`/training/runs/${id}/log${qs({ tail })}`);
  },
  cancelTrainingRun(id: string): Promise<TrainingRun> {
    return request(`/training/runs/${id}/cancel`, { method: 'POST' });
  },

  // models
  listModels(): Promise<ModelInfo[]> {
    return request('/models');
  },
  activateModel(id: string): Promise<ModelInfo> {
    return request(`/models/${id}/activate`, { method: 'POST' });
  },

  // system
  health(): Promise<Health> {
    return request('/health');
  },
  stats(): Promise<Stats> {
    return request('/stats');
  },

  // settings
  getSettings(): Promise<Settings> {
    return request('/settings');
  },
  putSettings(body: Partial<Settings>): Promise<Settings> {
    return request('/settings', json('PUT', body));
  },
};
