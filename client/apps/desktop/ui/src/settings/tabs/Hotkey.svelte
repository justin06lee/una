<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "../../lib/Icon.svelte";
  import Keycap from "../../lib/Keycap.svelte";
  import KeyboardRow from "../../lib/KeyboardRow.svelte";
  import {
    capsFor,
    FN_KEYCODES,
    IS_MAC,
    MODIFIER_KEYCODES,
    parseNative,
    spoken,
  } from "../../lib/keycaps";
  import type { CapturedHotkey, Config } from "../../lib/types";

  let { config = $bindable(), save }: { config: Config; save: () => void } = $props();

  let captureSupported = $state(false); // native key capture (macOS, or Linux on X11)
  let capturing = $state(false);
  let recordingHotkey = $state(false); // JS combo recorder, where native capture is missing
  let captureError = $state("");
  let justSaved = $state(false);
  let savedTimer: ReturnType<typeof setTimeout> | undefined;
  /** macOS's own "Press 🌐 key to" action (0 = Do Nothing); null when never set. */
  let fnAction = $state<number | null | undefined>(undefined);

  function checkFnAction() {
    invoke<number | null>("fn_key_action")
      .then((a) => (fnAction = a))
      .catch(() => {});
  }

  onMount(() => {
    invoke<boolean>("hotkey_capture_supported")
      .then((s) => {
        captureSupported = s;
        if (s) checkFnAction();
      })
      .catch(() => {});
    // Coming back from System Settings: pick up a changed 🌐 action.
    const onFocus = () => captureSupported && checkFnAction();
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
      clearTimeout(savedTimer);
    };
  });

  const keycaps = $derived(capsFor(config.hotkey.binding));
  const keyName = $derived(spoken(config.hotkey.binding));
  const bindingNative = $derived(parseNative(config.hotkey.binding));
  const isFn = $derived(bindingNative !== null && FN_KEYCODES.includes(bindingNative.keycode));
  /** The bound key's keycode when it is a single key (Globe counts as Fn). */
  const boundKeycode = $derived(
    bindingNative === null ? null : isFn ? 63 : bindingNative.keycode,
  );
  /** A bare modifier stays a modifier: using it in a shortcut drops the recording. */
  const boundModifier = $derived(
    captureSupported && bindingNative !== null && MODIFIER_KEYCODES.includes(bindingNative.keycode),
  );
  /** Native non-modifier keys are consumed system-wide — warn about it. */
  const swallowWarning = $derived(
    bindingNative !== null && !MODIFIER_KEYCODES.includes(bindingNative.keycode),
  );
  /** What macOS does on top of una when fn is pressed, if anything. */
  const FN_ACTIONS: Record<number, string> = {
    1: "switches your input source",
    2: "opens the emoji picker",
    3: "starts Apple's dictation",
  };
  const fnClash = $derived(
    captureSupported && isFn && fnAction !== undefined && fnAction !== 0,
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
      flashSaved();
    } catch (e) {
      const msg = String(e);
      // A cancelled capture (Esc or the Cancel button) is not an error.
      if (!msg.toLowerCase().includes("cancel")) captureError = msg;
    } finally {
      capturing = false;
    }
  }

  const capitalized = (t: string) => t.charAt(0).toUpperCase() + t.slice(1);

  function pick(keycode: number, name: string) {
    if (listening) cancelCapture();
    recordingHotkey = false;
    captureError = "";
    config.hotkey.binding = `native:${keycode}:${name}`;
    save();
    flashSaved();
  }

  function flashSaved() {
    justSaved = true;
    clearTimeout(savedTimer);
    savedTimer = setTimeout(() => (justSaved = false), 1800);
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
      flashSaved();
    }
  }

  const MODES = $derived([
    { value: "hold", name: "Hold", desc: `Hold ${keyName} to talk, let go to insert.` },
    { value: "toggle", name: "Toggle", desc: `Press ${keyName} to start, press it again to finish.` },
    {
      value: "hybrid",
      name: "Hybrid",
      desc: `Hold ${keyName} to talk, or tap it to keep listening and tap again to finish.`,
    },
  ] as const);
</script>

<svelte:window onkeydown={onKeydown} />

<h1 class="page-title">Hotkey</h1>
<p class="page-sub">The key you hold, anywhere on your computer, to dictate.</p>

<div class="section">
  <div class="card" class:listening>
    <div class="binding">
      <div class="keys" aria-label="Current hotkey: {keyName}">
        {#if listening}
          <span class="listening-label">
            <span class="dot live"></span>
            {captureSupported ? "Press any key…" : "Press a key combination…"}
            <span class="faint">Esc cancels</span>
          </span>
        {:else}
          {#each keycaps as cap, i (i)}
            <Keycap {cap} />
          {/each}
        {/if}
      </div>
      <div class="actions">
        {#if justSaved && !listening}
          <span class="saved"><Icon name="check" size={13} stroke={2.2} />Saved</span>
        {/if}
        <button class="btn" class:btn-primary={!listening} onclick={toggleRecording}>
          {listening ? "Cancel" : "Change"}
        </button>
      </div>
    </div>
    {#if captureSupported}
      <div class="picker">
        <KeyboardRow selected={boundKeycode} onpick={pick} />
      </div>
    {/if}
  </div>

  <p class="hint">
    {#if captureSupported}
      Click a key, or Change to record any other key or a combination. Left and right
      {IS_MAC ? "⌘ and ⌥" : "Ctrl, Alt and Super"} are separate keys: bind one and the other works
      as usual.
      {#if boundModifier}
        <br />{capitalized(keyName)} still works in shortcuts. Press another key while holding it
        and una drops that recording.
      {/if}
    {:else}
      This session records modifier + key combinations. Single bare keys like right Ctrl need
      macOS or an X11 session. Esc cancels.
    {/if}
  </p>

  {#if fnClash}
    <div class="note warn with-action">
      <span class="dot warn"></span>
      <span>
        {#if fnAction != null && FN_ACTIONS[fnAction]}
          macOS also acts on fn: every press {FN_ACTIONS[fnAction]}.
        {:else}
          macOS may also act on fn, opening emoji or switching input source.
        {/if}
        Set “Press 🌐 key to” to <b>Do Nothing</b> so fn only dictates.
      </span>
      <button class="btn btn-sm" onclick={() => invoke("open_keyboard_settings").catch(() => {})}>
        Keyboard Settings…
      </button>
    </div>
  {/if}
  {#if bindingNative && !captureSupported}
    <div class="note warn">
      <span class="dot warn"></span>
      <span>
        The saved hotkey “{keyName}” is a single key, which this session can't hear. Record a
        combination above, or use a compositor keybind (below).
      </span>
    </div>
  {/if}
  {#if swallowWarning && captureSupported}
    <div class="note warn">
      <span class="dot warn"></span>
      <span>
        While una is running, “{keyName}” is captured system-wide — other apps won't receive it.
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
  .card {
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--panel);
    overflow: hidden;
    transition: border-color 160ms ease;
  }

  .card.listening {
    border-color: var(--muted);
  }

  .binding {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 18px 18px 20px;
  }

  .keys {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 46px;
  }

  .picker {
    padding: 14px 18px 16px 20px;
    border-top: 1px solid var(--line);
    background: var(--sidebar);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .saved {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
    color: var(--muted);
    animation: fade-in 160ms ease;
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }

  .with-action {
    align-items: center;
  }

  .with-action .btn {
    flex: none;
    margin-left: auto;
  }

  .listening-label {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    font-size: 14px;
    color: var(--muted);
  }

  .live {
    background: var(--fg);
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
</style>
