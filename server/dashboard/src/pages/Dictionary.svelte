<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type DictionaryEntry } from '../api';
  import EmptyState from '../lib/EmptyState.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import Toggle from '../lib/Toggle.svelte';

  type Sort = 'used' | 'newest' | 'alpha';

  let entries = $state<DictionaryEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let query = $state('');
  let sort = $state<Sort>('used');

  // Add form
  let phrase = $state('');
  let soundsLike = $state('');
  let notes = $state('');
  let adding = $state(false);
  let addError = $state<string | null>(null);
  let phraseEl = $state<HTMLInputElement>();

  // Inline edit
  let editingId = $state<string | null>(null);
  let editPhrase = $state('');
  let editSounds = $state('');
  let editNotes = $state('');
  let saving = $state(false);

  let confirmDelete = $state<string | null>(null);

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      entries = await api.listDictionary();
    } catch (e) {
      error = errMsg(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  async function add(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    const value = phrase.trim();
    if (!value || adding) return;
    adding = true;
    addError = null;
    try {
      const created = await api.createDictionaryEntry({
        phrase: value,
        sounds_like: soundsLike.trim() || null,
        notes: notes.trim() || null,
      });
      entries = [created, ...entries];
      phrase = '';
      soundsLike = '';
      notes = '';
      phraseEl?.focus();
    } catch (err) {
      addError = errMsg(err);
    } finally {
      adding = false;
    }
  }

  function startEdit(entry: DictionaryEntry): void {
    editingId = entry.id;
    editPhrase = entry.phrase;
    editSounds = entry.sounds_like ?? '';
    editNotes = entry.notes ?? '';
    confirmDelete = null;
  }

  async function saveEdit(): Promise<void> {
    if (!editingId || saving) return;
    const value = editPhrase.trim();
    if (!value) return;
    saving = true;
    try {
      const updated = await api.patchDictionaryEntry(editingId, {
        phrase: value,
        sounds_like: editSounds.trim() || null,
        notes: editNotes.trim() || null,
      });
      entries = entries.map((x) => (x.id === updated.id ? updated : x));
      editingId = null;
    } catch (e) {
      error = errMsg(e);
    } finally {
      saving = false;
    }
  }

  async function setActive(entry: DictionaryEntry, active: boolean): Promise<void> {
    try {
      const updated = await api.patchDictionaryEntry(entry.id, { active });
      entries = entries.map((x) => (x.id === updated.id ? updated : x));
    } catch (e) {
      error = errMsg(e);
    }
  }

  async function remove(id: string): Promise<void> {
    try {
      await api.deleteDictionaryEntry(id);
      entries = entries.filter((x) => x.id !== id);
      confirmDelete = null;
      if (editingId === id) editingId = null;
    } catch (e) {
      error = errMsg(e);
    }
  }

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const filtered = q
      ? entries.filter(
          (e) =>
            e.phrase.toLowerCase().includes(q) ||
            (e.sounds_like ?? '').toLowerCase().includes(q) ||
            (e.notes ?? '').toLowerCase().includes(q),
        )
      : entries;
    const rows = [...filtered];
    switch (sort) {
      case 'alpha':
        return rows.sort((a, b) => a.phrase.localeCompare(b.phrase));
      // Ids are ULIDs, so lexical order is creation order.
      case 'newest':
        return rows.sort((a, b) => b.id.localeCompare(a.id));
      default:
        return rows.sort((a, b) => b.hit_count - a.hit_count || a.phrase.localeCompare(b.phrase));
    }
  });

  const SORTS: { k: Sort; l: string }[] = [
    { k: 'used', l: 'Most used' },
    { k: 'newest', l: 'Newest' },
    { k: 'alpha', l: 'A–Z' },
  ];
</script>

<div class="mx-auto max-w-3xl px-8 py-10">
  <header class="mb-7">
    <h1 class="text-[20px] font-semibold tracking-tight">Dictionary</h1>
    <p class="mt-1 text-[13px] text-muted">
      Names, jargon and acronyms una should always get right. These take effect on your very next
      dictation — no training required.
    </p>
  </header>

  <!-- Add ---------------------------------------------------------------- -->
  <form class="card mb-6 p-5" onsubmit={add}>
    <div class="grid gap-3 sm:grid-cols-[1fr_1fr]">
      <label class="block">
        <span class="label">Word or phrase</span>
        <input
          bind:this={phraseEl}
          class="input mt-1.5"
          bind:value={phrase}
          placeholder="Figma"
          maxlength="60"
        />
      </label>
      <label class="block">
        <span class="label">Often misheard as <span class="normal-case">(optional)</span></span>
        <input class="input mt-1.5" bind:value={soundsLike} placeholder="figment, sigma" />
      </label>
    </div>
    <label class="mt-3 block">
      <span class="label">Note <span class="normal-case">(optional)</span></span>
      <input class="input mt-1.5" bind:value={notes} placeholder="design tool we use" />
    </label>
    {#if addError}
      <p class="mt-2.5 text-[12px]" style="color: var(--c-danger)">{addError}</p>
    {/if}
    <div class="mt-4 flex items-center justify-between gap-4">
      <p class="text-[11px] text-faint">
        The phrase biases transcription; the misheard spelling helps the cleanup pass fix it.
      </p>
      <button class="btn btn-primary" type="submit" disabled={!phrase.trim() || adding}>
        {adding ? 'Adding…' : 'Add word'}
      </button>
    </div>
  </form>

  <!-- Filters ------------------------------------------------------------ -->
  <div class="mb-3 flex flex-wrap items-center gap-2">
    <div class="relative min-w-48 flex-1">
      <svg
        class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-faint"
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" />
      </svg>
      <input class="input pl-9" placeholder="Search words…" bind:value={query} aria-label="Search dictionary" />
    </div>
    <div class="seg">
      {#each SORTS as s (s.k)}
        <button type="button" class="seg-item" class:active={sort === s.k} onclick={() => (sort = s.k)}>
          {s.l}
        </button>
      {/each}
    </div>
  </div>

  {#if error}
    <div
      class="mb-4 rounded-xl border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--c-danger) 28%, transparent); background: var(--c-danger-soft); color: var(--c-danger)"
    >
      {error}
    </div>
  {/if}

  <!-- List --------------------------------------------------------------- -->
  {#if loading}
    <div class="space-y-2">
      {#each [0, 1, 2] as i (i)}<Skeleton class="h-14 w-full" />{/each}
    </div>
  {:else if visible.length === 0}
    <div class="card">
      <EmptyState
        title={query ? 'No matches' : 'No words yet'}
        sub={query
          ? 'Nothing in your dictionary matches that search.'
          : 'Add the names and jargon una keeps getting wrong.'}
      />
    </div>
  {:else}
    <div class="card divide-y divide-border overflow-hidden">
      {#each visible as entry (entry.id)}
        <div class="px-5 py-3.5" class:opacity-55={!entry.active}>
          {#if editingId === entry.id}
            <div class="grid gap-2.5 sm:grid-cols-2">
              <input class="input" bind:value={editPhrase} aria-label="Phrase" />
              <input class="input" bind:value={editSounds} placeholder="often misheard as…" aria-label="Sounds like" />
            </div>
            <input class="input mt-2.5" bind:value={editNotes} placeholder="note" aria-label="Note" />
            <div class="mt-3 flex gap-2">
              <button class="btn btn-primary btn-sm" onclick={saveEdit} disabled={saving || !editPhrase.trim()}>
                {saving ? 'Saving…' : 'Save'}
              </button>
              <button class="btn btn-ghost btn-sm" onclick={() => (editingId = null)}>Cancel</button>
            </div>
          {:else}
            <div class="flex items-start gap-3">
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-baseline gap-2">
                  <span class="text-[14px] font-semibold">{entry.phrase}</span>
                  {#if entry.sounds_like}
                    <span class="text-[12px] text-faint">
                      often heard as <span class="italic">{entry.sounds_like}</span>
                    </span>
                  {/if}
                </div>
                {#if entry.notes}
                  <p class="mt-0.5 text-[12px] text-muted">{entry.notes}</p>
                {/if}
              </div>

              <div class="flex flex-none items-center gap-2">
                <span
                  class="chip tabular-nums"
                  title="Times this phrase appeared in a transcript"
                  class:chip-accent={entry.hit_count > 0}
                >
                  {entry.hit_count}×
                </span>
                <Toggle
                  checked={entry.active}
                  onchange={(v) => void setActive(entry, v)}
                  label="Active"
                />
                <button class="btn btn-ghost btn-sm" onclick={() => startEdit(entry)}>Edit</button>
                {#if confirmDelete === entry.id}
                  <button class="btn btn-danger btn-sm" onclick={() => void remove(entry.id)}>
                    Delete
                  </button>
                  <button class="btn btn-ghost btn-sm" onclick={() => (confirmDelete = null)}>
                    Cancel
                  </button>
                {:else}
                  <button class="btn btn-ghost btn-sm" onclick={() => (confirmDelete = entry.id)}>
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">
                      <path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13" />
                    </svg>
                    <span class="sr-only">Delete {entry.phrase}</span>
                  </button>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
    <p class="mt-3 text-center text-[11px] text-faint">
      {visible.length} word{visible.length === 1 ? '' : 's'}
      {#if entries.some((e) => !e.active)}· inactive words are kept but ignored{/if}
    </p>
  {/if}
</div>
