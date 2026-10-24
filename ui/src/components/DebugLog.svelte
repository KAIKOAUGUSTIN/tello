<script lang="ts">
  import { tick } from 'svelte';

  import { initDrone } from '../lib/drone.svelte';
  import { errorText, stamp } from '../lib/format';
  import { platform } from '../lib/platform.svelte';
  import { toast } from '../lib/toasts.svelte';
  import type { LogEntry } from '../lib/types';
  import Icon from './Icon.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  const MAX = 3000;
  const SHOWN = 400;
  type Filter = 'all' | LogEntry['ch'];
  const FILTERS: [Filter, string][] = [
    ['all', 'Tudo'],
    ['cmd', 'Comandos'],
    ['state', 'Estado'],
    ['video', 'Vídeo'],
    ['app', 'App'],
    ['core', 'Núcleo'],
  ];

  let entries: LogEntry[] = $state([]);
  let filter: Filter = $state('all');
  let paused = $state(false);
  let list: HTMLDivElement | undefined = $state();

  const visible = $derived(
    (filter === 'all' ? entries : entries.filter((e) => e.ch === filter)).slice(-SHOWN),
  );

  $effect(() => {
    if (!open) return;
    let off = () => {};
    let alive = true;
    void initDrone().then(async (b) => {
      const snap = await b.logSnapshot();
      if (!alive) return;
      entries = snap.slice(-MAX);
      off = b.onLog((batch) => {
        if (paused) return;
        const next = entries.concat(batch);
        entries = next.length > MAX ? next.slice(-MAX) : next;
      });
    });
    return () => {
      alive = false;
      off();
    };
  });

  $effect(() => {
    void visible.length;
    if (!paused && list) void tick().then(() => list && (list.scrollTop = list.scrollHeight));
  });

  async function exportLog() {
    try {
      const b = await initDrone();
      const path = await b.exportLog(stamp());
      toast('ok', 'Log exportado', path);
      if (b.kind === 'tauri' && !platform.mobile) void b.openFolder('logs').catch(() => {});
    } catch (e) {
      toast('error', 'Falha ao exportar o log', errorText(e));
    }
  }

  function time(ms: number): string {
    return (ms / 1000).toFixed(3).padStart(8, ' ');
  }
</script>

{#if open}
  <div class="log" class:compact={platform.compact} role="region" aria-label="Log de comunicação">
    <header>
      <h2><Icon name="bug" size={16} /> Caixa-preta</h2>
      <div class="filters">
        {#each FILTERS as [id, name] (id)}
          <button class:sel={filter === id} onclick={() => (filter = id)}>{name}</button>
        {/each}
      </div>
      <div class="spacer"></div>
      <button class="btn ghost" onclick={() => (paused = !paused)}>
        <Icon name={paused ? 'circle-dot' : 'pause'} size={15} />
        {paused ? 'Continuar' : 'Pausar'}
      </button>
      <button class="btn" onclick={exportLog}><Icon name="download" size={15} /> Exportar log</button>
      <button class="icon-btn" aria-label="Fechar" onclick={() => (open = false)}><Icon name="x" /></button>
    </header>
    <div class="list" bind:this={list}>
      {#each visible as e, i (i)}
        <div class="entry {e.dir} {e.ch}">
          <span class="t num">{time(e.t_ms)}</span>
          <span class="dir">{e.dir === 'tx' ? '→' : e.dir === 'rx' ? '←' : '•'}</span>
          <span class="ch">{e.ch}</span>
          <span class="text">{e.text}{#if e.hex}<span class="hex">{e.hex}</span>{/if}</span>
        </div>
      {:else}
        <p class="empty">Nenhuma mensagem ainda.</p>
      {/each}
    </div>
  </div>
{/if}

<style>
  .log {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 30;
    display: flex;
    flex-direction: column;
    height: min(42vh, 420px);
    border-top: 1px solid var(--border-strong);
    background: rgb(8 11 16 / 0.97);
    box-shadow: 0 -10px 30px rgb(0 0 0 / 0.4);
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 14px;
  }

  .filters {
    display: flex;
    gap: 4px;
    margin-left: 10px;
  }

  .filters button {
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .filters button.sel {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .spacer {
    flex: 1;
  }

  .list {
    flex: 1;
    overflow: auto;
    padding: 6px 12px;
    font: 12px/1.6 var(--mono);
    user-select: text;
  }

  .entry {
    display: grid;
    grid-template-columns: 72px 16px 46px 1fr;
    gap: 8px;
    color: var(--text-2);
  }

  .t {
    color: var(--muted);
    white-space: pre;
  }

  .tx .dir {
    color: var(--accent);
  }

  .rx .dir {
    color: var(--ok);
  }

  .info .dir,
  .info .text {
    color: var(--warn);
  }

  .ch {
    color: var(--muted);
  }

  .text {
    word-break: break-all;
  }

  .hex {
    display: block;
    color: var(--muted);
  }

  .empty {
    color: var(--muted);
  }

  /* Celular: ocupa a tela toda e os filtros rolam de lado. */
  .log.compact {
    top: 0;
    height: auto;
    z-index: 42;
    padding: var(--safe-top) var(--safe-right) var(--safe-bottom) var(--safe-left);
  }

  .compact header {
    flex-wrap: wrap;
  }

  .compact .filters {
    order: 5;
    flex-basis: 100%;
    margin-left: 0;
    overflow-x: auto;
  }

  .compact .filters button {
    flex: none;
  }
</style>
