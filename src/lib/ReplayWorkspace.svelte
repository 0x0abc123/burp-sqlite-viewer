<script lang="ts">
  import { untrack } from 'svelte'
  import { commandError, replayRequest, type InteractionDetail, type ReplayResult } from './ipc'

  interface Props {
    detail: InteractionDetail
    writable: boolean
    onclose: () => void
  }

  let { detail, writable, onclose }: Props = $props()
  const initial = untrack(() => ({ url: detail.url, bytes: detail.request.bytes }))
  let targetUrl = $state(initial.url)
  let editMode = $state<'text' | 'hex'>('text')
  let textDraft = $state(new TextDecoder().decode(new Uint8Array(initial.bytes)))
  let hexDraft = $state(initial.bytes.map((byte) => byte.toString(16).padStart(2, '0')).join(' '))
  let dirty = $state(false)
  let proxyUrl = $state(localStorage.getItem('replay-proxy-url-v1') ?? '')
  let proxyUsername = $state(localStorage.getItem('replay-proxy-username-v1') ?? '')
  let proxyPassword = $state('')
  let followRedirects = $state(localStorage.getItem('replay-follow-redirects-v1') === 'on')
  let timeoutSeconds = $state(Number(localStorage.getItem('replay-timeout-v1') ?? '30'))
  let sending = $state(false)
  let error = $state<string | null>(null)
  let result = $state.raw<ReplayResult | null>(null)

  function updateText(value: string): void {
    textDraft = value
    hexDraft = Array.from(new TextEncoder().encode(value), (byte) => byte.toString(16).padStart(2, '0')).join(' ')
    dirty = true
  }

  function updateHex(value: string): void {
    hexDraft = value
    dirty = true
  }

  function parseHex(): number[] {
    const compact = hexDraft.replace(/\s+/g, '')
    if (compact.length % 2 !== 0 || /[^0-9a-f]/i.test(compact)) throw new Error('Hexadecimal drafts require complete two-digit byte values.')
    return compact.match(/.{2}/g)?.map((value) => Number.parseInt(value, 16)) ?? []
  }

  function changeMode(next: 'text' | 'hex'): void {
    if (next === 'text' && editMode === 'hex') {
      try { textDraft = new TextDecoder().decode(new Uint8Array(parseHex())) }
      catch (caught) { error = caught instanceof Error ? caught.message : 'The hex draft is invalid.'; return }
    }
    editMode = next
  }

  async function send(): Promise<void> {
    error = null
    result = null
    let draftBytes: number[] | null = null
    try {
      if (dirty) draftBytes = editMode === 'hex' ? parseHex() : Array.from(new TextEncoder().encode(textDraft))
      new URL(targetUrl)
    } catch (caught) {
      error = caught instanceof Error ? caught.message : 'The replay draft is invalid.'
      return
    }
    localStorage.setItem('replay-proxy-url-v1', proxyUrl)
    localStorage.setItem('replay-proxy-username-v1', proxyUsername)
    localStorage.setItem('replay-follow-redirects-v1', followRedirects ? 'on' : 'off')
    localStorage.setItem('replay-timeout-v1', timeoutSeconds.toString())
    sending = true
    try {
      result = await replayRequest({
        interactionId: detail.id,
        targetUrl,
        draftBytes,
        proxyUrl: proxyUrl.trim() || null,
        proxyUsername,
        proxyPassword,
        followRedirects,
        timeoutSeconds,
      })
      if (result.error) error = result.error
    } catch (caught) {
      const problem = commandError(caught)
      error = `${problem.message}${problem.detail ? ` ${problem.detail}` : ''}`
    } finally {
      sending = false
    }
  }

  function responseText(bytes: number[]): string {
    return new TextDecoder().decode(new Uint8Array(bytes))
  }
</script>

<section class="replay" aria-labelledby="replay-heading">
  <header>
    <div><h3 id="replay-heading">Replay interaction {detail.id}</h3><p>The original capture remains unchanged.</p></div>
    <button type="button" onclick={onclose}>Back to traffic</button>
  </header>

  {#if !writable}<div class="warning" role="alert">Replay is disabled because this capture is read-only and replay history cannot be stored.</div>{/if}
  <div class="hazard"><strong>Structured replay normalises HTTP syntax.</strong> Deliberately malformed bytes remain preserved in the draft and history, but this sender may not transmit them literally. TLS certificate validation is disabled to support interception proxies. Expert raw-socket replay is intentionally unavailable.</div>

  <div class="target-row">
    <label for="replay-target">Target URL</label>
    <input id="replay-target" type="url" bind:value={targetUrl} />
    <button class="primary" type="button" onclick={send} disabled={!writable || sending}>{sending ? 'Sending…' : 'Send request'}</button>
  </div>

  <details>
    <summary>Proxy and transport settings</summary>
    <div class="settings">
      <label for="proxy-url">Proxy URL</label><input id="proxy-url" bind:value={proxyUrl} placeholder="http://127.0.0.1:8080 or socks5h://127.0.0.1:1080" />
      <label for="proxy-user">Proxy username</label><input id="proxy-user" bind:value={proxyUsername} autocomplete="username" />
      <label for="proxy-password">Proxy password</label><input id="proxy-password" type="password" bind:value={proxyPassword} autocomplete="current-password" />
      <label for="timeout">Timeout (seconds)</label><input id="timeout" type="number" min="1" max="300" bind:value={timeoutSeconds} />
      <label><input type="checkbox" bind:checked={followRedirects} /> Follow up to 10 redirects</label>
      <p>HTTP, HTTPS CONNECT, SOCKS4/4a and SOCKS5/5h proxy URLs are supported. Proxy passwords remain in memory only.</p>
    </div>
  </details>

  <div class="draft-heading">
    <h4>Editable request draft</h4>
    <div><button class:active={editMode === 'text'} type="button" onclick={() => changeMode('text')}>Text</button><button class:active={editMode === 'hex'} type="button" onclick={() => changeMode('hex')}>Hex bytes</button></div>
  </div>
  {#if detail.request.truncated}<p class="warning">This request exceeds the preview limit. Leave it unchanged to replay the complete original, or export it before editing.</p>{/if}
  {#if editMode === 'text'}
    <textarea aria-label="Editable HTTP request" value={textDraft} oninput={(event) => updateText(event.currentTarget.value)} disabled={detail.request.truncated}></textarea>
  {:else}
    <textarea aria-label="Editable hexadecimal request bytes" value={hexDraft} oninput={(event) => updateHex(event.currentTarget.value)} disabled={detail.request.truncated}></textarea>
  {/if}

  {#if error}<div class="error" role="alert"><span>{error}</span><button type="button" onclick={() => (error = null)}>Dismiss alert</button></div>{/if}
  {#if result}
    <section class="result" aria-labelledby="replay-result-heading">
      <h4 id="replay-result-heading">Replay result</h4>
      <p>History #{result.historyId} · {result.statusCode ?? 'No status'} · {result.elapsedMillis} ms · normalised structured send</p>
      {#if result.responseBytes.length}<pre>{responseText(result.responseBytes)}</pre>{/if}
    </section>
  {/if}
</section>

<style>
  .replay, .settings { display: flex; flex-direction: column; gap: var(--space-3); }
  header, .target-row, .draft-heading, .error { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
  header p, .settings p, .result p { color: var(--text-muted); font-size: var(--text-sm); }
  .hazard, .warning, .error { padding: var(--space-3); border: 1px solid var(--warning); border-radius: var(--radius-md); }
  .hazard { background: var(--surface-raised); }
  .target-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; }
  input, textarea { padding: var(--space-2); border: 1px solid var(--border-strong); border-radius: var(--radius-sm); background: var(--surface); color: var(--text); }
  textarea, pre { min-height: var(--replay-editor-height); padding: var(--space-3); overflow: auto; font-family: var(--font-mono); font-size: var(--text-sm); white-space: pre; }
  button, summary { min-height: var(--control-height); padding: var(--space-2) var(--space-3); border: 1px solid var(--border-strong); border-radius: var(--radius-md); background: var(--surface-raised); color: var(--text); cursor: pointer; }
  button.primary { border-color: var(--accent); background: var(--accent); color: var(--accent-contrast); }
  button.active { border-color: var(--accent); color: var(--accent); font-weight: var(--weight-bold); }
  .settings { display: grid; grid-template-columns: auto minmax(0, 1fr); padding: var(--space-3); }
  .settings p, .settings label:last-of-type { grid-column: 1 / -1; }
  .error { border-color: var(--danger); }
  .result { padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); }
  @media (max-width: 48rem) { .target-row, .settings { grid-template-columns: 1fr; } header, .draft-heading { align-items: stretch; flex-direction: column; } .settings p, .settings label:last-of-type { grid-column: auto; } }
</style>
