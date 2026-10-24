<script lang="ts">
  import { onMount } from 'svelte';

  import { setSnapshotProvider } from '../lib/commands.svelte';
  import { drone, initDrone, startVideo } from '../lib/drone.svelte';
  import { platform } from '../lib/platform.svelte';
  import { H264Player, type PlayerStats } from '../video/player';
  import Icon from './Icon.svelte';

  let { stats = $bindable(null) }: { stats?: PlayerStats | null } = $props();

  let canvas: HTMLCanvasElement;
  let player: H264Player | null = null;
  const supported = H264Player.supported();
  const hasFrames = $derived((stats?.decoded ?? 0) > 0);

  const codecHint = $derived(
    (
      {
        windows: 'No Windows isso não deve acontecer: atualize o Microsoft Edge WebView2.',
        linux: 'No Linux, instale os plugins do GStreamer (gstreamer1.0-libav e gstreamer1.0-plugins-bad).',
        android: 'Atualize o "Android System WebView" pela Play Store.',
        ios: 'O vídeo precisa do iOS 16.4 ou mais novo.',
        macos: 'O vídeo precisa do macOS 13 ou mais novo.',
      } as Record<string, string>
    )[platform.os] ?? 'Use um navegador com WebCodecs (Chrome, Edge ou Safari 16.4+).',
  );
  const blockedHint = $derived(
    platform.os === 'windows'
      ? 'Nenhum pacote chegou. Verifique o Firewall do Windows (redes públicas).'
      : `Nenhum pacote chegou. Confira se o ${platform.mobile ? 'celular' : 'computador'} está no Wi-Fi do drone e se a porta UDP 11111 está liberada.`,
  );

  onMount(() => {
    player = new H264Player(canvas, (s) => (stats = s));
    let off = () => {};
    void initDrone().then((b) => {
      off = b.onVideo((p) => player?.push(p));
    });
    setSnapshotProvider(() => player!.snapshotPng());
    return () => {
      off();
      player?.close();
      player = null;
      setSnapshotProvider(null);
    };
  });

  $effect(() => {
    if (!drone.videoOn) {
      player?.reset();
      stats = null;
    }
  });
</script>

<div class="video">
  <canvas bind:this={canvas} class:visible={drone.videoOn && hasFrames} width="960" height="720"></canvas>

  {#if !supported || stats?.unsupported}
    <div class="placeholder">
      <Icon name="triangle-alert" size={34} stroke={1.5} />
      <p>Este sistema não decodifica o vídeo H.264 do drone{stats?.codec ? ` (${stats.codec})` : ''}.</p>
      <p class="hint">{codecHint}</p>
    </div>
  {:else if !drone.videoOn}
    <div class="placeholder">
      <Icon name="video" size={34} stroke={1.5} />
      <p>Vídeo desligado</p>
      <button class="btn" onclick={startVideo}><Icon name="video" size={16} /> Ligar vídeo</button>
    </div>
  {:else if !hasFrames}
    <div class="placeholder">
      <Icon name="refresh-ccw" size={30} stroke={1.5} class="spin" />
      <p>Aguardando o vídeo do drone…</p>
      {#if drone.noVideo}
        <p class="hint">{blockedHint}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .video {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background:
      radial-gradient(ellipse at center, #111a26 0%, #06090d 75%),
      #06090d;
  }

  canvas {
    width: 100%;
    height: 100%;
    object-fit: contain;
    opacity: 0;
    transition: opacity 0.3s;
  }

  canvas.visible {
    opacity: 1;
  }

  .placeholder {
    position: absolute;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    max-width: 420px;
    padding: 20px 24px;
    border-radius: var(--radius);
    background: rgb(8 11 16 / 0.82);
    color: var(--muted);
    text-align: center;
  }

  .placeholder p {
    margin: 0;
    font-weight: 600;
  }

  .hint {
    max-width: 360px;
    color: var(--warn);
    font-size: 13px;
  }
</style>
