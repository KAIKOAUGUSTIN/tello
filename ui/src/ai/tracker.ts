// Trava uma pessoa e a acompanha entre as análises, para o drone não trocar de
// alvo quando outra pessoa aparece. Módulo puro.

import { torso, type Pose, type Torso } from './pose.ts';

/** Tamanho do tronco a ~4 m, o limite do modelo. */
export const MIN_LOCK_LEN = 0.12;
/** Tamanho do tronco a ~1,4 m: mais perto que isso não dá para seguir. */
export const MAX_LOCK_LEN = 0.35;
/** Só trava em quem está na faixa central da imagem. */
const LOCK_HALF_WIDTH = 0.3;
/** Deslocamento máximo entre análises (em alturas da imagem), que cresce enquanto a pessoa some. */
const MATCH_DIST = 0.25;
const MATCH_GROWTH_PER_S = 0.25;
const MATCH_DIST_MAX = 0.6;
const MATCH_SIZE: readonly [number, number] = [0.6, 1.6];

export interface Target extends Torso {
  pose: Pose;
}

export type Lock = { ok: true; target: Target } | { ok: false; reason: string };

export function lockTarget(poses: readonly Pose[], aspect = 4 / 3): Lock {
  const people = poses.flatMap((pose) => {
    const t = torso(pose, aspect);
    return t ? [{ ...t, pose }] : [];
  });
  if (!people.length) return { ok: false, reason: 'Ninguém na imagem: fique de frente para o drone' };
  const central = people.filter((t) => Math.abs(t.cx - 0.5) <= LOCK_HALF_WIDTH);
  if (!central.length) return { ok: false, reason: 'Fique no centro da imagem' };
  const target = central.reduce((a, b) => (b.len > a.len ? b : a));
  if (target.len < MIN_LOCK_LEN) return { ok: false, reason: 'Chegue mais perto (até uns 4 m)' };
  if (target.len > MAX_LOCK_LEN) return { ok: false, reason: 'Afaste-se um pouco (pelo menos 1,5 m)' };
  return { ok: true, target };
}

export class Tracker {
  last: Target;
  private aspect: number;
  private lostSince: number | null = null;

  constructor(target: Target, aspect = 4 / 3) {
    this.last = target;
    this.aspect = aspect;
  }

  /** Procura a pessoa travada entre as poses da análise; `null` se ela não está lá. */
  update(poses: readonly Pose[], now: number): Target | null {
    const lost = this.lostFor(now) / 1000;
    const reach = Math.min(MATCH_DIST_MAX, MATCH_DIST + MATCH_GROWTH_PER_S * lost);
    let best: Target | null = null;
    let bestDist = Infinity;
    for (const pose of poses) {
      const t = torso(pose, this.aspect);
      if (!t) continue;
      const ratio = t.len / this.last.len;
      if (ratio < MATCH_SIZE[0] || ratio > MATCH_SIZE[1]) continue;
      const dist = Math.hypot((t.cx - this.last.cx) * this.aspect, t.cy - this.last.cy);
      if (dist <= reach && dist < bestDist) {
        best = { ...t, pose };
        bestDist = dist;
      }
    }
    if (best) {
      this.last = best;
      this.lostSince = null;
    } else {
      this.lostSince ??= now;
    }
    return best;
  }

  /** Há quanto tempo (ms) a pessoa não aparece. */
  lostFor(now: number): number {
    return this.lostSince === null ? 0 : now - this.lostSince;
  }
}
