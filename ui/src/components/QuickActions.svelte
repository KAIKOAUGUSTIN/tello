<script lang="ts">
  // Botões grandes sobre o vídeo no layout compacto (celular): decolar/pousar e
  // pairar à esquerda, foto e gravação à direita, como no app oficial. O seguir
  // da IA fica ao lado do decolar/pousar, para não descer até os sticks.
  import { elapsed } from '../lib/clock.svelte';
  import { beginTakeoffHold, endTakeoffHold, hold, hover, land, photo, toggleRecording } from '../lib/commands.svelte';
  import { drone, isFlying } from '../lib/drone.svelte';
  import { settings } from '../lib/settings.svelte';
  import { autopilot, toggleFollow } from '../ai/autopilot.svelte';
  import Icon from './Icon.svelte';

  const flying = $derived(isFlying());
  const busy = $derived(!!drone.busy);
</script>

<div class="col left">
  {#if flying}
    <button class="act big land" disabled={busy} onclick={land} aria-label="Pousar">
      <Icon name="plane-landing" size={24} />
      <span>Pousar</span>
    </button>
  {:else}
    <button
      class="act big takeoff"
      style="--p: {hold.takeoff}"
      disabled={busy}
      onpointerdown={beginTakeoffHold}
      onpointerup={endTakeoffHold}
      onpointerleave={endTakeoffHold}
      onpointercancel={endTakeoffHold}
      aria-label="Segure para decolar"
    >
      <Icon name="plane-takeoff" size={24} />
      <span>Segure</span>
    </button>
  {/if}
  <button class="act" disabled={!flying} onclick={hover} aria-label="Pairar">
    <Icon name="pause" size={18} />
    <span>Pairar</span>
  </button>
  {#if settings.aiEnabled}
    <button
      class="act follow"
      class:on={autopilot.follow}
      disabled={!flying && !autopilot.follow}
      onclick={toggleFollow}
      aria-label={autopilot.follow ? 'Parar de seguir' : 'Seguir pessoa'}
    >
      <Icon name="person-standing" size={18} />
      <span>Seguir</span>
    </button>
  {/if}
</div>

<div class="col right">
  <button class="act big shutter" onclick={photo} aria-label="Foto">
    <Icon name="camera" size={24} />
    <span>Foto</span>
  </button>
  <button class="act rec" class:recording={!!drone.recording} onclick={toggleRecording} aria-label="Gravar vídeo">
    {#if drone.recording}
      <span class="rec-dot"></span>
      <span class="num">{elapsed(drone.recording.since)}</span>
    {:else}
      <Icon name="circle-dot" size={18} />
      <span>Gravar</span>
    {/if}
  </button>
</div>

<style>
  .col {
    position: absolute;
    top: 10px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .left {
    left: calc(12px + var(--safe-left));
    display: grid;
    grid-template-columns: 64px;
    grid-auto-columns: 52px;
    justify-items: center;
    align-items: center;
  }

  .left .follow {
    grid-area: 1 / 2;
  }

  .right {
    right: calc(12px + var(--safe-right));
  }

  .act {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    width: 52px;
    height: 52px;
    border: 1px solid rgb(255 255 255 / 0.16);
    border-radius: 50%;
    background: var(--glass);
    backdrop-filter: blur(8px);
    color: var(--hud);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    touch-action: none;
    cursor: pointer;
  }

  .act.big {
    width: 64px;
    height: 64px;
    font-size: 10px;
  }

  .act:disabled {
    opacity: 0.4;
  }

  .act:active:not(:disabled) {
    transform: scale(0.96);
  }

  /* Anel que enche enquanto o dedo segura o botão. */
  .takeoff {
    border-color: rgb(45 212 238 / 0.55);
    background:
      conic-gradient(rgb(45 212 238 / 0.45) calc(var(--p) * 1turn), transparent 0),
      var(--glass);
    color: var(--accent);
  }

  .land {
    border-color: rgb(251 191 36 / 0.55);
    color: var(--warn);
  }

  .follow.on {
    border-color: rgb(45 212 238 / 0.7);
    background: rgb(45 212 238 / 0.25);
    color: var(--accent);
  }

  .shutter {
    border-color: rgb(255 255 255 / 0.4);
  }

  .rec.recording {
    border-color: rgb(244 63 94 / 0.6);
    background: rgb(244 63 94 / 0.22);
    color: var(--text);
  }

  .rec-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--danger);
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
</style>
