<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "../../lib/Icon.svelte";
  import Toggle from "../../lib/Toggle.svelte";
  import type { Config, DiscoveredServer, EndpointStatus } from "../../lib/types";

  let { config = $bindable(), save }: { config: Config; save: () => void } = $props();

  let health = $state<"unknown" | "ok" | "down">("unknown");
  let healthDetail = $state("");
  let discovering = $state(false);
  let discovered = $state<DiscoveredServer[]>([]);
  let scanned = $state(false);
  let probes = $state<EndpointStatus[]>([]);
  let probing = $state(false);

  async function checkHealth() {
    try {
      const res = await invoke<Record<string, unknown>>("health_check", { url: null });
      health = "ok";
      healthDetail =
        res["asr_model_loaded"] === false ? "Connected — the speech model is still loading" : "Connected";
    } catch (e) {
      health = "down";
      healthDetail = String(e);
    }
  }

  onMount(() => {
    void checkHealth();
    const t = setInterval(checkHealth, 5000);
    return () => clearInterval(t);
  });

  async function discover() {
    discovering = true;
    try {
      discovered = await invoke<DiscoveredServer[]>("discover_servers");
    } catch {
      discovered = [];
    } finally {
      discovering = false;
      scanned = true;
    }
  }

  function adopt(s: DiscoveredServer) {
    if (config.server.urls.includes(s.url)) return;
    config.server.urls = [...config.server.urls, s.url];
    save();
    void checkHealth();
  }

  function setUrl(i: number, value: string) {
    config.server.urls = config.server.urls.map((u, k) => (k === i ? value : u));
    save();
  }

  function addUrl() {
    config.server.urls = [...config.server.urls, ""];
  }

  function removeUrl(i: number) {
    config.server.urls = config.server.urls.filter((_, k) => k !== i);
    probes = [];
    save();
    void checkHealth();
  }

  /** Test every address from wherever this machine currently is. */
  async function probeAll() {
    probing = true;
    try {
      probes = await invoke<EndpointStatus[]>("probe_endpoints");
    } catch {
      probes = [];
    } finally {
      probing = false;
    }
    void checkHealth();
  }

  function probeFor(url: string): EndpointStatus | undefined {
    return probes.find((p) => p.url === url.trim().replace(/\/+$/, ""));
  }
</script>

<h1 class="page-title">Server</h1>
<p class="page-sub">
  una tries every address below and uses whichever answers first, so one setup works at home and
  away. Put your fastest home-network address first and a VPN address (Tailscale, WireGuard)
  after it — that one keeps working from anywhere.
</p>

<div class="status" class:ok={health === "ok"} class:down={health === "down"}>
  <span class="dot" class:ok={health === "ok"} class:down={health === "down"}></span>
  <span class="status-text">
    {health === "ok" ? healthDetail : health === "down" ? "Can't reach the server" : "Checking…"}
  </span>
  {#if health === "down"}<span class="status-detail">{healthDetail}</span>{/if}
</div>

<div class="section">
  <div class="section-title">Addresses</div>
  <div class="group">
    {#each config.server.urls as url, i (i)}
      {@const probe = probeFor(url)}
      <div class="endpoint">
        <span
          class="dot"
          class:ok={probe?.reachable === true}
          class:down={probe?.reachable === false}
          title={probe
            ? probe.reachable
              ? `Answered in ${probe.ms} ms`
              : "No answer from here"
            : "Not tested yet"}
        ></span>
        <input
          class="input url"
          type="text"
          spellcheck="false"
          placeholder="http://192.168.1.20:8100"
          value={url}
          oninput={(e) => setUrl(i, e.currentTarget.value)}
          onchange={checkHealth}
          aria-label="Server address {i + 1}"
        />
        <span class="ms">{probe?.reachable ? `${probe.ms} ms` : ""}</span>
        <button
          class="btn btn-ghost btn-sm icon"
          onclick={() => removeUrl(i)}
          title="Remove this address"
          aria-label="Remove {url || 'address'}"
        >
          <Icon name="x" size={14} />
        </button>
      </div>
    {:else}
      <div class="empty">No addresses yet. Add one, or discover a server on this network below.</div>
    {/each}
    <div class="actions">
      <button class="btn btn-sm" onclick={addUrl}><Icon name="plus" size={13} /> Add address</button>
      <button
        class="btn btn-sm"
        onclick={probeAll}
        disabled={probing || config.server.urls.length === 0}
      >
        <Icon name="activity" size={13} />
        {probing ? "Testing…" : "Test all"}
      </button>
    </div>
  </div>
</div>

<div class="section">
  <div class="section-title">On this network</div>
  <div class="group">
    <div class="row">
      <div class="row-copy">
        <div class="row-title">Discover servers automatically</div>
        <div class="row-sub">
          Finds una on the same network with mDNS. Handy at home — not a substitute for a VPN
          address.
        </div>
      </div>
      <Toggle
        checked={config.server.autodiscover}
        onchange={(v) => {
          config.server.autodiscover = v;
          save();
        }}
        label="Discover servers automatically"
      />
    </div>
    {#each discovered as s (s.url)}
      {@const added = config.server.urls.includes(s.url)}
      <div class="row">
        <div class="row-copy">
          <div class="row-title">{s.name}</div>
          <div class="row-sub mono">{s.url}{s.version ? ` · v${s.version}` : ""}</div>
        </div>
        <button class="btn btn-sm" disabled={added} onclick={() => adopt(s)}>
          {added ? "Added" : "Use this"}
        </button>
      </div>
    {/each}
    <div class="row">
      <div class="row-sub">
        {discovering
          ? "Scanning for two seconds…"
          : scanned && discovered.length === 0
            ? "Nothing found on this network."
            : "Scan the network for a una server."}
      </div>
      <button class="btn btn-sm" onclick={discover} disabled={discovering}>
        <Icon name="wifi" size={13} />
        {discovering ? "Scanning…" : "Scan"}
      </button>
    </div>
  </div>
</div>

<style>
  .status {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 9px;
    margin-top: 18px;
    padding: 10px 14px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--line);
    background: var(--subtle);
  }

  .status-text {
    font-weight: 500;
  }

  .status-detail {
    flex-basis: 100%;
    padding-left: 15px;
    font-size: 12px;
    color: var(--muted);
    word-break: break-word;
  }

  .endpoint {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 14px;
  }

  .url {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .ms {
    width: 44px;
    text-align: right;
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .icon {
    width: 26px;
    padding: 0;
  }

  .empty {
    padding: 14px;
    color: var(--muted);
    font-size: 12px;
  }

  .actions {
    display: flex;
    gap: 6px;
    padding: 8px 14px;
    background: var(--subtle);
  }

  .mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
</style>
