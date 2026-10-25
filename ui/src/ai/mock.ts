// Cena do motor simulado (?ai=mock): uma pessoa andando devagar de um lado para
// o outro, para desenvolver a interface sem o modelo e sem drone.

import { figure } from './synthetic.ts';

export function mockScene(nowMs: number): Float32Array[] {
  const t = nowMs / 1000;
  const cx = 0.5 + 0.22 * Math.sin((t / 12) * 2 * Math.PI);
  const height = 0.6 + 0.06 * Math.sin((t / 17) * 2 * Math.PI);
  return [figure({ cx, height })];
}
