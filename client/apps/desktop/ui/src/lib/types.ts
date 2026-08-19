// Shapes mirrored from una-core's serde output.

export type Snapshot =
  | { state: "idle" }
  | { state: "recording"; latched: boolean }
  | { state: "transcribing" }
  | { state: "inserting" }
  | { state: "done" }
  | { state: "error"; kind: string; message: string; retryable: boolean };

export interface LevelFrame {
  rms: number;
  peak: number;
}

export interface Config {
  server: { url: string; autodiscover: boolean };
  hotkey: { binding: string; mode: "hold" | "toggle" | "hybrid" };
  audio: { input_device: string; prefer_builtin: boolean };
  insert: {
    restore_clipboard: boolean;
    restore_delay_ms: number;
    paste_overrides: Record<string, string>;
  };
  ui: { sounds: boolean; hud_mode: "pill" | "flash" };
  general: { launch_at_login: boolean };
}

export interface CapturedHotkey {
  binding: string;
  name: string;
  keycode: number | null;
  is_modifier: boolean;
  is_native: boolean;
}

export interface DiscoveredServer {
  name: string;
  url: string;
  version: string | null;
  api: string | null;
}

export interface PermissionsStatus {
  mic: string;
  accessibility: string;
  secure_input: boolean;
  probe: { backend: string; can_paste: boolean; detail: string };
}

export interface TestRecordResult {
  ok: boolean;
  max_rms: number;
  max_peak: number;
}
