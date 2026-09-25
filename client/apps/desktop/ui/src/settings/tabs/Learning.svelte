<script lang="ts">
  import Toggle from "../../lib/Toggle.svelte";
  import type { Config } from "../../lib/types";

  let { config = $bindable(), save }: { config: Config; save: () => void } = $props();

  const off = $derived(!config.correction.enabled);
</script>

<h1 class="page-title">Learning</h1>
<p class="page-sub">
  una gets better the more you use it. Dictations you leave alone confirm what it heard, which
  teaches the speech model your voice; fixes you make to the text it pasted show how you'd have
  written it, which teaches the cleanup model your style. Both fine-tunes run on your own server.
</p>

<div class="section">
  <div class="group">
    <div class="row">
      <div class="row-copy">
        <div class="row-title">Learn from my edits</div>
        <div class="row-sub">Notice when you fix text una just pasted.</div>
      </div>
      <Toggle
        checked={config.correction.enabled}
        onchange={(v) => {
          config.correction.enabled = v;
          save();
        }}
        label="Learn from my edits"
      />
    </div>
    <div class="row" class:dim={off}>
      <div class="row-copy">
        <div class="row-title">Count untouched dictations as correct</div>
        <div class="row-sub">Where most of the training data comes from.</div>
      </div>
      <Toggle
        checked={config.correction.auto_accept}
        disabled={off}
        onchange={(v) => {
          config.correction.auto_accept = v;
          save();
        }}
        label="Count untouched dictations as correct"
      />
    </div>
    <div class="row" class:dim={off}>
      <div class="row-copy">
        <div class="row-title">Ask when the text can't be read</div>
        <div class="row-sub">Opens a small editor in terminals and canvas apps.</div>
      </div>
      <Toggle
        checked={config.correction.popup}
        disabled={off}
        onchange={(v) => {
          config.correction.popup = v;
          save();
        }}
        label="Ask when the text can't be read"
      />
    </div>
  </div>
</div>

<div class="section">
  <div class="section-title">Timing</div>
  <div class="group">
    <div class="row" class:dim={off}>
      <div class="row-copy">
        <div class="row-title">Watch for edits</div>
        <div class="row-sub">How long after a paste an edit still counts.</div>
      </div>
      <div class="row-control">
        <input
          class="input"
          type="number"
          min="5"
          max="120"
          step="5"
          bind:value={config.correction.watch_seconds}
          disabled={off}
          onchange={save}
          aria-label="Watch for edits, seconds"
        />
        <span class="unit">sec</span>
      </div>
    </div>
    <div class="row" class:dim={off}>
      <div class="row-copy">
        <div class="row-title">Edit is finished after</div>
        <div class="row-sub">A pause in typing this long ends the edit.</div>
      </div>
      <div class="row-control">
        <input
          class="input"
          type="number"
          min="300"
          max="5000"
          step="100"
          bind:value={config.correction.settle_ms}
          disabled={off}
          onchange={save}
          aria-label="Edit settle time, milliseconds"
        />
        <span class="unit">ms</span>
      </div>
    </div>
  </div>
</div>

<p class="hint">
  Nothing extra is recorded: the audio and transcript already go to your server with every
  dictation. This only adds what you changed afterwards, and never touches text una didn't paste.
</p>
