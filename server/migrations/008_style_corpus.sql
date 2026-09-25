-- Your own writing, turned into cleanup-LLM training pairs by back-translation.
--
-- written_text is something the user actually typed (e.g. a prompt to a coding agent).
-- An LLM derives two things from it: target_text, the same text with only accidental
-- typos fixed, and spoken_text, what Whisper would have produced had they said it out
-- loud. (spoken_text -> target_text) then teaches the cleanup model the user's own style
-- before they have dictated much at all. Pairs are train-only, except a frozen ~10%
-- holdout used as a stand-in eval set until enough confirmed dictations exist.
CREATE TABLE style_corpus (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,          -- where it came from, e.g. 'claude-code', 'codex'
    app_name TEXT,                 -- the app it was written in; picks the cleanup tone
    written_text TEXT NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    target_text TEXT,
    spoken_text TEXT,
    generator_model TEXT,
    eval_holdout INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Which set a style run was judged on ('confirmed' dictations, or the 'writing'
-- holdout as a stand-in), and a short account of what went into it.
ALTER TABLE training_runs ADD COLUMN eval_set TEXT;
ALTER TABLE training_runs ADD COLUMN notes TEXT;
