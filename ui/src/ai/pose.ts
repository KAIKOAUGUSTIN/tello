// Pontos do corpo do modelo de pose do MediaPipe e as ligações do esqueleto.
// Módulo puro (sem DOM): roda nos testes do Node.

/**
 * 33 pontos em sequência: x e y (0..1, relativos à imagem) e a visibilidade
 * (0..1). "Esquerdo" e "direito" são os da pessoa: de frente para a câmera, o
 * ombro direito aparece à esquerda da imagem.
 */
export type Pose = ArrayLike<number>;

export const POINTS = 33;

export const LM = {
  nose: 0,
  leftShoulder: 11,
  rightShoulder: 12,
  leftElbow: 13,
  rightElbow: 14,
  leftWrist: 15,
  rightWrist: 16,
  leftHip: 23,
  rightHip: 24,
  leftKnee: 25,
  rightKnee: 26,
  leftAnkle: 27,
  rightAnkle: 28,
} as const;

/** Visibilidade mínima para um ponto contar. */
export const MIN_VISIBILITY = 0.6;

export interface Point {
  x: number;
  y: number;
  v: number;
}

export function point(p: Pose, i: number): Point {
  return { x: p[i * 3], y: p[i * 3 + 1], v: p[i * 3 + 2] };
}

export function visible(p: Pose, i: number, min = MIN_VISIBILITY): boolean {
  return p[i * 3 + 2] >= min;
}

/** Ligações do esqueleto desenhado: tronco, braços e pernas (sem o rosto). */
export const BONES: readonly (readonly [number, number])[] = [
  [LM.leftShoulder, LM.rightShoulder],
  [LM.leftShoulder, LM.leftHip],
  [LM.rightShoulder, LM.rightHip],
  [LM.leftHip, LM.rightHip],
  [LM.leftShoulder, LM.leftElbow],
  [LM.leftElbow, LM.leftWrist],
  [LM.rightShoulder, LM.rightElbow],
  [LM.rightElbow, LM.rightWrist],
  [LM.leftHip, LM.leftKnee],
  [LM.leftKnee, LM.leftAnkle],
  [LM.rightHip, LM.rightKnee],
  [LM.rightKnee, LM.rightAnkle],
];

/** Pontos marcados com um círculo no esqueleto. */
export const JOINTS: readonly number[] = [LM.nose, ...new Set(BONES.flat())];

/** Largura dos ombros em relação ao tronco, para estimar o tronco quando os quadris não aparecem. */
const SHOULDERS_PER_TORSO = 0.6;

export interface Torso {
  /** Centro do tronco (0..1). */
  cx: number;
  cy: number;
  /** Do meio dos ombros ao meio dos quadris, em fração da altura da imagem. */
  len: number;
  /** Os quadris aparecem; sem eles, `len` é uma estimativa pela largura dos ombros. */
  hips: boolean;
}

/** Tronco da pessoa; `aspect` é a largura dividida pela altura da imagem. */
export function torso(p: Pose, aspect = 4 / 3): Torso | null {
  if (!visible(p, LM.leftShoulder) || !visible(p, LM.rightShoulder)) return null;
  const ls = point(p, LM.leftShoulder);
  const rs = point(p, LM.rightShoulder);
  const sx = (ls.x + rs.x) / 2;
  const sy = (ls.y + rs.y) / 2;
  if (visible(p, LM.leftHip) && visible(p, LM.rightHip)) {
    const lh = point(p, LM.leftHip);
    const rh = point(p, LM.rightHip);
    const hx = (lh.x + rh.x) / 2;
    const hy = (lh.y + rh.y) / 2;
    return { cx: (sx + hx) / 2, cy: (sy + hy) / 2, len: Math.hypot((hx - sx) * aspect, hy - sy), hips: true };
  }
  const len = Math.hypot((ls.x - rs.x) * aspect, ls.y - rs.y) / SHOULDERS_PER_TORSO;
  return { cx: sx, cy: sy + len / 2, len, hips: false };
}

/** Distância aproximada até a pessoa, para um tronco de ~0,5 m e a câmera do Tello (~56° na vertical). */
export function distanceM(len: number): number {
  return 0.47 / len;
}
