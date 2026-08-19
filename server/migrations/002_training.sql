CREATE TABLE training_runs (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN (
        'queued', 'building', 'training', 'evaluating', 'converting',
        'promoted', 'rejected', 'failed', 'cancelled'
    )),
    started_at TEXT,
    finished_at TEXT,
    base_model_id TEXT REFERENCES models(id),
    produced_model_id TEXT,
    n_train INTEGER,
    n_eval INTEGER,
    train_minutes REAL,
    hyperparams_json TEXT,
    wer_baseline REAL,
    wer_candidate REAL,
    log_path TEXT,
    error TEXT,
    pid INTEGER,
    progress REAL NOT NULL DEFAULT 0
);
