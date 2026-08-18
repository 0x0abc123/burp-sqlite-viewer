<script lang="ts">
  import TrafficWorkspace from './lib/TrafficWorkspace.svelte'
  import InteractionDetailWindow from './lib/InteractionDetailWindow.svelte'

  const detailValue = new URLSearchParams(window.location.search).get('detail')
  const detailId = detailValue && /^\d+$/.test(detailValue) ? Number(detailValue) : null
</script>

{#if detailId !== null}
  <InteractionDetailWindow interactionId={detailId} />
{:else}
<a class="visually-hidden skip" href="#workspace">Skip to main content</a>

<div class="app">
  <main id="workspace" tabindex="-1">
    <TrafficWorkspace />
  </main>

  <footer>
    <p>Local desktop inspection · Original request and response bytes remain unchanged</p>
  </footer>
</div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-rows: 1fr auto;
    min-height: 100dvh;
    padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom)
      env(safe-area-inset-left);
  }

  footer {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }

  main {
    min-width: 0;
    padding: var(--space-4) var(--space-5);
  }

  footer {
    padding: var(--space-2) var(--space-5);
    border-top: 1px solid var(--border);
  }

  .skip:focus {
    position: fixed;
    inset-block-start: var(--space-3);
    inset-inline-start: var(--space-3);
    z-index: 10;
    padding: var(--space-2) var(--space-3);
    background: var(--surface-raised);
    border-radius: var(--radius-md);
  }
</style>
