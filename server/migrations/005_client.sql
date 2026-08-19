-- Store the client identifier the dictation arrived from (e.g. "una-desktop/0.1.0 macos-aarch64").
ALTER TABLE dictations ADD COLUMN client TEXT;
