import type { Sticks } from '../lib/types';

export interface Axis2 {
  x: number;
  y: number;
}

export const CENTER: Axis2 = { x: 0, y: 0 };

/** Zona morta + expo clássico de rádio controle (expo 0 = linear). */
export function shape(v: number, deadzone: number, expo: number): number {
  const a = Math.abs(v);
  if (a <= deadzone) return 0;
  const n = Math.min(1, (a - deadzone) / (1 - deadzone));
  return Math.sign(v) * (expo * n * n * n + (1 - expo) * n);
}

/**
 * Converte os dois sticks físicos/virtuais (y positivo = para cima) nos eixos do drone.
 * Modo 2: esquerdo = giro/altura, direito = lado/frente. Modo 1: altura e frente trocados.
 */
export function toDrone(left: Axis2, right: Axis2, mode: 1 | 2, invertPitch = false): Sticks {
  const pitch = mode === 2 ? right.y : left.y;
  const throttle = mode === 2 ? left.y : right.y;
  return { yaw: left.x, roll: right.x, pitch: invertPitch ? -pitch : pitch, throttle };
}

export function isNeutral(s: Sticks): boolean {
  return s.roll === 0 && s.pitch === 0 && s.throttle === 0 && s.yaw === 0;
}
