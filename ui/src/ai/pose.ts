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
