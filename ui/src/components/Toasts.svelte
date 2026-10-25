<script lang="ts">
  import type { IconName } from '../lib/icons';
  import { platform } from '../lib/platform.svelte';
  import { dismiss, toasts } from '../lib/toasts.svelte';
  import Icon from './Icon.svelte';

  const ICON: Record<string, IconName> = { info: 'info', ok: 'circle-check', warn: 'triangle-alert', error: 'octagon-alert' };
</script>

<div class="stack" class:compact={platform.compact} aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}" role="status">
      <Icon name={ICON[t.kind]} size={18} />
      <div class="text">
        <strong>{t.title}</strong>
        {#if t.body}<span>{t.body}</span>{/if}
      </div>
      <button class="icon-btn close" aria-label="Fechar" onclick={() => dismiss(t.id)}><Icon name="x" size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(380px, calc(100vw - 32px));
    pointer-events: none;
  }

  /* Celular: no topo, longe dos sticks (um toast em cima do stick roubaria o toque). */
  .stack.compact {
    top: calc(54px + var(--safe-top));
    right: auto;
    bottom: auto;
    left: 50%;
    transform: translateX(-50%);
    width: min(360px, calc(100vw - 180px));
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 8px 12px 14px;
    border: 1px solid var(--border);
    border-left-width: 3px;
    border-radius: var(--radius);
    background: rgb(18 25 36 / 0.96);
    backdrop-filter: blur(10px);
    box-shadow: var(--shadow);
    pointer-events: auto;
    animation: pop 0.18s ease-out;
  }

  @keyframes pop {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
  }

  .toast.info {
    border-left-color: var(--accent);
    color: var(--accent);
  }

  .toast.ok {
    border-left-color: var(--ok);
    color: var(--ok);
  }

  .toast.warn {
    border-left-color: var(--warn);
    color: var(--warn);
  }

  .toast.error {
    border-left-color: var(--danger);
    color: var(--danger);
  }

  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  strong {
    color: var(--text);
  }

  span {
    color: var(--text-2);
    font-size: 13px;
    word-break: break-word;
    user-select: text;
  }

  .close {
    width: 26px;
    height: 26px;
  }
</style>
