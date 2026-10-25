<script lang="ts">
  import { elapsed } from '../lib/clock.svelte';
  import { hold } from '../lib/commands.svelte';
  import { CONDITIONS } from '../lib/conditions';
  import { drone, isFlying } from '../lib/drone.svelte';
  import { groundSpeed, meters, num } from '../lib/format';
  import { platform } from '../lib/platform.svelte';
  import { safetyFromSettings, settings } from '../lib/settings.svelte';
  import { input } from '../input/mixer.svelte';
  import type { PlayerStats } from '../video/player';
  import Icon from './Icon.svelte';

  let { stats }: { stats: PlayerStats | null } = $props();

  const t = $derived(drone.telemetry);
  const roll = $derived(t?.roll_deg ?? 0);
  const pitch = $derived(t?.pitch_deg ?? 0);
  const yaw = $derived(t?.yaw_deg ?? 0);
  const flying = $derived(isFlying());
  const maxHeight = $derived(safetyFromSettings(settings).max_height_cm ?? 1000);
  const heightPct = $derived(Math.max(0, Math.min(100, ((t?.height_cm ?? 0) / maxHeight) * 100)));
  const climb = $derived(t?.speed_z === null || t?.speed_z === undefined ? null : -t.speed_z / 10);

  // Fita de rumo: marcas a cada 15° ao redor do rumo atual.
  const ticks = $derived.by(() => {
    const out: { x: number; deg: number; major: boolean }[] = [];
    const start = Math.floor((yaw - 90) / 15) * 15;
    for (let d = start; d <= yaw + 90; d += 15) {
      out.push({ x: (d - yaw) * 2, deg: ((d % 360) + 360) % 360, major: d % 45 === 0 });
    }
    return out;
  });

  const stateLabel = $derived(drone.busy ?? (flying ? 'Voando' : 'No chão'));
</script>

<div class="hud" class:compact={platform.compact} aria-hidden="true">
  <div class="top">
    <span class="pill" class:air={flying} class:busy={!!drone.busy}>
      {#if drone.busy}<Icon name="refresh-ccw" size={14} class="spin" />{/if}
      {stateLabel}
    </span>
    {#if drone.recording}
      <span class="banner rec"><span class="dot"></span> REC <span class="num">{elapsed(drone.recording.since)}</span></span>
    {/if}
    {#if !input.focused}
      <span class="banner warn">
        <Icon name="pause" size={14} />
        {platform.mobile ? 'App em segundo plano' : 'Janela sem foco'}: sticks no centro, drone pairando
      </span>
    {/if}
    {#if drone.linkLost}
      <span class="banner danger"><Icon name="wifi-off" size={14} /> Sinal do drone perdido</span>
    {/if}
    {#each drone.conditions as c (c)}
      <span class="banner warn"><Icon name={CONDITIONS[c].icon} size={14} /> {CONDITIONS[c].label}</span>
    {/each}
    {#if settings.beginner}
      <span class="banner info"><Icon name="shield" size={14} /> Modo iniciante</span>
    {/if}
  </div>

  <!-- Horizonte artificial -->
  <svg class="horizon" viewBox="-150 -100 300 200">
    <g transform="rotate({-roll}) translate(0 {pitch * 3})">
      <line x1="-140" x2="-40" y1="0" y2="0" />
      <line x1="40" x2="140" y1="0" y2="0" />
      {#each [-20, -10, 10, 20] as p (p)}
        <line class="minor" x1="-18" x2="18" y1={p * 3} y2={p * 3} />
      {/each}
    </g>
    <path class="aircraft" d="M -34 0 H -12 L 0 10 L 12 0 H 34" />
    <circle class="aircraft" r="2.5" />
  </svg>

  <!-- No desktop os painéis ficam nas laterais; no celular, numa faixa em baixo. -->
  <div class="metrics">
    <div class="panel left">
      <div class="metric">
        <span class="k">VEL</span>
        <span class="v num">{groundSpeed(t?.speed_x ?? null, t?.speed_y ?? null)}</span>
        <span class="u">m/s</span>
      </div>
      <div class="metric small">
        <span class="k">SUBIDA</span>
        <span class="v num">{num(climb, 1)}</span>
        <span class="u">m/s</span>
      </div>
    </div>

    <div class="panel right">
      <div class="alt-bar"><div class="alt-fill" style="height: {heightPct}%"></div></div>
      <div class="alt-values">
        <div class="metric">
          <span class="k">ALT</span>
          <span class="v num">{meters(t?.height_cm)}</span>
          <span class="u">m</span>
        </div>
        <div class="metric small">
          <span class="k">TOF</span>
          <span class="v num">{meters(t?.tof_cm)}</span>
          <span class="u">m</span>
        </div>
        <div class="metric small">
          <span class="k">MÁX</span>
          <span class="v num">{(maxHeight / 100).toFixed(0)}</span>
          <span class="u">m</span>
        </div>
      </div>
    </div>
  </div>

  <div class="heading">
    <svg viewBox="-180 0 360 34">
      {#each ticks as tk (tk.x)}
        <line x1={tk.x} x2={tk.x} y1={tk.major ? 4 : 9} y2="16" class:major={tk.major} />
        {#if tk.major}<text x={tk.x} y="30">{tk.deg}</text>{/if}
      {/each}
      <path class="caret" d="M -6 0 L 6 0 L 0 7 Z" />
    </svg>
    <span class="yaw num">{num(yaw)}°</span>
  </div>

  {#if hold.takeoff > 0}
    <div class="hold">
      <svg viewBox="0 0 100 100" width="120" height="120">
        <circle cx="50" cy="50" r="44" class="track" />
        <circle cx="50" cy="50" r="44" class="fill" style="stroke-dashoffset: {276.5 * (1 - hold.takeoff)}" />
      </svg>
      <span>Segure para decolar</span>
    </div>
  {/if}

  {#if stats && stats.decoded > 0}
    <div class="vstats num">{stats.width}×{stats.height} · {stats.fps} fps</div>
  {/if}
</div>

<style>
  .hud {
    position: absolute;
    inset: 0;
    pointer-events: none;
    color: var(--hud);
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.6);
  }

  .top {
    position: absolute;
    top: 14px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    width: max-content;
    max-width: 90%;
  }

  .pill,
  .banner {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    background: var(--glass);
    backdrop-filter: blur(8px);
    font-size: 13px;
    font-weight: 700;
    text-shadow: none;
  }

  .pill.air {
    color: var(--ok);
  }

  .pill.busy {
    color: var(--accent);
  }

  .banner.warn {
    color: var(--warn);
  }

  .banner.danger {
    color: var(--danger);
  }

  .banner.rec {
    color: var(--danger);
  }

  .banner.rec .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--danger);
    animation: blink 1.2s ease-in-out infinite;
  }

  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }

  .banner.info {
    color: var(--text-2);
    font-weight: 600;
  }

  .horizon {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 300px;
    height: 200px;
    transform: translate(-50%, -50%);
    overflow: visible;
  }

  .horizon line {
    stroke: var(--hud-dim);
    stroke-width: 2;
  }

  .horizon line.minor {
    stroke-width: 1.2;
    opacity: 0.6;
  }

  .aircraft {
    fill: none;
    stroke: var(--accent);
    stroke-width: 3;
    stroke-linejoin: round;
    filter: drop-shadow(0 0 3px rgb(0 0 0 / 0.6));
  }

  circle.aircraft {
    fill: var(--accent);
  }

  .panel {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--glass);
    backdrop-filter: blur(8px);
  }

  .panel.left {
    left: 18px;
    flex-direction: column;
  }

  .panel.right {
    right: 18px;
  }

  .metric {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: 4px;
  }

  .metric .k {
    flex-basis: 100%;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: var(--hud-dim);
  }

  .metric .v {
    font: 700 26px/1 var(--font-display);
  }

  .metric .u {
    font-size: 12px;
    color: var(--hud-dim);
  }

  .metric.small .v {
    font-size: 17px;
  }

  .alt-values {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .alt-bar {
    position: relative;
    width: 6px;
    border-radius: 3px;
    background: rgb(255 255 255 / 0.12);
    overflow: hidden;
  }

  .alt-fill {
    position: absolute;
    bottom: 0;
    width: 100%;
    background: linear-gradient(0deg, var(--accent), #a5f3fc);
    transition: height 0.2s;
  }

  .heading {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 6px 12px 8px;
    border-radius: var(--radius);
    background: var(--glass);
    backdrop-filter: blur(8px);
  }

  .heading svg {
    width: 300px;
    height: 30px;
    overflow: hidden;
  }

  .heading line {
    stroke: var(--hud-dim);
    stroke-width: 1.2;
  }

  .heading line.major {
    stroke: var(--hud);
    stroke-width: 1.6;
  }

  .heading text {
    fill: var(--hud-dim);
    font-size: 10px;
    font-weight: 600;
    text-anchor: middle;
  }

  .caret {
    fill: var(--accent);
  }

  .yaw {
    font-weight: 700;
    font-size: 13px;
  }

  .hold {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    font-weight: 700;
  }

  .hold .track {
    fill: rgb(10 14 20 / 0.5);
    stroke: rgb(255 255 255 / 0.15);
    stroke-width: 6;
  }

  .hold .fill {
    fill: none;
    stroke: var(--accent);
    stroke-width: 6;
    stroke-linecap: round;
    stroke-dasharray: 276.5;
    transform: rotate(-90deg);
    transform-origin: 50% 50%;
  }

  .vstats {
    position: absolute;
    right: 18px;
    top: 18px;
    font-size: 11px;
    color: var(--hud-dim);
  }

  .metrics {
    display: contents;
  }

  /* Celular: menos ruído entre os sticks e os botões. */
  .compact .top {
    top: 10px;
    max-width: calc(100% - 180px);
    gap: 6px;
  }

  .compact .pill,
  .compact .banner {
    padding: 4px 10px;
    font-size: 12px;
  }

  .compact .horizon {
    width: 180px;
    height: 120px;
  }

  .compact .metrics {
    position: absolute;
    left: 50%;
    bottom: calc(70px + var(--safe-bottom));
    transform: translateX(-50%);
    display: flex;
    gap: 8px;
  }

  .compact .panel {
    position: static;
    transform: none;
    padding: 6px 12px;
  }

  .compact .alt-bar,
  .compact .metric.small {
    display: none;
  }

  .compact .metric .v {
    font-size: 20px;
  }

  .compact .heading {
    bottom: calc(8px + var(--safe-bottom));
    padding: 4px 10px 6px;
  }

  .compact .heading svg {
    width: 220px;
    height: 26px;
  }

  .compact .vstats {
    display: none;
  }
</style>
