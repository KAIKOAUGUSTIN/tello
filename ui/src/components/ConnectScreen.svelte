<script lang="ts">
  import { onMount } from 'svelte';

  import { connect, drone, initDrone } from '../lib/drone.svelte';
  import { saveSettings, settings } from '../lib/settings.svelte';
  import type { NetworkCheck, Protocol } from '../lib/types';
  import Icon from './Icon.svelte';

  let { onOpenLog }: { onOpenLog: () => void } = $props();

  let net: NetworkCheck | null = $state(null);
  let checking = $state(false);
  let demo = $state(false);

  async function checkNetwork() {
    checking = true;
    try {
      const b = await initDrone();
      demo = b.kind === 'mock';
      net = await b.networkCheck(settings.droneIp.trim());
    } catch {
      net = null;
    } finally {
      checking = false;
    }
  }

  onMount(() => {
    void checkNetwork();
    const t = setInterval(checkNetwork, 4000);
    return () => clearInterval(t);
  });

  function pick(p: Protocol) {
    settings.protocol = p;
    saveSettings();
  }

  function useSimulator() {
    settings.droneIp = settings.droneIp === '127.0.0.1' ? '192.168.10.1' : '127.0.0.1';
    saveSettings();
    void checkNetwork();
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    saveSettings();
    await connect();
  }

  const connecting = $derived(drone.conn === 'connecting');
</script>

<div class="screen">
  <div class="glow"></div>
  <form class="card" onsubmit={submit}>
    <header>
      <div class="logo" aria-hidden="true">
        <svg viewBox="0 0 48 48" width="44" height="44">
          <circle cx="12" cy="12" r="7" fill="none" stroke="currentColor" stroke-width="2.5" />
          <circle cx="36" cy="12" r="7" fill="none" stroke="currentColor" stroke-width="2.5" />
          <circle cx="12" cy="36" r="7" fill="none" stroke="currentColor" stroke-width="2.5" />
          <circle cx="36" cy="36" r="7" fill="none" stroke="currentColor" stroke-width="2.5" />
          <path d="M17 17 L31 31 M31 17 L17 31" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
          <rect x="18" y="18" width="12" height="12" rx="3" fill="currentColor" />
        </svg>
      </div>
      <div>
        <h1>Tello Pilot</h1>
        <p class="sub">Conecte-se ao seu drone para começar</p>
      </div>
      {#if demo}<span class="demo">Demonstração</span>{/if}
    </header>

    <section class="step">
      <span class="label">1 · Wi-Fi</span>
      <div class="net" class:ok={net?.same_subnet} class:bad={net && !net.same_subnet}>
        <Icon name={net?.same_subnet ? 'wifi' : 'wifi-off'} size={20} />
        <div>
          {#if checking && !net}
            Verificando a rede…
          {:else if net?.same_subnet}
            <strong>Na rede do drone</strong>
            <span class="muted">IP local {net.local_ip}</span>
          {:else}
            <strong>Conecte o computador ao Wi-Fi <code>TELLO-XXXXXX</code></strong>
            <span class="muted">Ligue o drone e escolha a rede dele nas configurações de Wi-Fi.</span>
          {/if}
        </div>
      </div>
    </section>

    <section class="step">
      <span class="label">2 · Modo de conexão</span>
      <div class="modes" role="radiogroup" aria-label="Modo de conexão">
        <button
          type="button"
          role="radio"
          aria-checked={settings.protocol === 'sdk'}
          class="mode"
          class:selected={settings.protocol === 'sdk'}
          onclick={() => pick('sdk')}
        >
          <span class="mode-title"><Icon name="terminal" size={16} /> SDK oficial</span>
          <span class="mode-desc">Documentado e estável. Voo, flips, vídeo e telemetria. Detecta Tello, EDU e TT.</span>
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={settings.protocol === 'app'}
          class="mode"
          class:selected={settings.protocol === 'app'}
          onclick={() => pick('app')}
        >
          <span class="mode-title"><Icon name="rocket" size={16} /> Recursos do app</span>
          <span class="mode-desc">Throw & Go, pouso na palma, flips 8D, EZ Shots e fotos de 5 MP.</span>
        </button>
      </div>
    </section>

    <section class="step">
      <label class="label" for="ip">3 · Endereço do drone</label>
      <div class="ip-row">
        <input id="ip" bind:value={settings.droneIp} spellcheck="false" autocomplete="off" />
        <button type="button" class="btn ghost" onclick={useSimulator}>
          {settings.droneIp === '127.0.0.1' ? 'Usar drone real' : 'Usar simulador'}
        </button>
      </div>
    </section>

    {#if drone.error}
      <div class="error" role="alert">
        <Icon name="triangle-alert" size={18} />
        <div>
          <strong>Não foi possível conectar</strong>
          <p>{drone.error}</p>
          <ul>
            <li>O drone está ligado e com a luz piscando amarelo?</li>
            <li>O computador está no Wi-Fi do drone (e não em outra rede)?</li>
            <li>O app oficial do Tello está fechado no celular?</li>
          </ul>
          <button type="button" class="link" onclick={onOpenLog}>Ver log de comunicação</button>
        </div>
      </div>
    {/if}

    <button class="btn primary connect" type="submit" disabled={connecting}>
      {#if connecting}
        <Icon name="refresh-ccw" class="spin" /> Conectando…
      {:else}
        <Icon name="plug" /> Conectar
      {/if}
    </button>
  </form>
</div>

<style>
  .screen {
    position: relative;
    display: grid;
    place-items: center;
    height: 100%;
    padding: 24px;
    overflow: auto;
    background:
      radial-gradient(1200px 600px at 50% -10%, rgb(45 212 238 / 0.12), transparent 60%),
      linear-gradient(180deg, var(--bg-2), var(--bg));
  }

  .glow {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(rgb(255 255 255 / 0.025) 1px, transparent 1px),
      linear-gradient(90deg, rgb(255 255 255 / 0.025) 1px, transparent 1px);
    background-size: 40px 40px;
    mask-image: radial-gradient(circle at 50% 30%, black, transparent 70%);
    pointer-events: none;
  }

  .card {
    position: relative;
    width: min(560px, 100%);
    padding: 28px;
    border: 1px solid var(--border);
    border-radius: 18px;
    background: rgb(18 25 36 / 0.85);
    backdrop-filter: blur(12px);
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .logo {
    display: grid;
    place-items: center;
    width: 60px;
    height: 60px;
    border-radius: 16px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  h1 {
    margin: 0;
    font: 700 24px/1.1 var(--font-display);
    letter-spacing: -0.01em;
  }

  .sub {
    margin: 4px 0 0;
    color: var(--muted);
  }

  .demo {
    margin-left: auto;
    align-self: flex-start;
    padding: 3px 9px;
    border-radius: 999px;
    background: rgb(251 191 36 / 0.12);
    color: var(--warn);
    font-size: 12px;
    font-weight: 600;
  }

  .step {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .net {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-2);
    color: var(--text-2);
  }

  .net > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .net.ok {
    border-color: rgb(52 211 153 / 0.35);
    color: var(--ok);
  }

  .net.bad {
    border-color: rgb(251 191 36 / 0.35);
    color: var(--warn);
  }

  .net strong {
    color: var(--text);
    font-weight: 600;
  }

  .muted {
    color: var(--muted);
    font-size: 13px;
  }

  code {
    font-family: var(--mono);
    font-size: 0.92em;
    color: var(--accent);
  }

  .modes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .mode {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-2);
    text-align: left;
    cursor: pointer;
    transition:
      border-color 0.15s,
      background 0.15s;
  }

  .mode:hover {
    border-color: var(--border-strong);
  }

  .mode.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .mode-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
  }

  .mode.selected .mode-title {
    color: var(--accent);
  }

  .mode-desc {
    color: var(--muted);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .ip-row {
    display: flex;
    gap: 8px;
  }

  input {
    flex: 1;
    min-height: 38px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-2);
    color: var(--text);
    font-family: var(--mono);
    user-select: text;
  }

  input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .error {
    display: flex;
    gap: 12px;
    padding: 14px;
    border: 1px solid rgb(244 63 94 / 0.4);
    border-radius: var(--radius);
    background: rgb(244 63 94 / 0.08);
    color: var(--danger);
  }

  .error strong {
    color: var(--text);
  }

  .error p {
    margin: 4px 0 8px;
    color: var(--text-2);
    user-select: text;
  }

  .error ul {
    margin: 0 0 8px;
    padding-left: 18px;
    color: var(--muted);
    font-size: 13px;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
    cursor: pointer;
  }

  .connect {
    min-height: 46px;
    font-size: 15px;
  }

  @media (max-width: 520px) {
    .modes {
      grid-template-columns: 1fr;
    }
  }
</style>
