<script lang="ts">
  /**
   * Which model the Fix up button runs, and how hard it thinks. The choices
   * are whatever the agent CLIs signed in on this machine offer, as yagami
   * reports them, so a CLI installed tomorrow shows up here on its own.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "../../lib/Icon.svelte";
  import type { Config, YagamiInventory, YagamiModel } from "../../lib/types";

  let { config = $bindable(), save }: { config: Config; save: () => void } = $props();

  const LABELS: Record<string, string> = {
    claude: "Claude Code",
    codex: "Codex",
    opencode: "OpenCode",
    gemini: "Gemini CLI",
    copilot: "GitHub Copilot",
    cursor: "Cursor",
    qwen: "Qwen Code",
    goose: "Goose",
    kimi: "Kimi",
    amp: "Amp",
    grok: "Grok",
    droid: "Droid",
    cline: "Cline",
    kilo: "Kilo",
    auggie: "Auggie",
  };
  const label = (p: string) => LABELS[p] ?? p;

  const SAMPLE =
    "um so like can you uh make the the hold thing a little longer cuz right now i have to like tap it really fast, know what i mean";

  let inventory = $state<YagamiInventory | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let trying = $state(false);
  let tried = $state<{ ok: boolean; text: string; secs: number } | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      inventory = await invoke<YagamiInventory>("yagami_inventory");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => void load());

  /** The installed CLIs, each with its models; ones with none still listed. */
  const groups = $derived.by(() => {
    if (!inventory) return [];
    const order = [...inventory.providers];
    for (const m of inventory.models) if (!order.includes(m.provider)) order.push(m.provider);
    return order.map((provider) => ({
      provider,
      models: inventory!.models.filter((m) => m.provider === provider),
    }));
  });

  const selected = $derived(inventory?.models.find((m) => m.id === config.fixup.model) ?? null);
  const efforts = $derived(selected?.efforts ?? []);

  function pickModel(id: string) {
    const next: YagamiModel | undefined = inventory?.models.find((m) => m.id === id);
    config.fixup.model = id;
    // Keep the effort when the new model takes it; otherwise the lightest
    // one, since fixing up a transcript is not hard thinking.
    const levels = next?.efforts ?? [];
    if (!levels.includes(config.fixup.effort)) {
      config.fixup.effort = levels.includes("low") ? "low" : (levels[0] ?? "");
    }
    tried = null;
    save();
  }

  function pickEffort(effort: string) {
    config.fixup.effort = effort;
    tried = null;
    save();
  }

  async function tryIt() {
    trying = true;
    tried = null;
    const started = performance.now();
    try {
      // What's picked here, whether or not its save has landed yet.
      const res = await invoke<{ text: string }>("fixup_text", {
        text: SAMPLE,
        model: config.fixup.model,
        effort: config.fixup.effort,
      });
      tried = { ok: true, text: res.text, secs: (performance.now() - started) / 1000 };
    } catch (e) {
      tried = { ok: false, text: String(e), secs: 0 };
    } finally {
      trying = false;
    }
  }
</script>

<h1 class="page-title">Fix up</h1>
<p class="page-sub">
  The Fix up button in the fix and review windows rewrites what you said as clean text. It runs
  on the AI tools already signed in on this computer, through yagami, so it uses your own
  subscriptions and needs no API key.
</p>

{#if loading && !inventory}
  <div class="section">
    <div class="group">
      <div class="row"><span class="faint">Looking for AI tools on this machine…</span></div>
    </div>
  </div>
{:else if error && !inventory}
  <div class="note error"><span class="dot down"></span><span>{error}</span></div>
  <div class="actions">
    <button class="btn btn-sm" onclick={() => void load()}>Try again</button>
  </div>
{:else if inventory}
  <div class="section">
    <div class="group">
      <div class="row">
        <div class="row-copy">
          <div class="row-title">Model</div>
          <div class="row-sub">{selected ? label(selected.provider) : "Not installed here any more"}</div>
        </div>
        <select
          class="input model"
          value={config.fixup.model}
          onchange={(e) => pickModel(e.currentTarget.value)}
        >
          {#if !selected}
            <option value={config.fixup.model}>{config.fixup.model} (not found)</option>
          {/if}
          {#each groups.filter((g) => g.models.length > 0) as g (g.provider)}
            <optgroup label={label(g.provider)}>
              {#each g.models as m (m.id)}
                <option value={m.id}>{m.name}</option>
              {/each}
            </optgroup>
          {/each}
        </select>
      </div>
      <div class="row" class:dim={efforts.length === 0}>
        <div class="row-copy">
          <div class="row-title">Effort</div>
          <div class="row-sub">
            {#if efforts.length === 0}
              This model has no effort setting.
            {:else}
              How long it thinks. Low is plenty here.
            {/if}
          </div>
        </div>
        {#if efforts.length > 0}
          <div class="seg" role="radiogroup" aria-label="Effort">
            <button
              role="radio"
              aria-checked={config.fixup.effort === ""}
              class:on={config.fixup.effort === ""}
              onclick={() => pickEffort("")}
              title={selected?.default_effort ? `The model's default (${selected.default_effort})` : "The model's default"}
            >
              Auto
            </button>
            {#each efforts as e (e)}
              <button role="radio" aria-checked={config.fixup.effort === e} class:on={config.fixup.effort === e} onclick={() => pickEffort(e)}>
                {e}
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <div class="row try">
        <div class="row-copy">
          <div class="row-title">Try it</div>
          <div class="row-sub sample">“{SAMPLE}”</div>
          {#if tried}
            <div class="result" class:bad={!tried.ok}>
              {#if tried.ok}
                <span class="dot ok"></span>
                <span>{tried.text} <span class="faint">· {tried.secs.toFixed(1)}s</span></span>
              {:else}
                <span class="dot down"></span><span>{tried.text}</span>
              {/if}
            </div>
          {/if}
        </div>
        <button class="btn btn-sm" onclick={() => void tryIt()} disabled={trying}>
          {#if trying}<span class="spinner"></span> Fixing up…{:else}<Icon name="wand" size={13} /> Fix up{/if}
        </button>
      </div>
    </div>
  </div>

  <div class="section">
    <div class="section-title">On this computer</div>
    <div class="group">
      {#each groups as g (g.provider)}
        <div class="row cli">
          <div class="row-copy">
            <div class="row-title">{label(g.provider)}</div>
          </div>
          {#if g.models.length > 0}
            <span class="count"><span class="dot ok"></span>{g.models.length} model{g.models.length === 1 ? "" : "s"}</span>
          {:else}
            <span class="count" title="Installed, but it didn't say which models it runs — `yagami doctor` tells more">
              <span class="dot warn"></span>no models reported
            </span>
          {/if}
        </div>
      {/each}
    </div>
    <p class="hint">
      Found by yagami{inventory.version ? ` ${inventory.version}` : ""}, which una starts when it
      isn't running. Install another CLI and sign in, then
      <button class="link" onclick={() => void load()} disabled={loading}>
        {loading ? "looking…" : "look again"}</button
      >.
    </p>
  </div>
{/if}

<style>
  .model {
    width: 220px;
  }

  .seg {
    display: flex;
    flex: none;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--panel);
  }
  .seg button {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 12px;
    font-weight: 500;
    height: 28px;
    padding: 0 9px;
    transition:
      background-color 140ms ease,
      color 140ms ease;
  }
  .seg button + button {
    border-left: 1px solid var(--line);
  }
  .seg button:hover {
    color: var(--fg);
  }
  .seg button.on {
    background: var(--inverse);
    color: var(--on-inverse);
  }

  .try {
    align-items: flex-start;
  }
  .try .btn {
    margin-top: 1px;
    flex: none;
  }
  .sample {
    font-style: italic;
  }
  .result {
    display: flex;
    gap: 8px;
    align-items: baseline;
    margin-top: 8px;
    font-size: 12.5px;
    line-height: 1.5;
    user-select: text;
    -webkit-user-select: text;
  }
  .result.bad {
    color: var(--muted);
  }

  .cli {
    min-height: 40px;
  }
  .count {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
  }

  .actions {
    margin-top: 10px;
  }

  .link {
    border: 0;
    background: none;
    padding: 0;
    font: inherit;
    color: var(--fg);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .spinner {
    width: 11px;
    height: 11px;
    border-radius: 999px;
    border: 1.5px solid var(--line-strong);
    border-top-color: var(--fg);
    animation: spin 700ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
