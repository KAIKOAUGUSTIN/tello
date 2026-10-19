<script lang="ts">
  import {
    beginTakeoffHold,
    endTakeoffHold,
    hold,
    hover,
    land,
    palmLand,
    photo,
    throwAndGo,
    toggleFastMode,
    toggleVideo,
  } from '../lib/commands.svelte';
  import { applySafety, backendRef, drone, isFlying, run } from '../lib/drone.svelte';
  import { BEGINNER, saveSettings, settings } from '../lib/settings.svelte';
  import type { SmartVideo } from '../lib/types';
  import FlipPad from './FlipPad.svelte';
  import Icon from './Icon.svelte';

  const caps = $derived(drone.caps);
  const flying = $derived(isFlying());
  const busy = $derived(!!drone.busy);
  const appHint = 'Disponível no modo App';
  let bounce = $state(false);

  function holdKey(e: KeyboardEvent, start: boolean) {
    if (e.key !== 'Enter' && e.key !== ' ') return;
    e.preventDefault();
    if (start && !e.repeat) beginTakeoffHold();
    if (!start) endTakeoffHold();
  }

  function shot(mode: SmartVideo) {
    void run({ type: 'smart_video', mode });
  }

  async function toggleBounce() {
    if (await run({ type: 'bounce', on: !bounce }, null)) bounce = !bounce;
  }

  function toggleBeginner() {
    settings.beginner = !settings.beginner;
    saveSettings();
    applySafety();
  }
</script>

<aside class="side">
  <section>
    <h2 class="label">Voo</h2>
    {#if flying}
      <button class="btn big land" disabled={busy} onclick={land}>
        <Icon name="plane-landing" size={20} /> Pousar
      </button>
    {:else}
      <button
        class="btn big takeoff"
        disabled={busy}
        onpointerdown={beginTakeoffHold}
        onpointerup={endTakeoffHold}
        onpointerleave={endTakeoffHold}
        onkeydown={(e) => holdKey(e, true)}
        onkeyup={(e) => holdKey(e, false)}
        title="Segure por 1 segundo (T no teclado · Menu no controle)"
      >
        <span class="fill" style="width: {hold.takeoff * 100}%"></span>
        <span class="content"><Icon name="plane-takeoff" size={20} /> Segure para decolar</span>
      </button>
    {/if}
    <div class="row">
      <button class="btn" disabled={!flying} onclick={hover} title="Para e paira no lugar (Espaço · B)">
        <Icon name="pause" size={16} /> Pairar
      </button>
      {#if flying}
        <button class="btn" disabled={!caps?.palm_land || busy} title={caps?.palm_land ? 'RB no controle' : appHint} onclick={palmLand}>
          <Icon name="hand" size={16} /> Na palma
        </button>
      {:else}
        <button
          class="btn"
          disabled={!caps?.throw_and_go || busy}
          title={caps?.throw_and_go ? 'Lance o drone para decolar (RB no controle)' : appHint}
          onclick={throwAndGo}
        >
          <Icon name="rocket" size={16} /> Throw & Go
        </button>
      {/if}
    </div>
  </section>

  <section>
    <h2 class="label">Flips</h2>
    <FlipPad />
  </section>

  <section>
    <h2 class="label">EZ Shots {#if !caps?.smart_video}<span class="tag">App</span>{/if}</h2>
    <div class="grid3">
      <button class="btn tile" disabled={!caps?.smart_video || !flying} title={caps?.smart_video ? 'Gira 360°' : appHint} onclick={() => shot('spin360')}>
        <Icon name="rotate-cw" size={18} /> 360°
      </button>
      <button class="btn tile" disabled={!caps?.smart_video || !flying} title={caps?.smart_video ? 'Voa em círculo' : appHint} onclick={() => shot('circle')}>
        <Icon name="orbit" size={18} /> Círculo
      </button>
      <button class="btn tile" disabled={!caps?.smart_video || !flying} title={caps?.smart_video ? 'Sobe e se afasta' : appHint} onclick={() => shot('up_and_away')}>
        <Icon name="arrow-up-right" size={18} /> Up & Away
      </button>
    </div>
    <button class="toggle" disabled={!caps?.bounce} title={caps?.bounce ? 'Sobe e desce sozinho' : appHint} onclick={toggleBounce}>
      <span>Modo Bounce</span>
      <span class="switch" class:on={bounce}></span>
    </button>
  </section>

  <section>
    <h2 class="label">Câmera</h2>
    <div class="row">
      <button class="btn" onclick={photo} title="P no teclado · A no controle">
        <Icon name="camera" size={16} /> {caps?.hi_res_photo ? 'Foto 5 MP' : 'Capturar'}
      </button>
      <button class="btn" class:on={drone.videoOn} onclick={toggleVideo}>
        <Icon name="video" size={16} /> {drone.videoOn ? 'Vídeo on' : 'Vídeo off'}
      </button>
      <button class="btn icon" title="Abrir pasta de fotos" onclick={() => backendRef()?.openFolder('photos')}>
        <Icon name="folder-open" size={16} />
      </button>
    </div>
  </section>

  <section>
    <h2 class="label">Velocidade</h2>
    <button class="toggle" onclick={toggleBeginner}>
      <span>
        Modo iniciante
        <small>{settings.beginner ? `${BEGINNER.stickLimit * 100}% · até ${BEGINNER.maxHeightCm / 100} m` : `${settings.stickLimitPct}% · até ${settings.maxHeightM} m`}</small>
      </span>
      <span class="switch" class:on={settings.beginner}></span>
    </button>
    <button class="toggle" disabled={!caps?.fast_mode} title={caps?.fast_mode ? 'Y no controle' : appHint} onclick={toggleFastMode}>
      <span>Modo rápido <small>sticks mais sensíveis</small></span>
      <span class="switch" class:on={drone.telemetry?.fast_mode === true}></span>
    </button>
  </section>
</aside>

<style>
  .side {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 14px;
    border-left: 1px solid var(--border);
    background: var(--bg-2);
    overflow-y: auto;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 0 14px;
    border-bottom: 1px solid var(--border);
  }

  section:last-child {
    border-bottom: 0;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 2px;
  }

  .tag {
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--surface-3);
    color: var(--text-2);
    font-size: 10px;
    letter-spacing: 0.04em;
  }

  .big {
    position: relative;
    min-height: 52px;
    font-size: 15px;
    overflow: hidden;
  }

  .takeoff {
    border-color: rgb(45 212 238 / 0.5);
    background: var(--accent-soft);
    color: var(--accent);
    touch-action: none;
  }

  .takeoff .fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--accent);
    opacity: 0.35;
  }

  .takeoff .content {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .land {
    border-color: rgb(251 191 36 / 0.5);
    background: rgb(251 191 36 / 0.12);
    color: var(--warn);
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .row .btn {
    flex: 1;
    padding: 0 8px;
    font-size: 13px;
  }

  .row .btn.icon {
    flex: 0 0 38px;
  }

  .btn.on {
    border-color: rgb(52 211 153 / 0.45);
    color: var(--ok);
  }

  .grid3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }

  .tile {
    flex-direction: column;
    gap: 4px;
    min-height: 62px;
    padding: 6px 4px;
    font-size: 12px;
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 9px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    text-align: left;
    font-weight: 600;
    cursor: pointer;
  }

  .toggle:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .toggle small {
    display: block;
    color: var(--muted);
    font-size: 12px;
    font-weight: 500;
  }

  .switch {
    position: relative;
    flex: none;
    width: 36px;
    height: 20px;
    border-radius: 999px;
    background: var(--surface-3);
    transition: background 0.15s;
  }

  .switch::after {
    content: '';
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-2);
    transition: transform 0.15s;
  }

  .switch.on {
    background: var(--accent);
  }

  .switch.on::after {
    background: #032027;
    transform: translateX(16px);
  }
</style>
