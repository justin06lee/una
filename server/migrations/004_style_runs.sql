-- Training runs gain a kind: 'asr' (Whisper LoRA) or 'style' (cleanup-LLM LoRA).
ALTER TABLE training_runs ADD COLUMN kind TEXT NOT NULL DEFAULT 'asr';
