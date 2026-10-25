// Junta o que cada parte da IA comanda num só relato ao mixer: o seguir gira e
// avança ou recua; a mão sobe, desce e vai para os lados. Parte sem relato
// recente não conta (análises paradas = drone parado).

import { report } from '../input/mixer.svelte';
import { NEUTRAL, type Sticks } from '../lib/types';

export type AiPart = 'follow' | 'hand';

const FRESH_MS = 500;
const parts = new Map<AiPart, { sticks: Sticks; at: number }>();

export function reportAi(part: AiPart, sticks: Sticks): void {
  const now = performance.now();
  parts.set(part, { sticks, at: now });
  const out = { ...NEUTRAL };
  for (const p of parts.values()) {
    if (now - p.at > FRESH_MS) continue;
    for (const k of ['roll', 'pitch', 'throttle', 'yaw'] as const) out[k] = Math.max(-1, Math.min(1, out[k] + p.sticks[k]));
  }
  report('ai', out);
}
