-- How a correction was captured: 'review' (dashboard review queue, the
-- historical default), 'auto' (the client saw the pasted text change in the
-- focused field), or 'popup' (the client's correction window). Lets the
-- dashboard tell hand-reviewed pairs from ones real usage produced.
ALTER TABLE corrections ADD COLUMN source TEXT NOT NULL DEFAULT 'review';
