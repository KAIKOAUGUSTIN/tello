// Controle do "seguir": gira para manter o alvo no centro e anda para a frente
// ou para trás para manter o tamanho (a distância) de quando o seguir começou.
// Atua sobre onde o alvo deve estar agora (a previsão compensa o atraso do
// vídeo), com um termo acumulado no giro para não ficar para trás de quem anda
// de lado. Módulo puro.

import type { Estimate } from './tracker.ts';

export const FOLLOW = {
  yawGain: 2.5,
  /** Ganho do erro acumulado: sem ele, alguém andando de lado fica sempre fora do centro. */
  yawIntegral: 1.5,
  /** Parte máxima do giro que pode vir do erro acumulado. */
  yawIntegralMax: 0.4,
  yawDeadband: 0.03,
  yawMax: 0.6,
  pitchGain: 1.2,
  pitchDamping: 0.4,
  pitchDeadband: 0.08,
  forwardMax: 0.35,
  backMax: 0.4,
  /** Alvo maior que isso (em relação ao tamanho inicial): recua no máximo. */
  tooClose: 1.5,
  /** Caixa encostando na borda de baixo: o alvo está cortado e o tamanho não vale para avançar. */
  bottomEdge: 0.98,
  /** Atraso típico do vídeo do Tello até o quadro chegar à IA (s). */
  videoDelay: 0.2,
  /**
   * Quanto prever, em múltiplos desse atraso. Um pouco mais que 1 cobre também
   * a demora do próprio drone para começar a girar.
   */
  lead: 1.3,
  /** Variação máxima dos sticks por segundo. */
  slewPerS: 3,
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
  /** Tamanho do alvo a manter (o de quando o seguir começou). */
  readonly targetSize: number;
  private integral = 0;

  constructor(targetSize: number) {
    this.targetSize = targetSize;
  }

  /**
   * `est`: o alvo como visto no último quadro (`null` = perdido, fica parado).
   * `age`: segundos desde que esse quadro chegou à IA. `bottom`: borda de
   * baixo da caixa. `dt`: segundos desde o passo anterior. `fast`: modo
   * rápido do drone.
   */
  step(est: Estimate | null, { age = 0, bottom = 0.5, dt = 0, fast = false } = {}): FollowOutput {
    if (!est) {
      this.integral = 0;
      return { yaw: 0, pitch: 0 };
    }
    const ahead = FOLLOW.lead * (age + FOLLOW.videoDelay);
    const cx = est.cx + est.vx * ahead;
    const size = Math.max(1e-3, est.size + est.vSize * ahead);

    // O acumulado usa o erro inteiro (sem zona morta), senão fica oscilando
    // devagar dentro dela; e não acumula com o giro já no limite.
    const raw = FOLLOW.yawGain * deadband(cx - 0.5, FOLLOW.yawDeadband);
    const limit = FOLLOW.yawIntegralMax / FOLLOW.yawIntegral;
    const next = clamp(this.integral + (cx - 0.5) * Math.min(dt, 0.2), -limit, limit);
    if (Math.abs(raw + FOLLOW.yawIntegral * next) < FOLLOW.yawMax || Math.abs(next) < Math.abs(this.integral)) {
      this.integral = next;
    }
    const yaw = clamp(raw + FOLLOW.yawIntegral * this.integral, -FOLLOW.yawMax, FOLLOW.yawMax);

    let pitch: number;
    if (size > FOLLOW.tooClose * this.targetSize) {
      pitch = -FOLLOW.backMax;
    } else {
      const gap = (this.targetSize - size) / this.targetSize;
      const rate = -est.vSize / this.targetSize;
      pitch = clamp(
        FOLLOW.pitchGain * deadband(gap, FOLLOW.pitchDeadband) + FOLLOW.pitchDamping * rate,
        -FOLLOW.backMax,
        FOLLOW.forwardMax,
      );
      // Cortado na borda de baixo, o tamanho parece menor do que é: nunca avança por ele.
      if (bottom >= FOLLOW.bottomEdge) pitch = Math.min(0, pitch);
    }
    // No modo rápido o mesmo stick anda o dobro: tudo cai pela metade.
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
