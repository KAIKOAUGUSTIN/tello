// Cena do motor simulado (?ai=mock): uma pessoa andando devagar de um lado para
// o outro, para desenvolver a interface sem o modelo e sem drone. A cada 20 s,
// outra pessoa passa ao fundo e a principal some por 2 s.

import { figure } from './synthetic.ts';

export function mockScene(nowMs: number): Float32Array[] {
  const t = nowMs / 1000;
  const phase = t % 20;
  const people: Float32Array[] = [];
  if (phase < 16 || phase >= 18) {
    const cx = 0.5 + 0.22 * Math.sin((t / 12) * 2 * Math.PI);
    const height = 0.6 + 0.06 * Math.sin((t / 17) * 2 * Math.PI);
    people.push(figure({ cx, height }));
  }
  if (phase >= 8 && phase < 14) people.push(figure({ cx: 0.12 + (phase - 8) * 0.02, height: 0.38, feet: 0.8 }));
  return people;
}
