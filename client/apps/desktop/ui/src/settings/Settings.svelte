<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type {
    CapturedHotkey,
    Config,
    DiscoveredServer,
    LevelFrame,
    PermissionsStatus,
    TestRecordResult,
  } from "../lib/types";

  const TABS = ["General", "Hotkey", "Server", "Audio", "Insertion", "About"] as const;
  type Tab = (typeof TABS)[number];

  let tab = $state<Tab>("General");
  let config = $state<Config | null>(null);
  let saveError = $state("");

  // Server tab
  let health = $state<"unknown" | "ok" | "down">("unknown");
  let healthDetail = $state("");
  let discovering = $state(false);
  let discovered = $state<DiscoveredServer[]>([]);

  // Hotkey tab
  let recordingHotkey = $state(false); // legacy JS recorder (non-macOS)
  let captureSupported = $state(false); // native event-tap capture available
  let capturing = $state(false);
  let captureError = $state("");

  // Audio tab
  let devices = $state<string[]>([]);
  let testing = $state(false);
  let testResult = $state<TestRecordResult | null>(null);
  let liveLevel = $state(0);

  // Insertion tab
  let perms = $state<PermissionsStatus | null>(null);

  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  async function loadConfig() {
    config = await invoke<Config>("get_config");
  }

  function scheduleSave() {
    if (!config) return;
    clearTimeout(saveTimer);
    const snapshot = JSON.parse(JSON.stringify(config));
    saveTimer = setTimeout(async () => {
      try {
        await invoke("set_config", { config: snapshot });
        saveError = "";
      } catch (e) {
        saveError = String(e);
      }
    }, 350);
  }

  async function checkHealth() {
    if (!config) return;
    try {
      const res = await invoke<Record<string, unknown>>("health_check", {
        url: config.server.url || null,
      });
      health = "ok";
      const model = res["asr_model_loaded"];
      healthDetail =
        model === false ? "reachable, ASR model not loaded yet" : "healthy";
    } catch (e) {
      health = "down";
      healthDetail = String(e);
    }
  }

  async function discover() {
    discovering = true;
    try {
      discovered = await invoke<DiscoveredServer[]>("discover_servers");
    } catch {
      discovered = [];
    } finally {
      discovering = false;
    }
  }

  function adoptServer(s: DiscoveredServer) {
    if (!config) return;
    config.server.url = s.url;
    scheduleSave();
    checkHealth();
  }

  // ---- Native capture (macOS event tap) ------------------------------

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

  /** Keycap chips for the current binding ("⌃ ⌥ Space" or a single "Fn"). */
  const chips = $derived.by(() => {
    if (!config) return [] as string[];
    const native = parseNative(config.hotkey.binding);
    if (native) return [native.name];
    return config.hotkey.binding.split("+").map((t) => MOD_GLYPHS[t] ?? t);
  });

  const bindingNative = $derived(config ? parseNative(config.hotkey.binding) : null);
  /** Native non-modifier keys are consumed system-wide — warn about it. */
  const swallowWarning = $derived(
    bindingNative !== null && !MODIFIER_KEYCODES.includes(bindingNative.keycode),
  );

  async function startCapture() {
    if (capturing || !config) return;
    capturing = true;
    captureError = "";
    try {
      const cap = await invoke<CapturedHotkey>("capture_hotkey");
      config.hotkey.binding = cap.binding;
      scheduleSave();
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

  // ---- Legacy JS combo recorder (used where native capture is missing) --

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
    else if (
      [
        "Space", "Enter", "Tab", "Backspace", "Escape", "Home", "End",
        "PageUp", "PageDown", "Insert", "Delete",
        "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
      ].includes(code) ||
      /^F\d{1,2}$/.test(code)
    ) {
      key = code;
    } else if (["Backquote", "Minus", "Equal", "Slash", "Backslash", "Comma", "Period", "Semicolon", "Quote", "BracketLeft", "BracketRight"].includes(code)) {
      key = code;
    }
    if (!key) return null; // modifier-only or unsupported key
    parts.push(key);
    return parts.join("+");
  }

  function onHotkeyKeydown(e: KeyboardEvent) {
    if (!recordingHotkey || !config) return;
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
      scheduleSave();
    }
  }

  async function testMic() {
    testing = true;
    testResult = null;
    try {
      testResult = await invoke<TestRecordResult>("test_record");
    } catch (e) {
      testResult = { ok: false, max_rms: 0, max_peak: 0 };
      saveError = String(e);
    } finally {
      testing = false;
    }
  }

  async function refreshPerms() {
    try {
      perms = await invoke<PermissionsStatus>("permissions_status");
    } catch {
      perms = null;
    }
  }

  function promptAccessibility() {
    invoke("prompt_accessibility").catch(() => {});
    setTimeout(refreshPerms, 1500);
  }

  onMount(() => {
    loadConfig().then(() => checkHealth());
    invoke<string[]>("audio_devices")
      .then((d) => (devices = d))
      .catch(() => {});
    invoke<boolean>("hotkey_capture_supported")
      .then((s) => (captureSupported = s))
      .catch(() => {});
    refreshPerms();

    const healthInterval = setInterval(checkHealth, 5000);
    const permsInterval = setInterval(refreshPerms, 10000);
    const unlistenLevel = listen<LevelFrame>("audio-level", (e) => {
      liveLevel = Math.min(1, e.payload.rms * 3.2);
    });
    return () => {
      clearInterval(healthInterval);
      clearInterval(permsInterval);
      unlistenLevel.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={onHotkeyKeydown} />

<div class="layout">
  <nav class="sidebar">
    <div class="brand">
      <svg class="mark" viewBox="0 0 32 32" aria-hidden="true">
        <rect width="32" height="32" rx="9" fill="var(--accent)" />
        <rect x="13.25" y="7" width="5.5" height="12" rx="2.75" fill="var(--on-accent)" />
        <path
          d="M9.5 15.5a6.5 6.5 0 0 0 13 0"
          stroke="var(--on-accent)"
          stroke-width="2.4"
          fill="none"
          stroke-linecap="round"
        />
        <path d="M16 22v3.5" stroke="var(--on-accent)" stroke-width="2.4" stroke-linecap="round" />
      </svg>
      <div>
        <div class="brand-name">una</div>
        <div class="brand-sub">your voice, your model</div>
      </div>
    </div>
    {#each TABS as t}
      <button class="tab" class:active={tab === t} onclick={() => (tab = t)}>
        {t}
      </button>
    {/each}
  </nav>

  <main class="content">
    {#if !config}
      <p class="muted">Loading…</p>
    {:else if tab === "General"}
      <h2>General</h2>
      <label class="row">
        <span>Launch at login</span>
        <input
          type="checkbox"
          bind:checked={config.general.launch_at_login}
          onchange={scheduleSave}
        />
      </label>
      <label class="row">
        <span>Sounds</span>
        <input type="checkbox" bind:checked={config.ui.sounds} onchange={scheduleSave} />
      </label>
      <fieldset class="mode">
        <legend>Status pill</legend>
        {#each [["pill", "Always visible", "A tiny pill rests at the bottom of the screen and grows while you dictate."], ["flash", "Only while dictating", "The pill appears when a dictation starts and hides when it ends."]] as [value, name, desc]}
          <label class="radio">
            <input
              type="radio"
              name="hud_mode"
              {value}
              bind:group={config.ui.hud_mode}
              onchange={scheduleSave}
            />
            <span><strong>{name}</strong> — {desc}</span>
          </label>
        {/each}
      </fieldset>
      <p class="hint">
        una lives in the menu bar. Hold the hotkey anywhere, speak, release —
        the transcribed text is typed into the focused app.
      </p>
    {:else if tab === "Hotkey"}
      <h2>Hotkey</h2>

      <div class="binding-panel">
        <div class="chips" aria-label="Current hotkey">
          {#each chips as chip}
            <span class="keycap">{chip}</span>
          {/each}
        </div>

        {#if captureSupported}
          <button
            class="capture-btn"
            class:capturing
            onclick={() => (capturing ? cancelCapture() : startCapture())}
          >
            {#if capturing}
              <span class="pulse-dot"></span> Press any key… <em>(Esc cancels)</em>
            {:else}
              Click to record a new hotkey
            {/if}
          </button>
          <p class="hint">
            Press any single key — Fn, Right ⌘, F5, even a plain letter — and it
            becomes the hotkey, including as a hold-to-talk key. Hold modifiers
            and press a key to record a combo instead.
          </p>
        {:else}
          <button
            class="capture-btn"
            class:capturing={recordingHotkey}
            onclick={() => (recordingHotkey = !recordingHotkey)}
          >
            {#if recordingHotkey}
              <span class="pulse-dot"></span> Press a key combo… <em>(Esc cancels)</em>
            {:else}
              Click to record a new hotkey
            {/if}
          </button>
          <p class="hint">
            This platform records modifier+key combos. Single bare keys (Fn,
            Right ⌘, …) are a macOS feature.
          </p>
          {#if bindingNative}
            <p class="warn-note">
              The saved binding “{bindingNative.name}” is a macOS native key and
              is disabled on this platform — record a combo above, or use a
              compositor keybind (below).
            </p>
          {/if}
        {/if}

        {#if swallowWarning && captureSupported}
          <p class="warn-note">
            While una is running, “{bindingNative?.name}” is captured
            system-wide — other apps won’t receive this key.
          </p>
        {/if}
        {#if captureError}
          <p class="error">{captureError}</p>
        {/if}
      </div>

      <fieldset class="mode">
        <legend>Mode</legend>
        {#each [["hold", "Hold", "Hold to talk; release to insert."], ["toggle", "Toggle", "Press to start, press again to finish."], ["hybrid", "Hybrid", "Hold to talk — or tap to latch, tap again to finish."]] as [value, name, desc]}
          <label class="radio">
            <input
              type="radio"
              name="mode"
              {value}
              bind:group={config.hotkey.mode}
              onchange={scheduleSave}
            />
            <span><strong>{name}</strong> — {desc}</span>
          </label>
        {/each}
      </fieldset>

      {#if !captureSupported}
        <h3>Wayland / compositor keybinds</h3>
        <p class="hint">
          On Wayland, global hotkeys can be restricted by the compositor. The
          most reliable setup is a compositor keybind running the
          <code>una</code> CLI: e.g. Hyprland
          <code>bind = , F12, exec, una toggle</code> (or
          <code>bind</code>/<code>bindr</code> with <code>una start</code> and
          <code>una stop</code> for hold-to-talk), Sway
          <code>bindsym F12 exec una toggle</code>, or a GNOME/KDE custom
          shortcut running <code>una toggle</code>.
        </p>
      {/if}
    {:else if tab === "Server"}
      <h2>Server</h2>
      <div class="row">
        <span>Server URL</span>
        <span class="url-group">
          <span
            class="dot"
            class:ok={health === "ok"}
            class:down={health === "down"}
            title={healthDetail}
          ></span>
          <input
            type="text"
            placeholder="http://192.168.1.20:8765 (empty = autodiscover)"
            bind:value={config.server.url}
            oninput={scheduleSave}
            onchange={checkHealth}
          />
        </span>
      </div>
      <p class="hint">
        {health === "ok"
          ? `Server ${healthDetail}.`
          : health === "down"
            ? `Cannot reach server: ${healthDetail}`
            : "Checking…"}
      </p>
      <label class="row">
        <span>Discover servers on this network (mDNS)</span>
        <input
          type="checkbox"
          bind:checked={config.server.autodiscover}
          onchange={scheduleSave}
        />
      </label>
      <div class="row">
        <span></span>
        <button class="btn" onclick={discover} disabled={discovering}>
          {discovering ? "Scanning…" : "Discover"}
        </button>
      </div>
      {#if discovered.length > 0}
        <ul class="servers">
          {#each discovered as s}
            <li>
              <button class="server" onclick={() => adoptServer(s)}>
                <strong>{s.name}</strong>
                <span class="muted">{s.url}{s.version ? ` · v${s.version}` : ""}</span>
              </button>
            </li>
          {/each}
        </ul>
      {:else if !discovering}
        <p class="hint">No servers found yet — click Discover to scan for 2 seconds.</p>
      {/if}
    {:else if tab === "Audio"}
      <h2>Audio</h2>
      <div class="row">
        <span>Input device</span>
        <select bind:value={config.audio.input_device} onchange={scheduleSave}>
          <option value="auto">Automatic</option>
          {#each devices as d}
            <option value={d}>{d}</option>
          {/each}
        </select>
      </div>
      <label class="row">
        <span>Prefer built-in microphone</span>
        <input
          type="checkbox"
          bind:checked={config.audio.prefer_builtin}
          onchange={scheduleSave}
        />
      </label>
      <div class="row">
        <span>Microphone test</span>
        <button class="btn" onclick={testMic} disabled={testing}>
          {testing ? "Recording 2s…" : "Test"}
        </button>
      </div>
      <div class="meter">
        <div class="meter-fill" style="width: {Math.round(liveLevel * 100)}%"></div>
      </div>
      {#if testResult}
        <p class="hint">
          {testResult.ok
            ? `Microphone OK (peak ${(testResult.max_peak * 100).toFixed(0)}%).`
            : "No signal detected — check the selected device and mic permission."}
        </p>
      {/if}
    {:else if tab === "Insertion"}
      <h2>Insertion</h2>
      <label class="row">
        <span>Restore clipboard after paste</span>
        <input
          type="checkbox"
          bind:checked={config.insert.restore_clipboard}
          onchange={scheduleSave}
        />
      </label>
      <div class="row">
        <span>Restore delay (ms)</span>
        <input
          class="num"
          type="number"
          min="50"
          max="5000"
          step="50"
          bind:value={config.insert.restore_delay_ms}
          onchange={scheduleSave}
        />
      </div>

      <h3>Status</h3>
      {#if perms}
        <ul class="perm-list">
          <li>
            <span class="dot" class:ok={perms.mic === "granted"} class:down={perms.mic === "denied"}></span>
            Microphone: {perms.mic}
            {#if perms.mic !== "granted"}
              <span class="muted">
                — grant in System Settings › Privacy &amp; Security › Microphone
              </span>
            {/if}
          </li>
          <li>
            <span
              class="dot"
              class:ok={perms.accessibility === "granted"}
              class:down={perms.accessibility === "denied"}
            ></span>
            Accessibility: {perms.accessibility}
            {#if perms.accessibility !== "granted"}
              <button class="btn small" onclick={promptAccessibility}>Grant…</button>
            {/if}
          </li>
          <li>
            <span class="dot" class:ok={perms.probe.can_paste} class:down={!perms.probe.can_paste}></span>
            Paste backend: {perms.probe.backend}
            <span class="muted">— {perms.probe.detail}</span>
          </li>
          {#if perms.secure_input}
            <li>
              <span class="dot warn"></span>
              Secure input is currently active (a password field has focus
              somewhere); pasting is temporarily disabled.
            </li>
          {/if}
        </ul>
      {:else}
        <p class="muted">Checking permissions…</p>
      {/if}
    {:else if tab === "About"}
      <h2>About</h2>
      <p><strong>Una</strong> — self-hosted dictation.</p>
      <p class="hint">
        Hold a hotkey, speak, release. Audio goes to your own una server on the
        LAN; the cleaned transcript is typed into whatever app has focus.
        Nothing leaves your network.
      </p>
      <p class="hint">una-desktop 0.1.0</p>
    {/if}

    {#if saveError}
      <p class="error">{saveError}</p>
    {/if}
  </main>
</div>

<style>
  /* ---------------------------------------------------------------- brand */
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 4px 8px 14px;
  }

  .mark {
    width: 24px;
    height: 24px;
    flex: none;
  }

  .brand-name {
    font-size: 14px;
    font-weight: 650;
    letter-spacing: -0.01em;
    line-height: 1.1;
  }

  .brand-sub {
    font-size: 10.5px;
    color: var(--faint);
    margin-top: 2px;
  }

  /* ---------------------------------------------------------------- shell */
  .layout {
    display: flex;
    height: 100%;
  }

  .sidebar {
    width: 168px;
    flex: none;
    padding: 14px 10px;
    border-right: 1px solid var(--border);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .tab {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--muted);
    text-align: left;
    padding: 0 11px;
    height: 32px;
    border-radius: var(--radius);
    font-family: inherit;
    font-size: 13px;
    font-weight: 550;
    cursor: pointer;
    transition:
      background-color 160ms ease,
      color 160ms ease;
  }

  .tab:hover {
    background: var(--raised);
    color: var(--text);
  }

  .tab.active {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .content {
    flex: 1;
    padding: 26px 30px 40px;
    overflow-y: auto;
  }

  h2 {
    font-size: 18px;
    font-weight: 650;
    letter-spacing: -0.01em;
    margin-bottom: 18px;
  }

  h3 {
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--faint);
    margin: 22px 0 10px;
  }

  /* ----------------------------------------------------------- rows/forms */
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 11px 0;
    border-bottom: 1px solid var(--border);
  }

  .row:last-of-type {
    border-bottom: 0;
  }

  .row > span:first-child {
    color: var(--text);
    font-weight: 500;
  }

  input[type="text"],
  select,
  .num {
    background: var(--surface);
    border: 1px solid var(--edge);
    color: var(--text);
    border-radius: var(--radius-sm);
    padding: 7px 10px;
    font-family: inherit;
    font-size: 13px;
    min-width: 260px;
    transition:
      border-color 160ms ease,
      box-shadow 160ms ease;
  }

  input[type="text"]:focus,
  select:focus,
  .num:focus {
    outline: none;
    border-color: color-mix(in oklab, var(--accent) 55%, var(--edge));
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 12%, transparent);
  }

  .num {
    min-width: 90px;
    width: 90px;
  }

  .url-group {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    justify-content: flex-end;
  }

  .url-group input {
    flex: 1;
    max-width: 340px;
  }

  /* -------------------------------------------------------------- buttons */
  .btn {
    appearance: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--edge);
    border-radius: var(--radius-sm);
    padding: 0 14px;
    height: 32px;
    font-family: inherit;
    font-size: 13px;
    font-weight: 550;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color 160ms ease,
      border-color 160ms ease,
      opacity 160ms ease;
  }

  .btn:hover:not(:disabled) {
    background: var(--raised);
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .btn.small {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
    margin-left: 8px;
  }

  /* ------------------------------------------------------- hotkey capture */
  .binding-panel {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    box-shadow: var(--shadow-card);
    margin-bottom: 16px;
  }

  .chips {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
  }

  .keycap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 34px;
    height: 34px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    border-bottom-width: 2.5px;
    background: var(--raised);
    font-family: inherit;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .capture-btn {
    appearance: none;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: var(--accent);
    color: var(--on-accent);
    border: 0;
    border-radius: var(--radius);
    padding: 11px 20px;
    font-family: inherit;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    transition: background-color 160ms ease;
  }

  .capture-btn:hover {
    background: var(--accent-hover);
  }

  .capture-btn em {
    font-style: normal;
    font-weight: 400;
    opacity: 0.75;
  }

  .capture-btn.capturing {
    animation: capture-pulse 1.2s ease-in-out infinite;
  }

  .pulse-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
    animation: dot-pulse 1.2s ease-in-out infinite;
  }

  @keyframes capture-pulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 color-mix(in oklab, var(--accent) 55%, transparent);
    }
    50% {
      box-shadow: 0 0 0 7px color-mix(in oklab, var(--accent) 0%, transparent);
    }
  }

  @keyframes dot-pulse {
    0%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    50% {
      opacity: 0.45;
      transform: scale(0.72);
    }
  }

  /* ----------------------------------------------------------------- misc */
  .warn-note {
    color: var(--warn);
    background: var(--warn-soft);
    border-radius: var(--radius-sm);
    padding: 8px 11px;
    font-size: 12px;
    line-height: 1.55;
    margin: 0;
  }

  code {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
    font-size: 11.5px;
    background: var(--raised);
    border-radius: 4px;
    padding: 1.5px 5px;
  }

  .mode {
    border: 0;
    margin-top: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .mode legend {
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--faint);
    margin-bottom: 8px;
  }

  .radio {
    display: flex;
    gap: 9px;
    align-items: baseline;
    line-height: 1.5;
    cursor: pointer;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint);
    display: inline-block;
    flex: none;
  }

  .dot.ok {
    background: var(--accent);
  }

  .dot.down {
    background: var(--danger);
  }

  .dot.warn {
    background: var(--warn);
  }

  .servers {
    list-style: none;
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .server {
    width: 100%;
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 9px 12px;
    color: var(--text);
    font-family: inherit;
    font-size: 13px;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 2px;
    transition:
      border-color 160ms ease,
      background-color 160ms ease;
  }

  .server:hover {
    border-color: color-mix(in oklab, var(--accent) 45%, var(--border));
    background: var(--accent-soft);
  }

  .meter {
    height: 8px;
    border-radius: 999px;
    background: var(--raised);
    overflow: hidden;
    margin: 8px 0;
  }

  .meter-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
    transition: width 60ms linear;
  }

  .perm-list {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .perm-list li {
    display: flex;
    align-items: baseline;
    gap: 9px;
  }

  .hint {
    color: var(--muted);
    font-size: 12px;
    margin: 10px 0;
    line-height: 1.6;
  }

  .muted {
    color: var(--muted);
  }

  .error {
    color: var(--danger);
    background: var(--danger-soft);
    border-radius: var(--radius-sm);
    padding: 8px 11px;
    margin-top: 14px;
  }
</style>
