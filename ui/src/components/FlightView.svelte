<script lang="ts">
  import { drone } from '../lib/drone.svelte';
  import { settings } from '../lib/settings.svelte';
  import { report } from '../input/mixer.svelte';
  import { toDrone, type Axis2 } from '../input/mapping';
  import type { PlayerStats } from '../video/player';
  import Hud from './Hud.svelte';
  import Icon from './Icon.svelte';
  import Sidebar from './Sidebar.svelte';
  import TopBar from './TopBar.svelte';
  import VideoView from './VideoView.svelte';
  import VirtualStick from './VirtualStick.svelte';

  let { onSettings, onLog, logOpen }: { onSettings: () => void; onLog: () => void; logOpen: boolean } = $props();

  let stats: PlayerStats | null = $state(null);
  let left: Axis2 = $state({ x: 0, y: 0 });
  let right: Axis2 = $state({ x: 0, y: 0 });
  let firewallDismissed = $state(false);

  $effect(() => {
    report('mouse', toDrone(left, right, settings.stickMode, settings.invertPitch));
  });

  const leftHint = $derived(settings.stickMode === 2 ? 'Altura · Giro' : 'Frente · Giro');
  const rightHint = $derived(settings.stickMode === 2 ? 'Frente · Lados' : 'Altura · Lados');
</script>

<div class="flight">
  <TopBar {onSettings} {onLog} {logOpen} />
  <main class="stage">
    <VideoView bind:stats />
    <Hud {stats} />
    {#if settings.showSticks}
      <div class="sticks">
        <VirtualStick bind:value={left} label="Stick esquerdo" hint={leftHint} />
        <VirtualStick bind:value={right} label="Stick direito" hint={rightHint} />
      </div>
    {/if}
    {#if drone.noState && !firewallDismissed}
      <div class="firewall" role="alert">
        <Icon name="shield" size={20} />
        <div>
          <strong>A telemetria não está chegando</strong>
          <p>
            O drone aceita comandos, mas o Windows está bloqueando as respostas (UDP 8890/11111). Abra
            <b>Segurança do Windows → Firewall → Permitir um aplicativo</b> e marque o Tello Pilot em
            <b>Redes públicas</b>, ou reinstale o app como administrador.
          </p>
        </div>
        <button class="icon-btn" aria-label="Fechar" onclick={() => (firewallDismissed = true)}><Icon name="x" /></button>
      </div>
    {/if}
  </main>
  <Sidebar />
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
