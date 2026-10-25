// Preferências do usuário, salvas no navegador do app (localStorage).

import type { Protocol, Safety, VideoPolicy } from './types';

export interface Settings {
  protocol: Protocol;
  droneIp: string;
  beginner: boolean;
  stickLimitPct: number;
  maxHeightM: number;
  autoLandBattery: number;
  /** Modo 2 (padrão do app): esquerdo = altura/giro, direito = frente/lado. */
  stickMode: 1 | 2;
  deadzone: number;
  expo: number;
  invertPitch: boolean;
  autoStartVideo: boolean;
  /** Ajusta a taxa do vídeo pela perda de pacotes (modo App). */
  adaptiveVideo: boolean;
  /** Teto (adaptativo) ou nível fixo da taxa: 0 = automático do drone, 1..5. */
  videoLevel: number;
  showSticks: boolean;
  rumble: boolean;
  /** IA experimental (detector de objetos no próprio aparelho). */
  aiEnabled: boolean;
  /** Já leu o aviso da IA experimental. */
  aiAcknowledged: boolean;
}

const DEFAULTS: Settings = {
  protocol: 'sdk',
  droneIp: '192.168.10.1',
  beginner: true,
  stickLimitPct: 100,
  maxHeightM: 10,
  autoLandBattery: 10,
  stickMode: 2,
  deadzone: 0.08,
  expo: 0.3,
  invertPitch: false,
  autoStartVideo: true,
  adaptiveVideo: true,
  videoLevel: 4,
  showSticks: true,
  rumble: true,
  aiEnabled: false,
  aiAcknowledged: false,
};

const KEY = 'tello-pilot.settings.v1';

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...DEFAULTS, ...JSON.parse(raw) };
  } catch {
    // armazenamento indisponível: usa os padrões
  }
  return { ...DEFAULTS };
}

export const settings: Settings = $state(load());

export function saveSettings(): void {
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(settings)));
  } catch {
    // ignorado
  }
}

export function resetSettings(): void {
  Object.assign(settings, DEFAULTS);
  saveSettings();
}

/** Modo iniciante: sticks a 40% e no máximo 3 m de altura. */
export const BEGINNER = { stickLimit: 0.4, maxHeightCm: 300 };

export function videoFromSettings(s: Settings): VideoPolicy {
  // No modo adaptativo o nível é o teto: "automático" não serve de teto.
  const level = s.adaptiveVideo && s.videoLevel === 0 ? DEFAULTS.videoLevel : s.videoLevel;
  return { adaptive: s.adaptiveVideo, level };
}

export function safetyFromSettings(s: Settings): Safety {
  return {
    stick_limit: s.beginner ? BEGINNER.stickLimit : s.stickLimitPct / 100,
    max_height_cm: s.beginner ? BEGINNER.maxHeightCm : Math.round(s.maxHeightM * 100),
    auto_land_battery: s.autoLandBattery > 0 ? s.autoLandBattery : null,
    stick_timeout_ms: 500,
  };
}
