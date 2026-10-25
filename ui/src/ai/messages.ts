// Mensagens entre a interface e o worker de visão.

import type { Box, Detection } from './boxes.ts';
import type { Seen } from './tracker.ts';

export type Delegate = 'GPU' | 'CPU';

/** Rastreador visual: `init` recomeça nessa caixa; `null` só continua. */
export interface TrackCommand {
  init: Box | null;
}

export type ToWorker =
  | { type: 'init' }
  | {
      type: 'frame';
      bitmap: ImageBitmap;
      ts: number;
      /** Rodar o detector neste quadro. */
      detect: boolean;
      /** `null`: sem rastreador (ele é descartado). */
      track: TrackCommand | null;
    };

export type FromWorker =
  | { type: 'ready'; delegate: Delegate }
  | { type: 'error'; message: string }
  | {
      type: 'result';
      ts: number;
      /** `null` quando o detector não rodou neste quadro. */
      detections: Detection[] | null;
      track: Seen | null;
      ms: number;
    };
