<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Toggle from "../../lib/Toggle.svelte";
  import type { Config, PermissionsStatus } from "../../lib/types";

  let { config = $bindable(), save }: { config: Config; save: () => void } = $props();

  let perms = $state<PermissionsStatus | null>(null);

  async function refresh() {
    try {
      perms = await invoke<PermissionsStatus>("permissions_status");
    } catch {
      perms = null;
    }
  }

  onMount(() => {
    void refresh();
    const t = setInterval(refresh, 10000);
    return () => clearInterval(t);
  });

  function grantAccessibility() {
    invoke("prompt_accessibility").catch(() => {});
    setTimeout(refresh, 1500);
  }
</script>

<h1 class="page-title">Pasting</h1>
<p class="page-sub">
  una puts the text on the clipboard and pastes it into the focused app, then puts your clipboard
  back the way it was.
</p>

<div class="section">
  <div class="group">
    <div class="row">
      <div class="row-copy">
        <div class="row-title">Restore the clipboard</div>
        <div class="row-sub">Put back whatever you had copied before the dictation.</div>
      </div>
      <Toggle
        checked={config.insert.restore_clipboard}
        onchange={(v) => {
          config.insert.restore_clipboard = v;
          save();
        }}
        label="Restore the clipboard"
      />
    </div>
    <div class="row" class:dim={!config.insert.restore_clipboard}>
      <div class="row-copy">
        <div class="row-title">Restore after</div>
        <div class="row-sub">Some apps read the clipboard late; raise this if pastes come out wrong.</div>
      </div>
      <div class="row-control">
        <input
          class="input"
          type="number"
          min="50"
          max="5000"
          step="50"
          bind:value={config.insert.restore_delay_ms}
          disabled={!config.insert.restore_clipboard}
          onchange={save}
          aria-label="Restore delay in milliseconds"
        />
        <span class="unit">ms</span>
      </div>
    </div>
  </div>
</div>

<div class="section">
  <div class="section-title">Permissions</div>
  {#if perms}
    <div class="group">
      <div class="row">
        <div class="row-copy">
          <div class="row-title">
            <span class="dot" class:ok={perms.mic === "granted"} class:down={perms.mic === "denied"}></span>
            &nbsp;Microphone
          </div>
          <div class="row-sub">
            {perms.mic === "granted"
              ? "Needed to hear you."
              : "Grant it in System Settings › Privacy & Security › Microphone."}
          </div>
        </div>
        <span class="faint">{perms.mic === "granted" ? "Granted" : "Not granted"}</span>
      </div>
      <div class="row">
        <div class="row-copy">
          <div class="row-title">
            <span
              class="dot"
              class:ok={perms.accessibility === "granted"}
              class:down={perms.accessibility === "denied"}
            ></span>
            &nbsp;Accessibility
          </div>
          <div class="row-sub">Needed to paste into other apps.</div>
        </div>
        {#if perms.accessibility === "granted"}
          <span class="faint">Granted</span>
        {:else}
          <button class="btn btn-sm btn-primary" onclick={grantAccessibility}>Grant…</button>
        {/if}
      </div>
      <div class="row">
        <div class="row-copy">
          <div class="row-title">
            <span class="dot" class:ok={perms.probe.can_paste} class:down={!perms.probe.can_paste}></span>
            &nbsp;Paste method
          </div>
          <div class="row-sub">{perms.probe.detail}</div>
        </div>
        <code>{perms.probe.backend}</code>
      </div>
    </div>
    {#if perms.secure_input}
      <div class="note warn">
        <span class="dot warn"></span>
        <span>
          Secure input is on — a password field has focus somewhere — so pasting is paused until it
          closes.
        </span>
      </div>
    {/if}
  {:else}
    <p class="hint">Checking permissions…</p>
  {/if}
</div>
