-- polished_text: the user's preferred FINAL rendering of the utterance
-- ("how you want it written") — training pairs for the personal style/cleanup LLM,
-- as opposed to corrected_text which fixes the raw transcript for Whisper training.
ALTER TABLE corrections ADD COLUMN polished_text TEXT;
