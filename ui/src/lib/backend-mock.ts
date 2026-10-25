// Backend simulado para desenvolver a UI no navegador (npm run dev) e para os
// testes com Playwright. Imita o comportamento do tello-core + simulador.

import { accessUnits, type AccessUnit } from '../video/h264';
import { Emitter, type Backend, type FolderKind } from './backend';
import { CONDITIONS } from './conditions';
import { stamp, VIDEO_MBPS } from './format';
import type {
  Action,
  Capabilities,
  Condition,
  Config,
  DroneEvent,
  LogEntry,
  NetworkCheck,
  PlatformInfo,
  Protocol,
  FlightSummary,
  RecordingResult,
  Safety,
  Sticks,
  Telemetry,
  VideoPacket,
  VideoPolicy,
} from './types';
import { NEUTRAL } from './types';

const fixtureUrl = new URL('../../../crates/tello-sim/fixtures/sample.h264', import.meta.url).href;

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

const BASE_CAPS: Capabilities = {
  protocol: 'sdk',
  model: 'tello_edu',
  sdk_version: '2.0',
  serial: '0TQZSIM0000001',
  firmware: null,
  flips: true,
  flips_diagonal: false,
  throw_and_go: false,
  palm_land: false,
  bounce: false,
  smart_video: false,
  hi_res_photo: false,
  fast_mode: false,
  max_height: false,
  video_bitrate: false,
  stop: true,
  mission_pads: true,
  raw_commands: true,
  imu_calibration: false,
};

function capsFor(protocol: Protocol): Capabilities {
  if (protocol === 'sdk') return { ...BASE_CAPS };
  return {
    ...BASE_CAPS,
    protocol: 'app',
    model: 'tello',
    sdk_version: null,
    firmware: '01.04.92.01',
    flips_diagonal: true,
    throw_and_go: true,
    palm_land: true,
    bounce: true,
    smart_video: true,
    hi_res_photo: true,
    fast_mode: true,
    max_height: true,
    video_bitrate: true,
    mission_pads: false,
    raw_commands: false,
    imu_calibration: true,
  };
}

const APP_ONLY = new Set([
  'throw_and_go',
  'palm_land',
  'bounce',
  'smart_video',
  'take_photo',
  'set_fast_mode',
  'calibrate_imu',
]);

export class MockBackend implements Backend {
  readonly kind = 'mock' as const;
  private events = new Emitter<DroneEvent>();
  private video = new Emitter<VideoPacket>();
  private logs = new Emitter<LogEntry[]>();
  private close = new Emitter<void>();
  private log: LogEntry[] = [];
  private t0 = performance.now();

  private caps: Capabilities | null = null;
  private safety: Safety = { stick_limit: 0.4, max_height_cm: 300, auto_land_battery: 10, stick_timeout_ms: 500 };
  private sticks: Sticks = NEUTRAL;
  private sticksAt = 0;
  private flying = false;
  private height = 0;
  private yaw = 0;
  private battery = 87;
  private motorTime = 0;
  private streaming = false;
  private frames: AccessUnit[] | null = null;
  private timers: number[] = [];
  /** Início do voo (performance.now()) e bateria na decolagem, para o resumo do pouso. */
  private flightStart = 0;
  private flightBattery = 0;
  /** Simula a queda do Wi-Fi: a telemetria para de chegar. */
  private offline = false;
  private videoTimer = 0;
  private statsTimer = 0;
  private videoPolicy: VideoPolicy = { adaptive: true, level: 4 };
  private frameIndex = 0;
  private seq = 0;

  /** `?mobile=1` (ou `?mobile=ios`) simula o app no celular. */
  platform(): Promise<PlatformInfo> {
    const mobile = new URLSearchParams(location.search).get('mobile');
    if (!mobile) return Promise.resolve({ os: 'web', mobile: false });
    return Promise.resolve({ os: mobile === 'ios' ? 'ios' : 'android', mobile: true });
  }

  networkCheck(): Promise<NetworkCheck> {
    return Promise.resolve({ local_ip: '192.168.10.2', same_subnet: true });
  }

  async connect(protocol: Protocol, config: Config): Promise<Capabilities> {
    this.record('info', 'core', `conectando em ${config.drone_ip} (modo ${protocol}, simulado)`);
    await delay(600);
    this.safety = config.safety;
    this.videoPolicy = config.video;
    this.caps = capsFor(protocol);
    this.record('tx', protocol === 'sdk' ? 'cmd' : 'app', protocol === 'sdk' ? 'command' : 'conn_req:\\x96\\x17');
    this.record('rx', protocol === 'sdk' ? 'cmd' : 'app', protocol === 'sdk' ? 'ok' : 'conn_ack');
    this.timers.push(window.setInterval(() => this.tick(), 100));
    this.events.emit({ type: 'connected', caps: this.caps });
    this.mockConditions();
    this.mockLinkDrop();
    return this.caps;
  }

  /** `?linkdrop=1` derruba o link 3 s depois de conectar e reconecta na 3ª tentativa. */
  private mockLinkDrop(): void {
    if (!new URLSearchParams(location.search).get('linkdrop')) return;
    const at = (s: number, f: () => void) => this.timers.push(window.setTimeout(f, s * 1000));
    at(3, () => (this.offline = true));
    at(6, () =>
      this.events.emit({
        type: 'warning',
        code: 'link_lost',
        message: 'A telemetria parou de chegar. Verifique o Wi-Fi do drone.',
      }),
    );
    for (const attempt of [1, 2, 3]) at(6 + attempt * 2, () => this.events.emit({ type: 'reconnecting', attempt }));
    at(13, () => {
      this.offline = false;
      if (this.caps) this.events.emit({ type: 'connected', caps: this.caps });
      this.events.emit({ type: 'warning', code: 'link_restored', message: 'Telemetria voltou a chegar.' });
    });
  }

  /** `?conditions=low_light,wind` simula os avisos de condição de voo. */
  private mockConditions(): void {
    const list = new URLSearchParams(location.search).get('conditions');
    if (!list) return;
    const active = list.split(',').filter((c): c is Condition => c in CONDITIONS);
    window.setTimeout(() => {
      for (const c of active) {
        this.events.emit({ type: 'warning', code: c, message: `Aviso simulado: ${CONDITIONS[c].label}.` });
      }
      this.events.emit({ type: 'conditions', active });
    }, 1500);
  }

  async disconnect(): Promise<void> {
    this.timers.forEach(clearInterval);
    this.timers = [];
    this.offline = false;
    this.stopVideo();
    this.caps = null;
    this.flying = false;
    this.height = 0;
    this.record('info', 'core', 'desconectado');
    this.events.emit({ type: 'disconnected', reason: 'desconectado pelo usuário' });
  }

  async action(a: Action): Promise<string> {
    if (!this.caps) throw 'não conectado';
    this.record('tx', 'cmd', JSON.stringify(a));
    if (this.caps.protocol === 'sdk' && APP_ONLY.has(a.type)) {
      throw 'não suportado neste modo/modelo: recurso do app oficial: conecte no modo App';
    }
    switch (a.type) {
      case 'takeoff':
        if (this.flying) throw 'o drone recusou o comando: error Motor stop';
        await delay(1200);
        this.flying = true;
        this.height = 80;
        this.flightStart = performance.now();
        this.flightBattery = Math.round(this.battery);
        break;
      case 'throw_and_go':
        await delay(1500);
        this.flying = true;
        this.height = 120;
        break;
      case 'land':
      case 'palm_land':
        await delay(1200);
        this.flying = false;
        this.height = 0;
        this.flightReport();
        break;
      case 'emergency':
        this.flying = false;
        this.height = 0;
        break;
      case 'calibrate_imu':
        if (this.flying) throw 'o drone recusou o comando: pouse o drone antes de calibrar a IMU';
        this.mockCalibration();
        break;
      case 'flip':
        if (!this.flying) throw 'o drone recusou o comando: error Not joystick';
        if (this.battery < 50) throw 'o drone recusou o comando: error No valid imu';
        await delay(900);
        break;
      case 'stream_on':
        await this.startVideo();
        break;
      case 'stream_off':
        this.stopVideo();
        break;
      case 'take_photo':
        await delay(800);
        this.events.emit({ type: 'photo_saved', path: 'C:\\Users\\voce\\Pictures\\Tello Pilot\\tello-foto.jpg' });
        break;
      case 'smart_video':
        if (!this.flying) throw 'o drone recusou o comando: precisa estar voando';
        break;
      default:
        await delay(100);
    }
    this.record('rx', 'cmd', 'ok');
    return 'ok';
  }

  setSticks(sticks: Sticks): void {
    this.sticks = sticks;
    this.sticksAt = performance.now();
  }

  setSafety(safety: Safety): void {
    this.safety = safety;
  }

  setVideoPolicy(policy: VideoPolicy): void {
    this.videoPolicy = policy;
  }

  onEvent(cb: (e: DroneEvent) => void) {
    return this.events.on(cb);
  }

  onVideo(cb: (p: VideoPacket) => void) {
    return this.video.on(cb);
  }

  onLog(cb: (entries: LogEntry[]) => void) {
    return this.logs.on(cb);
  }

  onCloseRequest(cb: () => void) {
    return this.close.on(cb);
  }

  async logSnapshot(): Promise<LogEntry[]> {
    return [...this.log];
  }

  async exportLog(stamp: string): Promise<string> {
    return `Documentos/Tello Pilot/logs/tello-log-${stamp}.jsonl (simulado)`;
  }

  async saveSnapshot(_png: Uint8Array, stamp: string): Promise<string> {
    return `Imagens/Tello Pilot/tello-${stamp}.png (simulado)`;
  }

  private recordingSince = 0;

  async startRecording(stamp: string): Promise<string> {
    this.recordingSince = performance.now();
    return `Vídeos/Tello Pilot/tello-video-${stamp}.mp4 (simulado)`;
  }

  async stopRecording(): Promise<RecordingResult> {
    const seconds = (performance.now() - this.recordingSince) / 1000;
    return { path: 'Vídeos/Tello Pilot/tello-video.mp4 (simulado)', frames: Math.round(seconds * 30), seconds };
  }

  async openFolder(_kind: FolderKind): Promise<void> {}

  async openReport(_path: string): Promise<void> {}

  /** Uma posição a cada 2 s, como alguém girando o drone com calma. */
  private mockCalibration(): void {
    this.events.emit({ type: 'calibration', steps: 0, progress: 0, state: 'running' });
    for (let i = 1; i <= 12; i++) {
      this.timers.push(
        window.setTimeout(() => {
          const progress = Math.round((i / 12) * 100);
          const state = i === 12 ? 'done' : 'running';
          this.events.emit({ type: 'calibration', steps: Math.floor(i / 2), progress, state });
        }, i * 1000),
      );
    }
  }

  /** Resumo como o do tello-core; com `?conditions=` aparece uma causa. */
  private flightReport(): void {
    const conditions = new URLSearchParams(location.search).get('conditions');
    const seconds = (performance.now() - this.flightStart) / 1000;
    const summary: FlightSummary = {
      started_unix_ms: Date.now() - seconds * 1000,
      duration_s: Math.round(seconds),
      interrupted: false,
      battery_start: this.flightBattery,
      battery_end: Math.round(this.battery),
      max_height_cm: 180,
      wifi_min: 72,
      wifi_avg: 88,
      video_loss_avg: this.caps?.protocol === 'app' ? 0.4 : null,
      temp_max_c: 66,
      warnings: conditions ? conditions.split(',').length : 0,
      link_lost_s: 0,
      causes: conditions ? ['Pouca luz para a câmera de baixo em 40% do voo: o drone perde a referência.'] : [],
    };
    const path = `C:\\Users\\voce\\Documents\\Tello Pilot\\logs\\voo-${stamp()}.html`;
    this.events.emit({ type: 'flight_report', path, summary });
  }

  async landAndQuit(): Promise<void> {
    await this.disconnect();
  }

  private effectiveSticks(): Sticks {
    if (performance.now() - this.sticksAt > this.safety.stick_timeout_ms) return NEUTRAL;
    const k = this.safety.stick_limit;
    const c = (v: number) => Math.max(-1, Math.min(1, v)) * k;
    const s = { roll: c(this.sticks.roll), pitch: c(this.sticks.pitch), throttle: c(this.sticks.throttle), yaw: c(this.sticks.yaw) };
    const max = this.safety.max_height_cm;
    if (max !== null && this.height >= max - 10 && s.throttle > 0) s.throttle = 0;
    return s;
  }

  private tick(): void {
    if (this.offline) return;
    const dt = 0.1;
    const s = this.effectiveSticks();
    if (this.flying) {
      this.height = Math.max(20, Math.min(1000, this.height + s.throttle * 100 * dt));
      this.yaw = (((this.yaw + s.yaw * 90 * dt + 180) % 360) + 360) % 360 - 180;
      this.motorTime += dt;
      this.battery = Math.max(0, this.battery - dt / 20);
    }
    const t: Telemetry = {
      battery: Math.round(this.battery),
      height_cm: this.flying ? Math.round(this.height) : 0,
      tof_cm: this.flying ? Math.round(this.height) + 10 : 10,
      baro_m: 100 + this.height / 100,
      temp_low_c: 62,
      temp_high_c: 65,
      pitch_deg: this.flying ? Math.round(-s.pitch * 15) : 0,
      roll_deg: this.flying ? Math.round(s.roll * 15) : 0,
      yaw_deg: Math.round(this.yaw),
      speed_x: this.flying ? Math.round(s.pitch * 30) : 0,
      speed_y: this.flying ? Math.round(s.roll * 30) : 0,
      speed_z: this.flying ? Math.round(-s.throttle * 10) : 0,
      accel: [-3, 2, -998],
      flight_time_s: Math.round(this.motorTime),
      wifi_strength: 90,
      flying: this.flying,
      mission_pad: null,
      battery_low: this.battery < 20,
      light_strength: this.caps?.protocol === 'app' ? 0 : null,
      fast_mode: null,
      wind: this.caps?.protocol === 'app' ? false : null,
      wifi_interference: this.caps?.protocol === 'app' ? 0 : null,
    };
    this.events.emit({ type: 'telemetry', data: t });
    if (Math.random() < 0.3) this.record('rx', 'state', `h:${t.height_cm};bat:${t.battery};yaw:${t.yaw_deg};`);
  }

  private async startVideo(): Promise<void> {
    if (!this.frames) {
      const buf = new Uint8Array(await (await fetch(fixtureUrl)).arrayBuffer());
      this.frames = accessUnits(buf);
    }
    if (this.streaming) return;
    this.streaming = true;
    this.frameIndex = 0;
    this.videoTimer = window.setInterval(() => {
      const frames = this.frames!;
      const f = frames[this.frameIndex % frames.length];
      this.frameIndex++;
      this.video.emit({ data: f.data, key: f.key, hasSps: f.hasSps, seq: ++this.seq });
    }, 1000 / 30);
    this.statsTimer = window.setInterval(() => this.videoStats(), 1000);
  }

  /** Resumo do vídeo como o do tello-core; `?videoloss=8` simula perda no modo App. */
  private videoStats(): void {
    const app = this.caps?.protocol === 'app';
    const p = this.videoPolicy;
    const loss = Number(new URLSearchParams(location.search).get('videoloss') ?? 0);
    const level = app && (p.adaptive || p.level > 0) ? Math.max(1, p.level - (loss > 5 ? 1 : 0)) : null;
    this.events.emit({
      type: 'video_stats',
      fps: 30,
      kbps: level ? Math.round(VIDEO_MBPS[level] * 900) : 2400,
      loss_pct: app ? loss : null,
      level,
    });
  }

  private stopVideo(): void {
    this.streaming = false;
    clearInterval(this.videoTimer);
    clearInterval(this.statsTimer);
  }

  private record(dir: LogEntry['dir'], ch: LogEntry['ch'], text: string): void {
    const e: LogEntry = { t_ms: Math.round(performance.now() - this.t0), dir, ch, text };
    this.log.push(e);
    if (this.log.length > 5000) this.log.shift();
    this.logs.emit([e]);
  }
}
