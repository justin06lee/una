<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import Mark from "../../lib/Mark.svelte";

  let version = $state("");

  onMount(() => {
    getVersion()
      .then((v) => (version = v))
      .catch(() => {});
  });
</script>

<div class="about">
  <Mark size={56} />
  <h1 class="page-title">una</h1>
  <p class="faint">{version ? `Version ${version}` : ""}</p>
  <p class="page-sub">
    Self-hosted dictation. Hold a hotkey, speak, let go. Audio goes to your own una server; the
    cleaned-up text is typed into whatever app has focus. Nothing leaves your network, and the
    longer you use it, the better it knows your voice.
  </p>
</div>

<style>
  .about {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 6px;
    padding-top: 36px;
  }

  .about :global(svg) {
    margin-bottom: 10px;
  }

  .about .page-sub {
    margin-top: 10px;
  }
</style>
