// Preferências do usuário, salvas no navegador do app (localStorage).

import type { Protocol, Safety } from './types';

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
  showSticks: boolean;
  rumble: boolean;
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
  showSticks: true,
  rumble: true,
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

export function safetyFromSettings(s: Settings): Safety {
  return {
    stick_limit: s.beginner ? BEGINNER.stickLimit : s.stickLimitPct / 100,
    max_height_cm: s.beginner ? BEGINNER.maxHeightCm : Math.round(s.maxHeightM * 100),
    auto_land_battery: s.autoLandBattery > 0 ? s.autoLandBattery : null,
    stick_timeout_ms: 500,
  };
}
