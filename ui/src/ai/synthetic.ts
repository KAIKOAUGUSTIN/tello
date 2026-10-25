// Poses sintéticas de uma pessoa em pé, de frente para a câmera. Servem ao motor
// simulado (?ai=mock) e aos testes. Módulo puro.

import { LM, POINTS } from './pose.ts';

export type ArmPose = 'rest' | 'up' | 'side' | 'down';

export interface Figure {
  /** Centro do tronco na horizontal (0..1). */
  cx: number;
  /** Altura da pessoa como fração da altura da imagem. */
  height: number;
  /** Posição dos pés (0..1). */
  feet?: number;
  left?: ArmPose;
  right?: ArmPose;
  /** Visibilidade de todos os pontos. */
  visibility?: number;
}

// Proporções aproximadas do corpo, em frações da altura da pessoa.
const NOSE = 0.07;
const SHOULDER = 0.18;
const HIP = 0.5;
const KNEE = 0.73;
const HALF_SHOULDER = 0.11;
const HALF_HIP = 0.08;
const UPPER_ARM = 0.17;
const FOREARM = 0.15;

/** Cotovelo e punho em relação ao ombro, com x positivo para fora do corpo. */
function arm(pose: ArmPose): { elbow: [number, number]; wrist: [number, number] } {
  const diag = Math.SQRT1_2;
  switch (pose) {
    case 'up':
      return { elbow: [0.03, -UPPER_ARM], wrist: [0.05, -UPPER_ARM - FOREARM] };
    case 'side':
      return { elbow: [UPPER_ARM, 0], wrist: [UPPER_ARM + FOREARM, 0] };
    case 'down':
      return {
        elbow: [UPPER_ARM * diag, UPPER_ARM * diag],
        wrist: [(UPPER_ARM + FOREARM) * diag, (UPPER_ARM + FOREARM) * diag],
      };
    default:
      return { elbow: [0.02, UPPER_ARM], wrist: [0.03, UPPER_ARM + FOREARM] };
  }
}

/** Pose em coordenadas da imagem (0..1); `aspect` é a largura dividida pela altura da imagem. */
export function figure(f: Figure, aspect = 4 / 3): Float32Array {
  const pose = new Float32Array(POINTS * 3);
  const h = f.height;
  const feet = f.feet ?? 0.5 + h / 2;
  const top = feet - h;
  const vis = f.visibility ?? 0.99;
  // Medidas horizontais em frações da largura: a imagem é mais larga que alta.
  const sx = h / aspect;
  const set = (i: number, x: number, y: number) => {
    pose[i * 3] = x;
    pose[i * 3 + 1] = y;
    pose[i * 3 + 2] = vis;
  };
  // Pontos do rosto e das mãos que o esqueleto não usa ficam no nariz.
  for (let i = 0; i < POINTS; i++) set(i, f.cx, top + NOSE * h);

  // De frente para a câmera, o lado direito da pessoa fica à esquerda da imagem.
  for (const [side, dir] of [
    ['right', -1],
    ['left', 1],
  ] as const) {
    const shoulder = side === 'right' ? LM.rightShoulder : LM.leftShoulder;
    const elbow = side === 'right' ? LM.rightElbow : LM.leftElbow;
    const wrist = side === 'right' ? LM.rightWrist : LM.leftWrist;
    const hip = side === 'right' ? LM.rightHip : LM.leftHip;
    const knee = side === 'right' ? LM.rightKnee : LM.leftKnee;
    const ankle = side === 'right' ? LM.rightAnkle : LM.leftAnkle;
    const sxp = f.cx + dir * HALF_SHOULDER * sx;
    const syp = top + SHOULDER * h;
    const a = arm((side === 'right' ? f.right : f.left) ?? 'rest');
    set(shoulder, sxp, syp);
    set(elbow, sxp + dir * a.elbow[0] * sx, syp + a.elbow[1] * h);
    set(wrist, sxp + dir * a.wrist[0] * sx, syp + a.wrist[1] * h);
    set(hip, f.cx + dir * HALF_HIP * sx, top + HIP * h);
    set(knee, f.cx + dir * HALF_HIP * sx, top + KNEE * h);
    set(ankle, f.cx + dir * HALF_HIP * sx, feet);
  }
  return pose;
}
