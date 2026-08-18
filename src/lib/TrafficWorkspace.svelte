<script lang="ts">
  import {
    chooseDatabasePath,
    closeDatabase,
    commandError,
    exportInteractionPart,
    getInteraction,
    openDatabase,
    queryInteractions,
    type DatabaseSummary,
    type InteractionDetail,
    type InteractionSummary,
    type PageCursor,
  } from './ipc'

  let database = $state.raw<DatabaseSummary | null>(null)
  let rows = $state.raw<InteractionSummary[]>([])
  let nextCursor = $state.raw<PageCursor | null>(null)
  let selectedId = $state<number | null>(null)
  let detail = $state.raw<InteractionDetail | null>(null)
  let opening = $state(false)
  let loadingRows = $state(false)
  let loadingDetail = $state(false)
  let exporting = $state<'request' | 'response' | null>(null)
  let error = $state<string | null>(null)
  let status = $state('Choose a SQLite capture to begin.')

  const selected = $derived(rows.find((row) => row.id === selectedId) ?? null)
  const requestText = $derived(detail ? decodeBytes(detail.request.bytes) : '')
  const responseText = $derived(detail ? decodeBytes(detail.response.bytes) : '')

  function decodeBytes(bytes: number[]): string {
    return new TextDecoder('utf-8', { fatal: false }).decode(new Uint8Array(bytes))
  }

  function formatTimestamp(value: number, valid: boolean): string {
    if (!valid) return `Invalid (${value})`
    return new Intl.DateTimeFormat(undefined, {
      dateStyle: 'short',
      timeStyle: 'medium',
    }).format(new Date(value))
  }

  function formatBytes(value: number): string {
    if (value < 1024) return `${value} B`
    if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`
    return `${(value / (1024 * 1024)).toFixed(1)} MiB`
  }

  function showError(caught: unknown, recovery: string): void {
    const problem = commandError(caught)
    error = `${problem.message}${problem.detail ? ` ${problem.detail}` : ''} ${recovery}`
    status = 'Operation failed.'
  }

  async function loadFirstPage(): Promise<void> {
    loadingRows = true
    try {
      const page = await queryInteractions()
      rows = page.rows
      nextCursor = page.nextCursor
      status = page.rows.length
        ? `Loaded ${page.rows.length.toLocaleString()} interaction rows.`
        : 'The database contains no HTTP interactions.'
    } finally {
      loadingRows = false
    }
  }

  async function openCapture(): Promise<void> {
    error = null
    opening = true
    status = database ? 'Choosing another database…' : 'Choosing a database…'
    try {
      const path = await chooseDatabasePath()
      if (!path) {
        status = database ? 'Kept the current database open.' : 'Open cancelled.'
        return
      }
      const opened = await openDatabase(path)
      database = opened
      rows = []
      nextCursor = null
      selectedId = null
      detail = null
      await loadFirstPage()
    } catch (caught) {
      showError(caught, 'Choose another SQLite capture or retry.')
    } finally {
      opening = false
    }
  }

  async function loadMore(): Promise<void> {
    if (!nextCursor || loadingRows) return
    error = null
    loadingRows = true
    try {
      const page = await queryInteractions(nextCursor)
      rows = [...rows, ...page.rows]
      nextCursor = page.nextCursor
      status = `Loaded ${rows.length.toLocaleString()} of ${database?.rowCount.toLocaleString() ?? '—'} rows.`
    } catch (caught) {
      showError(caught, 'Retry loading the next page.')
    } finally {
      loadingRows = false
    }
  }

  async function selectInteraction(id: number): Promise<void> {
    if (id === selectedId && detail) return
    selectedId = id
    detail = null
    error = null
    loadingDetail = true
    status = `Loading interaction ${id}…`
    try {
      detail = await getInteraction(id)
      status = `Interaction ${id} selected.`
    } catch (caught) {
      showError(caught, 'Select the row again to retry.')
    } finally {
      loadingDetail = false
    }
  }

  async function exportPart(part: 'request' | 'response'): Promise<void> {
    if (selectedId === null || exporting) return
    error = null
    exporting = part
    status = `Choosing where to export the ${part}…`
    try {
      const result = await exportInteractionPart(selectedId, part)
      status = result
        ? `Exported ${formatBytes(result.byteCount)} to ${result.path}.`
        : 'Export cancelled.'
    } catch (caught) {
      showError(caught, `Choose another destination and retry the ${part} export.`)
    } finally {
      exporting = null
    }
  }

  async function closeCapture(): Promise<void> {
    error = null
    try {
      await closeDatabase()
      database = null
      rows = []
      nextCursor = null
      selectedId = null
      detail = null
      status = 'Database closed. Choose another SQLite capture to continue.'
    } catch (caught) {
      showError(caught, 'Retry closing the database.')
    }
  }
</script>

<section class="workspace" aria-labelledby="workspace-heading">
  <div class="toolbar">
    <div class="title-group">
      <h2 id="workspace-heading">HTTP interactions</h2>
      {#if database}
        <p title={database.path}>
          {database.displayName} · {database.mode} · {database.rowCount.toLocaleString()} rows
        </p>
      {:else}
        <p>Inspect captured requests and responses from a SQLite database.</p>
      {/if}
    </div>

    <div class="actions" aria-label="Database actions">
      <button class="primary" type="button" onclick={openCapture} disabled={opening}>
        {opening ? 'Opening…' : database ? 'Open another…' : 'Open database…'}
      </button>
      {#if database}
        <button type="button" onclick={closeCapture}>Close database</button>
      {/if}
    </div>
  </div>

  {#if error}
    <div class="message error" role="alert">
      <strong>Could not complete the operation.</strong>
      <span>{error}</span>
    </div>
  {/if}

  {#if database?.diagnostics.length}
    <div class="message warning" role="status">
      <strong>Database diagnostics</strong>
      <ul>
        {#each database.diagnostics as diagnostic}
          <li>{diagnostic}</li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if !database}
    <div class="empty-state">
      <div>
        <h3>No database open</h3>
        <p>
          Choose a SQLite 3 file containing an <code>http_interactions</code> table. The file
          is opened read-only and validated before the current workspace changes.
        </p>
      </div>
      <button class="primary" type="button" onclick={openCapture} disabled={opening}>
        {opening ? 'Opening…' : 'Open database…'}
      </button>
    </div>
  {:else}
    <div class="table-region" aria-busy={loadingRows}>
      <table>
        <caption class="visually-hidden">
          Captured HTTP interactions, newest first
        </caption>
        <thead>
          <tr>
            <th scope="col">Select</th>
            <th scope="col">Time</th>
            <th scope="col">Method</th>
            <th scope="col">Host</th>
            <th scope="col">URL</th>
            <th scope="col">Status</th>
            <th scope="col">Type</th>
            <th scope="col">Size</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id)}
            <tr class:selected={row.id === selectedId}>
              <td>
                <input
                  type="radio"
                  name="interaction"
                  value={row.id}
                  checked={row.id === selectedId}
                  aria-label={`Select interaction ${row.id}`}
                  onchange={() => selectInteraction(row.id)}
                />
              </td>
              <td class:invalid={!row.capturedAtValid} title={`Raw: ${row.capturedAt}`}>
                {formatTimestamp(row.capturedAt, row.capturedAtValid)}
              </td>
              <td><span class="method">{row.method}</span></td>
              <td>{row.host}{row.port === 80 || row.port === 443 ? '' : `:${row.port}`}</td>
              <td class="url" title={row.url}>{row.url}</td>
              <td class="numeric">{row.statusCode}</td>
              <td>{row.mimeType || '—'}</td>
              <td class="numeric">{formatBytes(row.responseLength)}</td>
            </tr>
          {:else}
            <tr>
              <td colspan="8" class="table-empty">No HTTP interactions were found.</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    {#if nextCursor}
      <div class="pagination">
        <span>{rows.length.toLocaleString()} of {database.rowCount.toLocaleString()} rows</span>
        <button type="button" onclick={loadMore} disabled={loadingRows}>
          {loadingRows ? 'Loading…' : 'Load more'}
        </button>
      </div>
    {/if}

    <section class="detail" aria-labelledby="detail-heading" aria-busy={loadingDetail}>
      <div class="detail-header">
        <div>
          <h3 id="detail-heading">
            {selected ? `${selected.method} ${selected.url}` : 'Request and response'}
          </h3>
          <p>
            {selected
              ? `Interaction ${selected.id} · ${selected.statusCode}`
              : 'Select one interaction to inspect its raw bytes.'}
          </p>
        </div>
        <div class="actions" aria-label="Export selected interaction">
          <button
            type="button"
            onclick={() => exportPart('request')}
            disabled={!detail || exporting !== null}
          >
            {exporting === 'request' ? 'Exporting…' : 'Export request…'}
          </button>
          <button
            type="button"
            onclick={() => exportPart('response')}
            disabled={!detail || exporting !== null}
          >
            {exporting === 'response' ? 'Exporting…' : 'Export response…'}
          </button>
        </div>
      </div>

      {#if loadingDetail}
        <p class="detail-state">Loading the selected request and response…</p>
      {:else if detail}
        <div class="message-pair">
          <section class="message-pane" aria-labelledby="request-heading">
            <div class="pane-heading">
              <h4 id="request-heading">Request</h4>
              <span>{formatBytes(detail.request.totalLength)}</span>
            </div>
            {#if detail.request.truncated}
              <p class="preview-warning">Showing the first 1 MiB. Export preserves every byte.</p>
            {/if}
            <textarea aria-label="Raw HTTP request" readonly value={requestText}></textarea>
          </section>
          <section class="message-pane" aria-labelledby="response-heading">
            <div class="pane-heading">
              <h4 id="response-heading">Response</h4>
              <span>{formatBytes(detail.response.totalLength)}</span>
            </div>
            {#if detail.response.truncated}
              <p class="preview-warning">Showing the first 1 MiB. Export preserves every byte.</p>
            {/if}
            <textarea aria-label="Raw HTTP response" readonly value={responseText}></textarea>
          </section>
        </div>
      {:else}
        <p class="detail-state">No interaction selected.</p>
      {/if}
    </section>
  {/if}

  <p class="status" role="status" aria-live="polite">{status}</p>
</section>

<style>
  .workspace {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }

  .toolbar,
  .detail-header,
  .pagination,
  .pane-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .title-group,
  .detail-header > div:first-child {
    min-width: 0;
  }

  .title-group p,
  .detail-header p,
  .pane-heading span,
  .pagination,
  .status {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  .title-group p,
  .detail-header p {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
  }

  button {
    min-height: var(--control-height);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--surface-raised);
    color: var(--text);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    background: var(--surface-sunken);
  }

  button.primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-contrast);
  }

  button.primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .empty-state,
  .message,
  .detail {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-6);
    padding: var(--space-6);
  }

  .empty-state div {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .message {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4);
  }

  .message.error {
    border-color: var(--danger);
  }

  .message.warning,
  .preview-warning,
  .invalid {
    color: var(--warning);
  }

  .message ul {
    padding-inline-start: var(--space-5);
  }

  .table-region {
    min-width: 0;
    max-height: var(--table-max-height);
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
  }

  table {
    width: 100%;
    min-width: var(--table-min-width);
    border-collapse: collapse;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  th,
  td {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    text-align: left;
    white-space: nowrap;
  }

  th {
    position: sticky;
    inset-block-start: 0;
    z-index: 1;
    background: var(--surface-raised);
    color: var(--text-muted);
    font-family: var(--font-ui);
    font-weight: var(--weight-medium);
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  tbody tr.selected {
    background: var(--surface-sunken);
    box-shadow: inset var(--selection-rail-width) 0 0 var(--accent);
  }

  input[type='radio'] {
    accent-color: var(--accent);
  }

  .method {
    font-weight: var(--weight-bold);
  }

  .url {
    max-width: var(--url-column-max-width);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .numeric {
    text-align: right;
  }

  .table-empty {
    padding: var(--space-6);
    color: var(--text-muted);
    text-align: center;
  }

  .pagination {
    justify-content: flex-end;
  }

  .detail {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    min-width: 0;
  }

  .message-pair {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-3);
    min-width: 0;
  }

  .message-pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
  }

  .pane-heading {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
  }

  .preview-warning {
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-xs);
  }

  textarea {
    width: 100%;
    min-height: var(--message-pane-min-height);
    max-height: var(--message-pane-max-height);
    margin: 0;
    padding: var(--space-3);
    overflow: auto;
    resize: vertical;
    border: 0;
    border-radius: 0 0 var(--radius-md) var(--radius-md);
    background: var(--surface);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
    white-space: pre;
  }

  .detail-state {
    min-height: var(--message-pane-min-height);
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }

  .status {
    min-height: var(--space-5);
  }

  @media (max-width: 48rem) {
    .toolbar,
    .detail-header,
    .empty-state {
      align-items: stretch;
      flex-direction: column;
    }

    .actions {
      flex-wrap: wrap;
    }

    .message-pair {
      grid-template-columns: 1fr;
    }
  }
</style>
