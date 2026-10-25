// Caixas do detector, em fração do quadro (0 a 1 nos dois eixos, origem no
// canto de cima à esquerda). Módulo puro.

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Detection extends Box {
  /** Categoria do COCO, em inglês (como o modelo devolve). */
  category: string;
  score: number;
}

/** O que dá para seguir, com o nome mostrado na tela. */
export const FOLLOWABLE: Record<string, string> = {
  person: 'Pessoa',
  bicycle: 'Bicicleta',
  car: 'Carro',
  motorcycle: 'Moto',
  bus: 'Ônibus',
  truck: 'Caminhão',
  boat: 'Barco',
  dog: 'Cachorro',
  cat: 'Gato',
  horse: 'Cavalo',
  'sports ball': 'Bola',
};

/** Nome de uma área marcada à mão que não casou com nenhuma detecção. */
export const GENERIC_LABEL = 'Objeto';

/** IoU mínimo para uma área marcada à mão ser a mesma coisa que uma detecção. */
const MATCH_IOU = 0.3;
/** Fração da detecção que precisa estar dentro da área marcada para valer. */
const MATCH_INSIDE = 0.8;

export function labelOf(category: string | null): string {
  return (category && FOLLOWABLE[category]) || GENERIC_LABEL;
}

export function area(b: Box): number {
  return Math.max(0, b.w) * Math.max(0, b.h);
}

function intersection(a: Box, b: Box): number {
  const w = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
  const h = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
  return w > 0 && h > 0 ? w * h : 0;
}

export function iou(a: Box, b: Box): number {
  const i = intersection(a, b);
  return i > 0 ? i / (area(a) + area(b) - i) : 0;
}

export function contains(b: Box, x: number, y: number): boolean {
  return x >= b.x && x <= b.x + b.w && y >= b.y && y <= b.y + b.h;
}

/** Caixa normalizada entre dois cantos quaisquer, cortada nas bordas do quadro. */
export function rectFrom(x0: number, y0: number, x1: number, y1: number): Box {
  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  const [l, r] = [clamp(Math.min(x0, x1)), clamp(Math.max(x0, x1))];
  const [t, b] = [clamp(Math.min(y0, y1)), clamp(Math.max(y0, y1))];
  return { x: l, y: t, w: r - l, h: b - t };
}

/** A menor detecção sob o toque (a da frente, quando uma está dentro da outra). */
export function pickAt(detections: Detection[], x: number, y: number): Detection | null {
  let best: Detection | null = null;
  for (const d of detections) if (contains(d, x, y) && (!best || area(d) < area(best))) best = d;
  return best;
}

/** A detecção que corresponde a uma área marcada à mão, ou `null` (objeto genérico). */
export function matchRect(detections: Detection[], r: Box): Detection | null {
  let best: Detection | null = null;
  let bestIou = 0;
  for (const d of detections) {
    const overlap = iou(d, r);
    // Uma detecção inteira dentro de uma área não muito maior que ela também vale.
    const inside = area(d) > 0 && intersection(d, r) / area(d) >= MATCH_INSIDE && area(d) >= area(r) / 4;
    if ((overlap >= MATCH_IOU || inside) && overlap > bestIou) [best, bestIou] = [d, overlap];
  }
  return best;
}

/** Altura típica de cada classe (m), para estimar a distância pela caixa. */
const HEIGHT_M: Record<string, number> = {
  person: 1.7,
  bicycle: 1.1,
  car: 1.5,
  motorcycle: 1.2,
  bus: 3.1,
  truck: 3,
  boat: 1.5,
  dog: 0.6,
  cat: 0.3,
  horse: 1.6,
  'sports ball': 0.22,
};

/** Campo de visão vertical da câmera do Tello (graus). */
const VFOV_DEG = 56;

/** Distância aproximada pela altura da caixa, ou `null` para objetos sem classe. */
export function distanceM(box: Box, category: string | null): number | null {
  const real = category ? HEIGHT_M[category] : undefined;
  if (!real || box.h <= 0) return null;
  return real / (2 * Math.tan(((VFOV_DEG / 2) * Math.PI) / 180) * box.h);
}
