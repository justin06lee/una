<script lang="ts">
  import { onMount } from 'svelte';

  import { api, errMsg, type DictionaryEntry } from '../api';
  import EmptyState from '../lib/EmptyState.svelte';
  import Skeleton from '../lib/Skeleton.svelte';
  import Toggle from '../lib/Toggle.svelte';

  let entries = $state<DictionaryEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  // add row
  let newPhrase = $state('');
  let newSounds = $state('');
  let newNotes = $state('');
  let adding = $state(false);
  let addError = $state<string | null>(null);

  // inline edit
  let editId = $state<string | null>(null);
  let editPhrase = $state('');
  let editSounds = $state('');
  let editNotes = $state('');
  let saving = $state(false);

  let confirmId = $state<string | null>(null);

  onMount(() => void load());

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

  async function add(): Promise<void> {
    const phrase = newPhrase.trim();
    if (!phrase || adding) return;
    adding = true;
    addError = null;
    try {
      const entry = await api.createDictionaryEntry({
        phrase,
        sounds_like: newSounds.trim() || null,
        notes: newNotes.trim() || null,
      });
      entries = [...entries, entry];
      newPhrase = '';
      newSounds = '';
      newNotes = '';
    } catch (e) {
      addError = errMsg(e);
    } finally {
      adding = false;
    }
  }

  function startEdit(entry: DictionaryEntry): void {
    editId = entry.id;
    editPhrase = entry.phrase;
    editSounds = entry.sounds_like ?? '';
    editNotes = entry.notes ?? '';
    confirmId = null;
  }

  async function saveEdit(): Promise<void> {
    if (!editId || saving) return;
    const phrase = editPhrase.trim();
    if (!phrase) return;
    saving = true;
    error = null;
    try {
      const updated = await api.patchDictionaryEntry(editId, {
        phrase,
        sounds_like: editSounds.trim() || null,
        notes: editNotes.trim() || null,
      });
      entries = entries.map((e) => (e.id === updated.id ? updated : e));
      editId = null;
    } catch (e) {
      error = errMsg(e);
    } finally {
      saving = false;
    }
  }

  async function toggleActive(entry: DictionaryEntry): Promise<void> {
    try {
      const updated = await api.patchDictionaryEntry(entry.id, { active: !entry.active });
      entries = entries.map((e) => (e.id === updated.id ? updated : e));
    } catch (e) {
      error = errMsg(e);
    }
  }

  async function remove(id: string): Promise<void> {
    try {
      await api.deleteDictionaryEntry(id);
      entries = entries.filter((e) => e.id !== id);
      confirmId = null;
    } catch (e) {
      error = errMsg(e);
    }
  }

  function onAddKeydown(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      e.preventDefault();
      void add();
    }
  }

  function onEditKeydown(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      e.preventDefault();
      void saveEdit();
    } else if (e.key === 'Escape') {
      editId = null;
    }
  }
</script>

<div class="mx-auto max-w-5xl px-6 py-8">
  <header class="mb-6">
    <h1 class="text-base font-semibold tracking-tight">Dictionary</h1>
    <p class="mt-0.5 text-[13px] text-muted">
      Names and jargon the model should get right. Prompt injection is capped — the most-used
      terms are prioritized.
    </p>
  </header>

  {#if error}
    <div
      class="mb-4 rounded-lg border px-4 py-2.5 text-[13px]"
      style="border-color: color-mix(in oklab, var(--color-danger) 30%, transparent); background: color-mix(in oklab, var(--color-danger) 7%, transparent); color: var(--color-danger)"
    >
      {error}
    </div>
  {/if}

  <div class="card overflow-x-auto">
    <table class="w-full min-w-[640px] text-sm">
      <thead>
        <tr class="border-b border-border">
          <th class="th w-[24%]">Phrase</th>
          <th class="th w-[20%]">Sounds like</th>
          <th class="th">Notes</th>
          <th class="th w-16 !text-right">Uses</th>
          <th class="th w-16">Active</th>
          <th class="th w-28"><span class="sr-only">Actions</span></th>
        </tr>
      </thead>
      <tbody class="divide-y divide-border">
        <tr class="bg-raised/30">
          <td class="px-3 py-2">
            <input
              class="input"
              placeholder="Add a phrase…"
              bind:value={newPhrase}
              onkeydown={onAddKeydown}
              aria-label="New phrase"
            />
          </td>
          <td class="px-3 py-2">
            <input
              class="input"
              placeholder="e.g. wisper, whispr"
              bind:value={newSounds}
              onkeydown={onAddKeydown}
              aria-label="Sounds like"
            />
          </td>
          <td class="px-3 py-2">
            <input
              class="input"
              placeholder="Optional context"
              bind:value={newNotes}
              onkeydown={onAddKeydown}
              aria-label="Notes"
            />
          </td>
          <td class="px-3 py-2"></td>
          <td class="px-3 py-2"></td>
          <td class="px-3 py-2 text-right">
            <button
              class="btn btn-sm btn-primary"
              onclick={() => void add()}
              disabled={adding || !newPhrase.trim()}
            >
              Add
            </button>
          </td>
        </tr>
        {#if addError}
          <tr>
            <td colspan="6" class="px-3 py-2 text-xs" style="color: var(--color-danger)">
              {addError}
            </td>
          </tr>
        {/if}

        {#if loading}
          {#each Array(4) as _, i (i)}
            <tr>
              <td class="px-3 py-3"><Skeleton class="h-4 w-24" /></td>
              <td class="px-3 py-3"><Skeleton class="h-4 w-20" /></td>
              <td class="px-3 py-3"><Skeleton class="h-4 w-32" /></td>
              <td class="px-3 py-3"><Skeleton class="ml-auto h-4 w-6" /></td>
              <td class="px-3 py-3"><Skeleton class="h-4 w-8" /></td>
              <td></td>
            </tr>
          {/each}
        {:else}
          {#each entries as entry (entry.id)}
            <tr class="group transition-colors duration-150 hover:bg-hover/50">
              {#if editId === entry.id}
                <td class="px-3 py-2">
                  <input
                    class="input"
                    bind:value={editPhrase}
                    onkeydown={onEditKeydown}
                    aria-label="Phrase"
                  />
                </td>
                <td class="px-3 py-2">
                  <input
                    class="input"
                    bind:value={editSounds}
                    onkeydown={onEditKeydown}
                    aria-label="Sounds like"
                  />
                </td>
                <td class="px-3 py-2">
                  <input
                    class="input"
                    bind:value={editNotes}
                    onkeydown={onEditKeydown}
                    aria-label="Notes"
                  />
                </td>
                <td class="px-3 py-2 text-right text-xs text-faint tabular-nums">
                  {entry.hit_count}
                </td>
                <td class="px-3 py-2">
                  <Toggle
                    checked={entry.active}
                    onchange={() => void toggleActive(entry)}
                    label="Active"
                  />
                </td>
                <td class="px-3 py-2 text-right whitespace-nowrap">
                  <button
                    class="btn btn-sm btn-primary"
                    onclick={() => void saveEdit()}
                    disabled={saving || !editPhrase.trim()}
                  >
                    Save
                  </button>
                  <button class="btn btn-sm btn-ghost" onclick={() => (editId = null)}>
                    Cancel
                  </button>
                </td>
              {:else}
                <td class="px-3 py-2.5 font-medium" class:opacity-50={!entry.active}>
                  {entry.phrase}
                </td>
                <td class="px-3 py-2.5 text-muted" class:opacity-50={!entry.active}>
                  {entry.sounds_like ?? '—'}
                </td>
                <td
                  class="max-w-0 truncate px-3 py-2.5 text-muted"
                  class:opacity-50={!entry.active}
                  title={entry.notes ?? undefined}
                >
                  {entry.notes ?? '—'}
                </td>
                <td class="px-3 py-2.5 text-right text-muted tabular-nums">{entry.hit_count}</td>
                <td class="px-3 py-2.5">
                  <Toggle
                    checked={entry.active}
                    onchange={() => void toggleActive(entry)}
                    label="Active"
                  />
                </td>
                <td class="px-3 py-2.5 text-right whitespace-nowrap">
                  {#if confirmId === entry.id}
                    <button class="btn btn-sm btn-danger" onclick={() => void remove(entry.id)}>
                      Delete?
                    </button>
                    <button class="btn btn-sm btn-ghost" onclick={() => (confirmId = null)}>
                      Cancel
                    </button>
                  {:else}
                    <span
                      class="opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus-within:opacity-100"
                    >
                      <button class="btn btn-sm btn-ghost" onclick={() => startEdit(entry)}>
                        Edit
                      </button>
                      <button
                        class="btn btn-sm btn-ghost btn-danger"
                        onclick={() => (confirmId = entry.id)}
                      >
                        Delete
                      </button>
                    </span>
                  {/if}
                </td>
              {/if}
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>

    {#if !loading && entries.length === 0}
      <EmptyState
        title="No dictionary entries"
        sub="Add product names, acronyms, or people the transcriber keeps mishearing."
      />
    {/if}
  </div>
</div>
