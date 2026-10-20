<script lang="ts">
  import { canFlip, flip } from '../lib/commands.svelte';
  import type { FlipDir } from '../lib/types';

  const cells: (FlipDir | null)[] = [
    'forward_left',
    'forward',
    'forward_right',
    'left',
    null,
    'right',
    'back_left',
    'back',
    'back_right',
  ];

  const ARROW: Record<FlipDir, number> = {
    forward: 0,
    forward_right: 45,
    right: 90,
    back_right: 135,
    back: 180,
    back_left: 225,
    left: 270,
    forward_left: 315,
  };

  const NAMES: Record<FlipDir, string> = {
    forward: 'frente',
    back: 'trás',
    left: 'esquerda',
    right: 'direita',
    forward_left: 'frente-esquerda',
    forward_right: 'frente-direita',
    back_left: 'trás-esquerda',
    back_right: 'trás-direita',
  };

  const general = $derived(canFlip());
</script>

<div class="pad">
  {#each cells as dir, i (i)}
    {#if dir}
      {@const check = canFlip(dir)}
      <button
        class="cell"
        class:diag={dir.includes('_')}
        disabled={!check.ok}
        title={check.ok ? `Flip para ${NAMES[dir]}` : check.reason}
        aria-label={`Flip para ${NAMES[dir]}`}
        onclick={() => flip(dir)}
      >
        <svg viewBox="0 0 24 24" width="18" height="18" style="transform: rotate({ARROW[dir]}deg)">
          <path d="M12 4 L18 12 H14 V20 H10 V12 H6 Z" fill="currentColor" />
        </svg>
      </button>
    {:else}
      <div class="center" title="LB + D-pad no controle">8D</div>
    {/if}
  {/each}
</div>
{#if !general.ok}
  <p class="hint">{general.reason}</p>
{/if}

<style>
  .pad {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }

  .cell,
  .center {
    display: grid;
    place-items: center;
    aspect-ratio: 1.6;
    border-radius: var(--radius-sm);
  }

  .cell {
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
    transition:
      background 0.15s,
      border-color 0.15s;
  }

  .cell.diag {
    color: var(--text-2);
  }

  .cell:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .cell:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .center {
    color: var(--muted);
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0.08em;
  }

  .hint {
    margin: 8px 0 0;
    color: var(--muted);
    font-size: 12px;
  }
</style>
