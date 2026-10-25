// Mensagens entre a interface e o worker de visão.

import type { Box, Detection } from './boxes.ts';
import type { HandSeen } from './hand.ts';
import type { Motion } from './motion.ts';
import type { Seen } from './tracker.ts';

export type Delegate = 'GPU' | 'CPU';

/**
 * Rastreador visual: `init` recomeça na caixa vista no quadro `ts` (o worker a
 * traz até o quadro atual com o giro da câmera); `null` só continua.
 */
export interface TrackCommand {
  init: { box: Box; ts: number } | null;
}

/** Recorte do quadro em resolução cheia onde procurar a mão. */
export interface HandCrop {
  bitmap: ImageBitmap;
  /** Onde o recorte fica no quadro (fração). */
  box: Box;
}

export type ToWorker =
  | { type: 'init' }
  /** Liga (carrega) ou desliga o reconhecedor de gestos. */
  | { type: 'hands'; on: boolean }
  | {
      type: 'frame';
      bitmap: ImageBitmap;
      ts: number;
      /** Rodar o detector neste quadro. */
      detect: boolean;
      /** `null`: sem rastreador (ele é descartado). */
      track: TrackCommand | null;
      /** Procurar a mão neste quadro. */
      hand: HandCrop | null;
    };

export type FromWorker =
  | { type: 'ready'; delegate: Delegate }
  | { type: 'error'; message: string }
  | { type: 'hands'; ok: true }
  | { type: 'hands'; ok: false; message: string }
  | {
      type: 'result';
      ts: number;
      /** `null` quando o detector não rodou neste quadro. */
      detections: Detection[] | null;
      track: Seen | null;
      /** Giro da câmera desde o quadro rastreado anterior (`null`: sem rastrear ou sem medida). */
      motion: Motion | null;
      /** `null` quando a mão não foi procurada neste quadro. */
      hand: HandSeen | null;
      ms: number;
    };
