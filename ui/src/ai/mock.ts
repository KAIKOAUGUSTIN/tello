// Cena do motor simulado (?ai=mock), para desenvolver a interface sem o modelo
// e sem drone: uma pessoa andando de um lado para o outro (some por 2 s a cada
// 20 s), um carro que passa de vez em quando e um balão que o detector não
// conhece (para testar a marcação à mão). Com os gestos ligados, a pessoa faz
// uma sequência fixa de gestos.

import type { Box } from './boxes.ts';
import type { Gesture, HandSeen } from './hand.ts';

export interface MockObject {
  id: string;
  box: Box;
  /** `null`: o detector não reconhece (só dá para marcar à mão). */
  category: string | null;
  shape: 'person' | 'car' | 'balloon';
}

const at = (cx: number, cy: number, w: number, h: number): Box => ({ x: cx - w / 2, y: cy - h / 2, w, h });

export function mockScene(nowMs: number): MockObject[] {
  const t = nowMs / 1000;
  const phase = t % 20;
  const objects: MockObject[] = [];
  if (phase < 16 || phase >= 18) {
    const cx = 0.5 + 0.22 * Math.sin((t / 12) * 2 * Math.PI);
    const h = 0.55 + 0.05 * Math.sin((t / 17) * 2 * Math.PI);
    objects.push({ id: 'pessoa', box: at(cx, 0.6, h * 0.36, h), category: 'person', shape: 'person' });
  }
  if (phase >= 6 && phase < 14) {
    objects.push({ id: 'carro', box: at(-0.15 + (phase - 6) * 0.16, 0.72, 0.3, 0.17), category: 'car', shape: 'car' });
  }
  objects.push({
    id: 'balao',
    box: at(0.78 + 0.05 * Math.sin(t / 3), 0.22 + 0.04 * Math.cos(t / 2), 0.07, 0.1),
    category: null,
    shape: 'balloon',
  });
  return objects;
}

/**
 * Gestos roteirizados, contados de quando os gestos foram ligados (a cada 24 s):
 * palma subindo e voltando (2–8 s), V (9–10,5 s), palma parada (15–17 s) e
 * punho (17–18,5 s).
 */
export function mockHand(sinceMs: number, person: Box | null): HandSeen {
  if (!person) return { gesture: 'none', score: 0, palm: null };
  const t = (sinceMs / 1000) % 24;
  let gesture: Gesture | null = null;
  let raise = 0;
  if (t >= 2 && t < 8) {
    gesture = 'palm';
    // Sobe 20% da altura da pessoa entre 3 e 5 s, segura e volta entre 6 e 8 s.
    raise = 0.2 * Math.min(1, Math.max(0, Math.min(t - 3, 8 - t) / 2));
  } else if (t >= 9 && t < 10.5) gesture = 'victory';
  else if (t >= 15 && t < 17) gesture = 'palm';
  else if (t >= 17 && t < 18.5) gesture = 'fist';
  if (!gesture) return { gesture: 'none', score: 0, palm: null };
  const palm = { x: person.x + person.w * 0.85, y: person.y + person.h * (0.3 - raise) };
  return { gesture, score: 0.9, palm };
}
