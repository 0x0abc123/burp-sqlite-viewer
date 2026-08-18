<script lang="ts">
  import { untrack } from 'svelte'
  import { commandError, type AnnotationInput, type InteractionDetail, type InteractionSummary } from './ipc'

  interface Props {
    selectedRows: InteractionSummary[]
    detail: InteractionDetail | null
    writable: boolean
    enabled: boolean
    onenable: () => Promise<void>
    onsave: (annotation: AnnotationInput) => Promise<number>
  }

  let { selectedRows, detail, writable, enabled, onenable, onsave }: Props = $props()
  const initial = untrack(() => ({ single: selectedRows.length === 1, first: selectedRows[0], detail }))
  let updateNotes = $state(initial.single)
  let notes = $state(initial.single ? initial.detail?.notes ?? '' : '')
  let updateColour = $state(initial.single)
  let colour = $state(initial.single ? initial.first?.colour ?? '' : '')
  let updateTag = $state(initial.single)
  let tag = $state(initial.single ? initial.first?.tag ?? '' : '')
  let updateIcon = $state(initial.single)
  let icon = $state(initial.single ? initial.first?.icon ?? '' : '')
  let busy = $state(false)
  let error = $state<string | null>(null)
  let status = $state('')

  async function enable(): Promise<void> {
    busy = true
    error = null
    try {
      await onenable()
      status = 'Annotation columns enabled.'
    } catch (caught) {
      error = commandError(caught).message
    } finally {
      busy = false
    }
  }

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault()
    if (!updateNotes && !updateColour && !updateTag && !updateIcon) {
      error = 'Choose at least one annotation field to change.'
      return
    }
    busy = true
    error = null
    try {
      const count = await onsave({
        interactionIds: selectedRows.map((row) => row.id),
        updateNotes,
        notes,
        updateColour,
        colour,
        updateTag,
        tag: tag.trim(),
        updateIcon,
        icon,
      })
      status = `Saved annotations for ${count} interaction${count === 1 ? '' : 's'}.`
    } catch (caught) {
      const problem = commandError(caught)
      error = `${problem.message}${problem.detail ? ` ${problem.detail}` : ''}`
    } finally {
      busy = false
    }
  }
</script>

<details class="annotation-editor">
  <summary>Annotate selection ({selectedRows.length})</summary>
  {#if !writable}
    <p>This capture is read-only. Make the database file writable to add notes or markers.</p>
  {:else if !enabled}
    <div class="migration">
      <p>This legacy capture needs <code>notes</code> and <code>metadata</code> columns. Enabling annotations changes its schema in one transaction; captured request and response bytes are untouched.</p>
      <button type="button" onclick={enable} disabled={busy}>{busy ? 'Enabling…' : 'Enable annotations'}</button>
    </div>
  {:else if selectedRows.length === 0}
    <p>Select one or more interactions before annotating them.</p>
  {:else}
    <form onsubmit={save}>
      <fieldset>
        <legend>Fields to change</legend>
        <label><input type="checkbox" bind:checked={updateColour} /> Colour</label>
        <select aria-label="Row highlight colour" bind:value={colour} disabled={!updateColour}>
          <option value="">No colour</option><option value="red">Red</option><option value="orange">Orange</option>
          <option value="yellow">Yellow</option><option value="green">Green</option><option value="blue">Blue</option><option value="purple">Purple</option>
        </select>

        <label><input type="checkbox" bind:checked={updateTag} /> Tag</label>
        <input aria-label="Interaction tag" bind:value={tag} maxlength="100" disabled={!updateTag} />

        <label><input type="checkbox" bind:checked={updateIcon} /> Icon</label>
        <select aria-label="Interaction icon" bind:value={icon} disabled={!updateIcon}>
          <option value="">No icon</option><option value="star">Star</option><option value="flag">Flag</option>
          <option value="bookmark">Bookmark</option><option value="bug">Bug</option><option value="check">Check</option>
        </select>

        <label><input type="checkbox" bind:checked={updateNotes} /> Notes</label>
        <textarea aria-label="Interaction notes" bind:value={notes} maxlength="10000" disabled={!updateNotes}></textarea>
      </fieldset>
      <button class="primary" type="submit" disabled={busy}>{busy ? 'Saving…' : `Apply to ${selectedRows.length} interaction${selectedRows.length === 1 ? '' : 's'}`}</button>
    </form>
  {/if}

  {#if error}<div class="error" role="alert"><span>{error}</span><button type="button" onclick={() => (error = null)}>Dismiss alert</button></div>{/if}
  <p class="status" role="status" aria-live="polite">{status}</p>
</details>

<style>
  .annotation-editor {
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface-raised);
  }

  summary {
    cursor: pointer;
    font-weight: var(--weight-medium);
  }

  form,
  fieldset,
  .migration {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-block-start: var(--space-3);
  }

  fieldset {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    margin-inline: 0;
    padding: var(--space-3);
    border: 1px solid var(--border);
  }

  label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  input,
  select,
  textarea {
    min-height: var(--control-height);
    padding: var(--space-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
  }

  textarea {
    min-height: var(--annotation-notes-height);
    resize: vertical;
  }

  button {
    min-height: var(--control-height);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--surface-raised);
    color: var(--text);
  }

  button.primary {
    align-self: flex-start;
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-contrast);
  }

  .error {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    margin-block-start: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--danger);
    border-radius: var(--radius-md);
  }

  .status {
    min-height: var(--space-5);
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  @media (max-width: 48rem) {
    fieldset { grid-template-columns: 1fr; }
  }
</style>
