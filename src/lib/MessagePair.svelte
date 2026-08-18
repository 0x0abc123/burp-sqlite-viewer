<script lang="ts">
  import { onMount } from 'svelte'
  import { commandError, saveMessageSelection, type InteractionDetail } from './ipc'

  type Orientation = 'side-by-side' | 'stacked'
  type ViewMode = 'raw' | 'hex'

  interface Props {
    detail: InteractionDetail
    onexport?: (part: 'request' | 'response') => void
    exporting?: 'request' | 'response' | null
  }

  let { detail, onexport, exporting = null }: Props = $props()
  let orientation = $state<Orientation>(loadOrientation())
  let narrow = $state(false)
  let wrap = $state(localStorage.getItem('message-wrap-v1') === 'on')
  let requestMode = $state<ViewMode>('raw')
  let responseMode = $state<ViewMode>('raw')
  let split = $state(loadSplit())
  let requestPane: HTMLElement
  let responsePane: HTMLElement
  let operationStatus = $state('')

  const requestText = $derived(decodeBytes(detail.request.bytes))
  const responseText = $derived(decodeBytes(detail.response.bytes))
  const requestDisplay = $derived(requestMode === 'hex' ? bytesToHex(detail.request.bytes) : requestText)
  const responseDisplay = $derived(responseMode === 'hex' ? bytesToHex(detail.response.bytes) : responseText)
  const requestHighlighted = $derived(highlightHttp(requestText, false))
  const responseHighlighted = $derived(highlightHttp(responseText, detail.mimeType.includes('javascript')))
  const requestHasInvalidUtf8 = $derived(requestText.includes('\uFFFD'))
  const responseHasInvalidUtf8 = $derived(responseText.includes('\uFFFD'))
  const effectiveOrientation = $derived(narrow ? 'stacked' : orientation)

  onMount(() => {
    const query = window.matchMedia('(max-width: 48rem)')
    const update = (): void => { narrow = query.matches }
    update()
    query.addEventListener('change', update)
    return () => query.removeEventListener('change', update)
  })

  function loadOrientation(): Orientation {
    return localStorage.getItem('message-orientation-v1') === 'stacked' ? 'stacked' : 'side-by-side'
  }

  function loadSplit(): number {
    const saved = Number(localStorage.getItem('message-split-v1'))
    return Number.isFinite(saved) ? Math.min(80, Math.max(20, saved)) : 50
  }

  function setOrientation(next: Orientation): void {
    orientation = next
    localStorage.setItem('message-orientation-v1', next)
  }

  function setWrap(next: boolean): void {
    wrap = next
    localStorage.setItem('message-wrap-v1', next ? 'on' : 'off')
  }

  function setSplit(next: number): void {
    split = Math.min(80, Math.max(20, next))
    localStorage.setItem('message-split-v1', split.toString())
  }

  function decodeBytes(bytes: number[]): string {
    return new TextDecoder('utf-8', { fatal: false }).decode(new Uint8Array(bytes))
  }

  function bytesToHex(bytes: number[]): string {
    const lines: string[] = []
    for (let offset = 0; offset < bytes.length; offset += 16) {
      const chunk = bytes.slice(offset, offset + 16)
      const hex = chunk.map((byte) => byte.toString(16).padStart(2, '0')).join(' ').padEnd(47, ' ')
      const ascii = chunk.map((byte) => byte >= 32 && byte <= 126 ? String.fromCharCode(byte) : '.').join('')
      lines.push(`${offset.toString(16).padStart(8, '0')}  ${hex}  |${ascii}|`)
    }
    return lines.join('\n')
  }

  function escapeHtml(value: string): string {
    return value
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#039;')
  }

  function highlightHttp(value: string, javascriptBody: boolean): string {
    const [head, ...bodyParts] = value.split(/\r?\n\r?\n/)
    const lines = head.split(/\r?\n/)
    const highlightedHead = lines.map((line, index) => {
      const safe = escapeHtml(line)
      if (index === 0) return `<span class="syntax-start">${safe}</span>`
      const colon = safe.indexOf(':')
      if (colon < 1) return safe
      return `<span class="syntax-header">${safe.slice(0, colon)}</span>${safe.slice(colon)}`
    }).join('\n')
    let body = escapeHtml(bodyParts.join('\n\n'))
    if (javascriptBody) {
      body = body.replace(/\b(const|let|var|function|return|if|else|async|await|class|new)\b/g,
        '<span class="syntax-keyword">$1</span>')
    }
    return bodyParts.length ? `${highlightedHead}\n\n${body}` : highlightedHead
  }

  async function copyDisplayed(part: 'request' | 'response'): Promise<void> {
    const pane = part === 'request' ? requestPane : responsePane
    const selectedText = selectedTextIn(pane)
    const fallback = part === 'request' ? requestDisplay : responseDisplay
    try {
      await navigator.clipboard.writeText(selectedText || fallback)
      operationStatus = `Copied ${selectedText ? 'selected' : 'displayed'} ${part} text.`
    } catch {
      operationStatus = `Could not copy the ${part} text. Select it and use the system copy command.`
    }
  }

  function selectedTextIn(pane: HTMLElement): string {
    const selection = window.getSelection()
    return selection && selection.rangeCount > 0 && pane.contains(selection.anchorNode)
      ? selection.toString()
      : ''
  }

  async function saveSelection(part: 'request' | 'response'): Promise<void> {
    const text = selectedTextIn(part === 'request' ? requestPane : responsePane)
    if (!text) {
      operationStatus = `Select ${part} text before saving a selection.`
      return
    }
    try {
      const result = await saveMessageSelection(detail.id, part, text)
      operationStatus = result
        ? `Saved ${result.byteCount.toLocaleString()} bytes of selected text.`
        : 'Save selection cancelled.'
    } catch (caught) {
      operationStatus = commandError(caught).message
    }
  }

  function resizeFromPointer(event: PointerEvent): void {
    const container = event.currentTarget instanceof HTMLElement ? event.currentTarget.parentElement : null
    if (!container) return
    const bounds = container.getBoundingClientRect()
    const next = effectiveOrientation === 'side-by-side'
      ? ((event.clientX - bounds.left) / bounds.width) * 100
      : ((event.clientY - bounds.top) / bounds.height) * 100
    setSplit(next)
  }

  function resizeFromKeyboard(event: KeyboardEvent): void {
    const decrease = effectiveOrientation === 'side-by-side' ? event.key === 'ArrowLeft' : event.key === 'ArrowUp'
    const increase = effectiveOrientation === 'side-by-side' ? event.key === 'ArrowRight' : event.key === 'ArrowDown'
    if (!decrease && !increase && event.key !== 'Home' && event.key !== 'End') return
    event.preventDefault()
    if (event.key === 'Home') setSplit(20)
    else if (event.key === 'End') setSplit(80)
    else setSplit(split + (increase ? 5 : -5))
  }
</script>

<div class="message-toolbar" aria-label="Message display controls">
  <div class="segmented" aria-label="Pane orientation">
    <button class:active={orientation === 'side-by-side'} type="button" onclick={() => setOrientation('side-by-side')}>Left–right</button>
    <button class:active={orientation === 'stacked'} type="button" onclick={() => setOrientation('stacked')}>Top–bottom</button>
  </div>
  <label><input type="checkbox" checked={wrap} onchange={(event) => setWrap(event.currentTarget.checked)} /> Wrap lines</label>
</div>

<div
  class:stacked={effectiveOrientation === 'stacked'}
  class="message-pair"
  style={`--pane-split: ${split}%; --pane-remainder: ${100 - split}%`}
>
  <section class="message-pane" aria-labelledby={`request-heading-${detail.id}`} bind:this={requestPane}>
    <div class="pane-heading">
      <h4 id={`request-heading-${detail.id}`}>Request</h4>
      <span>{detail.request.totalLength.toLocaleString()} bytes</span>
      <div class="pane-actions">
        <button class:active={requestMode === 'raw'} type="button" onclick={() => (requestMode = 'raw')}>Raw</button>
        <button class:active={requestMode === 'hex'} type="button" onclick={() => (requestMode = 'hex')}>Hex</button>
        <button type="button" onclick={() => copyDisplayed('request')}>Copy displayed</button>
        <button type="button" onclick={() => saveSelection('request')}>Save selection…</button>
        {#if onexport}<button type="button" onclick={() => onexport?.('request')} disabled={exporting !== null}>{exporting === 'request' ? 'Exporting…' : 'Export…'}</button>{/if}
      </div>
    </div>
    {#if detail.request.truncated}<p class="preview-warning">Showing the first 1 MiB. Export preserves every byte.</p>{/if}
    {#if requestMode === 'raw' && requestHasInvalidUtf8}<p class="preview-warning">Invalid UTF-8 is shown with replacement characters. Use Hex for exact bytes.</p>{/if}
    <div class="message-content" role="textbox" aria-readonly="true" tabindex="0" aria-label={requestMode === 'raw' ? 'Raw HTTP request' : 'Hexadecimal HTTP request'}>
      {#if requestMode === 'raw'}<pre class:wrap>{@html requestHighlighted}</pre>{:else}<pre>{requestDisplay}</pre>{/if}
    </div>
  </section>

  <div
    class="splitter"
    role="slider"
    aria-label="Resize request and response panes"
    aria-orientation={effectiveOrientation === 'side-by-side' ? 'vertical' : 'horizontal'}
    aria-valuemin="20"
    aria-valuemax="80"
    aria-valuenow={split}
    tabindex="0"
    onpointerdown={(event) => event.currentTarget.setPointerCapture(event.pointerId)}
    onpointermove={(event) => event.currentTarget.hasPointerCapture(event.pointerId) && resizeFromPointer(event)}
    onkeydown={resizeFromKeyboard}
  ></div>

  <section class="message-pane" aria-labelledby={`response-heading-${detail.id}`} bind:this={responsePane}>
    <div class="pane-heading">
      <h4 id={`response-heading-${detail.id}`}>Response</h4>
      <span>{detail.response.totalLength.toLocaleString()} bytes</span>
      <div class="pane-actions">
        <button class:active={responseMode === 'raw'} type="button" onclick={() => (responseMode = 'raw')}>Raw</button>
        <button class:active={responseMode === 'hex'} type="button" onclick={() => (responseMode = 'hex')}>Hex</button>
        <button type="button" onclick={() => copyDisplayed('response')}>Copy displayed</button>
        <button type="button" onclick={() => saveSelection('response')}>Save selection…</button>
        {#if onexport}<button type="button" onclick={() => onexport?.('response')} disabled={exporting !== null}>{exporting === 'response' ? 'Exporting…' : 'Export…'}</button>{/if}
      </div>
    </div>
    {#if detail.response.truncated}<p class="preview-warning">Showing the first 1 MiB. Export preserves every byte.</p>{/if}
    {#if responseMode === 'raw' && responseHasInvalidUtf8}<p class="preview-warning">Invalid UTF-8 is shown with replacement characters. Use Hex for exact bytes.</p>{/if}
    <div class="message-content" role="textbox" aria-readonly="true" tabindex="0" aria-label={responseMode === 'raw' ? 'Raw HTTP response' : 'Hexadecimal HTTP response'}>
      {#if responseMode === 'raw'}<pre class:wrap>{@html responseHighlighted}</pre>{:else}<pre>{responseDisplay}</pre>{/if}
    </div>
  </section>
</div>
<p class="operation-status" role="status" aria-live="polite">{operationStatus}</p>

<style>
  .message-toolbar,
  .pane-heading,
  .pane-actions,
  .segmented,
  label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .message-toolbar {
    justify-content: space-between;
    flex-wrap: wrap;
  }

  button {
    min-height: var(--control-height);
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
  }

  button.active {
    border-color: var(--accent);
    color: var(--accent);
    font-weight: var(--weight-bold);
  }

  .message-pair {
    display: grid;
    grid-template-columns: minmax(0, var(--pane-split)) var(--splitter-size) minmax(0, var(--pane-remainder));
    min-width: 0;
  }

  .message-pair.stacked {
    height: var(--stacked-pair-height);
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, var(--pane-split)) var(--splitter-size) minmax(0, var(--pane-remainder));
  }

  .message-pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
  }

  .pane-heading {
    justify-content: space-between;
    flex-wrap: wrap;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
  }

  .pane-heading > span,
  .preview-warning,
  .operation-status {
    color: var(--text-muted);
    font-size: var(--text-xs);
  }

  .preview-warning {
    padding: var(--space-2) var(--space-3);
  }

  .operation-status {
    min-height: var(--space-5);
  }

  .message-content {
    min-height: var(--message-pane-min-height);
    max-height: var(--message-pane-max-height);
    overflow: auto;
  }

  pre {
    margin: 0;
    padding: var(--space-3);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
    white-space: pre;
    user-select: text;
  }

  pre.wrap {
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  pre :global(.syntax-start),
  pre :global(.syntax-header) {
    color: var(--syntax-header);
    font-weight: var(--weight-bold);
  }

  pre :global(.syntax-keyword) {
    color: var(--syntax-keyword);
  }

  .splitter {
    background: var(--border);
    cursor: col-resize;
  }

  .stacked .splitter {
    cursor: row-resize;
  }

  .splitter:hover,
  .splitter:focus-visible {
    background: var(--accent);
  }

</style>
