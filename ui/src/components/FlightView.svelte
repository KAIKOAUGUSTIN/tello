<script lang="ts">
  import { drone } from '../lib/drone.svelte';
  import { platform } from '../lib/platform.svelte';
  import { settings } from '../lib/settings.svelte';
  import { report } from '../input/mixer.svelte';
  import { toDrone, type Axis2 } from '../input/mapping';
  import type { PlayerStats } from '../video/player';
  import AiOverlay from './AiOverlay.svelte';
  import Hud from './Hud.svelte';
  import Icon from './Icon.svelte';
  import QuickActions from './QuickActions.svelte';
  import Sidebar from './Sidebar.svelte';
  import TopBar from './TopBar.svelte';
  import VideoView from './VideoView.svelte';
  import VirtualStick from './VirtualStick.svelte';

  let { onSettings, onLog, logOpen }: { onSettings: () => void; onLog: () => void; logOpen: boolean } = $props();

  let stats: PlayerStats | null = $state(null);
  let left: Axis2 = $state({ x: 0, y: 0 });
  let right: Axis2 = $state({ x: 0, y: 0 });
  let firewallDismissed = $state(false);
  let menuOpen = $state(false);
  let stageHeight = $state(0);

  // No celular os sticks crescem com a tela, sem invadir os botões de cima.
  const stickSize = $derived(platform.compact ? Math.max(110, Math.min(170, Math.round(stageHeight * 0.4))) : 150);

  $effect(() => {
    report('mouse', toDrone(left, right, settings.stickMode, settings.invertPitch));
  });

  const leftHint = $derived(settings.stickMode === 2 ? 'Altura · Giro' : 'Frente · Giro');
  const rightHint = $derived(settings.stickMode === 2 ? 'Frente · Lados' : 'Altura · Lados');

  $effect(() => {
    if (!platform.compact) menuOpen = false;
  });
</script>

<div class="flight" class:compact={platform.compact}>
  <TopBar {onSettings} {onLog} {logOpen} onMenu={() => (menuOpen = !menuOpen)} {menuOpen} />
  <main class="stage" bind:clientHeight={stageHeight}>
    <VideoView bind:stats />
    {#if settings.aiEnabled && settings.aiOverlay && drone.videoOn}
      <AiOverlay {stats} />
    {/if}
    <Hud {stats} />
    {#if platform.compact}
      <QuickActions />
    {/if}
    {#if settings.showSticks}
      <div class="sticks">
        <VirtualStick bind:value={left} label="Stick esquerdo" hint={leftHint} size={stickSize} />
        <VirtualStick bind:value={right} label="Stick direito" hint={rightHint} size={stickSize} />
      </div>
    {/if}
    {#if drone.noState && !firewallDismissed}
      <div class="firewall" role="alert">
        <Icon name="shield" size={20} />
        <div>
          <strong>A telemetria não está chegando</strong>
          {#if platform.os === 'windows'}
            <p>
              O drone aceita comandos, mas o Windows está bloqueando as respostas (UDP 8890/11111). Abra
              <b>Segurança do Windows → Firewall → Permitir um aplicativo</b> e marque o Tello Pilot em
              <b>Redes públicas</b>, ou reinstale o app como administrador.
            </p>
          {:else if platform.mobile}
            <p>O drone aceita comandos, mas as respostas não chegam. Confira se o celular continua no Wi-Fi do drone.</p>
          {:else}
            <p>
              O drone aceita comandos, mas as respostas (UDP 8890/11111) não chegam. Libere essas portas no firewall do
              sistema.
            </p>
          {/if}
        </div>
        <button class="icon-btn" aria-label="Fechar" onclick={() => (firewallDismissed = true)}><Icon name="x" /></button>
      </div>
    {/if}
  </main>
  {#if platform.compact}
    {#if menuOpen}
      <div class="scrim" role="presentation" onclick={() => (menuOpen = false)}></div>
    {/if}
    <div class="slide" class:open={menuOpen} inert={!menuOpen}>
      <header>
        <h2>Comandos</h2>
        <button class="icon-btn" aria-label="Fechar" onclick={() => (menuOpen = false)}><Icon name="x" /></button>
      </header>
      <Sidebar />
    </div>
  {:else}
    <Sidebar />
  {/if}
</div>

<style>
  .flight {
    display: grid;
    grid-template-columns: 1fr 312px;
    grid-template-rows: auto 1fr;
    height: 100%;
  }

  .flight > :global(header) {
    grid-column: 1 / -1;
  }

  .flight.compact {
    grid-template-columns: 1fr;
  }

  .stage {
    position: relative;
    overflow: hidden;
    min-width: 0;
  }

  .sticks {
    position: absolute;
    left: 24px;
    right: 24px;
    bottom: 22px;
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    pointer-events: none;
  }

  .compact .sticks {
    left: calc(16px + var(--safe-left));
    right: calc(16px + var(--safe-right));
    bottom: calc(10px + var(--safe-bottom));
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: rgb(0 0 0 / 0.4);
  }

  /* Barra lateral como painel deslizante no celular. */
  .slide {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 21;
    display: flex;
    flex-direction: column;
    width: min(340px, 88vw);
    padding: var(--safe-top) var(--safe-right) var(--safe-bottom) 0;
    border-left: 1px solid var(--border);
    background: var(--bg-2);
    box-shadow: var(--shadow);
    transform: translateX(100%);
    visibility: hidden;
    transition:
      transform 0.2s ease-out,
      visibility 0s 0.2s;
  }

  .slide.open {
    transform: none;
    visibility: visible;
    transition: transform 0.2s ease-out;
  }

  .slide header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 8px 0 14px;
  }

  .slide h2 {
    margin: 0;
    font: 700 16px var(--font-display);
  }

  .slide :global(.side) {
    flex: 1;
    min-height: 0;
    border-left: 0;
  }

  .firewall {
    position: absolute;
    top: 64px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    gap: 12px;
    width: min(560px, 90%);
    padding: 14px 10px 14px 16px;
    border: 1px solid rgb(251 191 36 / 0.45);
    border-radius: var(--radius);
    background: rgb(26 21 8 / 0.94);
    color: var(--warn);
    box-shadow: var(--shadow);
  }

  .firewall strong {
    color: var(--text);
  }

  .firewall p {
    margin: 4px 0 0;
    color: var(--text-2);
    font-size: 13px;
  }

  .firewall b {
    color: var(--text);
  }
</style>
