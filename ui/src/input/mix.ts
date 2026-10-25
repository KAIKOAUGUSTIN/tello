// Escolhe a fonte que comanda os sticks. Módulo puro (testado no Node).

import type { Sticks } from '../lib/types';
import { isNeutral } from './mapping.ts';

export type SourceId = 'mouse' | 'keyboard' | 'gamepad' | 'ai';

export interface SourceState {
  sticks: Sticks;
  /** Última vez que saiu do centro. */
  activeAt: number;
  /** Último relato. */
  at: number;
}

/** A IA só vale com relatos recentes: se as análises param, o drone para. */
export const AI_STALE_MS = 300;

/**
 * Entre as fontes manuais, a que se mexeu por último. A IA só comanda quando
 * nenhuma fonte manual está fora do centro.
 */
export function pick(sources: ReadonlyMap<SourceId, SourceState>, now: number): SourceId | null {
  let best: SourceId | null = null;
  let bestAt = -Infinity;
  for (const [id, s] of sources) {
    if (id === 'ai' || isNeutral(s.sticks)) continue;
    if (s.activeAt > bestAt) {
      best = id;
      bestAt = s.activeAt;
    }
  }
  if (best) return best;
  const ai = sources.get('ai');
  return ai && !isNeutral(ai.sticks) && now - ai.at <= AI_STALE_MS ? 'ai' : null;
}
