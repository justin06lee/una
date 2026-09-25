<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type DictionaryEntry } from '../api';
  import Banner from '../lib/Banner.svelte';
  import EmptyState from '../lib/EmptyState.svelte';
  import Icon from '../lib/Icon.svelte';
  import PageHeader from '../lib/PageHeader.svelte';
  import SearchInput from '../lib/SearchInput.svelte';
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

<div class="mx-auto max-w-[46rem] px-10 py-12">
  <PageHeader
    title="Dictionary"
    sub="Names, jargon and acronyms una should always get right. They apply from your very next dictation, no training needed."
  />

  <!-- Add ---------------------------------------------------------------- -->
  <form class="panel mb-10 p-4" onsubmit={add}>
    <div class="grid gap-3 sm:grid-cols-[1.25fr_1fr_1fr_auto] sm:items-end">
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
        <span class="label">Often heard as</span>
        <input class="input mt-1.5" bind:value={soundsLike} placeholder="figment, sigma" />
      </label>
      <label class="block">
        <span class="label">Note</span>
        <input class="input mt-1.5" bind:value={notes} placeholder="Design tool" />
      </label>
      <button class="btn btn-primary" type="submit" disabled={!phrase.trim() || adding}>
        <Icon name="plus" size={14} />
        {adding ? 'Adding…' : 'Add'}
      </button>
    </div>
    {#if addError}
      <p class="mt-3 text-[12.5px] text-danger">{addError}</p>
    {/if}
    <p class="mt-3 text-[12px] text-faint">
      The phrase steers transcription toward the right spelling; the misheard version helps the
      cleanup pass catch it when it slips through.
    </p>
  </form>

  <!-- Filters ------------------------------------------------------------ -->
  <div class="mb-4 flex flex-wrap items-center gap-2">
    <SearchInput bind:value={query} placeholder="Search words…" label="Search dictionary" />
    <div class="seg">
      {#each SORTS as s (s.k)}
        <button type="button" class="seg-item" class:active={sort === s.k} onclick={() => (sort = s.k)}>
          {s.l}
        </button>
      {/each}
    </div>
  </div>

  {#if error}
    <Banner message={error} onretry={() => void load()} />
  {/if}

  <!-- List --------------------------------------------------------------- -->
  {#if loading}
    <div class="space-y-2">
      {#each [0, 1, 2] as i (i)}<Skeleton class="h-14 w-full" />{/each}
    </div>
  {:else if visible.length === 0}
    <div class="panel">
      <EmptyState
        title={query ? 'No matches' : 'No words yet'}
        sub={query
          ? 'Nothing in your dictionary matches that search.'
          : 'Add the names and jargon una keeps getting wrong.'}
      />
    </div>
  {:else}
    <div class="panel divide-y divide-line overflow-hidden">
      {#each visible as entry (entry.id)}
        <div class="group px-4 py-3">
          {#if editingId === entry.id}
            <div class="grid gap-2 py-0.5 sm:grid-cols-3">
              <input class="input" bind:value={editPhrase} aria-label="Phrase" />
              <input class="input" bind:value={editSounds} placeholder="Often heard as" aria-label="Often heard as" />
              <input class="input" bind:value={editNotes} placeholder="Note" aria-label="Note" />
            </div>
            <div class="mt-2.5 flex justify-end gap-1.5">
              <button class="btn btn-ghost btn-sm" onclick={() => (editingId = null)}>Cancel</button>
              <button class="btn btn-primary btn-sm" onclick={saveEdit} disabled={saving || !editPhrase.trim()}>
                {saving ? 'Saving…' : 'Save'}
              </button>
            </div>
          {:else}
            <div class="flex items-center gap-4">
              <div class="min-w-0 flex-1" class:opacity-45={!entry.active}>
                <div class="flex flex-wrap items-baseline gap-x-2">
                  <span class="text-[14px] font-medium">{entry.phrase}</span>
                  {#if entry.sounds_like}
                    <span class="text-[12.5px] text-faint">heard as {entry.sounds_like}</span>
                  {/if}
                </div>
                {#if entry.notes}
                  <p class="mt-0.5 truncate text-[12.5px] text-muted">{entry.notes}</p>
                {/if}
              </div>

              <div class="flex flex-none items-center gap-1 opacity-0 transition-opacity duration-150 group-focus-within:opacity-100 group-hover:opacity-100">
                {#if confirmDelete === entry.id}
                  <button class="btn btn-ghost btn-sm" onclick={() => (confirmDelete = null)}>Cancel</button>
                  <button class="btn btn-danger btn-sm" onclick={() => void remove(entry.id)}>Delete</button>
                {:else}
                  <button
                    class="btn btn-ghost btn-sm btn-icon"
                    onclick={() => startEdit(entry)}
                    title="Edit"
                    aria-label="Edit {entry.phrase}"
                  >
                    <Icon name="pencil" size={13} />
                  </button>
                  <button
                    class="btn btn-ghost btn-sm btn-icon hover:!text-danger"
                    onclick={() => (confirmDelete = entry.id)}
                    title="Delete"
                    aria-label="Delete {entry.phrase}"
                  >
                    <Icon name="trash" size={13} />
                  </button>
                {/if}
              </div>
              <span
                class="w-10 text-right text-[12px] tabular-nums {entry.hit_count > 0 ? 'text-muted' : 'text-faint'}"
                title="Times this phrase appeared in a transcript"
              >
                {entry.hit_count}×
              </span>
              <Toggle
                checked={entry.active}
                onchange={(v) => void setActive(entry, v)}
                label={entry.active ? `Turn off ${entry.phrase}` : `Turn on ${entry.phrase}`}
              />
            </div>
          {/if}
        </div>
      {/each}
    </div>
    <p class="mt-4 text-center text-[12px] text-faint">
      {visible.length} word{visible.length === 1 ? '' : 's'}
      {#if entries.some((e) => !e.active)}· switched-off words are kept but ignored{/if}
    </p>
  {/if}
</div>
