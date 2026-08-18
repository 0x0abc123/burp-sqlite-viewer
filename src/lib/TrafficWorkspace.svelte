<script lang="ts">
  import {
    chooseDatabasePath,
    closeDatabase,
    commandError,
    annotateInteractions,
    enableAnnotations,
    exportInteractionPart,
    getInteraction,
    openDatabase,
    openDetailWindow,
    queryInteractions,
    type DatabaseSummary,
    type InteractionDetail,
    type InteractionSummary,
    type FilterField,
    type FilterOperator,
    type FilterSpec,
    type PageCursor,
    type QuerySpec,
    type SortDirection,
    type SortField,
    type AnnotationInput,
  } from './ipc'
  import AnnotationEditor from './AnnotationEditor.svelte'
  import MessagePair from './MessagePair.svelte'
  import ReplayWorkspace from './ReplayWorkspace.svelte'

  interface ColumnDefinition {
    key: SortField | 'notesPreview'
    label: string
    visible: boolean
    width: number
    sortable: boolean
    numeric?: boolean
  }

  type TableDensity = 'compact' | 'comfortable'

  const defaultColumns: ColumnDefinition[] = [
    { key: 'icon', label: 'Icon', visible: true, width: 6, sortable: true },
    { key: 'tag', label: 'Tag', visible: true, width: 10, sortable: true },
    { key: 'capturedAt', label: 'Time', visible: true, width: 13, sortable: true },
    { key: 'method', label: 'Method', visible: true, width: 7, sortable: true },
    { key: 'host', label: 'Host', visible: true, width: 14, sortable: true },
    { key: 'url', label: 'URL', visible: true, width: 28, sortable: true },
    { key: 'statusCode', label: 'Status', visible: true, width: 7, sortable: true, numeric: true },
    { key: 'mimeType', label: 'Type', visible: true, width: 11, sortable: true },
    { key: 'responseLength', label: 'Size', visible: true, width: 8, sortable: true, numeric: true },
    { key: 'id', label: 'ID', visible: false, width: 7, sortable: true, numeric: true },
    { key: 'tool', label: 'Tool', visible: false, width: 9, sortable: true },
    { key: 'scheme', label: 'Scheme', visible: false, width: 7, sortable: true },
    { key: 'port', label: 'Port', visible: false, width: 7, sortable: true, numeric: true },
    { key: 'colour', label: 'Colour', visible: false, width: 8, sortable: true },
    { key: 'notesPreview', label: 'Notes', visible: false, width: 16, sortable: false },
  ]

  const numericFilterFields = new Set<FilterField>([
    'id',
    'capturedAt',
    'port',
    'statusCode',
    'responseLength',
  ])

  let database = $state.raw<DatabaseSummary | null>(null)
  let rows = $state.raw<InteractionSummary[]>([])
  let nextCursor = $state.raw<PageCursor | null>(null)
  let filteredCount = $state(0)
  let selectedId = $state<number | null>(null)
  let selectedIds = $state.raw<number[]>([])
  let selectionAnchor = $state<number | null>(null)
  let detail = $state.raw<InteractionDetail | null>(null)
  let opening = $state(false)
  let loadingRows = $state(false)
  let loadingDetail = $state(false)
  let exporting = $state<'request' | 'response' | null>(null)
  let error = $state<string | null>(null)
  let status = $state('Choose a SQLite capture to begin.')
  let columns = $state.raw<ColumnDefinition[]>(loadColumnPreferences())
  let sortField = $state<SortField>('capturedAt')
  let sortDirection = $state<SortDirection>('desc')
  let filters = $state.raw<FilterSpec[]>([])
  let draftFilterField = $state<FilterField>('host')
  let draftFilterOperator = $state<FilterOperator>('contains')
  let draftFilterValue = $state('')
  let tableDensity = $state<TableDensity>(loadTableDensity())
  let dismissedDiagnosticsPath = $state<string | null>(null)
  let replaying = $state(false)

  const selected = $derived(rows.find((row) => row.id === selectedId) ?? null)
  const selectedRows = $derived(rows.filter((row) => selectedIds.includes(row.id)))
  const visibleColumns = $derived(columns.filter((column) => column.visible))
  const draftIsNumeric = $derived(numericFilterFields.has(draftFilterField))
  const allPageSelected = $derived(rows.length > 0 && rows.every((row) => selectedIds.includes(row.id)))
  const somePageSelected = $derived(rows.some((row) => selectedIds.includes(row.id)) && !allPageSelected)

  function loadColumnPreferences(): ColumnDefinition[] {
    try {
      const saved = localStorage.getItem('traffic-columns-v1')
      if (!saved) return defaultColumns.map((column) => ({ ...column }))
      const parsed = JSON.parse(saved) as Array<Partial<ColumnDefinition> & { key?: string }>
      const known = new Map(defaultColumns.map((column) => [column.key, column]))
      const restored = parsed.flatMap((savedColumn) => {
        const fallback = savedColumn.key ? known.get(savedColumn.key as ColumnDefinition['key']) : null
        if (!fallback) return []
        known.delete(fallback.key)
        return [{
          ...fallback,
          visible: typeof savedColumn.visible === 'boolean' ? savedColumn.visible : fallback.visible,
          width: typeof savedColumn.width === 'number' ? Math.min(40, Math.max(6, savedColumn.width)) : fallback.width,
        }]
      })
      return [...restored, ...known.values()].map((column) => ({ ...column }))
    } catch {
      return defaultColumns.map((column) => ({ ...column }))
    }
  }

  function loadTableDensity(): TableDensity {
    return localStorage.getItem('traffic-table-density-v1') === 'comfortable'
      ? 'comfortable'
      : 'compact'
  }

  function changeTableDensity(next: TableDensity): void {
    tableDensity = next
    localStorage.setItem('traffic-table-density-v1', next)
    status = `Table density changed to ${next}.`
  }

  function saveColumnPreferences(next: ColumnDefinition[]): void {
    columns = next
    localStorage.setItem('traffic-columns-v1', JSON.stringify(next))
  }

  function currentQuery(): QuerySpec {
    return { sortField, sortDirection, filters }
  }

  function cellText(row: InteractionSummary, key: ColumnDefinition['key']): string {
    switch (key) {
      case 'id': return row.id.toString()
      case 'capturedAt': return formatTimestamp(row.capturedAt, row.capturedAtValid)
      case 'tool': return row.tool
      case 'scheme': return row.scheme
      case 'host': return row.host
      case 'port': return row.port.toString()
      case 'method': return row.method
      case 'url': return row.url
      case 'statusCode': return row.statusCode.toString()
      case 'mimeType': return row.mimeType || '—'
      case 'responseLength': return formatBytes(row.responseLength)
      case 'colour': return row.colour || '—'
      case 'tag': return row.tag || '—'
      case 'icon': return row.metadataValid ? row.icon || '—' : 'Invalid metadata'
      case 'notesPreview': return row.notesPreview || '—'
    }
  }

  function updateColumn(key: ColumnDefinition['key'], change: Partial<ColumnDefinition>): void {
    saveColumnPreferences(columns.map((column) => column.key === key ? { ...column, ...change } : column))
  }

  function moveColumn(key: ColumnDefinition['key'], delta: -1 | 1): void {
    const index = columns.findIndex((column) => column.key === key)
    const target = index + delta
    if (index < 0 || target < 0 || target >= columns.length) return
    const next = [...columns]
    ;[next[index], next[target]] = [next[target], next[index]]
    saveColumnPreferences(next)
  }

  function resetColumns(): void {
    saveColumnPreferences(defaultColumns.map((column) => ({ ...column })))
    status = 'Column layout reset.'
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
      const page = await queryInteractions(currentQuery())
      rows = page.rows
      nextCursor = page.nextCursor
      filteredCount = page.filteredCount
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
      dismissedDiagnosticsPath = null
      rows = []
      nextCursor = null
      selectedId = null
      selectedIds = []
      selectionAnchor = null
      detail = null
      replaying = false
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
      const page = await queryInteractions(currentQuery(), nextCursor)
      rows = [...rows, ...page.rows]
      nextCursor = page.nextCursor
      status = `Loaded ${rows.length.toLocaleString()} of ${filteredCount.toLocaleString()} matching rows.`
    } catch (caught) {
      showError(caught, 'Retry loading the next page.')
    } finally {
      loadingRows = false
    }
  }

  async function applyQuery(message: string): Promise<void> {
    nextCursor = null
    selectedId = null
    selectedIds = []
    selectionAnchor = null
    detail = null
    replaying = false
    error = null
    try {
      await loadFirstPage()
      status = message
    } catch (caught) {
      showError(caught, 'Adjust the table query and retry.')
    }
  }

  async function changeSort(field: SortField): Promise<void> {
    if (sortField === field) sortDirection = sortDirection === 'asc' ? 'desc' : 'asc'
    else {
      sortField = field
      sortDirection = 'asc'
    }
    await applyQuery(`Sorted by ${field}, ${sortDirection === 'asc' ? 'ascending' : 'descending'}.`)
  }

  async function addFilter(event: SubmitEvent): Promise<void> {
    event.preventDefault()
    if (draftIsNumeric && !['equals', 'greaterThan', 'lessThan'].includes(draftFilterOperator)) {
      draftFilterOperator = 'equals'
    } else if (!draftIsNumeric && ['greaterThan', 'lessThan'].includes(draftFilterOperator)) {
      draftFilterOperator = 'contains'
    }
    const value = draftFilterOperator === 'isEmpty' ? '' : draftFilterValue.trim()
    if (draftFilterOperator !== 'isEmpty' && !value) {
      error = 'Enter a filter value before adding the filter.'
      return
    }
    filters = [...filters, { field: draftFilterField, operator: draftFilterOperator, value }]
    draftFilterValue = ''
    await applyQuery(`Applied ${filters.length} column filter${filters.length === 1 ? '' : 's'}.`)
  }

  async function removeFilter(index: number): Promise<void> {
    filters = filters.filter((_, filterIndex) => filterIndex !== index)
    await applyQuery(filters.length ? 'Filter removed.' : 'All filters cleared.')
  }

  function toggleSelection(id: number, range: boolean): void {
    if (range && selectionAnchor !== null) {
      const anchorIndex = rows.findIndex((row) => row.id === selectionAnchor)
      const currentIndex = rows.findIndex((row) => row.id === id)
      if (anchorIndex >= 0 && currentIndex >= 0) {
        const [start, end] = anchorIndex < currentIndex
          ? [anchorIndex, currentIndex]
          : [currentIndex, anchorIndex]
        selectedIds = Array.from(new Set([...selectedIds, ...rows.slice(start, end + 1).map((row) => row.id)]))
      }
    } else if (selectedIds.includes(id)) {
      selectedIds = selectedIds.filter((selectedRowId) => selectedRowId !== id)
    } else {
      selectedIds = [...selectedIds, id]
    }
    selectionAnchor = id
    const nextSelectedId = selectedIds.includes(id) ? id : selectedIds.at(-1) ?? null
    if (nextSelectedId !== null) void selectInteraction(nextSelectedId)
    else {
      selectedId = null
      detail = null
    }
    status = `${selectedIds.length} interaction${selectedIds.length === 1 ? '' : 's'} selected.`
  }

  function selectRow(id: number, event: MouseEvent): void {
    if (event.target instanceof HTMLInputElement) return
    if (event.shiftKey) {
      toggleSelection(id, true)
      return
    }
    if (event.ctrlKey || event.metaKey) {
      toggleSelection(id, false)
      return
    }
    selectedIds = [id]
    selectionAnchor = id
    void selectInteraction(id)
    status = '1 interaction selected.'
  }

  function togglePageSelection(): void {
    if (allPageSelected) selectedIds = selectedIds.filter((id) => !rows.some((row) => row.id === id))
    else selectedIds = Array.from(new Set([...selectedIds, ...rows.map((row) => row.id)]))
    const nextSelectedId = selectedIds.at(-1) ?? null
    selectionAnchor = nextSelectedId
    if (nextSelectedId !== null) void selectInteraction(nextSelectedId)
    else {
      selectedId = null
      detail = null
    }
    status = `${selectedIds.length} interaction${selectedIds.length === 1 ? '' : 's'} selected.`
  }

  async function selectInteraction(id: number): Promise<void> {
    if (id === selectedId && detail) return
    selectedId = id
    replaying = false
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

  async function detachInteraction(): Promise<void> {
    if (selectedId === null) return
    error = null
    try {
      await openDetailWindow(selectedId)
      status = `Opened interaction ${selectedId} in a detail window.`
    } catch (caught) {
      showError(caught, 'Retry opening the detached detail window.')
    }
  }

  async function enableCaptureAnnotations(): Promise<void> {
    database = await enableAnnotations()
    status = 'Annotation columns enabled. Captured request and response bytes were unchanged.'
  }

  async function saveAnnotations(annotation: AnnotationInput): Promise<number> {
    const result = await annotateInteractions(annotation)
    const page = await queryInteractions(currentQuery())
    rows = page.rows
    nextCursor = page.nextCursor
    filteredCount = page.filteredCount
    selectedIds = selectedIds.filter((id) => rows.some((row) => row.id === id))
    if (selectedId !== null && selectedIds.includes(selectedId)) detail = await getInteraction(selectedId)
    status = `Saved annotations for ${result.updatedCount} interaction${result.updatedCount === 1 ? '' : 's'}.`
    return result.updatedCount
  }

  async function closeCapture(): Promise<void> {
    error = null
    try {
      await closeDatabase()
      database = null
      dismissedDiagnosticsPath = null
      rows = []
      nextCursor = null
      selectedId = null
      selectedIds = []
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

  {#if database}
    <div class="query-controls">
      <form class="filter-form" onsubmit={addFilter}>
        <label for="filter-field">Filter column</label>
        <select id="filter-field" bind:value={draftFilterField}>
          {#each defaultColumns.filter((column) => column.key !== 'notesPreview') as column}
            <option value={column.key}>{column.label}</option>
          {/each}
        </select>
        <label for="filter-operator">Operator</label>
        <select id="filter-operator" bind:value={draftFilterOperator}>
          {#if draftIsNumeric}
            <option value="equals">equals</option>
            <option value="greaterThan">greater than</option>
            <option value="lessThan">less than</option>
          {:else}
            <option value="contains">contains</option>
            <option value="equals">equals</option>
            <option value="startsWith">starts with</option>
            <option value="endsWith">ends with</option>
            <option value="isEmpty">is empty</option>
          {/if}
        </select>
        <label for="filter-value">Value</label>
        <input
          id="filter-value"
          type={draftIsNumeric ? 'number' : 'text'}
          bind:value={draftFilterValue}
          disabled={draftFilterOperator === 'isEmpty'}
        />
        <button type="submit" disabled={filters.length >= 8}>Add filter</button>
      </form>

      <label class="density-control" for="table-density">
        Table density
        <select
          id="table-density"
          value={tableDensity}
          onchange={(event) => changeTableDensity(event.currentTarget.value as TableDensity)}
        >
          <option value="compact">Compact</option>
          <option value="comfortable">Comfortable</option>
        </select>
      </label>

      <details class="column-chooser">
        <summary>Columns ({visibleColumns.length} shown)</summary>
        <div class="column-list">
          {#each columns as column, index (column.key)}
            <div class="column-item">
              <label>
                <input
                  type="checkbox"
                  checked={column.visible}
                  onchange={(event) => updateColumn(column.key, { visible: event.currentTarget.checked })}
                />
                {column.label}
              </label>
              <div class="column-actions" aria-label={`${column.label} column position and width`}>
                <button type="button" onclick={() => moveColumn(column.key, -1)} disabled={index === 0}>
                  Move up
                </button>
                <button
                  type="button"
                  onclick={() => moveColumn(column.key, 1)}
                  disabled={index === columns.length - 1}
                >Move down</button>
                <button
                  type="button"
                  onclick={() => updateColumn(column.key, { width: Math.max(6, column.width - 2) })}
                  disabled={column.width <= 6}
                >Narrower</button>
                <button
                  type="button"
                  onclick={() => updateColumn(column.key, { width: Math.min(40, column.width + 2) })}
                  disabled={column.width >= 40}
                >Wider</button>
              </div>
            </div>
          {/each}
          <button type="button" onclick={resetColumns}>Reset columns</button>
        </div>
      </details>
    </div>

    {#if filters.length}
      <div class="active-filters" aria-label="Active filters">
        {#each filters as filter, index}
          <span class="filter-chip">
            {filter.field} {filter.operator} {filter.value || 'empty'}
            <button type="button" onclick={() => removeFilter(index)}>
              Remove filter
            </button>
          </span>
        {/each}
      </div>
    {/if}

    {#key `${selectedIds.join(',')}:${detail?.id ?? ''}:${database.schemaVariant}`}
      <AnnotationEditor
        {selectedRows}
        {detail}
        writable={database.mode === 'read-write'}
        enabled={database.schemaVariant === 'current'}
        onenable={enableCaptureAnnotations}
        onsave={saveAnnotations}
      />
    {/key}
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
    <div class:compact={tableDensity === 'compact'} class="table-region" aria-busy={loadingRows}>
      <table>
        <caption class="visually-hidden">
          Captured HTTP interactions, newest first
        </caption>
        <thead>
          <tr>
            <th scope="col" class="selection-cell">
              <input
                type="checkbox"
                checked={allPageSelected}
                indeterminate={somePageSelected}
                aria-label="Select all rows on this page"
                onclick={togglePageSelection}
              />
            </th>
            {#each visibleColumns as column (column.key)}
              <th scope="col" style:width={`${column.width}rem`}>
                {#if column.sortable}
                  <button
                    class="sort-button"
                    type="button"
                    onclick={() => changeSort(column.key as SortField)}
                    aria-label={`Sort by ${column.label}${sortField === column.key ? `, currently ${sortDirection}` : ''}`}
                  >
                    {column.label}
                    {sortField === column.key ? (sortDirection === 'asc' ? ' ↑' : ' ↓') : ''}
                  </button>
                {:else}
                  {column.label}
                {/if}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id)}
            <tr
              class:selected={selectedIds.includes(row.id)}
              data-colour={row.colour || undefined}
              onclick={(event) => selectRow(row.id, event)}
            >
              <td class="selection-cell">
                <input
                  type="checkbox"
                  value={row.id}
                  checked={selectedIds.includes(row.id)}
                  aria-label={`Select interaction ${row.id}; hold Shift to select a range`}
                  onclick={(event) => toggleSelection(row.id, event.shiftKey)}
                />
              </td>
              {#each visibleColumns as column (column.key)}
                <td
                  class:numeric={column.numeric}
                  class:invalid={column.key === 'capturedAt' && !row.capturedAtValid}
                  class:method={column.key === 'method'}
                  title={column.key === 'capturedAt' ? `Raw: ${row.capturedAt}` : cellText(row, column.key)}
                  style:width={`${column.width}rem`}
                >{cellText(row, column.key)}</td>
              {/each}
            </tr>
          {:else}
            <tr>
              <td colspan={visibleColumns.length + 1} class="table-empty">
                No HTTP interactions match the active filters.
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    {#if nextCursor}
      <div class="pagination">
        <span>{rows.length.toLocaleString()} of {filteredCount.toLocaleString()} matching rows</span>
        <button type="button" onclick={loadMore} disabled={loadingRows}>
          {loadingRows ? 'Loading…' : 'Load more'}
        </button>
      </div>
    {/if}

    {#if replaying && detail}
      <ReplayWorkspace {detail} writable={database.mode === 'read-write'} onclose={() => (replaying = false)} />
    {:else}
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
          <button type="button" onclick={detachInteraction} disabled={!detail}>
            Open detail window
          </button>
          <button type="button" onclick={() => (replaying = true)} disabled={!detail}>
            Edit and replay…
          </button>
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
        <MessagePair {detail} onexport={exportPart} {exporting} />
      {:else}
        <p class="detail-state">No interaction selected.</p>
      {/if}
    </section>
    {/if}
  {/if}

  <footer class="workspace-footer">
    {#if error}
      <div class="message error" role="alert">
        <div>
          <strong>Could not complete the operation.</strong>
          <span>{error}</span>
        </div>
        <button type="button" onclick={() => (error = null)}>Dismiss alert</button>
      </div>
    {/if}

    {#if database?.diagnostics.length && dismissedDiagnosticsPath !== database.path}
      <div class="message warning">
        <details>
          <summary>Database diagnostics ({database.diagnostics.length})</summary>
          <ul>
            {#each database.diagnostics as diagnostic}
              <li>{diagnostic}</li>
            {/each}
          </ul>
        </details>
        <button type="button" onclick={() => (dismissedDiagnosticsPath = database?.path ?? null)}>
          Dismiss diagnostics
        </button>
      </div>
    {/if}

    <p class="status" role="status" aria-live="polite">{status}</p>
  </footer>
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
  .pagination {
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

  .query-controls {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .density-control {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-muted);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .filter-form {
    display: flex;
    align-items: flex-end;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .filter-form label {
    align-self: center;
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  input,
  select {
    min-height: var(--control-height);
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface);
  }

  .column-chooser {
    position: relative;
    flex: none;
  }

  .column-chooser summary {
    min-height: var(--control-height);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--surface-raised);
    cursor: pointer;
  }

  .column-list {
    position: absolute;
    inset-block-start: calc(100% + var(--space-1));
    inset-inline-end: 0;
    z-index: 4;
    width: var(--column-chooser-width);
    max-height: var(--column-chooser-max-height);
    padding: var(--space-3);
    overflow: auto;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--surface-raised);
    box-shadow: var(--shadow-raised);
  }

  .column-item {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding-block: var(--space-2);
    border-bottom: 1px solid var(--border);
  }

  .column-item label,
  .column-actions,
  .active-filters,
  .filter-chip {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .column-actions {
    flex-wrap: wrap;
  }

  .column-actions button,
  .filter-chip button {
    min-height: var(--control-height);
    padding: var(--space-1) var(--space-2);
    font-size: var(--text-xs);
  }

  .active-filters {
    flex-wrap: wrap;
  }

  .filter-chip {
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface-raised);
    font-size: var(--text-sm);
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
    align-items: flex-start;
    justify-content: space-between;
    flex-direction: row;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4);
  }

  .message > div,
  .message details {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .message button {
    flex: none;
  }

  .message.error {
    border-color: var(--danger);
  }

  .message.warning,
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
    table-layout: fixed;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  th,
  td {
    padding: var(--table-cell-padding-block-comfortable) var(--space-3);
    border-bottom: 1px solid var(--border);
    text-align: left;
    white-space: nowrap;
  }

  .table-region.compact td {
    padding-block: var(--table-cell-padding-block-compact);
    line-height: var(--leading-tight);
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

  .sort-button {
    width: 100%;
    min-height: var(--control-height);
    padding: var(--space-1);
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
  }

  .selection-cell {
    width: var(--selection-column-width);
    text-align: center;
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  tbody tr {
    cursor: pointer;
  }

  tbody tr:hover {
    background: var(--surface-raised);
  }

  tbody tr[data-colour='red'] { background: var(--marker-red); }
  tbody tr[data-colour='orange'] { background: var(--marker-orange); }
  tbody tr[data-colour='yellow'] { background: var(--marker-yellow); }
  tbody tr[data-colour='green'] { background: var(--marker-green); }
  tbody tr[data-colour='blue'] { background: var(--marker-blue); }
  tbody tr[data-colour='purple'] { background: var(--marker-purple); }

  tbody tr.selected {
    box-shadow: inset var(--selection-rail-width) 0 0 var(--accent);
  }

  tbody tr.selected:not([data-colour]) {
    background: var(--surface-sunken);
  }

  input[type='checkbox'] {
    accent-color: var(--accent);
  }

  .method {
    font-weight: var(--weight-bold);
  }

  td {
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

  .detail-state {
    min-height: var(--message-pane-min-height);
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }

  .status {
    min-height: var(--space-5);
  }

  .workspace-footer {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
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

    .query-controls,
    .filter-form {
      align-items: stretch;
      flex-direction: column;
    }

    .density-control {
      align-items: stretch;
      flex-direction: column;
    }

    .message {
      flex-direction: column;
    }

    .column-chooser,
    .column-list {
      width: 100%;
    }

    .column-list {
      position: static;
    }

  }
</style>
