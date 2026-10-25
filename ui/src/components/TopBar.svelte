<script lang="ts">
  import { emergency } from '../lib/commands.svelte';
  import { disconnect, drone } from '../lib/drone.svelte';
  import { duration, modelName, num } from '../lib/format';
  import type { IconName } from '../lib/icons';
  import { platform } from '../lib/platform.svelte';
  import { input } from '../input/mixer.svelte';
  import Icon from './Icon.svelte';

  let {
    onSettings,
    onLog,
    logOpen,
    onMenu,
    menuOpen = false,
  }: { onSettings: () => void; onLog: () => void; logOpen: boolean; onMenu?: () => void; menuOpen?: boolean } = $props();

  const t = $derived(drone.telemetry);
  const battery = $derived(t?.battery ?? null);
  const batteryIcon: IconName = $derived(
    battery === null
      ? 'battery'
      : battery > 60
        ? 'battery-full'
        : battery > 30
          ? 'battery-medium'
          : battery > 15
            ? 'battery-low'
            : 'battery-warning',
  );
  const batteryLevel = $derived(battery === null ? '' : battery <= 15 ? 'crit' : battery <= 30 ? 'low' : 'good');
  const sourceIcon: IconName = $derived(
    input.source === 'gamepad' ? 'gamepad-2' : input.source === 'keyboard' ? 'keyboard' : 'mouse',
  );
  const sourceLabel = $derived(
    input.source === 'gamepad'
      ? (input.gamepad ?? 'Controle')
      : input.source === 'keyboard'
        ? 'Teclado'
        : input.gamepad
          ? `Mouse · ${input.gamepad}`
          : 'Mouse',
  );
  const protocolLabel = $derived(drone.caps?.protocol === 'app' ? 'App' : `SDK ${drone.caps?.sdk_version ?? ''}`);
</script>

<header class="bar" class:compact={platform.compact}>
  <div class="brand">
    <span class="mark" aria-hidden="true"></span>
    <span class="name">Tello Pilot</span>
  </div>

  <div class="status" class:lost={drone.linkLost}>
    <span class="dot"></span>
    <span>{drone.reconnecting ? 'Reconectando…' : drone.linkLost ? 'Sinal perdido' : 'Conectado'}</span>
    <span class="sep"></span>
    <span class="muted">{modelName(drone.caps?.model ?? 'unknown')} · {protocolLabel}</span>
  </div>

  <div class="chips">
    <span class="chip {batteryLevel}" title="Bateria">
      <Icon name={batteryIcon} size={18} />
      <span class="num">{num(battery)}%</span>
    </span>
    <span class="chip" title="Sinal Wi-Fi">
      <Icon name="wifi" size={16} />
      <span class="num">{num(t?.wifi_strength)}</span>
    </span>
    <span class="chip" title="Tempo de motor">
      <Icon name="timer" size={16} />
      <span class="num">{duration(t?.flight_time_s)}</span>
    </span>
    {#if t?.temp_high_c != null}
      <span class="chip" title="Temperatura">
        <Icon name="thermometer" size={16} />
        <span class="num">{num(t.temp_high_c)}°</span>
      </span>
    {/if}
  </div>

  <div class="spacer"></div>

  <span class="chip source" title="Entrada ativa">
    <Icon name={sourceIcon} size={16} />
    <span class="source-name">{sourceLabel}</span>
  </span>

  {#if platform.compact && onMenu}
    <button class="icon-btn" class:active={menuOpen} title="Comandos" aria-label="Comandos" onclick={onMenu}>
      <Icon name="menu" />
    </button>
  {/if}
  <button class="icon-btn" class:active={logOpen} title="Log de comunicação" onclick={onLog}>
    <Icon name="bug" />
  </button>
  <button class="icon-btn" title="Configurações" onclick={onSettings}>
    <Icon name="settings" />
  </button>
  <button class="icon-btn" title="Desconectar" onclick={() => disconnect()}>
    <Icon name="unplug" />
  </button>
  <button class="btn danger emergency" title="Corta os motores (Esc duas vezes · View + Menu)" onclick={emergency}>
    <Icon name="octagon-alert" />
    <span>Emergência</span>
  </button>
</header>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: calc(56px + var(--safe-top));
    padding: var(--safe-top) calc(12px + var(--safe-right)) 0 calc(16px + var(--safe-left));
    border-bottom: 1px solid var(--border);
    background: var(--bg-2);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-right: 6px;
  }

  .mark {
    width: 22px;
    height: 22px;
    border-radius: 7px;
    background:
      radial-gradient(circle at 30% 30%, #a5f3fc, transparent 55%),
      linear-gradient(135deg, var(--accent), #3b82f6);
    box-shadow: 0 0 16px rgb(45 212 238 / 0.45);
  }

  .name {
    font: 700 15px var(--font-display);
    letter-spacing: -0.01em;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    border: 1px solid rgb(52 211 153 / 0.3);
    border-radius: 999px;
    background: rgb(52 211 153 / 0.08);
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
  }

  .status .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 8px var(--ok);
  }

  .status.lost {
    border-color: rgb(251 191 36 / 0.4);
    background: rgb(251 191 36 / 0.1);
  }

  .status.lost .dot {
    background: var(--warn);
    box-shadow: 0 0 8px var(--warn);
    animation: blink 1s steps(2) infinite;
  }

  .sep {
    width: 1px;
    height: 14px;
    background: var(--border-strong);
  }

  .muted {
    color: var(--text-2);
    font-weight: 500;
  }

  .chips {
    display: flex;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-2);
    font-weight: 600;
    white-space: nowrap;
  }

  .chip .num {
    color: var(--text);
  }

  .chip.good :global(svg) {
    color: var(--ok);
  }

  .chip.low :global(svg),
  .chip.low .num {
    color: var(--warn);
  }

  .chip.crit {
    background: rgb(244 63 94 / 0.15);
  }

  .chip.crit :global(svg),
  .chip.crit .num {
    color: var(--danger);
  }

  .spacer {
    flex: 1;
  }

  .source {
    max-width: 260px;
  }

  .source-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .emergency {
    margin-left: 4px;
    letter-spacing: 0.02em;
    box-shadow: 0 0 0 0 rgb(244 63 94 / 0.5);
  }

  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }

  @media (max-width: 1180px) {
    .chips .chip:nth-child(n + 3),
    .source {
      display: none;
    }
  }

  /* Celular: só o essencial, e emergência só com o ícone. */
  .bar.compact {
    gap: 6px;
    height: calc(48px + var(--safe-top));
    padding-left: calc(10px + var(--safe-left));
    padding-right: calc(8px + var(--safe-right));
  }

  .compact .brand {
    margin-right: 0;
  }

  .compact .name,
  .compact .status .sep,
  .compact .status .muted,
  .compact .source,
  .compact .emergency span {
    display: none;
  }

  .compact .status {
    padding: 4px 10px;
    font-size: 12px;
  }

  .compact .chips .chip:nth-child(-n + 3) {
    display: inline-flex;
  }

  .compact .chips .chip:nth-child(n + 4) {
    display: none;
  }

  .compact .chip {
    height: 30px;
    padding: 0 8px;
    gap: 4px;
    font-size: 13px;
  }

  .compact .emergency {
    min-height: 36px;
    padding: 0 10px;
  }

  @media (max-width: 560px) {
    .compact .status span:not(.dot),
    .compact .chips .chip:nth-child(n + 2) {
      display: none;
    }
  }
</style>
