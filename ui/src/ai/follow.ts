// Controle do "seguir pessoa": gira para manter a pessoa no centro e anda para
// a frente ou para trás para manter a distância de quando o seguir começou.
// Ganhos baixos de propósito: o vídeo chega com 200 a 300 ms de atraso.
// Módulo puro.

import type { Torso } from './pose.ts';

export const FOLLOW = {
  /** Suavização das medidas (média móvel exponencial). */
  smoothing: 0.4,
  yawGain: 0.9,
  yawDeadband: 0.06,
  yawMax: 0.35,
  pitchGain: 0.8,
  pitchDeadband: 0.12,
  forwardMax: 0.25,
  backMax: 0.3,
  /** Tronco maior que isso (em relação ao alvo): recua no máximo. */
  tooClose: 1.5,
};

export interface FollowOutput {
  yaw: number;
  pitch: number;
}

function deadband(v: number, band: number): number {
  return Math.sign(v) * Math.max(0, Math.abs(v) - band);
}

function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

export class FollowController {
  /** Tamanho do tronco a manter (o de quando o seguir começou). */
  readonly targetLen: number;
  private cx: number | null = null;
  private len: number | null = null;

  constructor(targetLen: number) {
    this.targetLen = targetLen;
  }

  /** `target` nulo (pessoa perdida) = parado. `fast`: modo rápido do drone. */
  step(target: Torso | null, fast = false): FollowOutput {
    if (!target) {
      this.cx = this.len = null;
      return { yaw: 0, pitch: 0 };
    }
    const a = FOLLOW.smoothing;
    this.cx = this.cx === null ? target.cx : this.cx + a * (target.cx - this.cx);
    this.len = this.len === null ? target.len : this.len + a * (target.len - this.len);
    const yaw = clamp(FOLLOW.yawGain * deadband(this.cx - 0.5, FOLLOW.yawDeadband), -FOLLOW.yawMax, FOLLOW.yawMax);

    let pitch: number;
    if (this.len > FOLLOW.tooClose * this.targetLen) {
      pitch = -FOLLOW.backMax;
    } else {
      let error = (this.targetLen - this.len) / this.targetLen;
      // Sem os quadris o tamanho é só estimado: nunca avança com base nele.
      if (!target.hips) error = Math.min(0, error);
      pitch = clamp(FOLLOW.pitchGain * deadband(error, FOLLOW.pitchDeadband), -FOLLOW.backMax, FOLLOW.forwardMax);
    }
    // No modo rápido o mesmo stick anda o dobro: ganho e limites caem pela metade.
    const scale = fast ? 0.5 : 1;
    return { yaw: yaw * scale, pitch: pitch * scale };
  }
}

/** Limita a variação de cada eixo a `maxStep` por chamada (sem trancos). */
export function slew<K extends string>(
  prev: Record<K, number>,
  next: Record<K, number>,
  maxStep: number,
): Record<K, number> {
  const out = { ...next };
  for (const k of Object.keys(next) as K[]) out[k] = prev[k] + clamp(next[k] - prev[k], -maxStep, maxStep);
  return out;
}
