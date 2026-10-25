<script lang="ts">
  import { onMount, untrack } from 'svelte';

  import ConnectScreen from './components/ConnectScreen.svelte';
  import DebugLog from './components/DebugLog.svelte';
  import FlightView from './components/FlightView.svelte';
  import Icon from './components/Icon.svelte';
  import SettingsDrawer from './components/SettingsDrawer.svelte';
  import Toasts from './components/Toasts.svelte';
  import {
    beginTakeoffHold,
    emergency,
    endTakeoffHold,
    hover,
    land,
    photo,
    toggleRecording,
  } from './lib/commands.svelte';
  import { drone, initDrone, landAndQuit } from './lib/drone.svelte';
  import { startPlatform } from './lib/platform.svelte';
  import { settings } from './lib/settings.svelte';
  import { keepScreenOn } from './lib/wakelock';
  import { startGamepad } from './input/gamepad';
  import { startKeyboard, type Shortcut } from './input/keyboard';
  import { startMixer } from './input/mixer.svelte';
  import { toggleFollow } from './ai/autopilot.svelte';
  import { startAi, stopAi } from './ai/engine.svelte';

  let settingsOpen = $state(false);
  let logOpen = $state(false);

  function onShortcut(s: Shortcut, pressed: boolean) {
    if (drone.conn !== 'connected') return;
    if (s === 'takeoff') return pressed ? beginTakeoffHold() : endTakeoffHold();
    if (!pressed) return;
    if (s === 'land') land();
    if (s === 'stop') hover();
    if (s === 'photo') photo();
    if (s === 'record') void toggleRecording();
    if (s === 'emergency') emergency();
    if (s === 'follow' && settings.aiEnabled) toggleFollow();
  }

  onMount(() => {
    const stops: (() => void)[] = [startKeyboard(onShortcut), startGamepad()];
    void initDrone().then((b) => {
      stops.push(startPlatform(b));
      stops.push(startMixer((s) => b.setSticks(s), () => drone.conn === 'connected'));
    });
    return () => stops.forEach((s) => s());
  });

  $effect(() => {
    if (drone.conn === 'connected') return keepScreenOn();
  });

  // O modelo carrega ao ligar a IA (não a cada conexão) e só analisa enquanto o vídeo chega.
  // Só depende do ajuste: o estado do motor muda dentro de startAi/stopAi.
  $effect(() => {
    if (!settings.aiEnabled) return;
    untrack(startAi);
    return () => untrack(stopAi);
  });
</script>

{#if drone.conn === 'connected'}
  <FlightView onSettings={() => (settingsOpen = true)} onLog={() => (logOpen = !logOpen)} {logOpen} />
{:else}
  <ConnectScreen onOpenLog={() => (logOpen = true)} />
{/if}

<SettingsDrawer bind:open={settingsOpen} />
<DebugLog bind:open={logOpen} />
<Toasts />

{#if drone.confirmClose}
  <div class="modal-scrim">
    <div class="modal" role="alertdialog" aria-labelledby="close-title">
      <Icon name="triangle-alert" size={26} />
      <h2 id="close-title">O drone está voando</h2>
      <p>Se o app fechar agora, o drone fica sem comandos e pousa sozinho depois de alguns segundos.</p>
      <div class="actions">
        <button class="btn ghost" onclick={() => (drone.confirmClose = false)}>Continuar voando</button>
        <button class="btn primary" onclick={landAndQuit}><Icon name="plane-landing" size={16} /> Pousar e sair</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-scrim {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.55);
  }

  .modal {
    width: min(420px, calc(100vw - 32px));
    padding: 24px;
    border: 1px solid var(--border);
    border-radius: 16px;
    background: var(--surface);
    color: var(--warn);
    box-shadow: var(--shadow);
  }

  h2 {
    margin: 10px 0 6px;
    color: var(--text);
    font: 700 18px var(--font-display);
  }

  p {
    margin: 0 0 18px;
    color: var(--text-2);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
