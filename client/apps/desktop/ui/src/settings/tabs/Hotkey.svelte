<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "../../lib/Icon.svelte";
  import type { CapturedHotkey, Config } from "../../lib/types";

  let { config = $bindable(), save }: { config: Config; save: () => void } = $props();

  let captureSupported = $state(false); // native event-tap capture available (macOS)
  let capturing = $state(false);
  let recordingHotkey = $state(false); // JS combo recorder, where native capture is missing
  let captureError = $state("");

  onMount(() => {
    invoke<boolean>("hotkey_capture_supported")
      .then((s) => (captureSupported = s))
      .catch(() => {});
  });

  const MOD_GLYPHS: Record<string, string> = {
    Ctrl: "⌃",
    Control: "⌃",
    Alt: "⌥",
    Option: "⌥",
    Shift: "⇧",
    Super: "⌘",
    Cmd: "⌘",
    Command: "⌘",
    Meta: "⌘",
    CmdOrCtrl: "⌘",
  };
  const MODIFIER_KEYCODES = [54, 55, 56, 57, 58, 59, 60, 61, 62, 63];

  function parseNative(b: string): { keycode: number; name: string } | null {
    if (!b.startsWith("native:")) return null;
    const rest = b.slice("native:".length);
    const i = rest.indexOf(":");
    const code = Number(i >= 0 ? rest.slice(0, i) : rest);
    const name = i >= 0 ? rest.slice(i + 1) : "";
    return { keycode: code, name: name || `Key ${code}` };
  }

  /** Keycaps for the current binding ("⌃ ⌥ Space" or a single "Fn"). */
  const keycaps = $derived.by(() => {
    const native = parseNative(config.hotkey.binding);
    if (native) return [native.name];
    return config.hotkey.binding.split("+").map((t) => MOD_GLYPHS[t] ?? t);
  });

  const bindingNative = $derived(parseNative(config.hotkey.binding));
  /** Native non-modifier keys are consumed system-wide — warn about it. */
  const swallowWarning = $derived(
    bindingNative !== null && !MODIFIER_KEYCODES.includes(bindingNative.keycode),
  );
  const listening = $derived(capturing || recordingHotkey);

  async function startCapture() {
    if (capturing) return;
    capturing = true;
    captureError = "";
    try {
      const cap = await invoke<CapturedHotkey>("capture_hotkey");
      config.hotkey.binding = cap.binding;
      save();
    } catch (e) {
      const msg = String(e);
      // A cancelled capture (Esc or the Cancel button) is not an error.
      if (!msg.toLowerCase().includes("cancel")) captureError = msg;
    } finally {
      capturing = false;
    }
  }

  function cancelCapture() {
    invoke("cancel_hotkey_capture").catch(() => {});
  }

  function toggleRecording() {
    if (captureSupported) {
      if (capturing) cancelCapture();
      else void startCapture();
    } else {
      recordingHotkey = !recordingHotkey;
    }
  }

  // ---- JS combo recorder (used where native capture is missing) ----------

  const NAMED_KEYS = [
    "Space", "Enter", "Tab", "Backspace", "Escape", "Home", "End", "PageUp", "PageDown",
    "Insert", "Delete", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Backquote", "Minus",
    "Equal", "Slash", "Backslash", "Comma", "Period", "Semicolon", "Quote", "BracketLeft",
    "BracketRight",
  ];

  function hotkeyFromEvent(e: KeyboardEvent): string | null {
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");
    const code = e.code;
    let key: string | null = null;
    if (code.startsWith("Key")) key = code.slice(3);
    else if (code.startsWith("Digit")) key = code.slice(5);
    else if (NAMED_KEYS.includes(code) || /^F\d{1,2}$/.test(code)) key = code;
    if (!key) return null; // modifier-only or unsupported key
    parts.push(key);
    return parts.join("+");
  }

  function onKeydown(e: KeyboardEvent) {
    if (!recordingHotkey) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      recordingHotkey = false;
      return;
    }
    const combo = hotkeyFromEvent(e);
    if (combo) {
      config.hotkey.binding = combo;
      recordingHotkey = false;
      save();
    }
  }

  const MODES = [
    { value: "hold", name: "Hold", desc: "Hold to talk, let go to insert." },
    { value: "toggle", name: "Toggle", desc: "Press to start, press again to finish." },
    { value: "hybrid", name: "Hybrid", desc: "Hold to talk, or tap to keep listening and tap again to finish." },
  ] as const;
</script>

<svelte:window onkeydown={onKeydown} />

<h1 class="page-title">Hotkey</h1>
<p class="page-sub">The key you hold, anywhere on your computer, to dictate.</p>

<div class="section">
  <div class="binding" class:listening>
    <div class="keys" aria-label="Current hotkey">
      {#if listening}
        <span class="listening-label">
          <span class="dot live"></span>
          {captureSupported ? "Press any key…" : "Press a key combination…"}
        </span>
      {:else}
        {#each keycaps as k, i (i)}
          <span class="keycap">{k}</span>
        {/each}
      {/if}
    </div>
    <button class="btn" class:btn-primary={!listening} onclick={toggleRecording}>
      {listening ? "Cancel" : "Change hotkey"}
    </button>
  </div>

  <p class="hint">
    {#if captureSupported}
      Any single key works — Fn, Right ⌘, F5, even a plain letter — including as a hold-to-talk
      key. Hold modifiers and press a key to record a combination instead. Esc cancels.
    {:else}
      This platform records modifier + key combinations. Single bare keys like Fn or Right ⌘ are
      a macOS feature. Esc cancels.
    {/if}
  </p>

  {#if bindingNative && !captureSupported}
    <div class="note warn">
      <span class="dot warn"></span>
      <span>
        The saved hotkey “{bindingNative.name}” is a macOS-only key and does nothing here. Record a
        combination above, or use a compositor keybind (below).
      </span>
    </div>
  {/if}
  {#if swallowWarning && captureSupported}
    <div class="note warn">
      <span class="dot warn"></span>
      <span>
        While una is running, “{bindingNative?.name}” is captured system-wide — other apps won't
        receive it.
      </span>
    </div>
  {/if}
  {#if captureError}
    <div class="note error"><span class="dot down"></span>{captureError}</div>
  {/if}
</div>

<div class="section">
  <div class="section-title">Mode</div>
  <div class="group" role="radiogroup" aria-label="Hotkey mode">
    {#each MODES as m (m.value)}
      <button
        class="choice"
        class:selected={config.hotkey.mode === m.value}
        role="radio"
        aria-checked={config.hotkey.mode === m.value}
        onclick={() => {
          config.hotkey.mode = m.value;
          save();
        }}
      >
        <span class="row-copy">
          <span class="row-title">{m.name}</span>
          <span class="row-sub" style="display: block">{m.desc}</span>
        </span>
        <span class="check"><Icon name="check" size={15} stroke={2} /></span>
      </button>
    {/each}
  </div>
</div>

{#if !captureSupported}
  <div class="section">
    <div class="section-title">Wayland and compositor keybinds</div>
    <p class="hint" style="margin-top: 0">
      On Wayland the compositor may restrict global hotkeys. The most reliable setup is a
      compositor keybind that runs the <code>una</code> CLI — Hyprland
      <code>bind = , F12, exec, una toggle</code> (or <code>bind</code>/<code>bindr</code> with
      <code>una start</code> and <code>una stop</code> for hold-to-talk), Sway
      <code>bindsym F12 exec una toggle</code>, or a GNOME/KDE custom shortcut running
      <code>una toggle</code>.
    </p>
  </div>
{/if}

<style>
  .binding {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 18px 18px 20px;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--panel);
    transition: border-color 160ms ease;
  }

  .binding.listening {
    border-color: var(--muted);
  }

  .keys {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
  }

  .keycap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 36px;
    height: 36px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    background: var(--subtle);
    font-size: 15px;
    font-weight: 500;
  }

  .listening-label {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
    color: var(--muted);
  }

  .live {
    background: var(--danger);
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
</style>
