<script lang="ts">
  import {
    applySafety,
    applyVideo,
    calibrateCog,
    COG_MAX_HEIGHT_CM,
    drone,
    isFlying,
    run,
  } from '../lib/drone.svelte';
  import { mbps } from '../lib/format';
  import { platform } from '../lib/platform.svelte';
  import { resetSettings, saveSettings, settings } from '../lib/settings.svelte';
  import { input } from '../input/mixer.svelte';
  import Icon from './Icon.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();
  // O WebView do Android não tem a Gamepad API; no iOS o controle funciona.
  const noGamepad = $derived(platform.os === 'android' && !input.gamepad);

  function changed() {
    saveSettings();
    applySafety();
  }

  // Só o modo App informa a perda de pacotes, que o ajuste automático usa.
  const adaptive = $derived(settings.adaptiveVideo && drone.caps?.protocol === 'app');

  function videoChanged() {
    if (settings.adaptiveVideo && settings.videoLevel === 0) settings.videoLevel = 4;
    saveSettings();
    applyVideo();
  }

  const calibration = $derived(drone.calibration);
  const calibrating = $derived(calibration?.state === 'running');
  const flying = $derived(isFlying());

  function calibrate() {
    if (!confirm('Tire as hélices antes de começar: você vai girar o drone com as mãos. Continuar?')) return;
    void run({ type: 'calibrate_imu' }, null);
  }

  // Calibração do centro de massa: o núcleo confere de novo antes de enviar.
  const height = $derived(drone.telemetry?.height_cm ?? null);
  const cogBlocker = $derived(
    !flying
      ? 'Decole para calibrar'
      : height === null || height > COG_MAX_HEIGHT_CM
        ? 'Desça para menos de 1,5 m'
        : drone.busy
          ? 'Aguarde'
          : null,
  );

  const bars = $derived([
    ['Giro', input.sticks.yaw],
    ['Altura', input.sticks.throttle],
    ['Lados', input.sticks.roll],
    ['Frente', input.sticks.pitch],
  ] as const);
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={() => (open = false)}></div>
  <div class="drawer" class:compact={platform.compact} role="dialog" aria-label="Configurações">
    <header>
      <h2>Configurações</h2>
      <button class="icon-btn" aria-label="Fechar" onclick={() => (open = false)}><Icon name="x" /></button>
    </header>

    <div class="body">
      <section>
        <h3 class="label"><Icon name="shield" size={14} /> Segurança</h3>
        <label class="row">
          <span>Modo iniciante <small>sticks a 40% e altura máxima de 3 m</small></span>
          <input type="checkbox" bind:checked={settings.beginner} onchange={changed} />
        </label>
        <label class="slider" class:disabled={settings.beginner}>
          <span>Limite dos sticks <b class="num">{settings.stickLimitPct}%</b></span>
          <input type="range" min="20" max="100" step="5" bind:value={settings.stickLimitPct} disabled={settings.beginner} onchange={changed} />
        </label>
        <label class="slider" class:disabled={settings.beginner}>
          <span>Altura máxima <b class="num">{settings.maxHeightM} m</b></span>
          <input type="range" min="1" max="30" step="1" bind:value={settings.maxHeightM} disabled={settings.beginner} onchange={changed} />
        </label>
        <label class="slider">
          <span>Pouso automático com bateria em <b class="num">{settings.autoLandBattery > 0 ? `${settings.autoLandBattery}%` : 'desligado'}</b></span>
          <input type="range" min="0" max="30" step="1" bind:value={settings.autoLandBattery} onchange={changed} />
        </label>
      </section>

      <section>
        <h3 class="label"><Icon name="gamepad-2" size={14} /> Controles</h3>
        <div class="row">
          <span>Modo dos sticks <small>{settings.stickMode === 2 ? 'esquerdo: altura e giro' : 'esquerdo: frente e giro'}</small></span>
          <div class="seg">
            {#each [1, 2] as m (m)}
              <button class:sel={settings.stickMode === m} onclick={() => ((settings.stickMode = m as 1 | 2), changed())}>Modo {m}</button>
            {/each}
          </div>
        </div>
        <label class="slider">
          <span>Zona morta <b class="num">{Math.round(settings.deadzone * 100)}%</b></span>
          <input type="range" min="0" max="0.3" step="0.01" bind:value={settings.deadzone} onchange={changed} />
        </label>
        <label class="slider">
          <span>Expo (suavidade no centro) <b class="num">{Math.round(settings.expo * 100)}%</b></span>
          <input type="range" min="0" max="0.8" step="0.05" bind:value={settings.expo} onchange={changed} />
        </label>
        <label class="row">
          <span>Inverter frente/trás</span>
          <input type="checkbox" bind:checked={settings.invertPitch} onchange={changed} />
        </label>
        {#if !noGamepad}
          <label class="row">
            <span>Vibração do controle</span>
            <input type="checkbox" bind:checked={settings.rumble} onchange={changed} />
          </label>
        {/if}
        <label class="row">
          <span>Mostrar sticks na tela</span>
          <input type="checkbox" bind:checked={settings.showSticks} onchange={changed} />
        </label>
        <div class="monitor">
          <span class="muted">
            {input.gamepad
              ? `Controle: ${input.gamepad}`
              : noGamepad
                ? 'Controle Xbox ainda não funciona no Android: use os sticks na tela'
                : 'Nenhum controle detectado — aperte um botão do controle'}
          </span>
          {#each bars as [name, v] (name)}
            <div class="bar">
              <span>{name}</span>
              <div class="track"><div class="val" style="left: {50 + Math.min(0, v) * 50}%; width: {Math.abs(v) * 50}%"></div></div>
              <span class="num">{Math.round(v * 100)}</span>
            </div>
          {/each}
        </div>
      </section>

      <section>
        <h3 class="label"><Icon name="video" size={14} /> Vídeo</h3>
        <label class="row">
          <span>Ligar vídeo ao conectar</span>
          <input type="checkbox" bind:checked={settings.autoStartVideo} onchange={changed} />
        </label>
        {#if drone.caps?.video_bitrate}
          {#if drone.caps.protocol === 'app'}
            <label class="row">
              <span>Ajustar a qualidade sozinho <small>baixa a taxa quando o Wi-Fi perde pacotes</small></span>
              <input type="checkbox" bind:checked={settings.adaptiveVideo} onchange={videoChanged} />
            </label>
          {/if}
          <div class="row">
            <span>{adaptive ? 'Qualidade máxima' : 'Taxa de vídeo'}</span>
            <select bind:value={settings.videoLevel} onchange={videoChanged}>
              {#if !adaptive}<option value={0}>Automática (drone)</option>{/if}
              {#each [1, 2, 3, 4, 5] as level (level)}<option value={level}>{mbps(level)}</option>{/each}
            </select>
          </div>
        {/if}
        {#if drone.video}
          <span class="muted">
            Agora: {drone.video.fps} fps · {(drone.video.kbps / 1000).toLocaleString('pt-BR', { maximumFractionDigits: 1 })} Mbps
            {#if drone.video.loss_pct !== null}· perda {Math.round(drone.video.loss_pct)}%{/if}
            {#if drone.video.level !== null}· limite {mbps(drone.video.level)}{/if}
          </span>
        {/if}
      </section>

      {#if drone.caps?.imu_calibration}
        <section>
          <h3 class="label"><Icon name="rotate-3d" size={14} /> Calibração da IMU</h3>
          <span class="muted">Use se o drone deriva mesmo com boa luz e sem vento, ou depois de uma queda.</span>
          <ol class="steps">
            <li>Tire as hélices e deixe o drone ligado sobre uma superfície plana.</li>
            <li>Toque em <b>Calibrar</b>.</li>
            <li>
              Gire o drone pelas 6 posições: normal, de cabeça para baixo, de cada lado e com o nariz para cima e para
              baixo. Segure parado em cada uma até o contador avançar.
            </li>
            <li>No fim, desligue e ligue o drone.</li>
          </ol>
          {#if calibration}
            <div class="calib" class:done={calibration.state === 'done'} class:failed={calibration.state === 'timed_out'}>
              <div class="progress"><span style="width: {calibration.progress}%"></span></div>
              <span>
                {#if calibration.state === 'running'}
                  Posições: {calibration.steps}/6 · {calibration.progress}%
                {:else if calibration.state === 'done'}
                  Pronto. Desligue e ligue o drone para aplicar.
                {:else}
                  Não terminou. Desligue e ligue o drone e tente de novo.
                {/if}
              </span>
            </div>
          {/if}
          <button class="btn" disabled={calibrating || flying} onclick={calibrate}>
            <Icon name="rotate-3d" size={16} />
            {calibrating ? 'Calibrando…' : flying ? 'Pouse para calibrar' : 'Calibrar'}
          </button>
        </section>
      {/if}

      {#if drone.caps?.cog_calibration}
        <section>
          <h3 class="label">
            <Icon name="crosshair" size={14} /> Centro de massa <span class="tag">Experimental</span>
          </h3>
          <span class="muted">
            Para o drone que puxa sempre para o mesmo lado ao pairar, mesmo com a IMU calibrada. O comando não é
            documentado pela fabricante: use num lugar fechado, com espaço e com o dedo na emergência.
          </span>
          <ol class="steps">
            <li>Decole e deixe o drone pairando a menos de 1,5 m, longe de paredes.</li>
            <li>Solte os sticks e toque em <b>Calibrar centro de massa</b>.</li>
            <li>
              Não mexa nos sticks por alguns segundos. Se o drone se mover de um jeito estranho, pouse ou use a
              emergência.
            </li>
          </ol>
          <button class="btn" disabled={cogBlocker !== null} onclick={() => void calibrateCog()}>
            <Icon name="crosshair" size={16} />
            {cogBlocker ?? 'Calibrar centro de massa'}
          </button>
        </section>
      {/if}

      <!-- No Android não há teclado nem controle: só os sticks na tela. -->
      {#if !noGamepad}
        <section>
          <h3 class="label"><Icon name="keyboard" size={14} /> Atalhos</h3>
          {#if !platform.mobile}
            <dl class="keys">
              <dt>W / S · A / D</dt><dd>Subir/descer · girar</dd>
              <dt>Setas</dt><dd>Frente/trás · lados</dd>
              <dt>T (segurar)</dt><dd>Decolar</dd>
              <dt>L</dt><dd>Pousar</dd>
              <dt>Espaço</dt><dd>Pairar</dd>
              <dt>P · R</dt><dd>Foto · gravar vídeo</dd>
              <dt>Esc Esc</dt><dd>Emergência</dd>
            </dl>
          {/if}
          <dl class="keys">
            <dt>Menu (segurar)</dt><dd>Decolar · no ar: pousar</dd>
            <dt>View + Menu</dt><dd>Emergência</dd>
            <dt>A · X</dt><dd>Foto · gravar vídeo</dd>
            <dt>B · Y</dt><dd>Pairar · modo rápido</dd>
            <dt>LB + D-pad</dt><dd>Flips (duas direções = diagonal)</dd>
            <dt>RB</dt><dd>Throw & Go · no ar: pouso na palma</dd>
          </dl>
        </section>
      {/if}

      <section class="about">
        <span class="muted">Tello Pilot {__APP_VERSION__} · GPL-3.0 · ícones Lucide (ISC)</span>
        <button class="btn ghost" onclick={() => (resetSettings(), applySafety())}>Restaurar padrões</button>
      </section>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: rgb(0 0 0 / 0.45);
  }

  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 41;
    display: flex;
    flex-direction: column;
    width: min(420px, 100%);
    border-left: 1px solid var(--border);
    background: var(--bg-2);
    box-shadow: var(--shadow);
    animation: slide 0.18s ease-out;
  }

  .drawer.compact {
    width: 100%;
    padding: var(--safe-top) var(--safe-right) var(--safe-bottom) var(--safe-left);
    border-left: 0;
  }

  @keyframes slide {
    from {
      transform: translateX(30px);
      opacity: 0;
    }
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    margin: 0;
    font: 700 17px var(--font-display);
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 6px 16px 20px;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 0;
    border-bottom: 1px solid var(--border);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-weight: 600;
  }

  small {
    display: block;
    color: var(--muted);
    font-size: 12px;
    font-weight: 500;
  }

  .slider {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-weight: 600;
  }

  .slider span {
    display: flex;
    justify-content: space-between;
  }

  .slider b {
    color: var(--accent);
  }

  .slider.disabled {
    opacity: 0.45;
  }

  input[type='range'] {
    accent-color: var(--accent);
  }

  input[type='checkbox'] {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }

  select {
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
  }

  .seg {
    display: flex;
    padding: 3px;
    border-radius: var(--radius-sm);
    background: var(--surface);
  }

  .seg button {
    padding: 5px 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    font-weight: 600;
    cursor: pointer;
  }

  .seg button.sel {
    background: var(--accent);
    color: #032027;
  }

  .monitor {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface);
  }

  .muted {
    color: var(--muted);
    font-size: 12px;
  }

  .tag {
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--surface-3);
    color: var(--warn);
    font-size: 10px;
    letter-spacing: 0.04em;
  }

  .steps {
    margin: 0;
    padding-left: 20px;
    color: var(--text-2);
    font-size: 13px;
    line-height: 1.45;
  }

  .calib {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
  }

  .calib.done {
    color: var(--ok);
  }

  .calib.failed {
    color: var(--warn);
  }

  .progress {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }

  .progress span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.3s;
  }

  .calib.done .progress span {
    background: var(--ok);
  }

  .bar {
    display: grid;
    grid-template-columns: 54px 1fr 34px;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-2);
  }

  .bar .num {
    text-align: right;
  }

  .track {
    position: relative;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
  }

  .track::after {
    content: '';
    position: absolute;
    left: 50%;
    top: -2px;
    width: 1px;
    height: 10px;
    background: var(--border-strong);
  }

  .val {
    position: absolute;
    top: 0;
    bottom: 0;
    border-radius: 3px;
    background: var(--accent);
  }

  .keys {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 14px;
    margin: 0;
    font-size: 13px;
  }

  dt {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--accent);
  }

  dd {
    margin: 0;
    color: var(--text-2);
  }

  .about {
    border-bottom: 0;
  }
</style>
