<script lang="ts">
  import { onMount } from 'svelte'
  import MessagePair from './MessagePair.svelte'
  import {
    commandError,
    exportInteractionPart,
    getInteraction,
    type InteractionDetail,
  } from './ipc'

  interface Props { interactionId: number }
  let { interactionId }: Props = $props()
  let detail = $state.raw<InteractionDetail | null>(null)
  let loading = $state(true)
  let error = $state<string | null>(null)
  let status = $state('Loading interaction…')
  let exporting = $state<'request' | 'response' | null>(null)

  onMount(() => { void load() })

  async function load(): Promise<void> {
    loading = true
    error = null
    try {
      detail = await getInteraction(interactionId)
      status = `Interaction ${interactionId} loaded.`
    } catch (caught) {
      const problem = commandError(caught)
      error = `${problem.message}${problem.detail ? ` ${problem.detail}` : ''}`
      status = 'Interaction unavailable.'
    } finally {
      loading = false
    }
  }

  async function exportPart(part: 'request' | 'response'): Promise<void> {
    exporting = part
    error = null
    try {
      const result = await exportInteractionPart(interactionId, part)
      status = result ? `Exported ${result.byteCount.toLocaleString()} bytes to ${result.path}.` : 'Export cancelled.'
    } catch (caught) {
      const problem = commandError(caught)
      error = `${problem.message}${problem.detail ? ` ${problem.detail}` : ''}`
      status = 'Export failed.'
    } finally {
      exporting = null
    }
  }
</script>

<main class="detail-window" aria-busy={loading}>
  {#if detail}
    <header>
      <div>
        <h1>{detail.method} {detail.url}</h1>
        <p>Interaction {detail.id} · Status {detail.statusCode} · {new Date(detail.capturedAt).toLocaleString()}</p>
      </div>
    </header>
    <MessagePair {detail} onexport={exportPart} {exporting} />
  {:else if loading}
    <section class="state"><h1>Interaction {interactionId}</h1><p>Loading request and response…</p></section>
  {:else}
    <section class="state">
      <h1>Interaction unavailable</h1>
      <p>{error}</p>
      <div><button type="button" onclick={load}>Retry</button> <button type="button" onclick={() => window.close()}>Close window</button></div>
    </section>
  {/if}

  {#if error && detail}<div class="alert" role="alert"><span>{error}</span><button type="button" onclick={() => (error = null)}>Dismiss alert</button></div>{/if}
  <footer role="status" aria-live="polite">{status}</footer>
</main>

<style>
  .detail-window {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 100dvh;
    padding: var(--space-4);
  }

  header,
  .alert {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
  }

  header > div {
    min-width: 0;
  }

  header h1 {
    overflow: hidden;
    font-size: var(--text-lg);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header p,
  footer {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  button {
    min-height: var(--control-height);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--surface-raised);
    color: var(--text);
  }

  .state {
    display: grid;
    place-content: center;
    gap: var(--space-3);
    flex: 1;
    text-align: center;
  }

  .alert {
    padding: var(--space-3);
    border: 1px solid var(--danger);
    border-radius: var(--radius-md);
  }

  footer {
    margin-block-start: auto;
  }
</style>
