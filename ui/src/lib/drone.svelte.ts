// Estado da conexão com o drone e ações de alto nível usadas pela UI.

import { getBackend, type Backend } from './backend';
import { CONDITIONS } from './conditions';
import { errorText } from './format';
import { safetyFromSettings, settings } from './settings.svelte';
import { toast } from './toasts.svelte';
import type { Action, Capabilities, Condition, DroneEvent, Telemetry } from './types';

export type ConnState = 'disconnected' | 'connecting' | 'connected';

interface DroneState {
  conn: ConnState;
  caps: Capabilities | null;
  telemetry: Telemetry | null;
  telemetryAt: number;
  linkLost: boolean;
  /** Rótulo da ação em andamento (ex.: "Decolando"). */
  busy: string | null;
  videoOn: boolean;
  error: string | null;
  noState: boolean;
  noVideo: boolean;
  /** Condições que costumam deixar o voo instável (pouca luz, vento...). */
  conditions: Condition[];
  confirmClose: boolean;
  /** Gravação em andamento (início em performance.now()). */
  recording: { since: number; path: string } | null;
}

export const drone: DroneState = $state({
  conn: 'disconnected',
  caps: null,
  telemetry: null,
  telemetryAt: 0,
  linkLost: false,
  busy: null,
  videoOn: false,
  error: null,
  noState: false,
  noVideo: false,
  conditions: [],
  confirmClose: false,
  recording: null,
});

let backend: Backend | null = null;
const batteryAlerts = new Set<number>();
type Listener = (level: 'warn' | 'critical') => void;
const lowBatteryListeners = new Set<Listener>();

export function onLowBattery(cb: Listener): () => void {
  lowBatteryListeners.add(cb);
  return () => lowBatteryListeners.delete(cb);
}

export async function initDrone(): Promise<Backend> {
  if (backend) return backend;
  backend = await getBackend();
  backend.onEvent(handleEvent);
  backend.onCloseRequest(() => (drone.confirmClose = true));
  return backend;
}

export function isFlying(): boolean {
  return drone.telemetry?.flying === true;
}

function handleEvent(e: DroneEvent): void {
  switch (e.type) {
    case 'telemetry':
      drone.telemetry = e.data;
      drone.telemetryAt = performance.now();
      checkBattery(e.data);
      break;
    case 'warning':
      if (e.code === 'no_state') drone.noState = true;
      if (e.code === 'no_video') drone.noVideo = true;
      if (e.code === 'link_lost') drone.linkLost = true;
      if (e.code === 'link_restored') drone.linkLost = false;
      toast(e.code === 'link_restored' ? 'ok' : 'warn', warningTitle(e.code), e.message, 9000);
      break;
    case 'conditions':
      drone.conditions = e.active;
      break;
    case 'photo_saved':
      toast('ok', 'Foto salva', e.path);
      break;
    case 'disconnected':
      drone.conn = 'disconnected';
      drone.caps = null;
      drone.videoOn = false;
      drone.recording = null;
      drone.conditions = [];
      break;
    case 'connected':
      drone.caps = e.caps;
      break;
  }
}

function warningTitle(code: string): string {
  if (code in CONDITIONS) return CONDITIONS[code as Condition].label;
  return (
    {
      no_state: 'Telemetria bloqueada',
      no_video: 'Vídeo não chegou',
      link_lost: 'Conexão instável',
      link_restored: 'Conexão restabelecida',
      auto_land: 'Pouso automático',
    } as Record<string, string>
  )[code] ?? 'Aviso';
}

function checkBattery(t: Telemetry): void {
  if (t.battery === null || !t.flying) return;
  for (const [level, kind, title, body] of [
    [20, 'warn', 'Bateria em 20%', 'Planeje o pouso.'],
    [15, 'critical', 'Bateria em 15%', 'Pouse agora.'],
  ] as const) {
    if (t.battery <= level && !batteryAlerts.has(level)) {
      batteryAlerts.add(level);
      toast(kind === 'critical' ? 'error' : 'warn', title, body);
      lowBatteryListeners.forEach((cb) => cb(kind));
    }
  }
}

export async function connect(): Promise<void> {
  const b = await initDrone();
  drone.conn = 'connecting';
  drone.error = null;
  drone.noState = drone.noVideo = drone.linkLost = false;
  drone.conditions = [];
  drone.telemetry = null;
  batteryAlerts.clear();
  try {
    const caps = await b.connect(settings.protocol, {
      drone_ip: settings.droneIp.trim(),
      safety: safetyFromSettings(settings),
    });
    drone.caps = caps;
    drone.conn = 'connected';
    if (settings.autoStartVideo) void startVideo();
  } catch (e) {
    drone.conn = 'disconnected';
    drone.error = errorText(e);
  }
}

export async function disconnect(): Promise<void> {
  if (isFlying() && !confirm('O drone está voando. Desconectar fará ele pousar sozinho em ~15 s. Continuar?')) return;
  await backend?.disconnect();
  drone.conn = 'disconnected';
  drone.caps = null;
  drone.videoOn = false;
  drone.recording = null;
  drone.conditions = [];
}

export async function startVideo(): Promise<void> {
  if (await run({ type: 'stream_on' }, null)) drone.videoOn = true;
}

export async function stopVideo(): Promise<void> {
  if (await run({ type: 'stream_off' }, null)) drone.videoOn = false;
}

/** Executa uma ação; erros viram toasts. `label` mostra um indicador de progresso. */
export async function run(action: Action, label: string | null = describe(action)): Promise<boolean> {
  if (!backend || drone.conn !== 'connected') return false;
  if (label) drone.busy = label;
  try {
    await backend.action(action);
    return true;
  } catch (e) {
    toast('error', `${label ?? 'Comando'} falhou`, errorText(e));
    return false;
  } finally {
    if (label && drone.busy === label) drone.busy = null;
  }
}

/** Emergência: corta os motores na hora, sem fila. */
export function emergency(): void {
  if (!backend || drone.conn !== 'connected') return;
  void backend.action({ type: 'emergency' }).catch((e) => toast('error', 'Emergência falhou', errorText(e)));
  toast('error', 'EMERGÊNCIA', 'Motores desligados.');
}

export function applySafety(): void {
  backend?.setSafety(safetyFromSettings(settings));
}

export async function landAndQuit(): Promise<void> {
  drone.confirmClose = false;
  await backend?.landAndQuit();
}

export function backendRef(): Backend | null {
  return backend;
}

function describe(a: Action): string {
  switch (a.type) {
    case 'takeoff':
      return 'Decolando';
    case 'land':
      return 'Pousando';
    case 'throw_and_go':
      return 'Lance o drone';
    case 'palm_land':
      return 'Pouso na palma';
    case 'flip':
      return 'Flip';
    case 'smart_video':
      return 'EZ Shot';
    case 'take_photo':
      return 'Tirando foto';
    default:
      return 'Comando';
  }
}
