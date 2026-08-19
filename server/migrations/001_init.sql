CREATE TABLE models (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('base', 'finetuned')),
    ct2_path TEXT,
    hf_source TEXT,
    parent_model_id TEXT REFERENCES models(id),
    training_run_id TEXT,
    eval_wer REAL,
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    notes TEXT
);

CREATE TABLE dictations (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    app_name TEXT,
    audio_path TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    sample_rate INTEGER NOT NULL DEFAULT 16000,
    language TEXT,
    raw_text TEXT NOT NULL,
    cleaned_text TEXT,
    cleanup_applied INTEGER NOT NULL DEFAULT 0,
    cleanup_error TEXT,
    asr_model_id TEXT REFERENCES models(id),
    llm_model TEXT,
    transcribe_ms INTEGER,
    cleanup_ms INTEGER,
    utterance_id TEXT,
    eval_holdout INTEGER NOT NULL DEFAULT 0,
    deleted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_dictations_created ON dictations(created_at DESC);
CREATE UNIQUE INDEX idx_dictations_utterance ON dictations(utterance_id) WHERE utterance_id IS NOT NULL;

CREATE TABLE corrections (
    id TEXT PRIMARY KEY,
    dictation_id TEXT NOT NULL UNIQUE REFERENCES dictations(id),
    corrected_text TEXT,
    action TEXT NOT NULL CHECK (action IN ('accepted', 'edited', 'skipped', 'excluded')),
    norm_edit_distance REAL,
    training_eligible INTEGER NOT NULL DEFAULT 0,
    eligibility_reason TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE dictionary_entries (
    id TEXT PRIMARY KEY,
    phrase TEXT NOT NULL UNIQUE,
    sounds_like TEXT,
    notes TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    hit_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
