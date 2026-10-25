// Ações de voo de alto nível, compartilhadas por botões, teclado e controle.

import { stopAutopilot } from '../ai/autopilot.svelte';
import { interruptGestures } from '../ai/gestures.svelte';
import { backendRef, drone, emergency as droneEmergency, isFlying, run, startVideo, stopVideo } from './drone.svelte';
import { errorText, stamp } from './format';
import { toast } from './toasts.svelte';
import type { FlipDir } from './types';

const HOLD_MS = 1000;
export const MIN_FLIP_BATTERY = 50;

/** Progresso (0..1) do "segure para decolar", visível na UI. */
export const hold = $state({ takeoff: 0 });

let holdStart = 0;
let holdFrame = 0;

export function beginTakeoffHold(): void {
  if (drone.conn !== 'connected' || isFlying() || drone.busy || holdStart) return;
  holdStart = performance.now();
  const step = () => {
    if (!holdStart) return;
    hold.takeoff = Math.min(1, (performance.now() - holdStart) / HOLD_MS);
    if (hold.takeoff >= 1) {
      endTakeoffHold();
      void run({ type: 'takeoff' });
      return;
    }
    holdFrame = requestAnimationFrame(step);
  };
  holdFrame = requestAnimationFrame(step);
}

export function endTakeoffHold(): void {
  holdStart = 0;
  cancelAnimationFrame(holdFrame);
  hold.takeoff = 0;
}

export function land(): void {
  stopAutopilot('Pousando', 'info');
  interruptGestures();
  void run({ type: 'land' });
}

export function hover(): void {
  stopAutopilot('Pairar', 'info');
  interruptGestures();
  void run({ type: 'stop' }, null);
}

export function emergency(): void {
  stopAutopilot('Emergência');
  interruptGestures();
  droneEmergency();
}

export interface FlipCheck {
  ok: boolean;
  reason?: string;
}

export function canFlip(dir?: FlipDir): FlipCheck {
  const caps = drone.caps;
  if (!caps?.flips) return { ok: false, reason: 'Flips indisponíveis' };
  if (dir && dir.includes('_') && !caps.flips_diagonal) return { ok: false, reason: 'Flips diagonais só no modo App' };
  if (!isFlying()) return { ok: false, reason: 'Decole antes de fazer flips' };
  const bat = drone.telemetry?.battery ?? 0;
  if (bat < MIN_FLIP_BATTERY) return { ok: false, reason: `Flips exigem bateria ≥ ${MIN_FLIP_BATTERY}%` };
  return { ok: true };
}

export function flip(dir: FlipDir): void {
  const check = canFlip(dir);
  if (!check.ok) {
    toast('warn', 'Flip indisponível', check.reason);
    return;
  }
  void run({ type: 'flip', dir });
}

export function throwAndGo(): void {
  if (!drone.caps?.throw_and_go) return void toast('warn', 'Throw & Go só no modo App');
  if (isFlying()) return;
  toast('info', 'Throw & Go', 'Lance o drone para frente nos próximos 5 segundos.');
  void run({ type: 'throw_and_go' });
}

export function palmLand(): void {
  if (!drone.caps?.palm_land) return void toast('warn', 'Pouso na palma só no modo App');
  if (!isFlying()) return;
  toast('info', 'Pouso na palma', 'Coloque a mão aberta embaixo do drone.');
  void run({ type: 'palm_land' });
}

let fastMode = false;

export function toggleFastMode(): void {
  if (!drone.caps?.fast_mode) return void toast('warn', 'Modo rápido só no modo App');
  fastMode = !fastMode;
  void run({ type: 'set_fast_mode', on: fastMode }, null).then((ok) => {
    if (ok) toast('info', fastMode ? 'Modo rápido ligado' : 'Modo rápido desligado');
    else fastMode = !fastMode;
  });
}

export function toggleVideo(): void {
  void (drone.videoOn ? stopVideo() : startVideo());
}

type SnapshotProvider = () => Promise<Uint8Array>;
let snapshotProvider: SnapshotProvider | null = null;

export function setSnapshotProvider(p: SnapshotProvider | null): void {
  snapshotProvider = p;
}

/** Captura o frame atual do vídeo (PNG). */
export async function snapshot(): Promise<void> {
  const backend = backendRef();
  if (!backend || !snapshotProvider || !drone.videoOn) return void toast('warn', 'Ligue o vídeo para capturar');
  try {
    const path = await backend.saveSnapshot(await snapshotProvider(), stamp());
    toast('ok', 'Captura salva', path);
  } catch (e) {
    toast('error', 'Captura falhou', errorText(e));
  }
}

/** Liga/desliga a gravação do vídeo em MP4 (liga o vídeo se preciso). */
export async function toggleRecording(): Promise<void> {
  const backend = backendRef();
  if (!backend || drone.conn !== 'connected') return;
  try {
    if (drone.recording) {
      const r = await backend.stopRecording();
      drone.recording = null;
      toast('ok', `Vídeo salvo (${r.seconds.toFixed(0)} s)`, r.path);
    } else {
      if (!drone.videoOn) await startVideo();
      const path = await backend.startRecording(stamp());
      drone.recording = { since: performance.now(), path };
    }
  } catch (e) {
    drone.recording = null;
    toast('error', 'Gravação falhou', errorText(e));
  }
}

/** Foto: em alta resolução no modo App, captura do vídeo no SDK. */
export function photo(): void {
  if (drone.caps?.hi_res_photo) void run({ type: 'take_photo' });
  else void snapshot();
}
