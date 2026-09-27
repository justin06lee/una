<script lang="ts">
  import Icon from "../../lib/Icon.svelte";
  import Toggle from "../../lib/Toggle.svelte";
  import type { Config } from "../../lib/types";

  let { config = $bindable(), save }: { config: Config; save: () => void } = $props();

  const HUD_MODES = [
    {
      value: "pill",
      name: "Always visible",
      desc: "A small pill rests at the bottom of the screen and grows while you dictate.",
    },
    {
      value: "flash",
      name: "Only while dictating",
      desc: "The pill appears when you start talking and hides when the text lands.",
    },
  ] as const;
</script>

<h1 class="page-title">General</h1>
<p class="page-sub">
  una lives in the menu bar. Hold your hotkey anywhere, speak, and let go — the text is typed
  into whatever app has focus.
</p>

<div class="section">
  <div class="group">
    <div class="row">
      <div class="row-copy">
        <div class="row-title">Launch at login</div>
        <div class="row-sub">Start una in the background when you log in.</div>
      </div>
      <Toggle
        checked={config.general.launch_at_login}
        onchange={(v) => {
          config.general.launch_at_login = v;
          save();
        }}
        label="Launch at login"
      />
    </div>
    <div class="row">
      <div class="row-copy">
        <div class="row-title">Sounds</div>
        <div class="row-sub">A soft click when recording starts and stops.</div>
      </div>
      <Toggle
        checked={config.ui.sounds}
        onchange={(v) => {
          config.ui.sounds = v;
          save();
        }}
        label="Sounds"
      />
    </div>
  </div>
</div>

<div class="section">
  <div class="section-title">Status pill</div>
  <div class="group" role="radiogroup" aria-label="Status pill">
    {#each HUD_MODES as m (m.value)}
      <button
        class="choice"
        class:selected={config.ui.hud_mode === m.value}
        role="radio"
        aria-checked={config.ui.hud_mode === m.value}
        onclick={() => {
          config.ui.hud_mode = m.value;
          save();
        }}
      >
        <span class="preview" class:empty={m.value === "flash"}>
          <span class="pill">
            <svg viewBox="0 0 604.8 360.0" aria-hidden="true">
              <rect x="259.2" y="156.6" width="86.4" height="136.8" rx="43.2" />
              <rect x="172.8" y="158.4" width="59.4" height="126" rx="29.7" />
              <rect x="86.4" y="79.2" width="59.4" height="243" rx="29.7" />
              <rect x="0" y="0" width="59.4" height="360" rx="29.7" />
              <rect x="372.6" y="158.4" width="59.4" height="126" rx="29.7" />
              <rect x="459" y="79.2" width="59.4" height="243" rx="29.7" />
              <rect x="545.4" y="0" width="59.4" height="360" rx="29.7" />
            </svg>
          </span>
        </span>
        <span class="row-copy">
          <span class="row-title">{m.name}</span>
          <span class="row-sub" style="display: block">{m.desc}</span>
        </span>
        <span class="check"><Icon name="check" size={15} stroke={2} /></span>
      </button>
    {/each}
  </div>
</div>

<style>
  /* A thumbnail of the screen's bottom edge: the resting pill, or nothing. */
  .preview {
    width: 46px;
    height: 30px;
    flex: none;
    border-radius: 7px;
    background: var(--subtle);
    border: 1px solid var(--line);
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding-bottom: 5px;
  }

  .pill {
    width: 20px;
    height: 6px;
    border-radius: 3px;
    background: #141414;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* The resting pill's bow, at thumbnail size. */
  .pill svg {
    width: 8px;
    height: 5px;
    fill: rgba(255, 255, 255, 0.85);
  }

  .empty .pill {
    background: transparent;
    border: 1px dashed var(--faint);
  }

  .empty .pill svg {
    display: none;
  }
</style>
