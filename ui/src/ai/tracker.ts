// Acompanha o alvo escolhido entre as análises. Junta o rastreador visual
// (todo quadro) com as detecções (de tempos em tempos, para corrigir o tamanho
// e a deriva) e suaviza com um filtro de Kalman, que também estima a
// velocidade, usada para compensar o atraso do vídeo. Módulo puro.

import { iou, labelOf, type Box, type Detection } from './boxes.ts';
import { MOSSE } from './mosse.ts';

export const TRACK = {
  /** Detecção com pelo menos esse IoU com o rastreador é o mesmo alvo. */
  matchIou: 0.3,
  /** Abaixo desse IoU o rastreador visual derivou: recomeça na detecção. */
  reanchorIou: 0.6,
  /**
   * Também recomeça se o tamanho difere do detectado mais que isso: a escala do
   * visual deriva devagar e o IoU demora a perceber, mas a distância vem dela.
   */
  reanchorSize: 0.1,
  /** Distância máxima para reencontrar o alvo perdido (em alturas do quadro), crescendo com o tempo. */
  regain: 0.15,
  regainPerS: 0.25,
  regainMax: 0.5,
  /** Tamanho aceito ao reencontrar, em relação ao último visto. */
  sizeRatio: [0.6, 1.6] as const,
  /** Ruído de processo (aceleração) e de medida do filtro de Kalman. */
  q: 4,
  r: 1e-4,
  /** Perdido: a velocidade estimada morre nesse tempo (s), para a previsão não fugir. */
  coastS: 0.4,
};

/** O que o rastreador visual achou no quadro (em fração do quadro). */
export interface Seen {
  box: Box;
  psr: number;
}

export interface Estimate {
  cx: number;
  cy: number;
  /** Tamanho usado para a distância (altura, ou √área para objetos genéricos). */
  size: number;
  /** Velocidades por segundo. */
  vx: number;
  vy: number;
  vSize: number;
}

/** Filtro de Kalman de velocidade constante, num eixo. */
class Kalman {
  p: number;
  v = 0;
  private p00: number;
  private p01 = 0;
  private p11 = 1;

  constructor(p: number) {
    this.p = p;
    this.p00 = TRACK.r;
  }

  predict(dt: number): void {
    const q = TRACK.q;
    this.p += this.v * dt;
    const p00 = this.p00 + dt * (2 * this.p01 + dt * this.p11) + (q * dt ** 3) / 3;
    const p01 = this.p01 + dt * this.p11 + (q * dt ** 2) / 2;
    this.p11 += q * dt;
    this.p00 = p00;
    this.p01 = p01;
  }

  update(z: number): void {
    const s = this.p00 + TRACK.r;
    const k0 = this.p00 / s;
    const k1 = this.p01 / s;
    const y = z - this.p;
    this.p += k0 * y;
    this.v += k1 * y;
    const [p00, p01, p11] = [this.p00, this.p01, this.p11];
    this.p00 = (1 - k0) * p00;
    this.p01 = (1 - k0) * p01;
    this.p11 = p11 - k1 * p01;
  }
}

export function sizeOf(box: Box, generic: boolean): number {
  // Um carro que vira muda de largura, mas não de altura; algo sem classe
  // conhecida usa a área.
  return generic ? Math.sqrt(box.w * box.h) : box.h;
}

export class Tracker {
  /** Categoria do COCO, ou `null` para algo marcado à mão que o detector não conhece. */
  readonly category: string | null;
  readonly label: string;
  /** Última caixa em que o alvo foi visto. */
  box: Box;
  /** Caixa para recomeçar o rastreador visual (depois de corrigir ou reencontrar). */
  reinit: Box | null = null;
  private readonly aspect: number;
  private readonly x: Kalman;
  private readonly y: Kalman;
  private readonly s: Kalman;
  private t: number;
  private lostSince: number | null = null;

  constructor(box: Box, category: string | null, now: number, aspect = 4 / 3) {
    this.box = box;
    this.category = category;
    this.label = labelOf(category);
    this.aspect = aspect;
    this.x = new Kalman(box.x + box.w / 2);
    this.y = new Kalman(box.y + box.h / 2);
    this.s = new Kalman(sizeOf(box, !category));
    this.t = now;
  }

  get generic(): boolean {
    return this.category === null;
  }

  /**
   * Junta o que o rastreador visual achou com as detecções deste quadro
   * (`null` quando o detector não rodou). Devolve a caixa medida, ou `null` se
   * o alvo não foi visto.
   */
  update(seen: Seen | null, detections: Detection[] | null, now: number): Box | null {
    const dt = Math.min(0.5, Math.max(0, (now - this.t) / 1000));
    this.t = now;
    for (const k of [this.x, this.y, this.s]) k.predict(dt);

    const tracked = seen && seen.psr >= MOSSE.lostPsr ? seen.box : null;
    let measured = tracked;
    this.reinit = null;
    if (detections && this.category) {
      const same = detections.filter((d) => d.category === this.category);
      if (tracked) {
        const best = bestBy(same, (d) => iou(d, tracked));
        if (best && best.score >= TRACK.matchIou) {
          measured = best.item;
          const ratio = best.item.h / tracked.h;
          if (best.score < TRACK.reanchorIou || Math.abs(ratio - 1) > TRACK.reanchorSize) this.reinit = best.item;
        }
      } else {
        const found = this.regain(same, now);
        if (found) measured = this.reinit = found;
      }
    }

    if (!measured) {
      this.lostSince ??= now;
      // Sem medida, a velocidade estimada vai morrendo.
      const decay = Math.exp(-dt / TRACK.coastS);
      for (const k of [this.x, this.y, this.s]) k.v *= decay;
      return null;
    }
    this.x.update(measured.x + measured.w / 2);
    this.y.update(measured.y + measured.h / 2);
    this.s.update(sizeOf(measured, this.generic));
    this.box = measured;
    this.lostSince = null;
    return measured;
  }

  /** Onde o alvo deve estar `ahead` segundos depois do último quadro. */
  estimate(ahead = 0): Estimate {
    return {
      cx: this.x.p + this.x.v * ahead,
      cy: this.y.p + this.y.v * ahead,
      size: Math.max(1e-3, this.s.p + this.s.v * ahead),
      vx: this.x.v,
      vy: this.y.v,
      vSize: this.s.v,
    };
  }

  /** Há quanto tempo (ms) o alvo não aparece. */
  lostFor(now: number): number {
    return this.lostSince === null ? 0 : now - this.lostSince;
  }

  /** Perdido: aceita uma detecção da mesma classe perto de onde ele deveria estar. */
  private regain(candidates: Detection[], now: number): Detection | null {
    const lost = this.lostFor(now) / 1000;
    const reach = Math.min(TRACK.regainMax, TRACK.regain + TRACK.regainPerS * lost);
    const last = sizeOf(this.box, false);
    const best = bestBy(candidates, (d) => {
      const ratio = sizeOf(d, false) / last;
      if (ratio < TRACK.sizeRatio[0] || ratio > TRACK.sizeRatio[1]) return -Infinity;
      const dist = Math.hypot((d.x + d.w / 2 - this.x.p) * this.aspect, d.y + d.h / 2 - this.y.p);
      return dist <= reach ? -dist : -Infinity;
    });
    return best && best.score > -Infinity ? best.item : null;
  }
}

function bestBy<T>(items: readonly T[], score: (item: T) => number): { item: T; score: number } | null {
  let best: { item: T; score: number } | null = null;
  for (const item of items) {
    const s = score(item);
    if (!best || s > best.score) best = { item, score: s };
  }
  return best;
}
