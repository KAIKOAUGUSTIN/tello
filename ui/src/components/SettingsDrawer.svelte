<script lang="ts">
  import { applySafety, drone, run } from '../lib/drone.svelte';
  import { resetSettings, saveSettings, settings } from '../lib/settings.svelte';
  import { input } from '../input/mixer.svelte';
  import Icon from './Icon.svelte';

  let { open = $bindable(false) }: { open?: boolean } = $props();
  let bitrate = $state(0);

  function changed() {
    saveSettings();
    applySafety();
  }

  function setBitrate() {
    void run({ type: 'set_video_bitrate', level: bitrate }, null);
  }

  const bars = $derived([
    ['Giro', input.sticks.yaw],
    ['Altura', input.sticks.throttle],
    ['Lados', input.sticks.roll],
    ['Frente', input.sticks.pitch],
  ] as const);
</script>

{#if open}
  <div class="scrim" role="presentation" onclick={() => (open = false)}></div>
  <div class="drawer" role="dialog" aria-label="Configurações">
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
        <label class="row">
          <span>Vibração do controle</span>
          <input type="checkbox" bind:checked={settings.rumble} onchange={changed} />
        </label>
        <label class="row">
          <span>Mostrar sticks na tela</span>
          <input type="checkbox" bind:checked={settings.showSticks} onchange={changed} />
        </label>
        <div class="monitor">
          <span class="muted">{input.gamepad ? `Controle: ${input.gamepad}` : 'Nenhum controle detectado — aperte um botão do controle'}</span>
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
          <div class="row">
            <span>Taxa de vídeo</span>
            <select bind:value={bitrate} onchange={setBitrate}>
              <option value={0}>Automática</option>
              {#each [1, 2, 3, 4, 5] as m (m)}<option value={m}>{m} Mbps</option>{/each}
            </select>
          </div>
        {/if}
      </section>

      <section>
        <h3 class="label"><Icon name="keyboard" size={14} /> Atalhos</h3>
        <dl class="keys">
          <dt>W / S · A / D</dt><dd>Subir/descer · girar</dd>
          <dt>Setas</dt><dd>Frente/trás · lados</dd>
          <dt>T (segurar)</dt><dd>Decolar</dd>
          <dt>L</dt><dd>Pousar</dd>
          <dt>Espaço</dt><dd>Pairar</dd>
          <dt>P · R</dt><dd>Foto · gravar vídeo</dd>
          <dt>Esc Esc</dt><dd>Emergência</dd>
        </dl>
        <dl class="keys">
          <dt>Menu (segurar)</dt><dd>Decolar · no ar: pousar</dd>
          <dt>View + Menu</dt><dd>Emergência</dd>
          <dt>A · X</dt><dd>Foto · gravar vídeo</dd>
          <dt>B · Y</dt><dd>Pairar · modo rápido</dd>
          <dt>LB + D-pad</dt><dd>Flips (duas direções = diagonal)</dd>
          <dt>RB</dt><dd>Throw & Go · no ar: pouso na palma</dd>
        </dl>
      </section>

      <section class="about">
        <span class="muted">Tello Pilot 0.1.0 · GPL-3.0 · ícones Lucide (ISC)</span>
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
