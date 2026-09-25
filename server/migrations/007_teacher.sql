-- The teacher: an optional second opinion on every dictation, used to pre-fill
-- review so confirming a dictation is a keypress rather than retyping it.
--
-- second_*  a slower, more accurate ASR pass over the same audio (e.g. large-v3,
--           beam 5), with the spans of the served transcript it disagrees on.
-- *_guess   an LLM's reconciliation of the two transcripts: what was literally
--           said, and how it should read once cleaned up.
--
-- Nothing here is training data on its own. Guesses become targets only once the
-- user confirms them in review (they land in corrections like any other edit);
-- the style trainer may use unconfirmed polished guesses as low-weight "silver".
CREATE TABLE teacher_labels (
    dictation_id TEXT PRIMARY KEY REFERENCES dictations(id),
    -- 'done': every enabled part succeeded; 'partial': the ASR pass is stored but
    -- the LLM part failed or is pending retry; 'failed': the ASR pass failed.
    status TEXT NOT NULL CHECK (status IN ('done', 'partial', 'failed')),
    second_text TEXT,
    second_model TEXT,
    -- JSON list of {start, end, alt, t0, t1}: character spans of raw_text the second
    -- pass heard differently (alt), with where in the audio that was (seconds).
    disagreements_json TEXT,
    literal_guess TEXT,
    polished_guess TEXT,
    llm_model TEXT,
    llm_error TEXT,
    -- 1 when anything here disagrees with what una pasted, so the dictation is worth
    -- a look; 0 when every opinion agreed with it.
    needs_review INTEGER NOT NULL DEFAULT 1,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_teacher_labels_status ON teacher_labels(status);
